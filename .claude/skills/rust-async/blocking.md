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
| `api`, uploads | Decode with EXIF orientation (HEIC too), the quality gate, face detection and blurring, resize, WebP (architecture [§8.1](../../../docs/architecture/README.md#81-scan--diagnosis--plan) step 2; [backend vision 7](../../../docs/backend/vision.md#7-the-upload-pipeline)) | CPU-bound, 0.1 to 1 s per image ([backend vision 12](../../../docs/backend/vision.md#12-concurrency-and-lifetimes)) |
| `api` | Text embeddings on CPU, depth measurement and plane fits, mask RLE ([architecture 10](../../../docs/architecture/README.md#10-models-and-gpu-placement); [backend vision 9](../../../docs/backend/vision.md#9-measurement), [10](../../../docs/backend/vision.md#10-the-text-embedder)) | CPU-bound |
| `perception-worker` | Image decoding and pre-processing into tensors; `ort` `Session::run` on the GPU; post-processing | Decoding is CPU-bound; a run is a synchronous call that lasts until the model finishes |
| anywhere | `std::thread::sleep`, `std::fs`, a `std::sync::Mutex` held for long, `std::net` | Synchronous I/O or waiting |

Not blocking: `tokio::time::sleep`, the async I/O of `reqwest`, `aws-sdk-s3` and the `surrealdb` SDK, and a `std::sync::Mutex` locked briefly with no `.await` inside ([SKILL.md, locks](SKILL.md#7-locks)).

## 2. Where blocking work goes

| | CPU-bound | Synchronous I/O | Runs forever |
|---|---|---|---|
| `spawn_blocking` | Suboptimal | OK | No |
| Dedicated thread (`std::thread::spawn`) | OK | OK | OK |

The table is from [Async: What is blocking?](https://ryhl.io/blog/async-what-is-blocking/#summary) by a tokio maintainer (its rayon row is left out: `rayon` isn't a project dependency); tokio's own docs say the same in [spawn_blocking: When to use spawn_blocking vs dedicated threads](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html#when-to-use-spawn_blocking-vs-dedicated-threads) and [CPU-bound tasks and blocking code](https://docs.rs/tokio/1.53.3/tokio/index.html#cpu-bound-tasks-and-blocking-code). Rules:

- **`spawn_blocking` is for bounded work that ends on its own.** It runs the closure on the blocking pool. The pool starts threads on demand up to **512** ([`Builder::max_blocking_threads`](https://docs.rs/tokio/1.53.3/tokio/runtime/struct.Builder.html#method.max_blocking_threads), whose default it is), keeps an idle thread for **10 s** ([`thread_keep_alive`](https://docs.rs/tokio/1.53.3/tokio/runtime/struct.Builder.html#method.thread_keep_alive)), and beyond the limit "the tasks are put in a queue". `spawn_blocking` is a plain function that returns at once, so nothing slows the caller down: there is **no backpressure** ([spawn_blocking](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html)).
- **512 threads is far more than the cores**, so many CPU-bound jobs on `spawn_blocking` oversubscribe the CPU. The docs: "When running many CPU-bound computations, a semaphore or some other synchronization primitive should be used to limit the number of computations executed in parallel." Section 3 does that. [spawn_blocking](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html)
- **Work that runs for the life of the process goes on a dedicated thread**, not `spawn_blocking`: each long task holds a pool thread for its whole life. [spawn_blocking: When to use spawn_blocking vs dedicated threads](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html#when-to-use-spawn_blocking-vs-dedicated-threads), [Spawn a dedicated thread](https://ryhl.io/blog/async-what-is-blocking/#spawn-a-dedicated-thread)
- **`spawn_blocking` can't be aborted once it has started**; `abort()` has no effect, and runtime shutdown waits for it. Pass a `CancellationToken` in and check `is_cancelled()` between steps. [spawn_blocking](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html), [Runtime: Shutdown](https://docs.rs/tokio/1.53.3/tokio/runtime/struct.Runtime.html#shutdown)
- **Don't use `block_in_place`.** It panics on a `current_thread` runtime (every `#[tokio::test]` by default), it suspends everything else in the same task (a `join!` sibling, the other `select!` branches), and its work can't be cancelled either. `spawn_blocking` has none of the first two problems. [block_in_place](https://docs.rs/tokio/1.53.3/tokio/task/fn.block_in_place.html)
- From a plain thread, talk to async code with `mpsc::Sender::blocking_send` and `Receiver::blocking_recv`; both panic if called inside an async context. [spawn_blocking: Related APIs](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html#related-apis-and-patterns-for-bridging-asynchronous-and-blocking-code), [blocking_recv](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/struct.Receiver.html#method.blocking_recv)

## 3. Bounded CPU work in `api`

`api` does the CPU work in section 1 on request paths and in pipeline steps. Each kind of work gets a `Semaphore` in front of `spawn_blocking`, so at most N jobs of it use the CPU at once and the rest wait **in async code** (no blocking thread is taken while waiting): upload pipelines (`UPLOAD_CPU_PERMITS`, 2) and measurement stages (`MEASURE_CPU_PERMITS`, 2) ([backend api 5](../../../docs/backend/api.md#5-configuration); [backend vision 12](../../../docs/backend/vision.md#12-concurrency-and-lifetimes)). The permit moves into the closure, so it is released when the work ends, even by panic, and even if the caller's future is dropped first. [Semaphore::acquire_owned](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Semaphore.html#method.acquire_owned). The `CpuPool` below is that pattern on its own:

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
- **`api`'s ONNX models on CPU** (the face detector and the text embedder, [architecture 10](../../../docs/architecture/README.md#10-models-and-gpu-placement)) sit in a **session pool**, one per model: `Session::run` takes `&mut self` in ort rc.14, so a session serves one caller at a time, and a pool of N sessions lets N calls run at once. A call checks a session out on its blocking thread (inside `spawn_blocking`) and waits on a `Condvar` when all N are busy, which is fine on a blocking thread and only there; the guard returns the session in `Drop`, however the call ends ([backend vision 4](../../../docs/backend/vision.md#4-onnx-runtime-environment-execution-providers-session-pool), the session pool; [12](../../../docs/backend/vision.md#12-concurrency-and-lifetimes)). Unlike a semaphore in async code, a waiting call holds a blocking thread, so N is sized to the callers (the upload permits for the face detector).
- **ONNX Runtime's threads.** By default it gives **each session** an intra-op pool of one thread per physical core, and the calling thread joins in ([ONNX Runtime: Set number of intra-op threads](https://onnxruntime.ai/docs/performance/tune-performance/threading.html#set-number-of-intra-op-threads)), so N sessions times a core-sized pool would oversubscribe the CPU. The design builds **one environment per process with one shared intra-op pool**, spinning off (`OrtRuntime::init`, `GlobalThreadPoolOptions`), sized in `api` to its blocking-work budget, so every session uses the same threads ([backend vision 4](../../../docs/backend/vision.md#4-onnx-runtime-environment-execution-providers-session-pool); [ONNX Runtime: thread spinning](https://onnxruntime.ai/docs/performance/tune-performance/threading.html#thread-spinning-behavior)). Session settings are in [rust-workers](../rust-workers/SKILL.md).

## 4. The perception worker and ONNX Runtime's threads

The perception worker holds `slots = 1` and batches inside the job ([contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)), so inference is one long synchronous call at a time, for the life of the process. That is "runs forever" in the table above: give the GPU sessions **one dedicated OS thread** that owns them, and talk to it over channels ([backend vision 6, the inference thread](../../../docs/backend/vision.md#6-perception)). Async work (heartbeat every 10 s, claim long poll, event posts) stays on the runtime and keeps running during inference, which [contracts 5](../../../docs/architecture/contracts.md#post-internalworkersheartbeat) depends on. Because the sessions never leave their thread, they don't need to be `Send`. **Cancellation is by drop**, as in [rust-workers, ort.md 4](../rust-workers/ort.md#4-the-inference-thread): the job's future is dropped, the request carries no token, and the thread skips a request whose `respond_to` is already closed ([`oneshot::Sender::is_closed`](https://docs.rs/tokio/1.53.3/tokio/sync/oneshot/struct.Sender.html#method.is_closed); [backend worker 7](../../../docs/backend/worker.md#7-cancellation); [backend vision 6](../../../docs/backend/vision.md#6-perception)).

**Decoding and pre-processing don't run on that thread.** The design decodes each image and builds its tensors on the blocking pool, behind the pipeline's own semaphore (`PerceptionPipeline.cpu`), and sends the inference thread **only owned tensors** (`Vec<(String, ArrayD<f32>)>`): the closure gets owned `Bytes` and the `OwnedSemaphorePermit`, so the permit is held until the work ends even if the job is dropped, and the GPU thread never waits on a JPEG decoder ([backend vision 12](../../../docs/backend/vision.md#12-concurrency-and-lifetimes)). A channel's item type must be `Send + 'static` to cross threads, which owned tensors are.

```rust
use std::sync::Arc;
use std::thread;

use tokio::sync::{Semaphore, mpsc, oneshot};

/// Owned input tensors: they cross a channel to another thread, so they borrow nothing from
/// the job. The real type is `Vec<(String, ArrayD<f32>)>` (vision 6).
pub type Inputs = Vec<f32>;

/// What the inference thread is asked to do.
pub struct InferRequest {
    pub inputs: Inputs,
    /// Closed when the job's future was dropped (cancellation is by drop, worker 7).
    pub respond_to: oneshot::Sender<usize>,
}

/// Owns the GPU sessions on one OS thread, which the last clone joins when it drops.
#[derive(Clone)]
pub struct Inference {
    // The sender drops first, so the channel is closed when `Joined` waits.
    requests: mpsc::Sender<InferRequest>,
    _joined: Arc<Joined>,
}

/// `exit` unloads native libraries under a detached thread that is still releasing sessions
/// (exit 139 on the 3070, 2026-10-20), so the thread is joined, not left to the process.
struct Joined(Option<thread::JoinHandle<()>>);

impl Drop for Joined {
    fn drop(&mut self) {
        if let Some(thread) = self.0.take() {
            let _ = thread.join();
        }
    }
}

impl Inference {
    /// `load` runs on the new thread, so the sessions never cross threads.
    pub fn start<M, L>(load: L) -> std::io::Result<Self>
    where
        L: FnOnce() -> M + Send + 'static,
        M: FnMut(&[f32]) -> usize,
    {
        let (requests, mut inbox) = mpsc::channel::<InferRequest>(1);
        let handle = thread::Builder::new()
            .name("inference".to_owned())
            .spawn(move || {
                let mut model = load();
                // blocking_recv: this is a plain thread, not a runtime worker.
                while let Some(req) = inbox.blocking_recv() {
                    // A job dropped while its request waited in the channel: skip the run.
                    // (A run that has started is stopped by rust-workers' `TerminateOnDrop`.)
                    if req.respond_to.is_closed() {
                        continue;
                    }
                    let out = model(&req.inputs);
                    // Err only means the job was dropped while the model ran.
                    let _ = req.respond_to.send(out);
                }
            })?;
        Ok(Self {
            requests,
            _joined: Arc::new(Joined(Some(handle))),
        })
    }

    /// `None` only when the inference thread has stopped. To cancel, drop the future.
    pub async fn run(&self, inputs: Inputs) -> Option<usize> {
        let (respond_to, response) = oneshot::channel();
        let req = InferRequest { inputs, respond_to };
        self.requests.send(req).await.ok()?;
        response.await.ok()
    }
}

/// Decodes and pre-processes one image on the blocking pool, at most `cpu` at once
/// (`PerceptionPipeline.cpu`, vision 12). The permit moves into the closure.
pub async fn prepare(cpu: &Arc<Semaphore>, image: Vec<u8>) -> Option<Inputs> {
    let permit = Arc::clone(cpu).acquire_owned().await.ok()?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit; // held until the work ends, even if the job is dropped
        image.iter().map(|&b| f32::from(b) / 255.0).collect()
    })
    .await
    .ok()
}

#[tokio::test]
async fn tensors_are_prepared_on_the_pool_and_run_on_the_thread() {
    let runs = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = runs.clone();
    let inference = Inference::start(move || {
        move |inputs: &[f32]| {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            thread::sleep(std::time::Duration::from_millis(50)); // stands in for session.run
            inputs.len()
        }
    })
    .expect("thread starts");
    let cpu = Arc::new(Semaphore::new(2));
    let inputs = prepare(&cpu, vec![0, 128, 255]).await.expect("decoded");
    assert!((inputs[2] - 1.0).abs() < f32::EPSILON, "255 maps to 1.0");
    assert_eq!(inference.run(inputs).await, Some(3));

    // A job dropped (here by a zero timeout) before the thread took its request: the thread
    // skips the run. A run already started is stopped by ort.md 4's `TerminateOnDrop`.
    let busy = tokio::spawn({
        let inference = inference.clone();
        async move { inference.run(vec![0.0; 2]).await }
    });
    while runs.load(std::sync::atomic::Ordering::SeqCst) < 2 {
        tokio::task::yield_now().await; // until the thread is running `busy`'s request
    }
    let dropped = tokio::time::timeout(std::time::Duration::ZERO, inference.run(vec![0.0; 9]));
    assert!(
        dropped.await.is_err(),
        "the wait was dropped while the request was queued"
    );
    assert_eq!(busy.await.expect("no panic"), Some(2));
    assert_eq!(inference.run(vec![0.0; 1]).await, Some(1));
    assert_eq!(
        runs.load(std::sync::atomic::Ordering::SeqCst),
        3,
        "the dropped one never ran"
    );
}
```

Threads on the rig's perception worker:

- **ONNX Runtime's own pools.** By default each session has an intra-op pool (one thread per physical core) and, only with `ORT_PARALLEL` execution, an inter-op pool (also one per physical core), and idle pool threads spin waiting for work, which uses CPU. The design replaces the per-session pools with one global pool and turns spinning off (below). [ONNX Runtime threading](https://onnxruntime.ai/docs/performance/tune-performance/threading.html), [inter-op threads](https://onnxruntime.ai/docs/performance/tune-performance/threading.html#set-number-of-inter-op-threads), [spinning](https://onnxruntime.ai/docs/performance/tune-performance/threading.html#thread-spinning-behavior)
- **Tokio's workers.** `#[tokio::main]` starts one per core ([tokio::main](https://docs.rs/tokio/1.53.3/tokio/attr.main.html)). The worker's async side is light (a heartbeat every 10 s, the claim long poll, event posts), so they are mostly idle.
- Run the models of a job one after another on the inference thread, and image decoding and pre-processing on the blocking pool behind the pipeline's semaphore, never on tokio workers ([backend vision 6](../../../docs/backend/vision.md#6-perception), [12](../../../docs/backend/vision.md#12-concurrency-and-lifetimes)). The process has one ONNX Runtime environment with one shared intra-op pool, 2 threads on a GPU worker to start with (a value to measure on day 2), spinning off, so the sessions don't each bring a core-sized pool ([backend vision 4](../../../docs/backend/vision.md#4-onnx-runtime-environment-execution-providers-session-pool); [ONNX Runtime threading](https://onnxruntime.ai/docs/performance/tune-performance/threading.html#set-number-of-intra-op-threads)). Session settings are in [rust-workers](../rust-workers/SKILL.md), which owns `ort` session configuration.
- Job cancellation (`cancel_job_ids` in the heartbeat response, [contracts 5](../../../docs/architecture/contracts.md#post-internalworkersheartbeat)) drops the job's future ([backend worker 7](../../../docs/backend/worker.md#7-cancellation)). A stage that has started can be stopped from another thread: ort 2.0.0-rc.14's `RunOptions::terminate` sets a flag the run checks, and the run ends with an error ([RunOptions::terminate](https://docs.rs/ort/2.0.0-rc.14/ort/session/struct.RunOptions.html#method.terminate)). `Inference::run` arms a `TerminateOnDrop` guard that calls it when the wait for the answer is dropped ([backend vision 6](../../../docs/backend/vision.md#6-perception); [rust-workers, ort.md](../rust-workers/ort.md#4-the-inference-thread)). Without it, a started stage finishes and its result is dropped.
- At process exit the inference thread doesn't hold anything up: when `main` returns the process ends even if other threads are running ([std::thread](https://doc.rust-lang.org/std/thread/index.html#the-threading-model)). Work on `spawn_blocking`, in contrast, is waited for by the runtime's drop ([shutdown.md](shutdown.md#4-what-runtime-drop-waits-for)).
