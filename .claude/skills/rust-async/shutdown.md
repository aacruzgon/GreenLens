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
2. **Cancel the root token.** Every subsystem watches it (or a `child_token()`), so one `cancel()` reaches all of them.
3. **Stop taking new work and finish what is in flight**: `api` stops accepting connections and waits for every open one; `worker` stops claiming and finishes the jobs it holds.
4. **Wait, each part under its own deadline, then exit** ([backend api 4](../../../docs/backend/api.md#4-start-up-and-shutdown); [backend worker 5](../../../docs/backend/worker.md#5-the-lifecycle-state-machine)):
   - `api`: once the server has returned, `tracker.close()` and `tracker.wait()` under `SHUTDOWN_DRAIN_S` (30 s), and **at the same time** the delete queue's `drain()` under `max(SHUTDOWN_DRAIN_S, 120 s)`. Pending RustFS deletes always get at least one full attempt, the client's 120-second operation timeout, because an interrupted delete can leak objects on RustFS 1.0.1 ([storage 4](../../../docs/storage/README.md#4-writes-and-deletes); [backend blob 6](../../../docs/backend/blob.md#6-deletes-that-are-never-cancelled)).
   - `worker`: wait for the held jobs under `DRAIN_SECONDS` (20 s), then cancel them and send nothing.

   When a deadline passes, log and exit; whatever was left is recovered by design: the broker requeues a job whose worker stops heartbeating for 30 seconds or whose lease expires ([contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)), the scheduler's sweep recovers `api`'s own steps ([backend api 16](../../../docs/backend/api.md#16-the-scheduler); [contracts 3](../../../docs/architecture/contracts.md#scan-lifecycle)), and deletion cascades resume on the next start ([backend api 17](../../../docs/backend/api.md#17-deletion)).

**Budgets.** The containers run under Docker Compose ([architecture 12](../../../docs/architecture/README.md#12-deployment): `infra/control-plane.compose.yml` and `infra/rig.compose.yml`), which sends SIGKILL `stop_grace_period` after SIGTERM, 10 s unless the service sets it ([Compose file: stop_grace_period](https://docs.docker.com/reference/compose-file/services/#stop_grace_period)):

| Binary | Drain | Compose `stop_grace_period` |
|---|---|---|
| `api` | The server first, until its last connection closes: the SSE streams and the claim long poll end on the token, and every other request gets its response head within its group's timeout, 60 s at most (`SLOW_TIMEOUT`, uploads and worker routes; [backend api 7](../../../docs/backend/api.md#7-the-router-and-the-layer-stack)). Then tasks 30 s (`SHUTDOWN_DRAIN_S`, [backend api 5](../../../docs/backend/api.md#5-configuration)) and deletes up to 120 s, side by side | `190s`: the slowest in-flight request (60 s), one full delete attempt (120 s) and 10 s of margin; a `SHUTDOWN_DRAIN_S` above 120 s raises it by the difference ([architecture 12](../../../docs/architecture/README.md#12-deployment), stop grace periods; [backend api 4](../../../docs/backend/api.md#4-start-up-and-shutdown), the container's grace period; [api 5](../../../docs/backend/api.md#5-configuration); [backend README 12](../../../docs/backend/README.md#12-builds-and-deployment)). Under Compose's 10 s default, SIGKILL would cut off the delete drain the storage notes require |
| `worker` | 20 s (`DRAIN_SECONDS`), the bound of the **whole** shutdown: jobs still running at it are cancelled and abandoned, and a `complete` or heartbeat call in flight is dropped ([backend worker 4](../../../docs/backend/worker.md#4-configuration), [5](../../../docs/backend/worker.md#5-the-lifecycle-state-machine), shutdown time) | At least `DRAIN_SECONDS` + 10 s: `30s` on the rig's `x-worker` anchor ([backend worker 12](../../../docs/backend/worker.md#12-deployment)); every worker service, the control-plane fallback included, sets its own from its drain ([architecture 12](../../../docs/architecture/README.md#12-deployment), stop grace periods; [worker 4](../../../docs/backend/worker.md#4-configuration)) |

**Streamed bodies end on the token too.** `TimeoutLayer` bounds only the time to the response **head** ([backend api 7](../../../docs/backend/api.md#7-the-router-and-the-layer-stack): "✓ an SSE body kept flowing past a 1 s layer"; [`TimeoutLayer`](https://docs.rs/tower-http/0.7.1/tower_http/timeout/struct.TimeoutLayer.html)), so the media routes' `stream_object` wraps the RustFS body in `take_until(shutdown.cancelled_owned())`: a slow download can't hold the drain open, and the client sees a short body and fetches the image again from the next process ([backend api 18](../../../docs/backend/api.md#18-media)). That is what makes 60 s the server's worst case.

*Design choice for this repo*: each budget leaves at least 2 s before SIGKILL for logs to flush. The worker drains at all because a job it finishes doesn't have to be redone after a 30 s requeue ([contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)).

## 2. Signals

- Listen for both: `tokio::signal::ctrl_c()` and, on Unix, `tokio::signal::unix::signal(SignalKind::terminate())`. `shutdown_signal` in section 5 is the one in axum's [graceful-shutdown example](https://github.com/tokio-rs/axum/tree/axum-v0.8.9/examples/graceful-shutdown), with the `expect`s replaced by a logged error. [ctrl_c](https://docs.rs/tokio/1.53.3/tokio/signal/fn.ctrl_c.html), [unix::signal](https://docs.rs/tokio/1.53.3/tokio/signal/unix/fn.signal.html)
- **Once tokio listens for a signal, that signal never again has its default effect** for the life of the process, even after the listener is dropped. So a second Ctrl-C does not kill a draining process by itself. `api`'s signal task listens once ([backend api 4](../../../docs/backend/api.md#4-start-up-and-shutdown)), so a drain that overruns is ended by SIGKILL: Compose's at `stop_grace_period`, or `docker kill` by hand. [ctrl_c: Caveats](https://docs.rs/tokio/1.53.3/tokio/signal/fn.ctrl_c.html#caveats)
- `ctrl_c()` only sees signals that arrive after its future is first polled, and a `Signal` stream may coalesce several signals into one. [ctrl_c](https://docs.rs/tokio/1.53.3/tokio/signal/fn.ctrl_c.html), [unix::signal](https://docs.rs/tokio/1.53.3/tokio/signal/unix/fn.signal.html)

## 3. `TaskTracker`

- `wait()` completes only when the tracker is **both closed and empty**. An open tracker never finishes waiting, even with no tasks, so call `close()` before `wait()`. `close()` doesn't stop you spawning more tasks. [TaskTracker](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html), [close](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html#method.close), [wait](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html#method.wait)
- It is cheap to clone and spawning takes `&self`, so it can live in `AppState`. A finished task is freed at once (unlike `JoinSet`), and dropping a tracker aborts nothing. [Comparison to JoinSet](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html#comparison-to-joinset)
- `spawn_blocking` work can be tracked too (`tracker.spawn_blocking`), but it can't be cancelled, so it counts against the budget until it ends. [spawn_blocking](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html#method.spawn_blocking)
- **Every tracked task needs an end condition tied to shutdown**: a `select!` on its token, or the drop of every handle that keeps it alive. The per-scan writer actor in [channels.md](channels.md#3-oneshot-and-the-actor-pattern-the-per-scan-event-writer) ends on the token or after 60 s without a command, and drains what was queued either way ([backend api 13](../../../docs/backend/api.md#13-scan-events-writer-hub-and-stream)). The delete queue's tasks are the deliberate exception: they never watch the token, and shutdown waits for them on their own, longer deadline (section 5). A task with no end condition uses up the whole budget:

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
    let budget = Duration::from_secs(30); // api's SHUTDOWN_DRAIN_S (section 1)
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

- `api`'s blocking work (uploads, measurement and the pooled CPU models in [blocking.md](blocking.md#3-bounded-cpu-work-in-api)) is bounded per image or per call, so `#[tokio::main]` is fine ([backend api 4](../../../docs/backend/api.md#4-start-up-and-shutdown)).
- A binary whose blocking work could hang builds the runtime with `tokio::runtime::Builder` and ends with `runtime.shutdown_timeout(…)`, which stops waiting and leaks the stuck threads. [shutdown_timeout](https://docs.rs/tokio/1.53.3/tokio/runtime/struct.Runtime.html#method.shutdown_timeout)
- A plain `std::thread` (the perception worker's inference thread) does not hold up exit: when `main` returns, the process ends. [std::thread: The threading model](https://doc.rust-lang.org/std/thread/index.html#the-threading-model)

## 5. `api`

The design is `main` in [backend api 4](../../../docs/backend/api.md#4-start-up-and-shutdown). What has to end for the server to drain:

- **axum's graceful shutdown stops accepting, then waits for every open connection's response to finish**; `serve(..).with_graceful_shutdown(..)` returns only when the last one closes. axum's own example says so ("Graceful shutdown will wait for outstanding requests to complete") and adds a request timeout. [graceful-shutdown example](https://github.com/tokio-rs/axum/tree/axum-v0.8.9/examples/graceful-shutdown), [Serve::with_graceful_shutdown](https://docs.rs/axum/0.8.9/axum/serve/struct.Serve.html#method.with_graceful_shutdown)
- **An SSE response never finishes by itself** (contracts 4 keeps it open until a resting point), so the scan stream and the chat stream end on the token with `.take_until(token.cancelled_owned())`. The app reconnects with `Last-Event-ID` and loses nothing ([contracts 4](../../../docs/architecture/contracts.md#transport); [backend api 13](../../../docs/backend/api.md#13-scan-events-writer-hub-and-stream)). The SSE endpoint is in [rust-axum-api](../rust-axum-api/SKILL.md). The test below proves both halves.
- **The claim long poll** (up to 30 s, [contracts 5](../../../docs/architecture/contracts.md#post-internaljobsclaim)) returns `204` on cancel, and the worker reconnects to the next process ([channels.md](channels.md#5-notify-the-job-claim-long-poll)).
- **The scheduler's loops** `select!` on the token and their next tick, so they leave between runs rather than in the middle of one ([backend api 16](../../../docs/backend/api.md#16-the-scheduler)). The writer actors end on the token and drain ([channels.md 3](channels.md#3-oneshot-and-the-actor-pattern-the-per-scan-event-writer)).
- **Deletion cascades** don't watch the token, except an account cascade's Clerk retries, which stop on it; a cascade still running at the deadline is cut off with the process and resumes on the next start from `deleting_at` or the account's tombstone ([backend api 17](../../../docs/backend/api.md#17-deletion)). The tombstone resuming the Clerk deletion after a restart is settled ([contracts 12, question 40](../../../docs/architecture/contracts.md#12-open-questions), v0.10.1; [contracts 9, account](../../../docs/architecture/contracts.md#account)). **Not in the contracts yet:** a resumed plant or scan deletion leaves its objects to the daily orphan sweep, up to a day later ([contracts 12, question 41](../../../docs/architecture/contracts.md#12-open-questions)).
- **RustFS deletes are never cut short by the drain.** Every delete goes through one `blob::DeleteQueue`, which runs each job in a task on its own `TaskTracker`, so dropping a request or a cascade never stops one ([backend blob 6](../../../docs/backend/blob.md#6-deletes-that-are-never-cancelled)). At shutdown `DeleteQueue::drain` gets `max(SHUTDOWN_DRAIN_S, 120 s)`: storage 4 says to wait for every pending delete "for up to one full attempt, even when the rest of the drain is shorter", one attempt being the client's 120-second operation timeout (`blob::client::OPERATION_TIMEOUT`, [storage 4](../../../docs/storage/README.md#4-writes-and-deletes); [backend blob 4](../../../docs/backend/blob.md#4-the-client)). The two waits run side by side with `tokio::join!` ([join!](https://docs.rs/tokio/1.53.3/tokio/macro.join.html)).
- The signal future passed to `with_graceful_shutdown` must be `Send + 'static`: `token.cancelled_owned()` is. [Serve::with_graceful_shutdown](https://docs.rs/axum/0.8.9/axum/serve/struct.Serve.html#method.with_graceful_shutdown), [cancelled_owned](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html#method.cancelled_owned)

The signal task listens once: a second Ctrl-C during the drain does nothing (section 2), and Compose's SIGKILL at `stop_grace_period` is what ends a drain that overruns.

**A handler that fails to install never fires.** api 4's `signal` passes each handler through `until_signal`, which logs the failure and then waits forever, so a missing handler can't shut the API down the moment it starts; the other signal, or Docker's SIGKILL, still stops it ([backend api 4](../../../docs/backend/api.md#4-start-up-and-shutdown): ✓ `a_signal_handler_that_fails_to_install_never_fires`; [`ctrl_c`](https://docs.rs/tokio/1.53.3/tokio/signal/fn.ctrl_c.html) returns `io::Result<()>`; [`unix::signal`, errors](https://docs.rs/tokio/1.53.3/tokio/signal/unix/fn.signal.html#errors)). `shutdown_signal` below does the same inline.

```rust
use std::future::Future;
use std::time::Duration;

use axum::Router;
use tokio::net::TcpListener;
use tokio::time::{self, Instant, MissedTickBehavior, timeout};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// `SHUTDOWN_DRAIN_S`'s default: how long shutdown waits for background tasks (api 5).
const SHUTDOWN_DRAIN: Duration = Duration::from_secs(30);

/// The `RustFS` client's per-attempt operation timeout (`blob::client::OPERATION_TIMEOUT`).
const OPERATION_TIMEOUT: Duration = Duration::from_secs(120);

/// Stands in for `blob::DeleteQueue` (blob 6): every delete runs in a task the queue owns, so
/// dropping the caller never stops it, and one `drain` waits for all of them.
#[derive(Clone, Default)]
pub struct DeleteQueue {
    tracker: TaskTracker,
}

impl DeleteQueue {
    pub fn submit(&self, delete: impl Future<Output = ()> + Send + 'static) {
        self.tracker.spawn(delete);
    }

    pub async fn drain(&self) {
        self.tracker.close();
        self.tracker.wait().await;
    }

    pub fn in_flight(&self) -> usize {
        self.tracker.len()
    }
}

/// The end of `api`'s `main` (api 4): serve until the token fires and every connection has
/// closed, then give background tasks `drain` and pending deletes at least one full attempt.
pub async fn serve_then_drain(
    listener: TcpListener,
    app: Router,
    shutdown: CancellationToken,
    tasks: &TaskTracker,
    deletes: &DeleteQueue,
    drain: Duration,
) -> std::io::Result<()> {
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await?;
    drain_after_serve(tasks, deletes, drain).await;
    Ok(())
}

async fn drain_after_serve(tasks: &TaskTracker, deletes: &DeleteQueue, drain: Duration) {
    tasks.close();
    let delete_drain = drain.max(OPERATION_TIMEOUT);
    let (tasks_waited, deletes_waited) = tokio::join!(
        timeout(drain, tasks.wait()),
        timeout(delete_drain, deletes.drain()),
    );
    if tasks_waited.is_err() {
        tracing::warn!(
            ?drain,
            "background tasks still running at exit; deletions resume on restart"
        );
    }
    if deletes_waited.is_err() {
        tracing::warn!(
            in_flight = deletes.in_flight(),
            "object deletes still running at exit"
        );
    }
}

/// Cancels the token on Ctrl-C (SIGINT) or SIGTERM. Spawned once at start. As api 4's `signal`
/// does, a handler that fails to install is logged and its branch stays pending.
pub async fn shutdown_signal(shutdown: CancellationToken) {
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
    tracing::info!("shutdown signal");
    shutdown.cancel();
}

/// One scheduler task (api 16's `every`): a first delay, then one run per period, leaving
/// between runs. `biased;` makes a cancel that arrives with a tick win, *this skill's choice*.
pub fn every<F, Fut>(
    tasks: &TaskTracker,
    shutdown: CancellationToken,
    first: Duration,
    period: Duration,
    body: F,
) where
    F: Fn() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    tasks.spawn(async move {
        let mut ticks = time::interval_at(Instant::now() + first, period);
        ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => break,
                _ = ticks.tick() => body().await,
            }
        }
    });
}

#[tokio::test(start_paused = true)]
async fn pending_deletes_get_a_full_attempt_after_the_task_drain() {
    let (tasks, deletes) = (TaskTracker::new(), DeleteQueue::default());
    tasks.spawn(time::sleep(Duration::from_secs(3600))); // a cascade that runs on
    deletes.submit(time::sleep(Duration::from_secs(100))); // a slow DeleteObjects
    let start = Instant::now();
    drain_after_serve(&tasks, &deletes, SHUTDOWN_DRAIN).await;
    // The tasks were given up on at 30 s; the delete was waited for until it finished.
    assert_eq!(start.elapsed(), Duration::from_secs(100));
    assert_eq!((tasks.len(), deletes.in_flight()), (1, 0));
}

#[tokio::test(start_paused = true)]
async fn the_delete_drain_stops_after_one_attempt() {
    let (tasks, deletes) = (TaskTracker::new(), DeleteQueue::default());
    deletes.submit(time::sleep(Duration::from_secs(600))); // hung past every timeout
    let start = Instant::now();
    drain_after_serve(&tasks, &deletes, SHUTDOWN_DRAIN).await;
    assert_eq!(start.elapsed(), OPERATION_TIMEOUT);
    assert_eq!(deletes.in_flight(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_scheduler_task_leaves_between_runs() {
    let (tasks, shutdown) = (TaskTracker::new(), CancellationToken::new());
    let runs = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let counter = runs.clone();
    every(
        &tasks,
        shutdown.clone(),
        Duration::from_secs(5),
        Duration::from_secs(5),
        move || {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                time::sleep(Duration::from_secs(2)).await; // the run's queries
            }
        },
    );
    time::sleep(Duration::from_secs(11)).await; // runs at 5 s and 10 s; the second is mid-way
    shutdown.cancel();
    tasks.close();
    tasks.wait().await; // the run in progress finishes, then the loop sees the token
    assert_eq!(runs.load(std::sync::atomic::Ordering::SeqCst), 2);
}
```

`main` calls `serve_then_drain` after start-up ([backend api 4](../../../docs/backend/api.md#4-start-up-and-shutdown)); the real one builds `AppState`, the router and the tracing setup from the sibling skills, and spawns `shutdown_signal(shutdown.clone())` (api 4's `signal`) with plain `tokio::spawn`, because nothing waits for it. The scheduler's intervals and first delays are api 16's `TASKS` table (5 s for the requeue, 30 s for the sweep, 60 s for the `idle` task, and so on).

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
- **Finish the jobs in hand** if they fit `DRAIN_SECONDS` (20 s, section 1). The heartbeat must keep running meanwhile, or the broker requeues the job after 30 seconds and the result is refused with `409 lease_lost` ([contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)). So the heartbeat's token is **not** a child of the shutdown token; it is cancelled after the drain ([backend worker 5](../../../docs/backend/worker.md#5-the-lifecycle-state-machine): `run_until_sigterm`, `stop_heartbeat`).
- **Out of budget**: cancel the jobs and send nothing. `Worker::drain` cancels the `abandon` token that parents every job's token, each job task returns at its `select!` (dropping the model's future is the cancellation), and nothing is sent for it. Every await in a job task selects on that token, a `complete` call in flight and the backoff between `complete` calls included, and the heartbeat's own token drops a call in flight, so the whole shutdown ends at `DRAIN_SECONDS`: without those two cuts a `complete` (10 s deadline) and then a heartbeat (5 s) could run 20 + 10 + 5 = 35 s, past the 30 s grace. The broker requeues the jobs, and another worker or the fallback worker takes them; a `complete` cut at the bound was either committed or is requeued like any abandoned job ([backend worker 5](../../../docs/backend/worker.md#5-the-lifecycle-state-machine), shutdown time; [worker 7](../../../docs/backend/worker.md#7-cancellation); [contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)).

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
    time::timeout(Duration::from_secs(20), worker) // DRAIN_SECONDS (section 1)
        .await
        .expect("drained within the budget")
        .expect("no panic");
    assert_eq!(api.claims.load(Ordering::SeqCst), 1);
    assert_eq!(api.finished.load(Ordering::SeqCst), 1);
    assert!(api.beats.load(Ordering::SeqCst) >= 1);
    assert_eq!(stop_at.elapsed(), Duration::from_millis(3100)); // the rest of job 0
}
```

**A claim dropped at shutdown** may already have leased a job on the server. Nothing lists that job in a heartbeat, so it isn't extended: the broker requeues it once the worker's heartbeat is 30 seconds stale, or when its lease expires, whichever comes first. There is no call to hand a lease back. Settled ([contracts 12, question 20](../../../docs/architecture/contracts.md#12-open-questions); [contracts 5, leases](../../../docs/architecture/contracts.md#leases-runs-and-attempts); [backend worker 16](../../../docs/backend/worker.md#16-decisions-on-the-contracts-open-questions); [backend store 21](../../../docs/backend/store.md#21-decisions-on-open-questions)).
