# Channels, actors and `Notify`

Part of [rust-async](SKILL.md). Versions: tokio 1.53.3, tokio-util 0.7.20. Every behaviour below is proved by the test block under it.

## Contents

1. [Which primitive](#1-which-primitive)
2. [`mpsc`: bounded, with backpressure](#2-mpsc-bounded-with-backpressure)
3. [`oneshot` and the actor pattern: the per-scan event writer](#3-oneshot-and-the-actor-pattern-the-per-scan-event-writer)
4. [`broadcast`: the SSE hub](#4-broadcast-the-sse-hub)
5. [`Notify`: the job-claim long poll](#5-notify-the-job-claim-long-poll)

## 1. Which primitive

| Need | Use | In this project |
|---|---|---|
| Many producers, one consumer, with backpressure | `mpsc::channel(n)` | Commands to an actor; work handed to a dedicated thread ([blocking.md](blocking.md#4-the-perception-worker-and-onnx-runtimes-threads)) |
| One reply to one request | `oneshot` | The reply inside an actor command |
| Every subscriber sees every value | `broadcast::channel(n)` | The SSE hub ([architecture 12](../../../docs/architecture/README.md#operations)) |
| "Something changed, look again", no data | `Notify` | The job-claim long poll ([database 4](../../../docs/database/README.md#job-claim-with-slots-)) |
| The latest value only | `watch` | Not used by the design yet |

The tokio tutorial's overview of the same primitives: [Tokio's channel primitives](https://tokio.rs/tokio/tutorial/channels#tokios-channel-primitives).

## 2. `mpsc`: bounded, with backpressure

- **Use `mpsc::channel(n)`, not `unbounded_channel`.** `send(v).await` waits until there is room, which slows the producer down to the consumer's pace. The capacity must be at least 1 (`channel(0)` panics). [mpsc::channel](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/fn.channel.html), [Sender::send](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/struct.Sender.html#method.send), [Backpressure and bounded channels](https://tokio.rs/tokio/tutorial/channels#backpressure-and-bounded-channels)
- **`try_send` never waits**: `TrySendError::Full(v)` when the buffer has `n` values, `TrySendError::Closed(v)` when the receiver is gone; both give the value back. Use it where waiting is wrong and dropping or failing is right. [Sender::try_send](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/struct.Sender.html#method.try_send), [TrySendError](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/error/enum.TrySendError.html)
- `send` errors only when the receiver is closed. `Ok` doesn't mean the value will be received. [Sender::send](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/struct.Sender.html#method.send)
- `recv()` returns `None` once every `Sender` is dropped and the buffer is empty. That is how an actor learns to stop. It is cancel safe; `send` is not (a losing `select!` branch drops the message). [Receiver::recv](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/struct.Receiver.html#method.recv), [cancellation.md](cancellation.md#1-select)
- Never build a cycle of bounded channels between actors; a full cycle deadlocks. A `oneshot` reply or a `try_send` can't be part of such a cycle, because they never wait. [Actors with Tokio: Beware of cycles](https://ryhl.io/blog/actors-with-tokio/#beware-of-cycles)

## 3. `oneshot` and the actor pattern: the per-scan event writer

Architecture §12 and contracts §4 require **one serialised writer per scan** that "assigns `seq`, stores the event and broadcasts it, in that order", because worker events, answers, the scheduler's sweep and species changes all write to the same scan ([architecture 12](../../../docs/architecture/README.md#operations), [contracts 4](../../../docs/architecture/contracts.md#transport)). That is an actor: one task owns the scan's write path and handles one command at a time; callers hold a cloneable handle and get the result back over a `oneshot`. The shape follows [Actors with Tokio](https://ryhl.io/blog/actors-with-tokio/#the-recipe): the task and the handle are separate types, the handle spawns the task, the channel is bounded, and the actor stops when every handle is gone.

The store call is the gap-free `seq` transaction from [database 4](../../../docs/database/README.md#scan-events-a-gap-free-seq-); its code belongs to [rust-surrealdb](../rust-surrealdb/SKILL.md). The SSE endpoint that subscribes belongs to [rust-axum-api](../rust-axum-api/SKILL.md).

```rust
use std::future::Future;
use std::sync::Arc;

use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::task::TaskTracker;

/// A stored scan event as the hub broadcasts it (the real envelope is contracts §4).
#[derive(Debug, Clone, PartialEq)]
pub struct StoredEvent {
    pub seq: u64,
    pub kind: String,
}

/// Stands in for the `store` crate's error (rust-errors mapping.md `StoreError`).
#[derive(Debug, Clone, thiserror::Error)]
#[error("storing the event failed")]
pub struct AppendError;

/// The one store call the writer needs: database §4's gap-free `seq` transaction.
pub trait EventStore: Send + Sync + 'static {
    fn append(
        &self,
        scan_id: &str,
        kind: &str,
    ) -> impl Future<Output = Result<u64, AppendError>> + Send;
}

#[derive(Debug, thiserror::Error)]
pub enum WriterError {
    #[error("the scan's writer task has stopped")]
    Stopped,
    #[error(transparent)]
    Store(#[from] AppendError),
}

struct Append {
    kind: String,
    respond_to: oneshot::Sender<Result<StoredEvent, AppendError>>,
}

/// Cheap to clone; every part of `api` that writes a scan event holds one.
#[derive(Clone)]
pub struct ScanWriter {
    commands: mpsc::Sender<Append>,
    live: broadcast::Sender<Arc<StoredEvent>>,
}

impl ScanWriter {
    pub fn spawn<S: EventStore>(scan_id: String, store: Arc<S>, tracker: &TaskTracker) -> Self {
        let (commands, mut inbox) = mpsc::channel::<Append>(32);
        let (live, _) = broadcast::channel(256);
        let out = live.clone();
        tracker.spawn(async move {
            // One command at a time: assign seq and store, then broadcast, then answer.
            while let Some(Append { kind, respond_to }) = inbox.recv().await {
                let result = store
                    .append(&scan_id, &kind)
                    .await
                    .map(|seq| StoredEvent { seq, kind });
                if let Ok(event) = &result {
                    // Err only means nobody is subscribed right now; the event is stored.
                    let _ = out.send(Arc::new(event.clone()));
                }
                // Err only means the caller stopped waiting.
                let _ = respond_to.send(result);
            }
            // Every ScanWriter clone is gone: the task ends.
        });
        Self { commands, live }
    }

    pub async fn append(&self, kind: &str) -> Result<StoredEvent, WriterError> {
        let (respond_to, response) = oneshot::channel();
        let cmd = Append {
            kind: kind.to_owned(),
            respond_to,
        };
        self.commands
            .send(cmd)
            .await
            .map_err(|_| WriterError::Stopped)?;
        Ok(response.await.map_err(|_| WriterError::Stopped)??)
    }

    /// Subscribe *before* replaying from the database (contracts §4, resume).
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<StoredEvent>> {
        self.live.subscribe()
    }
}

/// Counts like `last_event_seq += 1`, with an await in the middle to invite races.
#[derive(Default)]
struct CountingStore(std::sync::Mutex<u64>);

impl EventStore for CountingStore {
    async fn append(&self, _scan_id: &str, _kind: &str) -> Result<u64, AppendError> {
        let seq = {
            let mut last = self.0.lock().map_err(|_| AppendError)?;
            *last += 1;
            *last
        };
        tokio::task::yield_now().await;
        Ok(seq)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_writers_broadcast_in_seq_order() {
    let tracker = TaskTracker::new();
    let writer = ScanWriter::spawn(
        "01JAX".to_owned(),
        Arc::new(CountingStore::default()),
        &tracker,
    );
    let mut live = writer.subscribe();
    let mut callers = tokio::task::JoinSet::new();
    for i in 0..50 {
        let w = writer.clone();
        callers.spawn(async move { w.append(&format!("finding-{i}")).await });
    }
    while let Some(res) = callers.join_next().await {
        res.expect("no panic").expect("stored");
    }
    for expected in 1..=50 {
        assert_eq!(live.recv().await.expect("not lagged").seq, expected);
    }
    drop(writer);
    tracker.close();
    tracker.wait().await; // the writer task ends once every handle is dropped
}
```

Notes on the design:

- `let _ = respond_to.send(…)`: a `oneshot` send fails only when the receiver is gone, which here means the caller stopped waiting. That is not an error. [oneshot::Sender::send](https://docs.rs/tokio/1.53.3/tokio/sync/oneshot/struct.Sender.html#method.send), [Actors with Tokio: The Recipe](https://ryhl.io/blog/actors-with-tokio/#the-recipe)
- A caller whose `append` future is dropped after the command was queued doesn't undo it: the event is still stored and broadcast. That is the right behaviour for an event log.
- `tracker.spawn` (not `tokio::spawn`) lets shutdown wait for writers to finish what is queued ([shutdown.md](shutdown.md)).
- The writer for a scan is found in a `std::sync::Mutex<HashMap<ScanId, ScanWriter>>` in the hub (a short critical section, no `.await` while locked; [SKILL.md, locks](SKILL.md#7-locks)). Subscribing also goes through that lock, so a subscriber always gets the scan's current writer.
- **Not in the contracts yet:** the hub starts a scan's writer on its first event (or for a subscriber that arrives before one), and retires it only under the hub's lock and only when its broadcast has no receivers ([`receiver_count() == 0`](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/struct.Sender.html#method.receiver_count)), so a live subscriber is never left on a writer nobody uses ([contracts 12, question 22](../../../docs/architecture/contracts.md#12-open-questions)).

## 4. `broadcast`: the SSE hub

The hub is in-process `tokio::sync::broadcast`, which assumes a single `api` instance ([architecture 12](../../../docs/architecture/README.md#operations)). The SSE endpoint (`GET /scans/{id}/events`, [contracts 4](../../../docs/architecture/contracts.md#transport)) is defined in [rust-axum-api](../rust-axum-api/SKILL.md); this section is what the channel itself does. [broadcast module](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/index.html)

| Behaviour | Consequence for the hub | Source |
|---|---|---|
| `subscribe()` returns a receiver that sees only values sent **after** the call | Resume subscribes **first**, then replays from the database, then forwards live events, dropping any `seq` already sent (contracts 4) | [Sender::subscribe](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/struct.Sender.html#method.subscribe) |
| The capacity is rounded **up** to a power of two; when it is full, a send overwrites the oldest value | `channel(256)` keeps the last 256 events of a scan; a run sends dozens ([event catalogue](../../../docs/architecture/contracts.md#event-catalogue)), so lag needs a stalled subscriber | [Lagging](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/index.html#lagging) |
| A receiver that missed overwritten values gets `Err(RecvError::Lagged(n))`: `n` values are gone, the receiver is **not** closed, its cursor moves to the oldest retained value, and the next `recv` succeeds | On `Lagged`, the subscriber switches to replaying from the database after the last `seq` it sent (contracts 4), instead of continuing with a gap | [Lagging](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/index.html#lagging), [RecvError](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/error/enum.RecvError.html) |
| `Err(RecvError::Closed)` comes only after every `Sender` is dropped **and** the receiver has read what was retained | A subscriber sees it only when its scan's sender is gone: at shutdown, or a writer retired with receivers, which the lifecycle in section 3 doesn't do. The stream ends only on its resting-point rules (contracts 4, when the stream closes); on `Closed` the SSE stream subscribes again and replays from the database, so nothing is lost and it doesn't spin ([rust-axum-api sse 1](../rust-axum-api/sse.md#1-what-the-endpoint-does), step 7) | [Closing](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/index.html#closing), [contracts 4, transport](../../../docs/architecture/contracts.md#transport) |
| `send` returns `Err` when there are no receivers; a later `subscribe` makes sends work again | Ignore that error: the event is already stored | [Sender::send](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/struct.Sender.html#method.send) |
| `recv` is cancel safe | Safe in the SSE stream's `select!` with the keepalive timer and the shutdown token | [Receiver::recv](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/struct.Receiver.html#method.recv) |
| Values are cloned for each receiver | Broadcast `Arc<StoredEvent>`, so a clone is a reference count | [broadcast module](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/index.html) |

`tokio_stream::wrappers::BroadcastStream` (`tokio-stream` 0.1.19, feature `sync`) turns a receiver into a `Stream` and reports lag as `Err(BroadcastStreamRecvError::Lagged(n))`, with the same meaning. [BroadcastStream](https://docs.rs/tokio-stream/0.1.19/tokio_stream/wrappers/struct.BroadcastStream.html), [BroadcastStreamRecvError](https://docs.rs/tokio-stream/0.1.19/tokio_stream/wrappers/errors/enum.BroadcastStreamRecvError.html)

```rust
use tokio::sync::broadcast::{self, error::RecvError, error::TryRecvError};

#[tokio::test]
async fn a_lagging_receiver_skips_to_the_oldest_retained_message() {
    // Capacity 3 is rounded up to 4.
    let (tx, mut rx) = broadcast::channel::<u64>(3);
    for seq in 1..=6 {
        tx.send(seq).expect("a receiver exists");
    }
    // 6 sent, 4 retained (3..=6): seq 1 and 2 were overwritten.
    assert_eq!(rx.recv().await, Err(RecvError::Lagged(2)));
    // The receiver is still open; the next recv returns the oldest retained one.
    assert_eq!(rx.recv().await, Ok(3));
    assert_eq!(rx.recv().await, Ok(4));
}

#[tokio::test]
async fn four_behind_is_not_yet_lagging_with_capacity_three() {
    let (tx, mut rx) = broadcast::channel::<u64>(3);
    for seq in 1..=4 {
        tx.send(seq).expect("a receiver exists");
    }
    assert_eq!(rx.recv().await, Ok(1));
}

#[tokio::test]
async fn subscribe_sees_only_later_messages() {
    let (tx, _keep) = broadcast::channel::<u64>(16);
    tx.send(1).expect("a receiver exists");
    let mut late = tx.subscribe();
    tx.send(2).expect("a receiver exists");
    assert_eq!(late.recv().await, Ok(2));
    assert_eq!(late.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn closed_after_the_last_sender_and_the_backlog() {
    let (tx, mut rx) = broadcast::channel::<u64>(16);
    tx.send(1).expect("a receiver exists");
    drop(tx);
    assert_eq!(rx.recv().await, Ok(1)); // retained messages are still delivered
    assert_eq!(rx.recv().await, Err(RecvError::Closed));
}

#[test]
fn send_fails_when_nobody_is_subscribed() {
    let (tx, rx) = broadcast::channel::<u64>(16);
    drop(rx);
    // Not fatal: the event is already stored, and a later subscriber replays it.
    assert!(tx.send(1).is_err());
    let mut rx = tx.subscribe();
    tx.send(2).expect("a receiver exists again");
    assert_eq!(rx.try_recv(), Ok(2));
}
```

## 5. `Notify`: the job-claim long poll

[Database 4](../../../docs/database/README.md#job-claim-with-slots-): `POST /internal/jobs/claim` is a long poll ([contracts 5](../../../docs/architecture/contracts.md#post-internaljobsclaim): `wait_s` default 25, max 30; `204` when nothing arrived). It "waits on an in-process `tokio::sync::Notify` that every job insert signals, with a 1-second fallback poll". What `Notify` does, from its docs:

| Call | Effect | Source |
|---|---|---|
| `notify_one()` | Wakes the first waiting `Notified` if one is registered; otherwise **stores one permit** (never more than one) that the next `notified().await` consumes at once | [notify_one](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Notify.html#method.notify_one) |
| `notify_waiters()` | Wakes **every** `Notified` that exists at that moment, polled or not; **stores no permit** | [notify_waiters](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Notify.html#method.notify_waiters), [notified](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Notify.html#method.notified) |
| A `Notified` that was created but never polled or `enable()`d | Is not registered for `notify_one`, but does receive `notify_waiters` | [Notified::enable](https://docs.rs/tokio/1.53.3/tokio/sync/futures/struct.Notified.html#method.enable) |
| Dropping a `Notified` | Loses its place in the `notify_one` queue (not cancel safe in that sense) | [notified: Cancel safety](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Notify.html#method.notified) |

So for the claim:

- **The insert side calls `notify_waiters()`**, after the insert commits, and so does every **requeue** (a stale heartbeat, an expired lease, a retryable failure): a requeued job is claimable again at once ([contracts 5, leases](../../../docs/architecture/contracts.md#leases-runs-and-attempts)). Several workers wait at once with different capabilities (a perception worker, a vlm worker, the fallback worker); `notify_one` would wake one of them, maybe one that can't take the job, and the right one would wait for the next poll. Each woken claim re-runs the claim transaction; the losers find nothing or conflict and retry ([database 4](../../../docs/database/README.md#job-claim-with-slots-)). [notify_waiters](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Notify.html#method.notify_waiters), [notify_one](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Notify.html#method.notify_one)
- **The claim side creates `notified()` before it runs the query**, and awaits it only if the query found nothing. A `notify_waiters` that lands while the query runs then still wakes it, because a `Notified` receives `notify_waiters` from the moment it is created. No permit is needed, which is why `notify_waiters` storing none is fine. [Notify::notified](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Notify.html#method.notified); the test `an_insert_wakes_every_waiting_claim_at_once` below
- **The 1-second fallback poll** is still needed without any lost wakeup: a job can become claimable with no insert, for example when it passes 15 seconds after `queued_at` and the fallback worker may take it ([contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)).
- The wait also ends on the shutdown token, so a long poll doesn't hold up shutdown for up to 30 s ([shutdown.md](shutdown.md)).

```rust
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::{Instant, sleep, sleep_until, timeout};
use tokio_util::sync::CancellationToken;

/// How often a waiting claim re-runs its query without a signal (database §4).
const FALLBACK_POLL: Duration = Duration::from_secs(1);

/// The body of `POST /internal/jobs/claim`: `Ok(None)` becomes `204`.
pub async fn claim_long_poll<J, E, F, Fut>(
    job_queued: &Notify,
    wait: Duration,
    shutdown: &CancellationToken,
    mut try_claim: F,
) -> Result<Option<J>, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Option<J>, E>>,
{
    let deadline = Instant::now() + wait;
    loop {
        // Create the `Notified` before the query: a `notify_waiters` from an insert
        // that commits while the query runs still wakes this future.
        let queued = job_queued.notified();
        if let Some(job) = try_claim().await? {
            return Ok(Some(job));
        }
        tokio::select! {
            () = queued => {}
            () = sleep(FALLBACK_POLL) => {}
            () = sleep_until(deadline) => return Ok(None),
            () = shutdown.cancelled() => return Ok(None),
        }
    }
}

/// Stands in for the job table.
#[derive(Clone, Default)]
struct FakeQueue(Arc<Mutex<Vec<u32>>>);

impl FakeQueue {
    fn push(&self, job: u32) {
        self.0.lock().expect("not poisoned").push(job);
    }
    async fn claim(&self) -> Result<Option<u32>, std::convert::Infallible> {
        tokio::task::yield_now().await; // a real claim awaits the database
        Ok(self.0.lock().expect("not poisoned").pop())
    }
}

#[tokio::test(start_paused = true)]
async fn an_insert_wakes_every_waiting_claim_at_once() {
    let queue = FakeQueue::default();
    let notify = Arc::new(Notify::new());
    let shutdown = CancellationToken::new();
    let start = Instant::now();
    let mut waiters = Vec::new();
    for _ in 0..3 {
        let (queue, notify, shutdown) = (queue.clone(), notify.clone(), shutdown.clone());
        waiters.push(tokio::spawn(async move {
            claim_long_poll(&notify, Duration::from_secs(25), &shutdown, || {
                queue.claim()
            })
            .await
        }));
    }
    sleep(Duration::from_millis(2500)).await;
    queue.push(7);
    notify.notify_waiters(); // after the insert commits
    let mut claimed = Vec::new();
    for w in waiters {
        if let Some(job) = w.await.expect("no panic").expect("infallible") {
            claimed.push((job, start.elapsed()));
        }
    }
    // One waiter got the job at 2.5 s, not at the next 1 s poll; the others timed out.
    assert_eq!(claimed, [(7, Duration::from_millis(2500))]);
}

#[tokio::test(start_paused = true)]
async fn without_a_signal_the_fallback_poll_finds_the_job() {
    let queue = FakeQueue::default();
    let notify = Notify::new();
    let shutdown = CancellationToken::new();
    let q = queue.clone();
    tokio::spawn(async move {
        sleep(Duration::from_millis(2500)).await;
        q.push(9); // e.g. a job that just became eligible for the fallback worker
    });
    let start = Instant::now();
    let job = claim_long_poll(&notify, Duration::from_secs(25), &shutdown, || {
        queue.claim()
    })
    .await
    .expect("infallible");
    assert_eq!(job, Some(9));
    assert_eq!(start.elapsed(), Duration::from_secs(3));
}

#[tokio::test(start_paused = true)]
async fn notify_waiters_stores_no_permit_and_notify_one_stores_one() {
    let n = Notify::new();
    n.notify_waiters(); // nobody registered: lost
    assert!(timeout(Duration::from_secs(1), n.notified()).await.is_err());

    n.notify_one();
    n.notify_one(); // still one permit
    n.notified().await; // takes it at once
    assert!(timeout(Duration::from_secs(1), n.notified()).await.is_err());
}

#[tokio::test(start_paused = true)]
async fn notify_waiters_reaches_futures_that_were_created_but_not_polled() {
    let n = Notify::new();
    let first = n.notified();
    let second = n.notified();
    n.notify_waiters();
    timeout(Duration::from_secs(1), first).await.expect("woken");
    timeout(Duration::from_secs(1), second)
        .await
        .expect("woken");

    // notify_one doesn't see unpolled futures: it stores its single permit instead.
    let a = n.notified();
    let b = n.notified();
    n.notify_one();
    n.notify_one();
    timeout(Duration::from_secs(1), a)
        .await
        .expect("takes the permit");
    assert!(timeout(Duration::from_secs(1), b).await.is_err());
}
```
