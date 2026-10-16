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
| Every subscriber sees every value | `broadcast::channel(n)` | The SSE hub ([architecture 12](../../../docs/architecture/README.md#operations); [backend api 13](../../../docs/backend/api.md#13-scan-events-writer-hub-and-stream)) |
| "Something changed, look again", no data | `Notify` | The job-claim long poll ([database 4](../../../docs/database/README.md#job-claim-with-slots-); [backend api 15](../../../docs/backend/api.md#15-the-job-broker-and-the-worker-api)) |
| The latest value only | `watch` | Not used by the design yet |

The tokio tutorial's overview of the same primitives: [Tokio's channel primitives](https://tokio.rs/tokio/tutorial/channels#tokios-channel-primitives).

## 2. `mpsc`: bounded, with backpressure

- **Use `mpsc::channel(n)`, not `unbounded_channel`.** `send(v).await` waits until there is room, which slows the producer down to the consumer's pace. The capacity must be at least 1 (`channel(0)` panics). [mpsc::channel](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/fn.channel.html), [Sender::send](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/struct.Sender.html#method.send), [Backpressure and bounded channels](https://tokio.rs/tokio/tutorial/channels#backpressure-and-bounded-channels)
- **`try_send` never waits**: `TrySendError::Full(v)` when the buffer has `n` values, `TrySendError::Closed(v)` when the receiver is gone; both give the value back. Use it where waiting is wrong and dropping or failing is right. [Sender::try_send](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/struct.Sender.html#method.try_send), [TrySendError](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/error/enum.TrySendError.html)
- `send` errors only when the receiver is closed. `Ok` doesn't mean the value will be received. [Sender::send](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/struct.Sender.html#method.send)
- `recv()` returns `None` once every `Sender` is dropped and the buffer is empty. That is how an actor learns to stop. It is cancel safe; `send` is not (a losing `select!` branch drops the message). [Receiver::recv](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/struct.Receiver.html#method.recv), [cancellation.md](cancellation.md#1-select)
- Never build a cycle of bounded channels between actors; a full cycle deadlocks. A `oneshot` reply or a `try_send` can't be part of such a cycle, because they never wait. [Actors with Tokio: Beware of cycles](https://ryhl.io/blog/actors-with-tokio/#beware-of-cycles)

## 3. `oneshot` and the actor pattern: the per-scan event writer

Contracts §4 requires **one serialised writer per scan** that "assigns `seq`, stores the event and broadcasts it, in that order", because worker events, answers, the scheduler's sweep and species changes all write to the same scan ([contracts 4, transport](../../../docs/architecture/contracts.md#transport); [architecture 12](../../../docs/architecture/README.md#operations)). That is an actor: one task owns the scan's write path and handles one command at a time; callers hold a cloneable handle and get the result back over a `oneshot`. The shape follows [Actors with Tokio](https://ryhl.io/blog/actors-with-tokio/#the-recipe): the task and the handle are separate types, the channel is bounded, and the task owns its receiver. The design is [backend api 13](../../../docs/backend/api.md#13-scan-events-writer-hub-and-stream) (`ScanWriters`, `run_actor`, `EventHub`); the block below is that design with the event body, runs, resting points and pushes left out.

**Lifecycle** (settled: [contracts 12, question 22](../../../docs/architecture/contracts.md#12-open-questions); [contracts 4, transport](../../../docs/architecture/contracts.md#transport); [backend api 13](../../../docs/backend/api.md#13-scan-events-writer-hub-and-stream) and [22](../../../docs/backend/api.md#22-decisions-on-the-contracts-open-questions); [backend pipeline 12](../../../docs/backend/pipeline.md#12-idempotency-and-recovery) and [17](../../../docs/backend/pipeline.md#17-open-questions-and-decisions)). Three pieces, each with its own rule, so none keeps another alive:

- **The actor** starts on a scan's first `emit` and retires after **60 s** without a command, or on shutdown. It removes only its own entry from the map (a generation number tells it apart from a newer actor for the same scan), then closes its channel and drains what was already queued, so no accepted command is lost. An `emit` that races the retirement gets its command back from the failed `send` ([`SendError`](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/error/struct.SendError.html) holds the value) and looks again, which starts a new actor.
- **The broadcast channel** lives in the hub, not in the actor. A subscriber creates it; a publish to a scan without one is a no-op (the event is stored, and a later subscriber replays it). The scheduler's `idle` task removes a channel only when [`receiver_count() == 0`](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/struct.Sender.html#method.receiver_count), under the hub's lock, and the actor looks the sender up under the same lock on every publish, so a live subscriber is never left on a channel nobody sends to ([backend api 16](../../../docs/backend/api.md#16-the-scheduler)).
- **The plan lock** (a per-scan `tokio::sync::Mutex<()>`, separate from the event channel so a slow plan write never delays events) is removed by the same task only when nobody holds or waits for it, `Arc::strong_count == 1`. The `idle` task calls `EventHub::sweep_idle` and `ScanWriters::sweep_idle` ([backend flows 19](../../../docs/backend/flows.md#19-writer-actor-broadcast-channel-and-plan-lock)) ([`Arc::strong_count`](https://doc.rust-lang.org/std/sync/struct.Arc.html#method.strong_count)).

`seq` comes from the store's transaction (`append_event` increments `scan.last_event_seq` and creates the event in one query, [database 4](../../../docs/database/README.md#scan-events-a-gap-free-seq-)), so correctness doesn't depend on any of these lifetimes: an actor started after another retired continues the sequence. The store call's code belongs to [rust-surrealdb](../rust-surrealdb/SKILL.md); the SSE endpoint that subscribes belongs to [rust-axum-api](../rust-axum-api/SKILL.md).

```rust
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::{OwnedMutexGuard, broadcast, mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// An actor with nothing to do for this long stops; the next event starts a new one (api 13).
pub const WRITER_IDLE: Duration = Duration::from_secs(60);

/// Stands in for `domain`'s `ScanId` (`Id<table::Scan>`, which is `Copy` too).
pub type ScanId = u64;

/// A stored scan event as the hub broadcasts it (the real envelope is contracts §4).
#[derive(Debug, Clone, PartialEq)]
pub struct StoredEvent {
    pub seq: u64,
    pub kind: String,
}

/// Stands in for `domain::ports::StoreError` (rust-errors mapping.md).
#[derive(Debug, Clone, thiserror::Error)]
#[error("storing the event failed")]
pub struct AppendError;

/// The one store call the writer needs: database §4's gap-free `seq` transaction. A port, so
/// `#[async_trait]` and `Arc<dyn …>` (rust-async traits.md 3).
#[async_trait::async_trait]
pub trait EventStore: Send + Sync {
    async fn append(&self, scan: ScanId, kind: &str) -> Result<u64, AppendError>;
}

#[derive(Debug, thiserror::Error)]
pub enum WriterError {
    #[error("the scan's writer stopped before answering")]
    Stopped,
    #[error(transparent)]
    Store(#[from] AppendError),
}

/// What the writer broadcasts (api 13's `Published`): the stored event, and whether the scan
/// was at rest after the call that produced it. Resting points are left out of this example,
/// so `rests` is always `false` here.
#[derive(Debug, Clone, PartialEq)]
pub struct Published {
    pub event: StoredEvent,
    pub rests: bool,
}

/// One broadcast sender per scan with live subscribers (api 13, the hub).
#[derive(Default)]
pub struct EventHub {
    channels: Mutex<HashMap<ScanId, broadcast::Sender<Arc<Published>>>>,
}

impl EventHub {
    /// Creates the scan's channel if it has none. Subscribe *before* replaying (contracts 4).
    pub fn subscribe(&self, scan: ScanId) -> broadcast::Receiver<Arc<Published>> {
        let mut channels = self.channels.lock().unwrap_or_else(PoisonError::into_inner);
        channels
            .entry(scan)
            .or_insert_with(|| broadcast::channel(256).0)
            .subscribe()
    }

    /// Looked up under the lock on every publish, never cached. No channel: nobody to tell.
    pub fn publish(&self, scan: ScanId, published: Published) {
        let channels = self.channels.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(live) = channels.get(&scan) {
            // Err only means the last receiver just left; the event is stored.
            let _ = live.send(Arc::new(published));
        }
    }

    /// The scheduler's `idle` task: drops channels without receivers. Returns how many.
    pub fn sweep_idle(&self) -> usize {
        let mut channels = self.channels.lock().unwrap_or_else(PoisonError::into_inner);
        let before = channels.len();
        channels.retain(|_, live| live.receiver_count() > 0);
        before - channels.len()
    }
}

struct Emit {
    kind: String,
    reply: oneshot::Sender<Result<StoredEvent, AppendError>>,
}

/// A running actor: its generation, so a retiring actor removes only its own entry.
struct Handle {
    generation: u64,
    tx: mpsc::Sender<Emit>,
}

struct Inner {
    actors: Mutex<HashMap<ScanId, Handle>>,
    plan_locks: Mutex<HashMap<ScanId, Arc<tokio::sync::Mutex<()>>>>,
    generation: AtomicU64,
    store: Arc<dyn EventStore>,
    hub: Arc<EventHub>,
    tasks: TaskTracker,
    shutdown: CancellationToken,
}

/// Every scan's writer. Cheap to clone (one `Arc`); every part of `api` that writes holds one.
#[derive(Clone)]
pub struct ScanWriters {
    inner: Arc<Inner>,
}

impl ScanWriters {
    pub fn new(
        store: Arc<dyn EventStore>,
        hub: Arc<EventHub>,
        tasks: TaskTracker,
        shutdown: CancellationToken,
    ) -> Self {
        let inner = Inner {
            actors: Mutex::default(),
            plan_locks: Mutex::default(),
            generation: AtomicU64::new(0),
            store,
            hub,
            tasks,
            shutdown,
        };
        Self {
            inner: Arc::new(inner),
        }
    }

    /// Stores and broadcasts one event, then answers with it.
    pub async fn emit(&self, scan: ScanId, kind: &str) -> Result<StoredEvent, WriterError> {
        let mut kind = kind.to_owned();
        // At most twice: a send fails only when the actor retired between the lookup and the
        // send, and the second lookup then starts a new actor.
        for _ in 0..2 {
            let (reply, answer) = oneshot::channel();
            match self.sender(scan).send(Emit { kind, reply }).await {
                Ok(()) => return Ok(answer.await.map_err(|_| WriterError::Stopped)??),
                Err(mpsc::error::SendError(back)) => kind = back.kind,
            }
        }
        Err(WriterError::Stopped)
    }

    /// The scan's running actor, or a new one. Spawning takes no `.await`, so the std lock is fine.
    fn sender(&self, scan: ScanId) -> mpsc::Sender<Emit> {
        let mut actors = self
            .inner
            .actors
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(running) = actors.get(&scan) {
            return running.tx.clone();
        }
        let generation = self.inner.generation.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel(64);
        actors.insert(
            scan,
            Handle {
                generation,
                tx: tx.clone(),
            },
        );
        let inner = Arc::clone(&self.inner);
        self.inner
            .tasks
            .spawn(run_actor(inner, scan, generation, rx));
        tx
    }

    /// Plan writes of a scan run one at a time (api 13); the owned guard can cross `.await`s.
    pub async fn plan_lock(&self, scan: ScanId) -> OwnedMutexGuard<()> {
        let lock = Arc::clone(
            self.inner
                .plan_locks
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(scan)
                .or_default(),
        );
        lock.lock_owned().await
    }

    /// The scheduler's `idle` task: drops locks nobody holds or waits for (only the map's `Arc`).
    pub fn sweep_idle(&self) -> usize {
        let mut locks = self
            .inner
            .plan_locks
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let before = locks.len();
        locks.retain(|_, lock| Arc::strong_count(lock) > 1);
        before - locks.len()
    }

    /// How many actors are running, for the tests.
    fn running(&self) -> usize {
        self.inner
            .actors
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}

/// One command at a time until idle or shut down; then remove its own entry, close the channel
/// and drain what was already queued, so no accepted command is lost.
async fn run_actor(inner: Arc<Inner>, scan: ScanId, generation: u64, mut rx: mpsc::Receiver<Emit>) {
    loop {
        // `recv` and `cancelled` are cancel safe; the idle sleep restarts after every command.
        let next = tokio::select! {
            command = rx.recv() => command,
            () = tokio::time::sleep(WRITER_IDLE) => None,
            () = inner.shutdown.cancelled() => None,
        };
        match next {
            Some(command) => handle(&inner, scan, command).await,
            None => break,
        }
    }
    {
        let mut actors = inner.actors.lock().unwrap_or_else(PoisonError::into_inner);
        if actors
            .get(&scan)
            .is_some_and(|h| h.generation == generation)
        {
            actors.remove(&scan);
        }
    }
    rx.close();
    while let Some(command) = rx.recv().await {
        handle(&inner, scan, command).await;
    }
}

/// Assign `seq` and store, then broadcast, then answer.
async fn handle(inner: &Inner, scan: ScanId, Emit { kind, reply }: Emit) {
    let result = inner
        .store
        .append(scan, &kind)
        .await
        .map(|seq| StoredEvent { seq, kind });
    if let Ok(event) = &result {
        let published = Published {
            event: event.clone(),
            rests: false,
        };
        inner.hub.publish(scan, published);
    }
    // Err only means the caller stopped waiting; the event is stored either way.
    let _ = reply.send(result);
}

/// Counts like `last_event_seq += 1`, per scan, with a pause in the middle to invite races.
#[derive(Default)]
struct CountingStore {
    last: Mutex<HashMap<ScanId, u64>>,
    pause: Duration,
}

#[async_trait::async_trait]
impl EventStore for CountingStore {
    async fn append(&self, scan: ScanId, _kind: &str) -> Result<u64, AppendError> {
        let seq = {
            let mut last = self.last.lock().map_err(|_| AppendError)?;
            let seq = last.entry(scan).or_default();
            *seq += 1;
            *seq
        };
        if self.pause.is_zero() {
            tokio::task::yield_now().await;
        } else {
            tokio::time::sleep(self.pause).await;
        }
        Ok(seq)
    }
}

fn setup(pause: Duration) -> (ScanWriters, Arc<EventHub>, TaskTracker, CancellationToken) {
    let store = Arc::new(CountingStore {
        pause,
        ..CountingStore::default()
    });
    let (hub, tasks, shutdown) = (
        Arc::new(EventHub::default()),
        TaskTracker::new(),
        CancellationToken::new(),
    );
    let w = ScanWriters::new(store, hub.clone(), tasks.clone(), shutdown.clone());
    (w, hub, tasks, shutdown)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_emits_broadcast_in_seq_order() {
    let (writers, hub, _tasks, _shutdown) = setup(Duration::ZERO);
    let mut live = hub.subscribe(7);
    let mut callers = tokio::task::JoinSet::new();
    for i in 0..50 {
        let w = writers.clone();
        callers.spawn(async move { w.emit(7, &format!("finding-{i}")).await });
    }
    while let Some(res) = callers.join_next().await {
        res.expect("no panic").expect("stored");
    }
    for expected in 1..=50 {
        assert_eq!(live.recv().await.expect("not lagged").event.seq, expected);
    }
}

#[tokio::test(start_paused = true)]
async fn an_idle_actor_retires_after_60_s_and_the_next_emit_starts_another() {
    let (writers, _hub, _tasks, _shutdown) = setup(Duration::ZERO);
    assert_eq!(writers.emit(7, "queued").await.expect("stored").seq, 1);
    assert_eq!(writers.running(), 1);
    tokio::time::sleep(Duration::from_secs(59)).await;
    assert_eq!(writers.running(), 1, "59 s idle: still running");
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(writers.running(), 0, "retired at 60 s idle");
    // The sequence comes from the store, so a new actor continues it.
    assert_eq!(writers.emit(7, "analysing").await.expect("stored").seq, 2);
    assert_eq!(writers.running(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_send_to_a_retired_actor_gives_the_command_back() {
    let (writers, _hub, _tasks, _shutdown) = setup(Duration::ZERO);
    let stale = writers.sender(7); // looked up just before the actor retires
    tokio::time::sleep(WRITER_IDLE + Duration::from_secs(1)).await;
    let (reply, _answer) = oneshot::channel();
    let cmd = Emit {
        kind: "late".to_owned(),
        reply,
    };
    let back = stale.send(cmd).await.expect_err("the channel is closed");
    assert_eq!(back.0.kind, "late");
    // `emit` retries with the returned command, which starts a new actor.
    assert_eq!(writers.emit(7, "late").await.expect("stored").seq, 1);
}

#[tokio::test(start_paused = true)]
async fn shutdown_drains_every_accepted_command() {
    let (writers, _hub, tasks, shutdown) = setup(Duration::from_secs(1));
    let mut callers = tokio::task::JoinSet::new();
    for i in 0..3 {
        let w = writers.clone();
        callers.spawn(async move { w.emit(7, &format!("e{i}")).await });
    }
    tokio::task::yield_now().await; // the three callers have queued their commands
    shutdown.cancel();
    tasks.close();
    tasks.wait().await; // the actor drained its queue, then ended
    let mut seqs = Vec::new();
    while let Some(res) = callers.join_next().await {
        seqs.push(res.expect("no panic").expect("stored").seq);
    }
    seqs.sort_unstable();
    assert_eq!(seqs, [1, 2, 3]);
}

#[tokio::test]
async fn the_hub_and_the_plan_locks_are_swept_only_when_unused() {
    let (writers, hub, _tasks, _shutdown) = setup(Duration::ZERO);
    let live = hub.subscribe(7);
    let gone = hub.subscribe(8);
    drop(gone);
    assert_eq!(hub.sweep_idle(), 1, "scan 8 had no receiver left");
    writers.emit(7, "finding").await.expect("stored");
    drop(live);
    assert_eq!(hub.sweep_idle(), 1);
    writers
        .emit(9, "finding")
        .await
        .expect("stored, with nobody to tell");

    let held = writers.plan_lock(7).await;
    drop(writers.plan_lock(8).await);
    assert_eq!(writers.sweep_idle(), 1, "only scan 8's lock was free");
    drop(held);
    assert_eq!(writers.sweep_idle(), 1);
}
```

Notes on the design:

- `let _ = reply.send(…)`: a `oneshot` send fails only when the receiver is gone, which here means the caller stopped waiting. That is not an error. [oneshot::Sender::send](https://docs.rs/tokio/1.53.3/tokio/sync/oneshot/struct.Sender.html#method.send), [Actors with Tokio: The Recipe](https://ryhl.io/blog/actors-with-tokio/#the-recipe)
- A caller whose `emit` future is dropped after the command was queued doesn't undo it: the event is still stored and broadcast. That is the right behaviour for an event log.
- **The order inside `run_actor` is what makes the drain safe.** The entry is removed under the map's lock *before* `rx.close()`, so a lookup either finds the old sender (and its `send` is accepted and drained, or fails and is retried) or finds no entry and starts a new actor. `Receiver::close` "prevents any further messages from being sent on the channel while still enabling the receiver to drain messages that are buffered", and "to guarantee that no messages are dropped, after calling `close()`, `recv()` must be called until `None` is returned", which is the drain loop ([`Receiver::close`](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/struct.Receiver.html#method.close); `tokio-1.53.3/src/sync/mpsc/bounded.rs`); the test `a_send_to_a_retired_actor_gives_the_command_back` proves the failed send returns the command.
- **The generation** matters when an actor retires while a newer one for the same scan already sits in the map (the old one broke out of its loop, a racing `emit` found no entry and started another): the old actor must not remove the new entry ([backend api 13](../../../docs/backend/api.md#13-scan-events-writer-hub-and-stream)).
- `tasks.spawn` on the process `TaskTracker` (not `tokio::spawn`) lets shutdown wait for writers to drain what is queued ([shutdown.md](shutdown.md#5-api)). Every actor ends on the token or after 60 s idle, so none holds up the drain.
- The maps are `std::sync::Mutex`es: short critical sections, no `.await` while locked ([SKILL.md, locks](SKILL.md#7-locks)). `PoisonError::into_inner` is right here, because a panic can't leave an insert or remove half done.

## 4. `broadcast`: the SSE hub

The hub is in-process `tokio::sync::broadcast`, which assumes a single `api` instance ([architecture 12](../../../docs/architecture/README.md#operations)). The SSE endpoint (`GET /scans/{id}/events`, [contracts 4](../../../docs/architecture/contracts.md#transport)) is defined in [rust-axum-api](../rust-axum-api/SKILL.md); this section is what the channel itself does. [broadcast module](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/index.html)

| Behaviour | Consequence for the hub | Source |
|---|---|---|
| `subscribe()` returns a receiver that sees only values sent **after** the call | Resume subscribes **first**, then replays from the database, then forwards live events, dropping any `seq` already sent, except that a duplicate flagged `rests` ends the stream, since that resting event already went out in the replay (contracts 4; [backend api 13, the stream](../../../docs/backend/api.md#13-scan-events-writer-hub-and-stream), step 4) | [Sender::subscribe](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/struct.Sender.html#method.subscribe) |
| The capacity is rounded **up** to a power of two; when it is full, a send overwrites the oldest value | `channel(256)` keeps the last 256 events of a scan; a run sends dozens ([event catalogue](../../../docs/architecture/contracts.md#event-catalogue)), so lag needs a stalled subscriber | [Lagging](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/index.html#lagging) |
| A receiver that missed overwritten values gets `Err(RecvError::Lagged(n))`: `n` values are gone, the receiver is **not** closed, its cursor moves to the oldest retained value, and the next `recv` succeeds | On `Lagged`, the subscriber switches to replaying from the database after the last `seq` it sent (contracts 4), instead of continuing with a gap | [Lagging](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/index.html#lagging), [RecvError](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/error/enum.RecvError.html) |
| `Err(RecvError::Closed)` comes only after every `Sender` is dropped **and** the receiver has read what was retained | The hub removes a scan's sender only when it has no receivers (section 3), so a live subscriber doesn't see `Closed` while the process runs. If it ever does, the stream ends and the app reconnects with `Last-Event-ID`, losing nothing ([backend api 13, the stream](../../../docs/backend/api.md#13-scan-events-writer-hub-and-stream), step 5; [contracts 4, transport](../../../docs/architecture/contracts.md#transport)) | [Closing](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/index.html#closing) |
| `send` returns `Err` when there are no receivers; a later `subscribe` makes sends work again | Ignore that error: the event is already stored. A scan with no channel at all isn't sent to (section 3) | [Sender::send](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/struct.Sender.html#method.send) |
| `recv` is cancel safe | So a timed-out wait loses no message: the live loop waits `timeout(REST_CHECK, live.recv())` (15 s) and, with no event, goes back to read the scan's state, because some changes reach a resting point without an event and a deleted scan emits none (its `stream_state` answers `NotFound`, which ends the stream) ([backend api 13](../../../docs/backend/api.md#13-scan-events-writer-hub-and-stream), steps 4 and 5). Safe too wherever the stream's future is dropped mid-`recv`. The scan stream doesn't `select!` on the token: the whole generator is wrapped in `.take_until(token.cancelled_owned())`, which ends the stream when the token fires wherever the generator is waiting, in the replay's store reads as well as in `recv` (the generator is then never polled again, and is dropped with the body), and axum's `KeepAlive` writes the comments ([backend api 13, the stream](../../../docs/backend/api.md#13-scan-events-writer-hub-and-stream); [`StreamExt::take_until`](https://docs.rs/futures/0.3.34/futures/stream/trait.StreamExt.html#method.take_until)). A `select!` on the token inside the loop would miss a shutdown during the replay | [Receiver::recv](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/struct.Receiver.html#method.recv) |
| Values are cloned for each receiver | Broadcast `Arc<Published>`, so a clone is a reference count | [broadcast module](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/index.html) |

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

[Database 4](../../../docs/database/README.md#job-claim-with-slots-): `POST /internal/jobs/claim` is a long poll ([contracts 5](../../../docs/architecture/contracts.md#post-internaljobsclaim): `wait_s` default 25, max 30; `204` when nothing arrived). It "waits on an in-process `tokio::sync::Notify` that every job insert signals, with a 1-second fallback poll". The design is `JobBroker::claim` ([backend api 15](../../../docs/backend/api.md#15-the-job-broker-and-the-worker-api)), and the `Notify` is the store's own (`SurrealStore::jobs_queued()`, [backend store 14](../../../docs/backend/store.md#14-jobs-and-workers)). What `Notify` does, from its docs:

| Call | Effect | Source |
|---|---|---|
| `notify_one()` | Wakes the first waiting `Notified` if one is registered; otherwise **stores one permit** (never more than one) that the next `notified().await` consumes at once | [notify_one](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Notify.html#method.notify_one) |
| `notify_waiters()` | Wakes **every** `Notified` that exists at that moment, polled or not; **stores no permit** | [notify_waiters](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Notify.html#method.notify_waiters), [notified](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Notify.html#method.notified) |
| A `Notified` that was created but never polled or `enable()`d | Is not registered for `notify_one`, but does receive `notify_waiters` | [Notified::enable](https://docs.rs/tokio/1.53.3/tokio/sync/futures/struct.Notified.html#method.enable) |
| Dropping a `Notified` | Loses its place in the `notify_one` queue (not cancel safe in that sense) | [notified: Cancel safety](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Notify.html#method.notified) |

So for the claim:

- **Only the store signals, and every write that queues or requeues a job does**: `enqueue_job`, a step commit or run start that queues one, an upload's guided or segmentation job, a retryable failure's requeue in `complete_job`, and `requeue_stale` when any job went back to `queued`, each calling `notify_waiters()` after its commit. It is the long poll's only wake-up: neither `api` nor the pipeline signals it by hand (there is no `ScanWriter::job_queued`), so a new write path can't forget to, and a requeued job is claimable again at once ([backend store 21](../../../docs/backend/store.md#21-decisions-on-open-questions); [backend api 15](../../../docs/backend/api.md#15-the-job-broker-and-the-worker-api); [backend pipeline 4](../../../docs/backend/pipeline.md#4-ports-and-construction); [contracts 5, leases](../../../docs/architecture/contracts.md#leases-runs-and-attempts)). Several workers wait at once with different capabilities (a perception worker, a vlm worker, the fallback worker); `notify_one` would wake one of them, maybe one that can't take the job, and the right one would wait for the next poll. Each woken claim re-runs the claim transaction; the losers find nothing or conflict and retry ([database 4](../../../docs/database/README.md#job-claim-with-slots-)). [notify_waiters](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Notify.html#method.notify_waiters), [notify_one](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Notify.html#method.notify_one)
- **The claim side creates `notified()` before it runs the query**, and awaits it only if the query found nothing. A `notify_waiters` that lands while the query runs then still wakes it, because a `Notified` receives `notify_waiters` from the moment it is created. No permit is needed, which is why `notify_waiters` storing none is fine. `JobBroker::claim` also pins the future and calls `enable()` before the query; that only matters for `notify_one`, so it is harmless and keeps the wait correct if the store ever switches. [Notify::notified](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Notify.html#method.notified), [Notified::enable](https://docs.rs/tokio/1.53.3/tokio/sync/futures/struct.Notified.html#method.enable); the test `an_insert_wakes_every_waiting_claim_at_once` below
- **The 1-second fallback poll** is still needed without any lost wakeup: a job can become claimable with no write, for example when it passes 15 seconds after `queued_at` and the fallback worker may take it ([contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts); `CLAIM_POLL` in [backend api 15](../../../docs/backend/api.md#15-the-job-broker-and-the-worker-api)).
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
    notify.notify_waiters(); // the store, after the insert commits
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
