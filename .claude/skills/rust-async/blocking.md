# Blocking and CPU-bound work

Part of [rust-async](SKILL.md). Versions: tokio 1.53.3 on Rust 1.99.

## Contents

1. [What counts as blocking](#1-what-counts-as-blocking)
2. [Where blocking work goes](#2-where-blocking-work-goes)
3. [Bounded CPU work in `api`](#3-bounded-cpu-work-in-api)
4. [The perception worker and ONNX Runtime's threads](#4-the-perception-worker-and-onnx-runtimes-threads)

## 1. What counts as blocking

"Async code should never spend a long time without reaching an `.await`." Tokio can only switch tasks at an `.await`, so code that runs long between two of them stops every other task on that worker thread. A rule of thumb is no more than 10 to 100 microseconds between `.await`s. [Alice Ryhl, Async: What is blocking?](https://ryhl.io/blog/async-what-is-blocking/), [tokio: CPU-bound tasks and blocking code](https://docs.rs/tokio/1.53.3/tokio/index.html#cpu-bound-tasks-and-blocking-code)

Blocking in this project:

| Where | Work | Why it blocks |
|---|---|---|
| `api`, uploads | Blur and exposure check, EXIF orientation, resize, HEIC decode, face detection and blurring (architecture [§8.1](../../../docs/architecture/README.md#81-scan--diagnosis--plan) step 2) | CPU-bound, milliseconds to seconds per image |
| `api` | Text embeddings on CPU, depth measurement and plane fits ([§10](../../../docs/architecture/README.md#10-models-and-gpu-placement)) | CPU-bound |
| `perception-worker` | `ort` `Session::run` and pre/post-processing | A synchronous call that runs until the model finishes |
| anywhere | `std::thread::sleep`, `std::fs`, a `std::sync::Mutex` held for long, `std::net` | Synchronous I/O or waiting |

Not blocking: `tokio::time::sleep`, the async I/O of `reqwest`, `aws-sdk-s3` and the `surrealdb` SDK, and a `std::sync::Mutex` locked briefly with no `.await` inside ([SKILL.md, locks](SKILL.md#7-locks)).

## 2. Where blocking work goes

| | CPU-bound | Synchronous I/O | Runs forever |
|---|---|---|---|
| `spawn_blocking` | Suboptimal | OK | No |
| Dedicated thread (`std::thread::spawn`) | OK | OK | OK |

The table is from [Async: What is blocking?](https://ryhl.io/blog/async-what-is-blocking/#summary) by a tokio maintainer (its rayon row is left out: `rayon` isn't a project dependency); tokio's own docs say the same in [spawn_blocking: When to use spawn_blocking vs dedicated threads](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html#when-to-use-spawn_blocking-vs-dedicated-threads) and [CPU-bound tasks and blocking code](https://docs.rs/tokio/1.53.3/tokio/index.html#cpu-bound-tasks-and-blocking-code). Rules:

- **`spawn_blocking` is for bounded work that ends on its own.** It runs the closure on the blocking pool. The pool starts threads on demand up to **512** (`Builder::max_blocking_threads`), keeps an idle thread for **10 s** (`thread_keep_alive`), and beyond the limit queues work with **no backpressure**, so the queue can grow without bound. [spawn_blocking](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html), [max_blocking_threads](https://docs.rs/tokio/1.53.3/tokio/runtime/struct.Builder.html#method.max_blocking_threads), [thread_keep_alive](https://docs.rs/tokio/1.53.3/tokio/runtime/struct.Builder.html#method.thread_keep_alive)
- **512 threads is far more than the cores**, so many CPU-bound jobs on `spawn_blocking` oversubscribe the CPU. The docs: "When running many CPU-bound computations, a semaphore or some other synchronization primitive should be used to limit the number of computations executed in parallel." Section 3 does that. [spawn_blocking](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html)
- **Work that runs for the life of the process goes on a dedicated thread**, not `spawn_blocking`: each long task holds a pool thread for its whole life. [spawn_blocking: When to use spawn_blocking vs dedicated threads](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html#when-to-use-spawn_blocking-vs-dedicated-threads), [Spawn a dedicated thread](https://ryhl.io/blog/async-what-is-blocking/#spawn-a-dedicated-thread)
- **`spawn_blocking` can't be aborted once it has started**; `abort()` has no effect, and runtime shutdown waits for it. Pass a `CancellationToken` in and check `is_cancelled()` between steps. [spawn_blocking](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html), [Runtime: Shutdown](https://docs.rs/tokio/1.53.3/tokio/runtime/struct.Runtime.html#shutdown)
- **Don't use `block_in_place`.** It panics on a `current_thread` runtime (every `#[tokio::test]` by default), it suspends everything else in the same task (a `join!` sibling, the other `select!` branches), and its work can't be cancelled either. `spawn_blocking` has none of the first two problems. [block_in_place](https://docs.rs/tokio/1.53.3/tokio/task/fn.block_in_place.html)
- From a plain thread, talk to async code with `mpsc::Sender::blocking_send` and `Receiver::blocking_recv`; both panic if called inside an async context. [spawn_blocking: Related APIs](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html#related-apis-and-patterns-for-bridging-asynchronous-and-blocking-code), [blocking_recv](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/struct.Receiver.html#method.blocking_recv)

## 3. Bounded CPU work in `api`

`api` does all of the CPU work in section 1 on request paths. Run it through one `CpuPool` in `AppState`: a `Semaphore` in front of `spawn_blocking`, so at most N jobs use the CPU at once and the rest wait **in async code** (no blocking thread is taken while waiting). Use `std::thread::available_parallelism()` for N ([std](https://doc.rust-lang.org/std/thread/fn.available_parallelism.html)). The permit moves into the closure, so it is released when the work ends, even by panic. [Semaphore::acquire_owned](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Semaphore.html#method.acquire_owned)

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;

/// Runs CPU-bound work on the blocking pool, at most `permits` at a time.
#[derive(Clone)]
pub struct CpuPool {
    permits: Arc<Semaphore>,
}

#[derive(Debug, thiserror::Error)]
pub enum CpuError {
    #[error("the CPU pool is closed")]
    Closed,
    #[error("the CPU task panicked")]
    Panicked(#[source] tokio::task::JoinError),
}

impl CpuPool {
    pub fn new(permits: usize) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(permits)),
        }
    }

    pub async fn run<T, F>(&self, work: F) -> Result<T, CpuError>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        // Wait here, in async code, for a slot; the blocking thread only starts with one.
        let permit = self
            .permits
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| CpuError::Closed)?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit; // released when the work ends, even on panic
            work()
        })
        .await
        .map_err(CpuError::Panicked)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn never_more_than_the_permits_at_once() {
    let pool = CpuPool::new(2);
    let running = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let mut set = JoinSet::new();
    for _ in 0..8 {
        let (pool, running, peak) = (pool.clone(), running.clone(), peak.clone());
        set.spawn(async move {
            pool.run(move || {
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(20)); // stands in for a blur check
                running.fetch_sub(1, Ordering::SeqCst);
            })
            .await
        });
    }
    while let Some(res) = set.join_next().await {
        res.expect("task did not panic").expect("pool open");
    }
    assert!(peak.load(Ordering::SeqCst) <= 2);
}

#[tokio::test]
async fn a_panic_in_the_work_comes_back_as_an_error() {
    let pool = CpuPool::new(1);
    let err = pool.run(|| panic!("bad image")).await;
    assert!(matches!(err, Err(CpuError::Panicked(e)) if e.is_panic()));
    // The permit came back, so the pool still works.
    assert_eq!(pool.run(|| 2 + 2).await.expect("pool works"), 4);
}
```

Notes:

- `Semaphore::acquire_owned` is not cancel safe in the "fair queue" sense: a caller that gives up (its request is dropped) loses its place, which is what we want. [Semaphore::acquire_owned](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Semaphore.html#method.acquire_owned)
- If the caller's future is dropped (a request timeout, a `select!` that another branch won), work still waiting for a permit is dropped with it, but a closure already on the blocking pool runs to the end and its result is discarded ([spawn_blocking](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html)).
- `api` also runs ONNX models on CPU (the face detector and the text embedder, [architecture 10](../../../docs/architecture/README.md#10-models-and-gpu-placement)). ONNX Runtime gives **each session** an intra-op pool of one thread per physical core by default, and the calling thread joins in ([ONNX Runtime: Set number of intra-op threads](https://onnxruntime.ai/docs/performance/tune-performance/threading.html#set-number-of-intra-op-threads)). N pool permits times a core-sized intra-op pool oversubscribes the CPU. So either give those sessions a small intra-op count and keep N permits, or keep ONNX's default and run model calls one at a time (*this skill's choice*, from the same ONNX Runtime page). How to set the count on an `ort` session is in [rust-workers](../rust-workers/SKILL.md).

## 4. The perception worker and ONNX Runtime's threads

The perception worker holds `slots = 1` and batches inside the job ([contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)), so inference is one long synchronous call at a time, for the life of the process. That is "runs forever" in the table above: give the models **one dedicated OS thread** that owns them, and talk to it over channels. Async work (heartbeat every 10 s, claim long poll, event posts) stays on the runtime and keeps running during inference, which [contracts 5](../../../docs/architecture/contracts.md#post-internalworkersheartbeat) depends on. Because the models never leave their thread, they don't need to be `Send`.

```rust
use std::thread;

use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

/// What the inference thread is asked to do. The real request carries tensors.
pub struct InferRequest {
    pub image: Vec<u8>,
    /// Cancelled when the heartbeat response lists the job in `cancel_job_ids`.
    pub cancel: CancellationToken,
    pub respond_to: oneshot::Sender<Option<usize>>,
}

/// Owns the models on one OS thread for the life of the process.
#[derive(Clone)]
pub struct Inference {
    requests: mpsc::Sender<InferRequest>,
}

impl Inference {
    /// `load` runs on the new thread, so the models never cross threads.
    pub fn start<M, L>(load: L) -> std::io::Result<Self>
    where
        L: FnOnce() -> M + Send + 'static,
        M: FnMut(&[u8]) -> usize,
    {
        let (requests, mut inbox) = mpsc::channel::<InferRequest>(1);
        thread::Builder::new()
            .name("inference".to_owned())
            .spawn(move || {
                let mut model = load();
                // blocking_recv: this is a plain thread, not a runtime worker.
                while let Some(req) = inbox.blocking_recv() {
                    // Check between stages (rust-workers' ort.md also terminates a running stage).
                    let out = (!req.cancel.is_cancelled()).then(|| model(&req.image));
                    // Err only means the caller stopped waiting (cancelled or dropped).
                    let _ = req.respond_to.send(out);
                }
            })?;
        Ok(Self { requests })
    }

    pub async fn run(&self, image: Vec<u8>, cancel: CancellationToken) -> Option<usize> {
        let (respond_to, response) = oneshot::channel();
        let req = InferRequest {
            image,
            cancel,
            respond_to,
        };
        self.requests.send(req).await.ok()?;
        response.await.ok().flatten()
    }
}

#[tokio::test]
async fn the_runtime_stays_free_while_the_thread_works() {
    let inference = Inference::start(|| {
        |image: &[u8]| {
            thread::sleep(std::time::Duration::from_millis(50)); // stands in for session.run
            image.len()
        }
    })
    .expect("thread starts");
    let out = inference.run(vec![0; 3], CancellationToken::new()).await;
    assert_eq!(out, Some(3));

    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert_eq!(inference.run(vec![0; 3], cancelled).await, None);
}
```

Threads on the rig's perception worker:

- **ONNX Runtime's own pools.** Each session has an intra-op pool (default: one thread per physical core) and, only with `ORT_PARALLEL` execution, an inter-op pool (default: also one per physical core). By default idle pool threads spin waiting for work, which uses CPU. [ONNX Runtime threading](https://onnxruntime.ai/docs/performance/tune-performance/threading.html), [inter-op threads](https://onnxruntime.ai/docs/performance/tune-performance/threading.html#set-number-of-inter-op-threads), [spinning](https://onnxruntime.ai/docs/performance/tune-performance/threading.html#thread-spinning-behavior)
- **Tokio's workers.** `#[tokio::main]` starts one per core ([tokio::main](https://docs.rs/tokio/1.53.3/tokio/attr.main.html)). The worker's async side is light (a heartbeat every 10 s, the claim long poll, event posts), so they are mostly idle.
- Run the stages of a job one after another on the inference thread (several sessions running at once would each bring a core-sized intra-op pool, [ONNX Runtime threading](https://onnxruntime.ai/docs/performance/tune-performance/threading.html#set-number-of-intra-op-threads)), do image decode and resize on that thread too rather than on tokio workers, and leave the session thread settings to [rust-workers](../rust-workers/SKILL.md), which owns `ort` session configuration. *This skill's choice.*
- Job cancellation (`cancel_job_ids` in the heartbeat response, [contracts 5](../../../docs/architecture/contracts.md#post-internalworkersheartbeat)) is checked between stages. A stage that has started can also be stopped from another thread: ort 2.0.0-rc.14's `RunOptions::terminate` sets a flag the run checks, and the run ends with an error ([RunOptions::terminate](https://docs.rs/ort/2.0.0-rc.14/ort/session/struct.RunOptions.html#method.terminate)). [rust-workers, ort.md](../rust-workers/ort.md#4-the-inference-thread) calls it when the job's token fires and, through a drop guard, when the wait for the answer is dropped. Without it, a started stage finishes and its result is dropped.
- At process exit the inference thread doesn't hold anything up: when `main` returns the process ends even if other threads are running ([std::thread](https://doc.rust-lang.org/std/thread/index.html#the-threading-model)). Work on `spawn_blocking`, in contrast, is waited for by the runtime's drop ([shutdown.md](shutdown.md#4-what-runtime-drop-waits-for)).
