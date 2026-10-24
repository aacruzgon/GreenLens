//! The worker loop ([contracts 5]; the `rust-workers` skill): heartbeat every 10 s on its own
//! task, the claim long poll gated by `slots`, one task per job, `cancel_job_ids`, and the
//! SIGTERM drain, which ends within `DRAIN_SECONDS` whatever the jobs and calls are doing.
//!
//! ```text
//!            ┌──────────── heartbeat task (own token, stops last) ────────────┐
//! start ─▶ claiming ──claim 200──▶ job task: running ──▶ completing ──▶ done
//!            ▲  │204 (poll again)        │cancel_job_ids / lease_lost ──▶ dropped (no complete)
//!            │  └─error: back off ─┘     │soft deadline ──▶ failed (retryable) ──▶ completing
//!   SIGTERM ─┴▶ draining (no claims; jobs get `drain` to finish) ──▶ abandoned ──▶ exit
//! ```

use std::{
    collections::{BTreeMap, HashMap},
    future::Future,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use domain::{
    id::{JobId, WorkerId},
    jobs::{Capability, ClaimRequest, ClaimedJob, Heartbeat, HeldJob},
    ports::VisionModel,
};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    time::MissedTickBehavior,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::Instrument as _;

use crate::{
    api::{CallError, JobLease, WorkerApi},
    config::Timings,
    job::{JobRunner, run_job},
};

/// Exponential backoff with jitter that never goes below half the delay.
#[derive(Debug, Clone)]
pub struct Backoff {
    attempt: u32,
    base: Duration,
    cap: Duration,
}

impl Backoff {
    /// `base` doubling up to `cap`.
    pub const fn new(base: Duration, cap: Duration) -> Self {
        Self {
            attempt: 0,
            base,
            cap,
        }
    }

    /// After any answer.
    pub const fn reset(&mut self) {
        self.attempt = 0;
    }

    /// The next delay, drawn from `[d/2, d]`.
    pub fn next_delay(&mut self) -> Duration {
        let ceiling = self
            .base
            .saturating_mul(1 << self.attempt.min(16))
            .min(self.cap);
        self.attempt = self.attempt.saturating_add(1);
        rand::random_range(ceiling / 2..=ceiling)
    }
}

/// The wait before retrying after `e`: the backoff's next delay, or the API's `retry_after_s`
/// when it asked for longer ([contracts 2]).
pub fn retry_delay(e: &CallError, backoff: &mut Backoff) -> Duration {
    let delay = backoff.next_delay();
    e.retry_after().map_or(delay, |after| delay.max(after))
}

struct Entry {
    lease: JobLease,
    cancel: CancellationToken,
}

/// The held jobs: what the heartbeat lists in `current_jobs` and cancels on `cancel_job_ids`.
/// A `std` mutex, never held across an `.await`: each critical section is one insert, remove or
/// read.
#[derive(Clone, Default)]
pub struct Running(Arc<Mutex<HashMap<JobId, Entry>>>);

/// Removes the job from the heartbeat list when the job task ends, however it ends.
pub struct Registered {
    running: Running,
    job_id: JobId,
}

impl Drop for Registered {
    fn drop(&mut self) {
        self.running.lock().remove(&self.job_id);
    }
}

impl Running {
    fn lock(&self) -> MutexGuard<'_, HashMap<JobId, Entry>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Lists the job until the returned guard drops.
    pub fn register(&self, lease: JobLease, cancel: CancellationToken) -> Registered {
        let job_id = lease.job_id;
        self.lock().insert(job_id, Entry { lease, cancel });
        Registered {
            running: self.clone(),
            job_id,
        }
    }

    /// `current_jobs`.
    pub fn current_jobs(&self) -> Vec<HeldJob> {
        self.lock()
            .values()
            .map(|e| HeldJob {
                job_id: e.lease.job_id,
                lease_token: e.lease.lease_token.clone(),
            })
            .collect()
    }

    /// Cancels the listed jobs' tokens; ids the worker doesn't hold are ignored.
    pub fn cancel(&self, job_ids: &[JobId]) {
        let jobs = self.lock();
        for id in job_ids {
            if let Some(entry) = jobs.get(id) {
                entry.cancel.cancel();
            }
        }
    }
}

/// Who this worker is, sent in every heartbeat and claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// `worker_id`.
    pub worker_id: WorkerId,
    /// `caps`.
    pub caps: Vec<Capability>,
    /// `slots`.
    pub slots: u32,
}

