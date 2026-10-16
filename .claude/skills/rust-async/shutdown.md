# Graceful shutdown for `api` and `worker`

Part of [rust-async](SKILL.md). Versions: tokio 1.53.3, tokio-util 0.7.20 (feature `rt` for `TaskTracker`), axum 0.8.9. The project-wide decision: **one `CancellationToken` for the process, one `TaskTracker` for background tasks, and axum's `with_graceful_shutdown`**. Token semantics are in [cancellation.md](cancellation.md#3-cancellationtoken).

## Contents

1. [The sequence](#1-the-sequence)
2. [Signals](#2-signals)
3. [`TaskTracker`](#3-tasktracker)
4. [What runtime drop waits for](#4-what-runtime-drop-waits-for)
5. [`api`](#5-api)
6. [`worker`](#6-worker)

## 1. The sequence

1. **Signal**: SIGTERM (the stop signal in the project's Docker deployment) or Ctrl-C (a developer's terminal).
2. **Cancel the root token.** Every subsystem holds a `child_token()`, so one `cancel()` reaches all of them.
3. **Stop taking new work and finish what is in flight**: `api` stops accepting connections and lets requests finish; `worker` stops claiming and finishes its current job.
4. **`tracker.close()`, then wait** for the server future and `tracker.wait()` **under one timeout**. When it elapses, log and exit; whatever was left is recovered by design: the broker requeues a job whose worker stops heartbeating for 30 seconds ([contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)), and the scheduler's sweep recovers `api`'s own steps after a restart ([architecture 8.3](../../../docs/architecture/README.md#83-forecast-alerts), [contracts 3](../../../docs/architecture/contracts.md#scan-lifecycle)).

**Budgets.** The containers run under Docker Compose ([architecture 12](../../../docs/architecture/README.md#12-deployment): `infra/control-plane.compose.yml` and `infra/rig.compose.yml`), which sends SIGKILL `stop_grace_period` after SIGTERM, 10 s unless the service sets it ([Compose file: stop_grace_period](https://docs.docker.com/reference/compose-file/services/#stop_grace_period)). Architecture 12 doesn't set it and `infra/` doesn't exist yet, so each service sets it explicitly:

| Binary | Drain budget | Compose `stop_grace_period` |
|---|---|---|
| `api` | 8 s (`DRAIN` in section 5) | `10s` |
| `worker`, every `--caps` | 20 s for running jobs, plus 5 s for a cancelled runner to stop ([rust-workers, loop](../rust-workers/loop.md)) | `30s` |

*Design choice for this repo*: each budget leaves at least 2 s before SIGKILL for logs to flush. The worker gets longer because a job it finishes doesn't have to be redone after a 30 s requeue ([contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)).

## 2. Signals

- Listen for both: `tokio::signal::ctrl_c()` and, on Unix, `tokio::signal::unix::signal(SignalKind::terminate())`. The function below is the one in axum's [graceful-shutdown example](https://github.com/tokio-rs/axum/tree/axum-v0.8.9/examples/graceful-shutdown), with the `expect`s replaced by a logged error. [ctrl_c](https://docs.rs/tokio/1.53.3/tokio/signal/fn.ctrl_c.html), [unix::signal](https://docs.rs/tokio/1.53.3/tokio/signal/unix/fn.signal.html)
- **Once tokio listens for a signal, that signal never again has its default effect** for the life of the process, even after the listener is dropped. So a second Ctrl-C does not kill a draining process by itself. Listen again during the drain and exit early on a second signal (section 5). [ctrl_c: Caveats](https://docs.rs/tokio/1.53.3/tokio/signal/fn.ctrl_c.html#caveats)
- `ctrl_c()` only sees signals that arrive after its future is first polled, and a `Signal` stream may coalesce several signals into one. [ctrl_c](https://docs.rs/tokio/1.53.3/tokio/signal/fn.ctrl_c.html), [unix::signal](https://docs.rs/tokio/1.53.3/tokio/signal/unix/fn.signal.html)

## 3. `TaskTracker`

- `wait()` completes only when the tracker is **both closed and empty**. An open tracker never finishes waiting, even with no tasks, so call `close()` before `wait()`. `close()` doesn't stop you spawning more tasks. [TaskTracker](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html), [close](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html#method.close), [wait](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html#method.wait)
- It is cheap to clone and spawning takes `&self`, so it can live in `AppState`. A finished task is freed at once (unlike `JoinSet`), and dropping a tracker aborts nothing. [Comparison to JoinSet](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html#comparison-to-joinset)
- `spawn_blocking` work can be tracked too (`tracker.spawn_blocking`), but it can't be cancelled, so it counts against the budget until it ends. [spawn_blocking](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html#method.spawn_blocking)
- **Every tracked task needs an end condition tied to shutdown**: a `select!` on its token, or the drop of every handle that keeps it alive. The per-scan writer in [channels.md](channels.md#3-oneshot-and-the-actor-pattern-the-per-scan-event-writer) ends when the last `ScanWriter` is dropped, which happens once the server and the other tasks have let go of `AppState`. A task with neither uses up the whole budget:

```rust
use std::time::Duration;

use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

#[tokio::test(start_paused = true)]
async fn wait_returns_only_when_closed_and_empty() {
    let tracker = TaskTracker::new();
    // Empty but still open: wait doesn't return.
    assert!(
        timeout(Duration::from_secs(1), tracker.wait())
            .await
            .is_err()
    );
    tracker.spawn(sleep(Duration::from_secs(2)));
    tracker.close(); // closing doesn't stop spawning; it lets wait finish
    timeout(Duration::from_secs(3), tracker.wait())
        .await
        .expect("closed and empty after 2 s");
}

#[tokio::test(start_paused = true)]
async fn a_task_that_ignores_the_token_uses_up_the_budget() {
    let shutdown = CancellationToken::new();
    let tracker = TaskTracker::new();
    let stop = shutdown.child_token();
    tracker.spawn(async move { stop.cancelled().await }); // leaves on cancel
    tracker.spawn(sleep(Duration::from_secs(3600))); // never looks at the token
    shutdown.cancel();
    tracker.close();
    let budget = Duration::from_secs(8); // api's drain budget (section 1)
    assert!(timeout(budget, tracker.wait()).await.is_err());
    assert_eq!(
        tracker.len(),
        1,
        "only the task that ignored the token is left"
    );
}
```

## 4. What runtime drop waits for

Returning from `#[tokio::main]` drops the runtime. Async tasks still alive are dropped at their next `.await`. **Started `spawn_blocking` work is waited for, with no limit**: "The `Drop` implementation waits forever for this." [Runtime: Shutdown](https://docs.rs/tokio/1.53.3/tokio/runtime/struct.Runtime.html#shutdown), [task: Cancellation](https://docs.rs/tokio/1.53.3/tokio/task/index.html#cancellation)

- `api`'s blocking work (the CPU pool in [blocking.md](blocking.md#3-bounded-cpu-work-in-api)) is bounded per image, so `#[tokio::main]` is fine.
- A binary whose blocking work could hang builds the runtime with `tokio::runtime::Builder` and ends with `runtime.shutdown_timeout(…)`, which stops waiting and leaks the stuck threads. [shutdown_timeout](https://docs.rs/tokio/1.53.3/tokio/runtime/struct.Runtime.html#method.shutdown_timeout)
- A plain `std::thread` (the perception worker's inference thread) does not hold up exit: when `main` returns, the process ends. [std::thread: The threading model](https://doc.rust-lang.org/std/thread/index.html#the-threading-model)

## 5. `api`

What has to end for the server to drain:

- **axum's graceful shutdown stops accepting, then waits for every open connection's response to finish.** axum's own example says so ("Graceful shutdown will wait for outstanding requests to complete") and adds a request timeout. [graceful-shutdown example](https://github.com/tokio-rs/axum/tree/axum-v0.8.9/examples/graceful-shutdown), [Serve::with_graceful_shutdown](https://docs.rs/axum/0.8.9/axum/serve/struct.Serve.html#method.with_graceful_shutdown)
- **An SSE response never finishes by itself** (contracts 4 keeps it open until a resting point), so every SSE stream must also end when the token is cancelled. The app reconnects with `Last-Event-ID` and loses nothing ([contracts 4](../../../docs/architecture/contracts.md#transport)). How the stream watches the token is in [rust-axum-api](../rust-axum-api/SKILL.md). The test below proves both halves.
- **The claim long poll** (up to 30 s, [contracts 5](../../../docs/architecture/contracts.md#post-internaljobsclaim)) returns `204` on cancel ([channels.md](channels.md#5-notify-the-job-claim-long-poll)).
- **Background loops** (scheduler, forecast alerts) `select!` on their child token with `biased;` and the token first, so they leave between iterations rather than in the middle of one. [select!: Fairness](https://docs.rs/tokio/1.53.3/tokio/macro.select.html#fairness), [architecture 8.3](../../../docs/architecture/README.md#83-forecast-alerts)
- The signal future passed to `with_graceful_shutdown` must be `Send + 'static`: `token.clone().cancelled_owned()` is. [Serve::with_graceful_shutdown](https://docs.rs/axum/0.8.9/axum/serve/struct.Serve.html#method.with_graceful_shutdown)

```rust
use std::time::Duration;

use axum::Router;
use axum::routing::get;
use tokio::net::TcpListener;
use tokio::time::{self, MissedTickBehavior};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// How long `api` waits, after a signal, for requests, streams and tracked tasks:
/// under the `stop_grace_period: 10s` of its compose service (section 1).
const DRAIN: Duration = Duration::from_secs(8);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let shutdown = CancellationToken::new();
    let tasks = TaskTracker::new();

    tasks.spawn(scheduler(shutdown.child_token()));

    let app = Router::new().route("/healthz", get(|| async { "ok" }));
    let listener = TcpListener::bind("0.0.0.0:8080").await?;
    let server = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown.clone().cancelled_owned())
        .into_future();
    let mut server = std::pin::pin!(server);

    // 1. Serve until a signal (or until the server fails on its own).
    tokio::select! {
        result = server.as_mut() => return result.map_err(Into::into),
        () = shutdown_signal() => tracing::info!("shutdown signal received"),
    }

    // 2. One cancel reaches everything: the server stops accepting and starts
    //    draining, SSE streams and claim long polls end, loops leave at their next await.
    shutdown.cancel();
    tasks.close();

    // 3. Wait for in-flight requests, then tracked tasks, within the budget.
    let drain = async {
        let result = server.await;
        tasks.wait().await;
        result
    };
    tokio::select! {
        drained = time::timeout(DRAIN, drain) => {
            if let Ok(result) = drained {
                result?;
            } else {
                tracing::warn!(drain_s = DRAIN.as_secs(), "drain timed out, exiting");
            }
        }
        () = shutdown_signal() => tracing::warn!("second signal, exiting without draining"),
    }
    Ok(())
}

/// Completes on Ctrl-C (SIGINT) or SIGTERM.
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %e, "cannot listen for Ctrl-C");
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut sigterm) => {
                sigterm.recv().await;
            }
            Err(e) => {
                tracing::error!(error = %e, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}

/// A background loop (architecture §8.3): one sweep at a time, stop between sweeps.
async fn scheduler(stop: CancellationToken) {
    let mut every = time::interval(Duration::from_secs(60));
    every.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            () = stop.cancelled() => break,
            _ = every.tick() => sweep().await,
        }
    }
}

async fn sweep() {
    tokio::task::yield_now().await; // stands in for the sweep's queries
}
```

The 60-second scheduler interval is this skill's placeholder: [architecture 8.3](../../../docs/architecture/README.md#83-forecast-alerts) says "at a fixed interval" without a value. The real `main` builds `AppState`, the router and the tracing setup from the sibling skills.

Proof that an open stream holds up the drain, and that ending it on the token releases it (real sockets on 127.0.0.1, no external service):

```rust
use std::convert::Infallible;
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::routing::get;
use futures::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;

/// A response body like an SSE stream: one chunk, then it stays open.
fn endless_body(stop: Option<CancellationToken>) -> Body {
    let first =
        futures::stream::once(async { Ok::<_, Infallible>(Bytes::from_static(b": hi\n\n")) });
    let rest = futures::stream::pending::<Result<Bytes, Infallible>>();
    match stop {
        None => Body::from_stream(first.chain(rest)),
        Some(token) => Body::from_stream(first.chain(rest).take_until(token.cancelled_owned())),
    }
}

/// Starts a server with one open streaming response, cancels the token, and reports
/// whether the server future finished within 500 ms.
async fn drains_after_cancel(stream_watches_token: bool) -> bool {
    let shutdown = CancellationToken::new();
    let for_body = stream_watches_token.then(|| shutdown.clone());
    let app = Router::new().route(
        "/events",
        get(move || async move { endless_body(for_body.clone()) }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = tokio::spawn(
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown.clone().cancelled_owned())
            .into_future(),
    );
    let mut client = TcpStream::connect(addr).await.expect("connect");
    client
        .write_all(b"GET /events HTTP/1.1\r\nhost: x\r\n\r\n")
        .await
        .expect("write");
    let mut buf = [0u8; 256];
    let n = client.read(&mut buf).await.expect("read");
    assert!(buf[..n].starts_with(b"HTTP/1.1 200"));
    shutdown.cancel();
    tokio::time::timeout(Duration::from_millis(500), server)
        .await
        .is_ok()
}

#[tokio::test]
async fn graceful_shutdown_waits_for_an_open_stream() {
    assert!(!drains_after_cancel(false).await);
}

#[tokio::test]
async fn a_stream_that_ends_on_the_token_lets_the_server_drain() {
    assert!(drains_after_cancel(true).await);
}
```

## 6. `worker`

The claim, heartbeat and job calls themselves belong to [rust-workers](../rust-workers/SKILL.md). The shutdown rules:

- **Stop claiming at once.** The claim loop `select!`s on the token with `biased;` first, so a cancelled worker never starts another long poll; a claim already waiting is dropped, which abandons its request. [select!: Fairness](https://docs.rs/tokio/1.53.3/tokio/macro.select.html#fairness), [select!: Cancellation safety](https://docs.rs/tokio/1.53.3/tokio/macro.select.html#cancellation-safety)
- **Finish the job in hand** if it fits the worker's 20 s budget (section 1). The heartbeat must keep running meanwhile, or the broker requeues the job after 30 seconds and the result is refused with `409 lease_lost` ([contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)). So the heartbeat's token is **not** a child of the shutdown token; it is cancelled after the last job ends.
- **Out of budget**: exit without completing. The heartbeat stops, the broker requeues the job ([contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)), and another worker or the fallback worker takes it.

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use tokio::time::{self, Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// Stand-ins for the worker API calls (rust-workers owns the real ones).
#[derive(Default)]
struct FakeApi {
    claims: AtomicU32,
    finished: AtomicU32,
    beats: AtomicU32,
}

impl FakeApi {
    async fn claim(&self) -> Option<u32> {
        time::sleep(Duration::from_millis(100)).await; // a job is waiting
        Some(self.claims.fetch_add(1, Ordering::SeqCst))
    }
    async fn run_job(&self, _job: u32) {
        time::sleep(Duration::from_secs(5)).await; // inference
        self.finished.fetch_add(1, Ordering::SeqCst);
    }
    async fn heartbeat(&self) {
        time::sleep(Duration::from_millis(50)).await; // the HTTP call
        self.beats.fetch_add(1, Ordering::SeqCst);
    }
}

async fn heartbeat_loop(api: Arc<FakeApi>, stop: CancellationToken) {
    let mut beat = time::interval(Duration::from_secs(10));
    beat.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            () = stop.cancelled() => break,
            _ = beat.tick() => api.heartbeat().await,
        }
    }
}

/// Stops claiming when `shutdown` fires, finishes the job in hand, then stops the heartbeat.
async fn run_worker(api: Arc<FakeApi>, shutdown: CancellationToken) {
    // Not a child of `shutdown`: heartbeats must go on while the last job finishes,
    // or the broker requeues it (contracts §5).
    let heartbeat_stop = CancellationToken::new();
    let tasks = TaskTracker::new();
    tasks.spawn(heartbeat_loop(api.clone(), heartbeat_stop.clone()));
    loop {
        let job = tokio::select! {
            biased;
            () = shutdown.cancelled() => break,
            job = api.claim() => job,
        };
        if let Some(job) = job {
            api.run_job(job).await;
        }
    }
    heartbeat_stop.cancel();
    tasks.close();
    tasks.wait().await;
}

#[tokio::test(start_paused = true)]
async fn the_job_in_hand_finishes_and_no_new_one_is_claimed() {
    let api = Arc::new(FakeApi::default());
    let shutdown = CancellationToken::new();
    let worker = tokio::spawn(run_worker(api.clone(), shutdown.clone()));
    time::sleep(Duration::from_secs(2)).await; // mid-way through job 0
    let stop_at = Instant::now();
    shutdown.cancel();
    time::timeout(Duration::from_secs(20), worker) // the worker's drain budget (section 1)
        .await
        .expect("drained within the budget")
        .expect("no panic");
    assert_eq!(api.claims.load(Ordering::SeqCst), 1);
    assert_eq!(api.finished.load(Ordering::SeqCst), 1);
    assert!(api.beats.load(Ordering::SeqCst) >= 1);
    assert_eq!(stop_at.elapsed(), Duration::from_millis(3100)); // the rest of job 0
}
```

**Not in the contracts yet:** a claim dropped at shutdown may already have leased a job on the server. Nothing lists that job in a heartbeat, so it isn't extended: the broker requeues it once the worker's heartbeat is older than 30 seconds, or when its 2-minute lease expires, whichever comes first ([contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)); there is no call to hand a lease back ([contracts 12, question 20](../../../docs/architecture/contracts.md#12-open-questions)).
