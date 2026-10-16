//! The per-scan writer ([contracts 4], transport; [architecture 12], operations): one actor per
//! active scan, fed by an `mpsc` channel, so events from worker results, answers, the scheduler
//! and species changes get gap-free `seq`s in one order. For each event it appends through
//! `EventRepo::append_event` (which increments `scan.last_event_seq` in the event's transaction,
//! [database 4]), then broadcasts. It also holds each scan's plan-write lock and implements the
//! pipeline's `ScanWriter` port.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use domain::{
    events::{ScanEvent, ScanEventBody},
    id::ScanId,
    ports::{BoxError, Clock, NewEvent, Store, StoreError},
    url::Stored,
    wire::Run,
};

use crate::{
    events::hub::{EventHub, Published},
    push::Pusher,
    seams::{PlanGuard, ScanWriter},
};

/// An actor with nothing to do for this long stops; the next event starts a new one.
pub const WRITER_IDLE: Duration = Duration::from_secs(60);
/// Commands a writer queues before `emit` waits.
const WRITER_QUEUE: usize = 64;

type Reply = oneshot::Sender<Result<Vec<ScanEvent<Stored>>, StoreError>>;

/// A writer command.
enum Command {
    Emit {
        run: Run,
        events: Vec<ScanEventBody<Stored>>,
        reply: Reply,
    },
}

/// A running actor: its generation (so a retiring actor removes only its own entry) and sender.
struct Handle {
    generation: u64,
    tx: mpsc::Sender<Command>,
}

/// What the actors share.
struct Inner {
    actors: Mutex<HashMap<ScanId, Handle>>,
    plan_locks: Mutex<HashMap<ScanId, Arc<tokio::sync::Mutex<()>>>>,
    generation: AtomicU64,
    store: Arc<dyn Store>,
    clock: Arc<dyn Clock>,
    hub: Arc<EventHub>,
    pusher: Pusher,
    tasks: TaskTracker,
    shutdown: CancellationToken,
}

/// Every scan's writer. Cheap to clone (one `Arc`).
#[derive(Clone)]
pub struct ScanWriters {
    inner: Arc<Inner>,
}

impl ScanWriters {
    /// The writers. Actors run on `tasks`.
    pub fn new(
        store: Arc<dyn Store>,
        clock: Arc<dyn Clock>,
        hub: Arc<EventHub>,
        pusher: Pusher,
        tasks: TaskTracker,
        shutdown: CancellationToken,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                actors: Mutex::new(HashMap::new()),
                plan_locks: Mutex::new(HashMap::new()),
                generation: AtomicU64::new(0),
                store,
                clock,
                hub,
                pusher,
                tasks,
                shutdown,
            }),
        }
    }

    /// The scan's actor, started if it isn't running.
    fn sender(&self, scan: ScanId) -> mpsc::Sender<Command> {
        let mut actors = self
            .inner
            .actors
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(handle) = actors.get(&scan) {
            return handle.tx.clone();
        }
        let (tx, rx) = mpsc::channel(WRITER_QUEUE);
        let generation = self.inner.generation.fetch_add(1, Ordering::Relaxed);
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

    /// Drops the plan locks no write holds; run every minute with the hub sweep.
    pub fn sweep_idle(&self) {
        self.inner
            .plan_locks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|_, lock| Arc::strong_count(lock) > 1);
    }
}

/// The actor: one command at a time, until idle or shut down; then it removes its own entry,
/// closes its channel and drains what was already queued, so no accepted command is lost.
async fn run_actor(
    inner: Arc<Inner>,
    scan: ScanId,
    generation: u64,
    mut rx: mpsc::Receiver<Command>,
) {
    loop {
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

async fn handle(inner: &Inner, scan: ScanId, command: Command) {
    let Command::Emit { run, events, reply } = command;
    let result = append_all(inner, scan, run, events).await;
    let _ = reply.send(result);
}

/// Appends then broadcasts each event in order. The last event of the call is broadcast after
/// the scan's stream state is read, flagged `rests` when that state is a resting point.
async fn append_all(
    inner: &Inner,
    scan: ScanId,
    run: Run,
    events: Vec<ScanEventBody<Stored>>,
) -> Result<Vec<ScanEvent<Stored>>, StoreError> {
    let count = events.len();
    let mut stored = Vec::with_capacity(count);
    for (index, body) in events.into_iter().enumerate() {
        let push = Pusher::scan_reason(&body);
        let event = inner
            .store
            .append_event(
                scan,
                NewEvent {
                    run,
                    ts: inner.clock.now(),
                    body,
                },
            )
            .await?;
        let rests = if index + 1 == count && may_rest(&event.body) {
            inner.store.stream_state(scan).await?.at_rest()
        } else {
            false
        };
        inner.hub.publish(
            scan,
            Published {
                event: event.clone(),
                rests,
            },
        );
        if let Some(reason) = push {
            let pusher = inner.pusher.clone();
            inner
                .tasks
                .spawn(async move { pusher.to_scan_creator(scan, reason).await });
        }
        stored.push(event);
    }
    Ok(stored)
}

/// The events after which a scan can be at rest ([contracts 4], when the stream closes).
fn may_rest(body: &ScanEventBody<Stored>) -> bool {
    matches!(
        body,
        ScanEventBody::PlanReady { .. }
            | ScanEventBody::PlanSuperseded { .. }
            | ScanEventBody::Diagnosis { .. }
            | ScanEventBody::QuestionResolved { .. }
            | ScanEventBody::GuidedCheckDone { .. }
            | ScanEventBody::Failed { .. }
    )
}

#[async_trait]
impl ScanWriter for ScanWriters {
    async fn emit(
        &self,
        scan: ScanId,
        run: Run,
        events: Vec<ScanEventBody<Stored>>,
    ) -> Result<Vec<ScanEvent<Stored>>, StoreError> {
        let mut events = events;
        // At most twice: a send can fail only when the actor retired between the lookup and
        // the send, and the second lookup then starts a new actor.
        for _ in 0..2 {
            let (reply, answer) = oneshot::channel();
            match self
                .sender(scan)
                .send(Command::Emit { run, events, reply })
                .await
            {
                Ok(()) => {
                    return answer.await.unwrap_or_else(|e| {
                        Err(StoreError::Query {
                            what: "scan writer",
                            source: BoxError::from(e),
                        })
                    });
                }
                Err(mpsc::error::SendError(Command::Emit { events: back, .. })) => events = back,
            }
        }
        Err(StoreError::Query {
            what: "scan writer",
            source: "the writer kept retiring".into(),
        })
    }

    async fn plan_lock(&self, scan: ScanId) -> PlanGuard {
        let lock = Arc::clone(
            self.inner
                .plan_locks
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(scan)
                .or_default(),
        );
        PlanGuard::new(lock.lock_owned().await)
    }
}