/// The worker. Generic over the API (`HttpWorkerApi`, or a fake in tests) for static dispatch;
/// the model is a trait object chosen at start from `--caps`.
pub struct Worker<A: WorkerApi> {
    api: Arc<A>,
    runner: Arc<JobRunner>,
    identity: Arc<Identity>,
    running: Running,
    slots: Arc<Semaphore>,
    tracker: TaskTracker,
    abandon: CancellationToken,
    timings: Timings,
}

impl<A: WorkerApi> Worker<A> {
    /// A worker that runs jobs on `model`.
    pub fn new(api: A, model: Arc<dyn VisionModel>, identity: Identity, timings: Timings) -> Self {
        let slots = Arc::new(Semaphore::new(identity.slots as usize));
        let runner = Arc::new(JobRunner::new(model, identity.worker_id.clone()));
        Self {
            api: Arc::new(api),
            runner,
            identity: Arc::new(identity),
            running: Running::default(),
            slots,
            tracker: TaskTracker::new(),
            abandon: CancellationToken::new(),
            timings,
        }
    }

    /// The heartbeat's `versions`: the model's keys plus `worker_id`.
    fn versions(&self) -> BTreeMap<String, String> {
        self.runner.versions()
    }

    /// Every 10 s until `stop`: list the running jobs, cancel what the broker says to cancel.
    /// `Delay` sends one catch-up beat after a slow call instead of a burst of stale ones.
    /// `stop` also drops a call in flight, so the loop ends at once. A failed beat is logged,
    /// never retried early: the next one is 10 s away, and the broker waits 30 s.
    pub async fn heartbeat_loop(&self, stop: CancellationToken) {
        let mut every = tokio::time::interval(self.timings.heartbeat_every);
        every.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                () = stop.cancelled() => return,
                _ = every.tick() => {}
            }
            let hb = Heartbeat {
                worker_id: self.identity.worker_id.clone(),
                caps: self.identity.caps.clone(),
                slots: self.identity.slots,
                gpu: None,
                current_jobs: self.running.current_jobs(),
                versions: self.versions(),
            };
            let call =
                tokio::time::timeout(self.timings.heartbeat_timeout, self.api.heartbeat(&hb));
            let answer = tokio::select! {
                () = stop.cancelled() => return,
                answer = call => answer,
            };
            match answer {
                Ok(Ok(resp)) => self.running.cancel(&resp.cancel_job_ids),
                Ok(Err(e)) => tracing::warn!(error = %e, "heartbeat failed"),
                Err(_) => tracing::warn!("heartbeat timed out"),
            }
        }
    }

    /// Claims while a slot is free, until `shutdown`. A slot is taken before each claim, so
    /// the worker never polls while full and never holds more than `slots` jobs.
    pub async fn claim_loop(&self, shutdown: CancellationToken) {
        let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(30));
        loop {
            let slot = tokio::select! {
                biased;
                () = shutdown.cancelled() => return,
                slot = Arc::clone(&self.slots).acquire_owned() => match slot {
                    Ok(slot) => slot,
                    Err(_) => return, // the semaphore is never closed; stop if it ever is
                },
            };
            let req = ClaimRequest {
                worker_id: self.identity.worker_id.clone(),
                caps: self.identity.caps.clone(),
                slots: self.identity.slots,
                wait_s: Some(self.timings.claim_wait_s),
            };
            let claimed = tokio::select! {
                biased;
                () = shutdown.cancelled() => return, // drops the open long poll
                claimed = self.api.claim(&req) => claimed,
            };
            match claimed {
                Ok(Some(job)) => {
                    backoff.reset();
                    self.start(job, slot);
                }
                Ok(None) => backoff.reset(),
                Err(e) => {
                    drop(slot);
                    let wait = retry_delay(&e, &mut backoff);
                    tracing::warn!(error = %e, wait_ms = wait.as_millis(), "claim failed");
                    tokio::select! {
                        () = shutdown.cancelled() => return,
                        () = tokio::time::sleep(wait) => {}
                    }
                }
            }
        }
    }

    /// Registers the job and spawns its task. The task owns everything it uses (`Arc` clones,
    /// the job, the permit, the registry guard): `TaskTracker::spawn` needs a `'static + Send`
    /// future, because the task may outlive this call and run on any runtime thread.
    fn start(&self, claimed: ClaimedJob, slot: OwnedSemaphorePermit) {
        let lease = match &claimed {
            ClaimedJob::Job(job) => JobLease::from(job.as_ref()),
            ClaimedJob::Unsupported(head) => JobLease {
                job_id: head.job_id,
                lease_token: head.lease_token.clone(),
            },
        };
        let cancel = self.abandon.child_token();
        let registered = self.running.register(lease, cancel.clone());
        let (api, runner, timings) = (
            Arc::clone(&self.api),
            Arc::clone(&self.runner),
            self.timings,
        );
        // `in_current_span`: the process span (`stub_vision`) follows the job into its task.
        self.tracker.spawn(
            async move {
                run_job(&api, &runner, claimed, &cancel, timings).await;
                drop((registered, slot)); // leaves the heartbeat list, then frees the slot
            }
            .in_current_span(),
        );
    }

    /// After `shutdown`: wait up to the drain bound for running jobs, then abandon the rest
    /// (nothing is sent; the broker requeues them 30 s after the last heartbeat). Every await
    /// in a job task, a `complete` call in flight included, selects on the job's token, so
    /// the second wait ends at once: the drain never outlasts its bound.
    pub async fn drain(&self) {
        self.tracker.close();
        if tokio::time::timeout(self.timings.drain, self.tracker.wait())
            .await
            .is_err()
        {
            tracing::warn!(jobs = self.tracker.len(), "abandoning jobs at shutdown");
            self.abandon.cancel();
            self.tracker.wait().await;
        }
    }

    /// The registry, for tests.
    #[cfg(test)]
    pub fn running(&self) -> &Running {
        &self.running
    }
}

