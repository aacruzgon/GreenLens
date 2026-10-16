---
name: rust-async
description: "Tokio 1.53 and tokio-util 0.7 in the PDM backend: runtime, spawn vs spawn_blocking, select! cancel safety, JoinSet, CancellationToken, TaskTracker shutdown, mpsc/oneshot/broadcast/Notify, mutexes, timeouts, Send bounds, async traits. Use when writing or reviewing async Rust."
---

# Async Rust with tokio (PDM backend)

How `api` and `worker` use tokio 1.53.3 and tokio-util 0.7.20 on Rust 1.99 (edition 2024): the runtime, where blocking work goes, tasks, cancellation, shutdown, channels, locks, time, `Send` bounds and async traits. **The project documents win**: the behaviours this skill implements are fixed by [contracts 4](../../../docs/architecture/contracts.md#4-scan-events-sse) (one serialised writer per scan, `Lagged` switches to replay, keepalive every 15 s), [contracts 5](../../../docs/architecture/contracts.md#5-worker-api-and-jobs) (heartbeat every 10 s, requeue after 30 s, claim long poll up to 30 s), [database 4](../../../docs/database/README.md#job-claim-with-slots-) (`Notify` plus a 1-second fallback poll for the claim), [architecture 2](../../../docs/architecture/README.md#design-principles) (a trait at every external boundary) and [architecture 12](../../../docs/architecture/README.md#operations) (a single `api` instance with an in-process `broadcast` hub). Report any conflict between those documents and tokio's behaviour instead of picking one. Crate versions are in [versions.md](../rust/versions.md).

Reference files, each with the tests that prove it:

| File | Covers |
|---|---|
| [blocking.md](blocking.md) | What blocks, `spawn_blocking` vs a dedicated thread, the CPU pool in `api`, the perception worker next to ONNX Runtime's thread pools |
| [cancellation.md](cancellation.md) | `select!` and cancel safety, `JoinHandle`, `abort`, `JoinSet`, `CancellationToken` |
| [shutdown.md](shutdown.md) | Signals, `TaskTracker`, the shutdown sequence for `api` and `worker` |
| [channels.md](channels.md) | `mpsc`, `oneshot`, the per-scan writer actor, `broadcast` for SSE, `Notify` for the claim long poll |
| [traits.md](traits.md) | `async fn` in traits, `#[async_trait]` on the ports, mocking them, `Send` futures, `dyn`, async closures, what is stable in 1.99 |

## 1. Runtime

- **`api` and `worker` start with `#[tokio::main]`**: the multi-thread runtime, with one worker thread per core unless `worker_threads` or the `TOKIO_WORKER_THREADS` environment variable says otherwise. [tokio::main](https://docs.rs/tokio/1.53.3/tokio/attr.main.html), [Builder::worker_threads](https://docs.rs/tokio/1.53.3/tokio/runtime/struct.Builder.html#method.worker_threads)
- **The body of `main` doesn't run on a worker thread**; spawned tasks do. axum's `serve` already spawns a task per connection (`axum-0.8.9/src/serve/mod.rs`), so `main` only wires things up and waits. [tokio::main: Non-worker async function](https://docs.rs/tokio/1.53.3/tokio/attr.main.html#non-worker-async-function)
- **One runtime per process.** Never build a runtime or call `block_on` (`Runtime::block_on`, `Handle::block_on`) from async code: it panics with "Cannot start a runtime from within a runtime". `#[tokio::main]` on any function other than `main` builds a new runtime on every call. [Runtime::block_on: Panics](https://docs.rs/tokio/1.53.3/tokio/runtime/struct.Runtime.html#method.block_on), [tokio::main](https://docs.rs/tokio/1.53.3/tokio/attr.main.html)
- **`#[tokio::test]` gives each test its own current-thread runtime.** Use `flavor = "multi_thread"` only when a test needs parallel threads. [tokio::test](https://docs.rs/tokio/1.53.3/tokio/attr.test.html)
- Returning from `main` drops the runtime, which waits without limit for started `spawn_blocking` work ([shutdown.md](shutdown.md#4-what-runtime-drop-waits-for)).

```rust
#[tokio::test]
#[should_panic(expected = "Cannot start a runtime from within a runtime")]
async fn block_on_inside_async_panics() {
    tokio::runtime::Handle::current().block_on(async {});
}

#[tokio::test]
#[should_panic(expected = "Cannot start a runtime from within a runtime")]
async fn a_nested_runtime_panics_too() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime builds");
    rt.block_on(async {});
}
```

## 2. Blocking and CPU work

"Async code should never spend a long time without reaching an `.await`"; as a rule of thumb, no more than 10 to 100 µs between them ([Async: What is blocking?](https://ryhl.io/blog/async-what-is-blocking/)). Details and tests in [blocking.md](blocking.md).

- **Bounded blocking work** (the upload pipeline, measurement, CPU embeddings in `api`; the perception worker's decoding and pre-processing): `spawn_blocking`, behind a `Semaphore` per kind of work. The blocking pool alone allows 512 threads ([Builder::max_blocking_threads](https://docs.rs/tokio/1.53.3/tokio/runtime/struct.Builder.html#method.max_blocking_threads)) and queues the rest; `spawn_blocking` returns at once, so there is no backpressure ([spawn_blocking](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html)). [blocking.md 3](blocking.md#3-bounded-cpu-work-in-api); [backend vision 12](../../../docs/backend/vision.md#12-concurrency-and-lifetimes)
- **Work that runs for the life of the process** (the perception worker's GPU inference): a dedicated `std::thread` that owns the sessions, fed owned tensors over a channel. [spawn_blocking vs dedicated threads](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html#when-to-use-spawn_blocking-vs-dedicated-threads), [blocking.md 4](blocking.md#4-the-perception-worker-and-onnx-runtimes-threads)
- **`spawn_blocking` work can't be aborted** once it has started; pass a `CancellationToken` and check `is_cancelled()` between steps. [JoinHandle::abort](https://docs.rs/tokio/1.53.3/tokio/task/struct.JoinHandle.html#method.abort)
- ONNX Runtime brings its own thread pools (by default one intra-op thread per physical core, per session). The design builds one environment per process with **one shared intra-op pool**, spinning off, so several sessions don't each start a core-sized pool; `api`'s CPU models sit in a session pool, one session per concurrent call ([backend vision 4](../../../docs/backend/vision.md#4-onnx-runtime-environment-execution-providers-session-pool); [ONNX Runtime threading](https://onnxruntime.ai/docs/performance/tune-performance/threading.html)); session settings are in [rust-workers](../rust-workers/SKILL.md).

## 3. Tasks

- A spawned task runs whether or not its handle is awaited. **Dropping a `JoinHandle` detaches the task**; it doesn't stop it. [JoinHandle](https://docs.rs/tokio/1.53.3/tokio/task/struct.JoinHandle.html)
- `abort()` stops a task at its next `.await`; awaiting the handle then gives `JoinError::is_cancelled()`. [task: Cancellation](https://docs.rs/tokio/1.53.3/tokio/task/index.html#cancellation)
- A panic inside a task comes back as `Err(e)` with `e.is_panic()`; handle it, don't `unwrap` it. [JoinError](https://docs.rs/tokio/1.53.3/tokio/task/struct.JoinError.html)
- **Fan-out whose results you collect**: `JoinSet` (`join_next` loop; dropping the set aborts its tasks). **Background tasks you only wait for at shutdown**: `TaskTracker`. [JoinSet](https://docs.rs/tokio/1.53.3/tokio/task/struct.JoinSet.html), [TaskTracker](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html)
- No bare `tokio::spawn` for long-lived work: spawn it on the process's `TaskTracker` with a child token, so shutdown can stop it and wait for it (the project's decision, [shutdown.md](shutdown.md)). [TaskTracker](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html), [child_token](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html#method.child_token)

Proofs: [cancellation.md 2](cancellation.md#2-tasks-joinhandle-abort-joinset).

## 4. `select!` and cancellation

- `select!` drops every branch that didn't win. In a loop, use only **cancel-safe** futures: `mpsc`/`broadcast` `recv`, `CancellationToken::cancelled`, `Interval::tick`, `JoinSet::join_next`, `&mut JoinHandle`. Not `read_exact`, `write_all`, `mpsc::Sender::send` (the message is lost), `Mutex::lock`, `Semaphore::acquire` or `Notify::notified` (they lose their queue place). The exception is a `Notified` woken by `notify_waiters`, which has no queue place to lose: the claim long poll creates one per turn, before its query, and `select!`s on it ([channels.md 5](channels.md#5-notify-the-job-claim-long-poll), `claim_long_poll`; [backend api 15](../../../docs/backend/api.md#15-the-job-broker-and-the-worker-api)). [select!: Cancellation safety](https://docs.rs/tokio/1.53.3/tokio/macro.select.html#cancellation-safety)
- A future that must outlive one turn of the loop (a deadline) is created once, pinned, and polled as `&mut fut`. [select! examples](https://docs.rs/tokio/1.53.3/tokio/macro.select.html#examples)
- Use `biased;` with the shutdown branch first wherever "stop before taking more work" matters. [select!: Fairness](https://docs.rs/tokio/1.53.3/tokio/macro.select.html#fairness)
- **`CancellationToken`**: `child_token()` children are cancelled with their parent; cancelling a child leaves the parent alone; **dropping a token, parent or child, cancels nothing**: only `cancel()` or a `DropGuard` does. Use `cancelled_owned()` where a `'static` future is needed. [CancellationToken](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html)

Proofs and the full lists: [cancellation.md](cancellation.md).

## 5. Shutdown

Signal (SIGTERM or Ctrl-C) → `cancel()` the root token → axum's `with_graceful_shutdown` stops accepting and waits for every open connection → `tracker.close()` → wait for the tracked tasks under `SHUTDOWN_DRAIN_S` (30 s) and, side by side, for the delete queue under `max(SHUTDOWN_DRAIN_S, 120 s)` → exit ([backend api 4](../../../docs/backend/api.md#4-start-up-and-shutdown); [shutdown.md 1](shutdown.md#1-the-sequence); [Serve::with_graceful_shutdown](https://docs.rs/axum/0.8.9/axum/serve/struct.Serve.html#method.with_graceful_shutdown), [TaskTracker::wait](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html#method.wait)). The worker's whole shutdown ends within `DRAIN_SECONDS` (20 s): held jobs get that long, then they are cancelled, nothing is sent, and a call in flight is dropped ([backend worker 5](../../../docs/backend/worker.md#5-the-lifecycle-state-machine)). Each drain must fit its compose service's `stop_grace_period` (10 s unless set): at least `DRAIN_SECONDS` + 10 s for each worker service, `30s` on the rig's ([backend worker 4](../../../docs/backend/worker.md#4-configuration), [12](../../../docs/backend/worker.md#12-deployment)), and `190s` for `api`, the slowest request (60 s), one delete attempt (120 s) and 10 s of margin ([architecture 12](../../../docs/architecture/README.md#12-deployment), stop grace periods; [backend api 4](../../../docs/backend/api.md#4-start-up-and-shutdown), [5](../../../docs/backend/api.md#5-configuration); [backend README 12](../../../docs/backend/README.md#12-builds-and-deployment); [shutdown.md 1](shutdown.md#1-the-sequence); [Compose file: stop_grace_period](https://docs.docker.com/reference/compose-file/services/#stop_grace_period)). Three things that make or break it:

- **axum waits for every open response**, so SSE streams and the claim long poll must end when the token is cancelled; an SSE stream that doesn't keeps the server from returning at all (proved in [shutdown.md 5](shutdown.md#5-api)).
- **RustFS deletes are never cut short**: every delete goes through `blob::DeleteQueue`, whose tasks don't watch the token, and shutdown gives them at least one full 120 s attempt ([storage 4](../../../docs/storage/README.md#4-writes-and-deletes); [backend blob 6](../../../docs/backend/blob.md#6-deletes-that-are-never-cancelled)).
- **The worker keeps heartbeating until its drain ends**, or the broker requeues the jobs it is finishing ([shutdown.md 6](shutdown.md#6-worker), [contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)).

## 6. Channels

| Need | Use |
|---|---|
| Commands to a task, with backpressure | `mpsc::channel(n)`; `send().await` waits for room, `try_send` doesn't ([mpsc::channel](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/fn.channel.html)) |
| A reply | `oneshot` inside the command ([oneshot](https://docs.rs/tokio/1.53.3/tokio/sync/oneshot/index.html)) |
| One writer per scan: assign `seq`, store, then broadcast ([contracts 4](../../../docs/architecture/contracts.md#transport)) | An actor task owning the scan's writes, started on the first event and retired after 60 s idle; the broadcast channel and the plan lock live outside it, each with its own sweep rule ([backend api 13](../../../docs/backend/api.md#13-scan-events-writer-hub-and-stream); [Actors with Tokio](https://ryhl.io/blog/actors-with-tokio/), [tokio tutorial: channels](https://tokio.rs/tokio/tutorial/channels)) |
| SSE fan-out | `broadcast`: `subscribe()` before replaying; on `Lagged(n)` switch to replay from the database ([broadcast](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/index.html), [contracts 4](../../../docs/architecture/contracts.md#transport)) |
| Wake waiting claims on a job insert or requeue | The store's `Notify`: every write that queues or requeues a job calls `notify_waiters()` after its commit, and nothing else signals it; create `notified()` before the claim query ([Notify](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Notify.html), [database 4](../../../docs/database/README.md#job-claim-with-slots-); [backend store 21](../../../docs/backend/store.md#21-decisions-on-open-questions)) |

Code and proofs: [channels.md](channels.md). The SSE endpoint itself is in [rust-axum-api](../rust-axum-api/SKILL.md).

## 7. Locks

- **Default to `std::sync::Mutex`** for data, locked briefly with no `.await` while held. tokio's own docs: "it is ok and often preferred to use the ordinary `Mutex` from the standard library in asynchronous code". [tokio Mutex: Which kind of mutex should you use?](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Mutex.html#which-kind-of-mutex-should-you-use)
- **`tokio::sync::Mutex` only when the guard must be held across an `.await`**, typically around an I/O resource. Often an actor is better (channels.md 3). It is slower, and its `lock()` is not cancel safe. [tokio Mutex](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Mutex.html), [Shared state](https://tokio.rs/tokio/tutorial/shared-state#on-using-stdsyncmutex-and-tokiosyncmutex)
- **A `std` guard held across an `.await` makes the future `!Send`**, so `tokio::spawn` rejects it ("future cannot be sent between threads safely", no error code). Where it would compile (a future that isn't spawned), clippy's `await_holding_lock` (group `suspicious`, warn by default) catches it. Drop the guard in a block before the `.await`. [Holding a MutexGuard across an .await](https://tokio.rs/tokio/tutorial/shared-state#holding-a-mutexguard-across-an-await), [await_holding_lock](https://clippy.rust-lang.org/rust-1.99.0/index.html#await_holding_lock)
- **Poisoning**: a `std::sync::Mutex` whose holder panicked returns `Err` from `lock()`; the data is still reachable through `PoisonError::into_inner`. `clippy::unwrap_used` is on, so write `lock().expect("… mutex poisoned")` to propagate the panic, or `unwrap_or_else(PoisonError::into_inner)` when one panicked operation can't leave the data inconsistent (a map insert or remove). `tokio::sync::Mutex` doesn't poison: it unlocks. [std Mutex: Poisoning](https://doc.rust-lang.org/std/sync/struct.Mutex.html#poisoning), [tokio Mutex](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Mutex.html)
- `tokio::sync::RwLock` is fair (write-preferring, FIFO); `std::sync::RwLock`'s priority depends on the OS. [tokio RwLock](https://docs.rs/tokio/1.53.3/tokio/sync/struct.RwLock.html)

```rust,compile_fail
use std::sync::{Arc, Mutex};

async fn flush() {}

fn start(counts: Arc<Mutex<u64>>) {
    tokio::spawn(async move {
        let mut n = counts.lock().expect("counts mutex poisoned");
        *n += 1;
        flush().await; // the guard is still alive here
    });
}
```

```rust
use std::sync::{Arc, Mutex, PoisonError};

async fn flush() {
    tokio::task::yield_now().await;
}

fn start(counts: Arc<Mutex<u64>>) {
    tokio::spawn(async move {
        {
            let mut n = counts.lock().expect("counts mutex poisoned");
            *n += 1;
        } // guard dropped before the await
        flush().await;
    });
}

#[test]
fn a_panic_while_locked_poisons_a_std_mutex() {
    let counts = Arc::new(Mutex::new(0_u32));
    let c = counts.clone();
    let _ = std::thread::spawn(move || {
        let _guard = c.lock().expect("not poisoned yet");
        panic!("panic while holding the lock");
    })
    .join();
    assert!(counts.is_poisoned());
    assert!(counts.lock().is_err());
    let n = counts.lock().unwrap_or_else(PoisonError::into_inner);
    assert_eq!(*n, 0);
}

#[tokio::test]
async fn a_tokio_mutex_is_not_poisoned() {
    let m = Arc::new(tokio::sync::Mutex::new(0_u32));
    let m2 = m.clone();
    let res = tokio::spawn(async move {
        let _guard = m2.lock().await;
        panic!("panic while holding the lock");
    })
    .await;
    assert!(res.expect_err("task panicked").is_panic());
    assert_eq!(*m.lock().await, 0); // unlocked, no error
}
```

## 8. Time

- **`tokio::time::timeout(d, fut)`** returns `Err(Elapsed)` and **drops `fut`** when `d` passes. The future is polled before the deadline is checked, so one that never yields can overrun. Put one on every outbound call made from a loop (a heartbeat must not hang past its 10-second period). [timeout](https://docs.rs/tokio/1.53.3/tokio/time/fn.timeout.html)
- **`tokio::time::sleep`**, never `std::thread::sleep`, in async code: `sleep` is "an asynchronous analog to `std::thread::sleep`" that yields to other tasks, while the `std` one holds the worker thread, and tokio can only switch tasks at an `.await`. [tokio::time::sleep](https://docs.rs/tokio/1.53.3/tokio/time/fn.sleep.html), [tokio: CPU-bound tasks and blocking code](https://docs.rs/tokio/1.53.3/tokio/index.html#cpu-bound-tasks-and-blocking-code)
- **`interval(period)`**: the first tick completes at once; the default `MissedTickBehavior::Burst` fires missed ticks back to back. After a stall (a slow heartbeat call, a sweep that overran its period) `Burst` would fire the missed ticks at once: two or three heartbeats or sweeps back to back, adding load exactly when the API or the database is already slow. **`Delay`** fires one late tick and then waits a full period from it, so two runs are never closer than the period; `Skip` fires one late tick but keeps the original grid, so the next one can follow sooner (the `burst_catches_up_and_delay_does_not` test). The heartbeat and the scheduler's tasks use `Delay` ([backend api 16](../../../docs/backend/api.md#16-the-scheduler): "a run that overran doesn't cause a burst of catch-up runs"; [backend worker 6](../../../docs/backend/worker.md#6-leases-heartbeats-and-completion)). `interval` keeps its period even when the loop body takes time, unlike `sleep` in a loop. [interval](https://docs.rs/tokio/1.53.3/tokio/time/fn.interval.html), [MissedTickBehavior](https://docs.rs/tokio/1.53.3/tokio/time/enum.MissedTickBehavior.html)
- **Use `tokio::time::Instant`** in code that tests run with paused time; pausing affects only tokio's `Instant`, not `std::time::Instant`. [pause](https://docs.rs/tokio/1.53.3/tokio/time/fn.pause.html)
- **Tests**: `#[tokio::test(start_paused = true)]` (needs the `test-util` feature, which the dev-dependency has; current-thread runtime only). With time paused, an idle runtime jumps to the next timer, so a 10-minute timeout runs in microseconds. Running blocking tasks hold the clock still; real sockets don't, so keep real network I/O out of paused tests. [tokio::test: start_paused](https://docs.rs/tokio/1.53.3/tokio/attr.test.html#configure-the-runtime-to-start-with-time-paused), [pause: Auto-advance](https://docs.rs/tokio/1.53.3/tokio/time/fn.pause.html#auto-advance), [rust-testing](../rust-testing/SKILL.md)

```rust
use std::time::Duration;

use tokio::sync::oneshot;
use tokio::time::{self, Instant, MissedTickBehavior, timeout};

#[tokio::test(start_paused = true)]
async fn timeout_drops_the_inner_future() {
    let (tx, rx) = oneshot::channel::<()>();
    let slow = async move {
        let _tx = tx; // dropped with the future
        time::sleep(Duration::from_secs(60)).await;
    };
    let start = Instant::now();
    assert!(timeout(Duration::from_secs(5), slow).await.is_err());
    assert_eq!(start.elapsed(), Duration::from_secs(5)); // auto-advanced, no real wait
    assert!(rx.await.is_err(), "the sender went with the future");
}

#[tokio::test(start_paused = true)]
async fn the_first_tick_is_immediate() {
    let start = Instant::now();
    let mut beat = time::interval(Duration::from_secs(10));
    beat.tick().await;
    assert_eq!(start.elapsed(), Duration::ZERO);
    beat.tick().await;
    assert_eq!(start.elapsed(), Duration::from_secs(10));
}

#[tokio::test(start_paused = true)]
async fn burst_catches_up_and_delay_does_not() {
    // A 25 s stall (a slow heartbeat request) misses two 10 s ticks.
    let start = Instant::now();
    let mut burst = time::interval(Duration::from_secs(10));
    burst.tick().await;
    time::sleep(Duration::from_secs(25)).await;
    burst.tick().await; // due at 10 s: fires at once (25 s)
    burst.tick().await; // due at 20 s: fires at once (25 s)
    assert_eq!(start.elapsed(), Duration::from_secs(25));

    let start = Instant::now();
    let mut delay = time::interval(Duration::from_secs(10));
    delay.set_missed_tick_behavior(MissedTickBehavior::Delay);
    delay.tick().await;
    time::sleep(Duration::from_secs(25)).await;
    delay.tick().await; // late tick fires at once (25 s)
    delay.tick().await; // the next one is a full period later (35 s)
    assert_eq!(start.elapsed(), Duration::from_secs(35));
}
```

## 9. `Send` and `'static`

`tokio::spawn` needs `F: Future + Send + 'static` and `F::Output: Send + 'static` ([tokio::spawn](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn.html)). `'static`: the task owns its data (`async move`, `Arc` for shared state). `Send`: the runtime may move the task to another thread at any `.await`, so everything alive across an `.await` must be `Send`. [Spawning: 'static bound](https://tokio.rs/tokio/tutorial/spawning#static-bound), [Send bound](https://tokio.rs/tokio/tutorial/spawning#send-bound)

Common causes of a `!Send` future, each fixed by dropping the value before the `.await` or by using the `Send` type:

| Alive across an `.await` | Use instead |
|---|---|
| `Rc<T>`, a `RefCell` borrow | `Arc<T>`; a `std::sync::Mutex` locked in a block |
| A `std::sync::MutexGuard` | Drop it in a block before the `.await` (section 7) |
| `Box<dyn Error>` without `+ Send + Sync` | `Box<dyn Error + Send + Sync>`, `anyhow::Error` (which requires `Send + Sync + 'static`, [anyhow::Error](https://docs.rs/anyhow/1.0.104/anyhow/struct.Error.html)), or a thiserror enum of `Send` fields |
| The future of a trait's `async fn` | A port: `#[async_trait]`, whose futures are `Send`. A crate-internal trait: `-> impl Future<Output = T> + Send` ([traits.md 3](traits.md#3-what-the-project-does-with-its-boundary-traits)) |

```rust,compile_fail
use std::error::Error;

async fn fetch_weather() -> Result<u32, Box<dyn Error>> {
    Ok(20)
}

async fn report(_msg: String) {}

fn start() {
    tokio::spawn(async {
        if let Err(e) = fetch_weather().await {
            report(e.to_string()).await; // `e` is still alive here
        }
    });
}
```

## 10. Async traits

The ports in `domain::ports` are held as `Arc<dyn Port>`, so their async methods use **`#[async_trait]`** (`async-trait` 0.1.92), on the trait and on every implementation ([backend domain 10](../../../docs/backend/domain.md#dyn-dispatch-and-async-methods); [async-trait](https://docs.rs/async-trait/0.1.92/async_trait/)):

- **Native `async fn` in a trait isn't dyn compatible.** Neither is `-> impl Future`: `Arc<dyn Port>` fails with `E0038` ([Reference, dyn compatibility](https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility)). `#[async_trait]` returns `Pin<Box<dyn Future + Send + 'async_trait>>`, a nameable type, so the trait stays dyn compatible and every future is `Send`.
- **Not a hand-written `BoxFuture<'a, T>`.** `mockall` 0.15 can't mock a method whose boxed future borrows a named lifetime (`E0521`), and an `#[async_trait]` trait mocks cleanly with `#[automock]` above `#[async_trait]`.
- **A streaming port method is a plain method** in the same trait: `ChatModel::stream<'a>(&'a self, request: &'a ChatRequest) -> BoxStream<'a, …>`. mockall can't mock it, so tests use a hand-written `ScriptedChat` ([backend domain 11](../../../docs/backend/domain.md#11-testing)).
- **Synchronous ports** (`Clock`, `IdGen`, `UrlSigner`, `SpeciesPhotos`, `SpotSegmenter`, `WebhookVerifier`) stay plain methods ([backend domain 9](../../../docs/backend/domain.md#9-boundary-traits)).
- **Crate-internal traits** keep `-> impl Future + Send` for static dispatch (the agent's typed `Tool`) and `BoxFuture<'a, T>` where they are erased for a registry (`DynTool`) ([backend agent 12](../../../docs/backend/agent.md#typed-tools-erased-once)).

Return type notation is unstable in 1.99, and an `AsyncFn*` bound can't promise a `Send` future, which breaks a spawned helper whose closure borrows from its caller ([traits.md 1](traits.md#1-what-is-stable-in-rust-199), [4](traits.md#4-async-closures)). Proofs, with the `E0038` and `E0521` blocks: [traits.md 3](traits.md#3-what-the-project-does-with-its-boundary-traits).

## Review checklist

- [ ] No blocking call on a runtime thread: `std::thread::sleep`, `std::fs`, image decode, `ort` `run`, long CPU loops. Bounded work goes on `spawn_blocking` behind a semaphore; process-long work (GPU inference) goes on a dedicated thread.
- [ ] No `block_on`, nested runtime or `block_in_place` in async code.
- [ ] Every `select!` in a loop has only cancel-safe branches; long-lived futures are pinned outside the loop; shutdown branches come first under `biased;` (section 4).
- [ ] Every spawned task has an owner: a `JoinSet` that is drained, or the `TaskTracker` with a child token. Each `JoinHandle` drop is deliberate. `JoinError::is_panic` is handled.
- [ ] Everything long-lived ends on the shutdown token: SSE streams, the claim long poll, the scheduler, the writer actors (which also retire after 60 s idle), the worker's claim loop. The worker's heartbeat stops only after its drain. RustFS deletes go through the delete queue and get at least 120 s at shutdown. Each drain fits its compose service's `stop_grace_period` (section 5).
- [ ] Channels are bounded. Every ignored send error has a comment saying why it is harmless ([channels.md 3](channels.md#3-oneshot-and-the-actor-pattern-the-per-scan-event-writer)).
- [ ] `broadcast` `Lagged` switches to replay from the database; a `send` with no subscribers is ignored.
- [ ] `Notify`: `notified()` is created before the condition check; only the store signals it, after every write that queues or requeues a job (section 6).
- [ ] No `std` guard across an `.await` (`clippy::await_holding_lock`); a `tokio::sync::Mutex` only where a guard must cross an `.await` (section 7).
- [ ] Every outbound call in a loop has a `timeout`; every `interval` sets its `MissedTickBehavior`.
- [ ] `domain` ports are `Send + Sync` and use `#[async_trait]` for async methods; streaming methods return `BoxStream<'a, …>`; crate-internal traits use `impl Future + Send` or `BoxFuture` (section 10).
- [ ] Time-dependent tests use `start_paused = true`, and their behaviour claims are asserted, not assumed (section 8).

## Related skills

- [rust](../rust/SKILL.md): entry point, versions, the example checker.
- [rust-axum-api](../rust-axum-api/SKILL.md): routes, the SSE endpoint and its resume logic, `ApiError`.
- [rust-workers](../rust-workers/SKILL.md): claim, lease, heartbeat and job calls, `ort` session settings.
- [rust-surrealdb](../rust-surrealdb/SKILL.md): the store calls behind the scan writer and the job claim, conflict retries.
- [rust-errors](../rust-errors/SKILL.md): error types, what to do with a `JoinError`.
- [rust-tracing](../rust-tracing/SKILL.md): spans across tasks, request IDs.
- [rust-testing](../rust-testing/SKILL.md): test layout, mocks for the boundary traits, nextest.
- [rust-contract-types](../rust-contract-types/SKILL.md), [rust-cargo-lints](../rust-cargo-lints/SKILL.md).