/// The process: heartbeat and claims until SIGTERM or Ctrl-C, then [`run_until`]'s shutdown.
pub async fn run_until_sigterm<A: WorkerApi>(worker: Arc<Worker<A>>) -> std::io::Result<()> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut sigterm = signal(SignalKind::terminate())?;
    let stopped = async move {
        tokio::select! {
            _ = sigterm.recv() => Ok(()),
            r = tokio::signal::ctrl_c() => r,
        }
    };
    run_until(worker, stopped).await
}

/// Heartbeat and claims until `stopped`, then stop claiming, drain, stop heartbeating. The
/// heartbeat outlives the claim loop so draining jobs keep their leases. From `stopped` to the
/// return is at most the drain bound plus the model's release: the claim loop and the heartbeat
/// drop a call in flight, and the drain cuts every job at its bound. If `stopped` fails, the error is logged, the same
/// shutdown runs, and the error is returned at the end. It takes `worker` by value and every
/// task holding a clone has ended by the return, so the worker and its model drop here, before
/// `main` returns.
pub async fn run_until<A, S>(worker: Arc<Worker<A>>, stopped: S) -> std::io::Result<()>
where
    A: WorkerApi,
    S: Future<Output = std::io::Result<()>>,
{
    let (shutdown, stop_heartbeat) = (CancellationToken::new(), CancellationToken::new());
    let heartbeat = tokio::spawn({
        let (worker, stop) = (Arc::clone(&worker), stop_heartbeat.clone());
        async move { worker.heartbeat_loop(stop).await }.in_current_span()
    });
    let claims = tokio::spawn({
        let (worker, shutdown) = (Arc::clone(&worker), shutdown.clone());
        async move { worker.claim_loop(shutdown).await }.in_current_span()
    });
    let stopped = stopped.await;
    if let Err(e) = &stopped {
        tracing::error!(error = %e, "the stop signal failed; shutting down");
    }
    tracing::info!("shutting down: no new claims");
    shutdown.cancel();
    if let Err(e) = claims.await {
        tracing::error!(error = %e, "claim loop panicked");
    }
    worker.drain().await;
    stop_heartbeat.cancel();
    if let Err(e) = heartbeat.await {
        tracing::error!(error = %e, "heartbeat panicked");
    }
    stopped
}

#[cfg(test)]
mod tests;
