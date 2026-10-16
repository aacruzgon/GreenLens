# The worker loop

Part of [rust-workers](SKILL.md). Versions: tokio 1.53.3, tokio-util 0.7.20, rand 0.10.3, serde_json 1.0.154. The rules are in [SKILL.md, section 2](SKILL.md#2-the-loop); the behaviour they implement is [contracts 5](../../../docs/architecture/contracts.md#5-worker-api-and-jobs), which wins over this file. The worker's design is [backend worker 5–7](../../../docs/backend/worker.md#5-the-lifecycle-state-machine); its `lifecycle` and `job` modules are this loop on the domain's real types.

## Contents

1. [How the parts fit](#1-how-the-parts-fit)
2. [The loop, with its tests](#2-the-loop-with-its-tests)
3. [What the tests show](#3-what-the-tests-show)
4. [Sources](#4-sources)

## 1. How the parts fit

- **`WorkerApi`** is the boundary to the API: `heartbeat`, `claim`, `events`, `complete`. Production implements it with reqwest (`HttpWorkerApi`, [reqwest.md](reqwest.md)); the tests implement it with a fake. Its methods return `impl Future + Send` so job tasks can be spawned on the multi-thread runtime; it is used generically (`Worker<A: WorkerApi>`), never as `dyn`. [rust-async, traits](../rust-async/traits.md#3-what-the-project-does-with-its-boundary-traits), [backend worker 6](../../../docs/backend/worker.md#6-leases-heartbeats-and-completion)
- **`JobLease`** is what a call about a job needs, its `job_id` and lease token, owned and cheap to clone, so the job task, its progress sink and the registry each hold one. A job the worker can't read still has one, from its head ([backend worker 6](../../../docs/backend/worker.md#6-leases-heartbeats-and-completion)).
- **`CallError`** is what a call can end in, classified as `check` in [reqwest.md](reqwest.md#4-reading-errors) does: `LeaseLost` (`409 lease_lost`); `Unavailable`, carrying the API's `retry_after_s` when it sent one, for a connect error, a timeout, a body that couldn't be read, an error body with `retryable: true` (a `429 rate_limited`, a `503`), or a `429` or `5xx` without an error body (a proxy's own answer); and `Rejected` for anything else, a `500 internal` with `retryable: false` included. The body's `retryable` decides whatever the status, as [contracts 2](../../../docs/architecture/contracts.md#2-errors) defines it, and a `204` on a repeat `complete` is success: the rows of [backend worker 6](../../../docs/backend/worker.md#6-leases-heartbeats-and-completion), "What an answer means" ([SKILL.md, rule 9](SKILL.md#2-the-loop)). `retry_delay` waits the longer of the backoff and the answer's `retry_after_s`.
- **`Runner`** runs one decoded job (perception, VLM or fallback) and returns the `complete` body, or `LeaseLost` when a call got `409 lease_lost` and nothing more may be sent. In the worker it is `JobRunner::execute` over the process's `Arc<dyn VisionModel>` ([backend worker 6](../../../docs/backend/worker.md#6-leases-heartbeats-and-completion)). It gets **no token: cancellation is by drop**. `run_job` `select!`s on the job's token and the runner's future, and when the token fires it returns, dropping the future; each adapter makes the drop effective, as the perception runner's `TerminateOnDrop` guard terminates the ONNX run ([backend domain 9, models](../../../docs/backend/domain.md#models); [backend worker 7](../../../docs/backend/worker.md#7-cancellation); [ort.md, the inference thread](ort.md#4-the-inference-thread)).
- **`Progress`** is the job's progress sink (`ApiProgress` in the worker, a `domain::ports::ProgressSink`): it posts a stage's events with the lease token. On `409 lease_lost` it **cancels the job's token**, so the job task stops at once and sends nothing more, even if the runner ignores the error; any other failure is logged and the job goes on, because `complete` repeats every stage (rule 8; [backend worker 1, 7](../../../docs/backend/worker.md#7-cancellation)). A late batch that reaches the API after `complete` with the same token is answered `204` and dropped by the API, so it is a success here ([backend api 15](../../../docs/backend/api.md#15-the-job-broker-and-the-worker-api)).
- **`Running`** is the registry of held jobs: `job_id → (lease_token, CancellationToken)`. The heartbeat reads it for `current_jobs` and cancels the tokens listed in `cancel_job_ids`. A job task holds a `Registered` guard whose `Drop` removes the entry, so a job leaves the list however its task ends. The lock is a `std::sync::Mutex`, never held across an `.await`. [tokio Mutex: which kind of mutex](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Mutex.html#which-kind-of-mutex-should-you-use)
- **`Worker::claim_loop`** takes a slot permit, then claims. A job goes to `start`, which registers it and spawns `run_job` on the `TaskTracker` with the permit moved in. A `204` after the full long poll loops at once; an error backs off, and so does an early `204`, a defensive guard only: the API sends `204` at the poll's deadline or at shutdown, and the shutdown one is followed by a connection error ([backend worker 5](../../../docs/backend/worker.md#5-the-lifecycle-state-machine); [SKILL.md, rule 5](SKILL.md#2-the-loop)). [contracts 5, claim](../../../docs/architecture/contracts.md#post-internaljobsclaim)
- **`run_job`** completes an unsupported job at once with `unsupported_version`; otherwise it runs the `Runner` under the job's soft deadline and sends `complete` with retries. A cancelled job or a `LeaseLost` returns without sending anything. [contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)
- **`Worker::drain`** closes the tracker and waits for the jobs up to a bound, then cancels `abandon`, the parent of every job token, which drops every runner still going. [TaskTracker::wait](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html#method.wait)
- **`run_until_stopped`** wires it together through `run_until`: the heartbeat on its own token, the claim loop on `shutdown` until SIGTERM or Ctrl-C, then drain, then stop the heartbeat (`run_until_sigterm` in [backend worker 5](../../../docs/backend/worker.md#5-the-lifecycle-state-machine)). Nothing waits past the drain: the heartbeat call and an abandoned job's `complete` call are raced against their tokens, so the exit comes 20 s after the signal at most, inside the 30 s `stop_grace_period`; `run_until` takes the signal as a future, so a test can drive it ([backend worker 5](../../../docs/backend/worker.md#5-the-lifecycle-state-machine), "Shutdown time"; [SKILL.md, rule 13](SKILL.md#2-the-loop)). [rust-async, shutdown](../rust-async/shutdown.md#6-worker)

**The types.** The claim is decoded by `decode_claim` into a `ClaimedJob`, both from the `JobHead` region of [rust-contract-types, unions 3](../rust-contract-types/unions.md#3-the-worker-checks-v-first); the failure is a `JobError` with a `JobErrorCode`, from the `JobErrorCode` region of [unions 4](../rust-contract-types/unions.md#4-complete-tagged-by-status-result-decoded-by-the-jobs-kind). Both regions are copied into the block and are `domain::jobs`'s `JobHead`, `ClaimedJob`, `JobError` and `JobErrorCode` with `String` IDs ([backend domain-types 5](../../../docs/backend/domain-types.md#5-worker-api-and-jobs); [backend domain 7, jobs](../../../docs/backend/domain.md#jobs)). The `Job`, `Heartbeat`, `ClaimRequest` and `CompleteJob` structs are **sketches of the `domain::jobs` types**: every contract field, with plain types (`String` for IDs and timestamps, `serde_json::Value` for the payload). The real ones carry `V1`, the ID newtypes, `JobSpec` with the tagged payloads and `JobOutcome` ([unions 2](../rust-contract-types/unions.md#2-jobs-kind-and-payload-are-adjacent-tags), [4](../rust-contract-types/unions.md#4-complete-tagged-by-status-result-decoded-by-the-jobs-kind)).

**The settled codes** ([contracts 12, questions 18, 19 and 21](../../../docs/architecture/contracts.md#12-open-questions); [contracts 5, complete](../../../docs/architecture/contracts.md#post-internaljobsidcomplete); [backend worker 14, 16](../../../docs/backend/worker.md#16-decisions-on-the-contracts-open-questions)): a job of another `v`, or a `v: 1` job whose `kind` or payload this build can't read, is completed `failed` with `unsupported_version`, `retryable: false`, without running; a job the worker stopped at its soft deadline fails with `max_runtime_exceeded`, `retryable: true`. A job of a known kind that none of the worker's capabilities claims can't arrive, because the claim filters by them; if it did, it is the adapter that can't do the kind, `model_error` with `retryable: false` (in the worker, `ModelError::Unsupported` from the model). A claim body whose head doesn't decode is a `Rejected` claim, logged, and its lease expires.

## 2. The loop, with its tests

```rust
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

// ---- Shapes: a sketch of the `domain::jobs` types (contracts §5), only what the loop reads ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Perception,
    Vlm,
    Verify,
    Guided,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Perception,
    Vlm,
    Fallback,
}

impl Capability {
    /// The job kinds each capability claims (contracts §5, leases; `domain::jobs`).
    pub const fn kinds(self) -> &'static [JobKind] {
        match self {
            Self::Perception => &[JobKind::Perception],
            Self::Vlm => &[JobKind::Vlm, JobKind::Verify, JobKind::Guided],
            Self::Fallback => &[
                JobKind::Perception,
                JobKind::Vlm,
                JobKind::Verify,
                JobKind::Guided,
            ],
        }
    }
}

/// contracts §5 `Job`, decoded only after `decode_claim` checked its head and `v`.
#[derive(Debug, Clone, Deserialize)]
pub struct Job {
    pub v: u64, // `V1` in `domain`
    pub job_id: String,
    pub kind: JobKind, // an unknown kind doesn't decode, so the job is `Unsupported`
    pub scan_id: String,
    pub run: Option<u32>, // absent for guided jobs
    pub attempt: u32,
    pub lease_token: String,
    pub lease_until: String, // a `Timestamp` in `domain`; never compared with the local clock
    pub max_runtime_s: u32,
    pub payload: serde_json::Value, // `JobSpec` with the kind in `domain`
}

/// What a call about a job needs: its id and lease token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JobLease {
    pub job_id: String,
    pub lease_token: String,
}

impl From<&Job> for JobLease {
    fn from(job: &Job) -> Self {
        Self {
            job_id: job.job_id.clone(),
            lease_token: job.lease_token.clone(),
        }
    }
}

impl From<&JobHead> for JobLease {
    fn from(head: &JobHead) -> Self {
        Self {
            job_id: head.job_id.clone(),
            lease_token: head.lease_token.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Gpu {
    pub name: String,
    pub vram_used_mb: u32,
    pub vram_total_mb: u32,
}

#[derive(Debug, Serialize)]
pub struct Heartbeat<'a> {
    pub worker_id: &'a str,
    pub caps: &'a [Capability],
    pub slots: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu: Option<Gpu>, // left out until something measures VRAM
    pub current_jobs: Vec<JobLease>, // `{ job_id, lease_token }`
    pub versions: &'a BTreeMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct HeartbeatResponse {
    pub cancel_job_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ClaimRequest<'a> {
    pub worker_id: &'a str,
    pub caps: &'a [Capability],
    pub slots: u32,
    pub wait_s: u32,
}

/// `status` with `result` or `error` (`domain::jobs::JobOutcome`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum JobOutcome {
    Done { result: serde_json::Value },
    Failed { error: JobError },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CompleteJob {
    pub v: u64, // `V1` in `domain`
    #[serde(flatten)]
    pub outcome: JobOutcome,
    pub model_versions: BTreeMap<String, String>,
    pub timings_ms: BTreeMap<String, u64>,
}

impl CompleteJob {
    pub fn failed(error: JobError) -> Self {
        Self {
            v: 1,
            outcome: JobOutcome::Failed { error },
            model_versions: BTreeMap::new(),
            timings_ms: BTreeMap::new(),
        }
    }

    pub fn error(&self) -> Option<&JobError> {
        match &self.outcome {
            JobOutcome::Failed { error } => Some(error),
            JobOutcome::Done { .. } => None,
        }
    }
}

// ---- The API client boundary: reqwest in production, a fake in tests ----

#[derive(Debug, thiserror::Error)]
pub enum CallError {
    /// `409 lease_lost`: the job was requeued, expired or cancelled. Drop it.
    #[error("lease lost")]
    LeaseLost,
    /// Connect error, timeout, a body that couldn't be read, an error body with
    /// `retryable: true`, or a `429` or `5xx` without an error body: retry with backoff, and
    /// not before `retry_after` when the API sent `retry_after_s`. The body's `retryable`
    /// decides, not the status (contracts 2; backend worker 6, `classify_body`).
    #[error("worker API unavailable: {reason}")]
    Unavailable {
        reason: String,
        retry_after: Option<Duration>,
    },
    /// Any other answer, a `500` with `retryable: false` included: the same call would fail
    /// again.
    #[error("worker API rejected the call: {0}")]
    Rejected(String),
}

impl CallError {
    /// `Unavailable` without a `retry_after`.
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
            retry_after: None,
        }
    }

    /// The wait the API asked for, if any.
    pub const fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::Unavailable { retry_after, .. } => *retry_after,
            Self::LeaseLost | Self::Rejected(_) => None,
        }
    }
}

pub trait WorkerApi: Send + Sync + 'static {
    fn heartbeat(
        &self,
        hb: &Heartbeat<'_>,
    ) -> impl Future<Output = Result<HeartbeatResponse, CallError>> + Send;
    /// `Ok(None)` is the `204` of a long poll that found nothing. Production decodes the
    /// `200` body with `decode_claim`; a body without a readable head is `Rejected`.
    fn claim(
        &self,
        req: &ClaimRequest<'_>,
    ) -> impl Future<Output = Result<Option<ClaimedJob<Job>>, CallError>> + Send;
    fn events(
        &self,
        lease: &JobLease,
        events: &serde_json::Value,
    ) -> impl Future<Output = Result<(), CallError>> + Send;
    fn complete(
        &self,
        lease: &JobLease,
        body: &CompleteJob,
    ) -> impl Future<Output = Result<(), CallError>> + Send;
}

/// The job was lost while it ran: nothing more may be sent about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseLost;

/// The job's progress sink (`ApiProgress` in the worker).
pub struct Progress<'a, A> {
    api: &'a A,
    lease: &'a JobLease,
    cancel: CancellationToken,
}

impl<A: WorkerApi> Progress<'_, A> {
    /// Posts a stage's events. `lease_lost` cancels the job's token and ends the job; any
    /// other failure is logged and the job goes on, because `complete` repeats every stage
    /// (contracts §5, events).
    pub async fn send(&self, events: &serde_json::Value) -> Result<(), LeaseLost> {
        match self.api.events(self.lease, events).await {
            Ok(()) => Ok(()), // includes a late batch's 204, which the API drops
            Err(CallError::LeaseLost) => {
                self.cancel.cancel();
                Err(LeaseLost)
            }
            Err(e) => {
                let job_id = &self.lease.job_id;
                tracing::warn!(error = %e, %job_id, "events failed; complete repeats them");
                Ok(())
            }
        }
    }
}

/// Runs one decoded job's stages: perception, VLM or fallback (`JobRunner::execute`).
/// No token: the job is cancelled by dropping this future.
pub trait Runner: Send + Sync + 'static {
    fn run<A: WorkerApi>(
        &self,
        api: &A,
        job: &Job,
        progress: &Progress<'_, A>,
    ) -> impl Future<Output = Result<CompleteJob, LeaseLost>> + Send;
}

// ---- Running jobs: what the heartbeat lists, and how it cancels ----

struct Entry {
    lease_token: String,
    cancel: CancellationToken,
}

#[derive(Clone, Default)]
pub struct Running(Arc<Mutex<HashMap<String, Entry>>>);

/// Removes the job from the heartbeat list when the job task ends, however it ends.
pub struct Registered {
    running: Running,
    job_id: String,
}

impl Drop for Registered {
    fn drop(&mut self) {
        self.running.lock().remove(&self.job_id);
    }
}

impl Running {
    fn lock(&self) -> MutexGuard<'_, HashMap<String, Entry>> {
        // Each critical section is one insert, remove or read, so a poisoned map is still consistent.
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn register(&self, lease: &JobLease, cancel: CancellationToken) -> Registered {
        let entry = Entry {
            lease_token: lease.lease_token.clone(),
            cancel,
        };
        self.lock().insert(lease.job_id.clone(), entry);
        Registered {
            running: self.clone(),
            job_id: lease.job_id.clone(),
        }
    }

    pub fn current_jobs(&self) -> Vec<JobLease> {
        self.lock()
            .iter()
            .map(|(id, e)| JobLease {
                job_id: id.clone(),
                lease_token: e.lease_token.clone(),
            })
            .collect()
    }

    pub fn cancel(&self, job_ids: &[String]) {
        let jobs = self.lock();
        for id in job_ids {
            if let Some(entry) = jobs.get(id) {
                entry.cancel.cancel();
            }
        }
    }
}

// ---- Backoff: exponential, capped, with jitter that never goes below half ----

pub struct Backoff {
    attempt: u32,
    base: Duration,
    cap: Duration,
}

impl Backoff {
    pub fn new(base: Duration, cap: Duration) -> Self {
        Self {
            attempt: 0,
            base,
            cap,
        }
    }

    pub fn reset(&mut self) {
        self.attempt = 0;
    }

    pub fn next_delay(&mut self) -> Duration {
        let ceiling = self
            .base
            .saturating_mul(1 << self.attempt.min(16))
            .min(self.cap);
        self.attempt = self.attempt.saturating_add(1);
        rand::random_range(ceiling / 2..=ceiling)
    }
}

/// The wait before retrying after `e`: the backoff's next delay, or the API's
/// `retry_after_s` when it asked for longer (contracts §2).
pub fn retry_delay(e: &CallError, backoff: &mut Backoff) -> Duration {
    let delay = backoff.next_delay();
    e.retry_after().map_or(delay, |after| delay.max(after))
}

// ---- The worker ----

pub const HEARTBEAT_EVERY: Duration = Duration::from_secs(10); // contracts §5
pub const CLAIM_WAIT_S: u32 = 25; // contracts §5: default 25, max 30
/// A `204` sooner than half of `CLAIM_WAIT_S` backs off: a defensive guard, since the API
/// sends `204` only at the poll's deadline or at shutdown (backend worker 5).
pub const EARLY_EMPTY_CLAIM: Duration = Duration::from_millis(12_500);
pub const STOP_BEFORE_MAX_RUNTIME: Duration = Duration::from_secs(15);
/// `complete`'s deadline for a job without a readable `max_runtime_s`: the shortest one.
pub const UNSUPPORTED_COMPLETE_WITHIN: Duration = Duration::from_mins(2);
/// How long running jobs get after SIGTERM (`DRAIN_SECONDS`): under the
/// `stop_grace_period: 30s` of the worker's compose service.
pub const DRAIN: Duration = Duration::from_secs(20);

pub struct WorkerConfig {
    pub worker_id: String,
    pub caps: Vec<Capability>,
    pub slots: u32,
    pub versions: BTreeMap<String, String>,
}

pub struct Worker<A, R> {
    api: Arc<A>,
    runner: Arc<R>,
    cfg: Arc<WorkerConfig>,
    running: Running,
    slots: Arc<Semaphore>,
    tracker: TaskTracker,
    abandon: CancellationToken, // cancelled only when the shutdown drain runs out of time
}

impl<A: WorkerApi, R: Runner> Worker<A, R> {
    pub fn new(api: A, runner: R, cfg: WorkerConfig) -> Self {
        Self {
            api: Arc::new(api),
            runner: Arc::new(runner),
            slots: Arc::new(Semaphore::new(cfg.slots as usize)),
            cfg: Arc::new(cfg),
            running: Running::default(),
            tracker: TaskTracker::new(),
            abandon: CancellationToken::new(),
        }
    }

    /// Every 10 s until `stop`: list the running jobs, cancel what the broker says to cancel.
    pub async fn heartbeat_loop(&self, stop: CancellationToken) {
        let mut every = tokio::time::interval(HEARTBEAT_EVERY);
        every.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                () = stop.cancelled() => return,
                _ = every.tick() => {}
            }
            let hb = Heartbeat {
                worker_id: &self.cfg.worker_id,
                caps: &self.cfg.caps,
                slots: self.cfg.slots,
                gpu: None,
                current_jobs: self.running.current_jobs(),
                versions: &self.cfg.versions,
            };
            // Raced against `stop`, so a heartbeat in flight never holds up the exit.
            let sent = tokio::select! {
                () = stop.cancelled() => return,
                sent = self.api.heartbeat(&hb) => sent,
            };
            match sent {
                Ok(resp) => self.running.cancel(&resp.cancel_job_ids),
                Err(e) => tracing::warn!(error = %e, "heartbeat failed"),
            }
        }
    }

    /// Claims while a slot is free, until `shutdown`.
    pub async fn claim_loop(&self, shutdown: CancellationToken) {
        let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(30));
        loop {
            let slot = tokio::select! {
                biased; // a cancelled worker never starts another claim
                () = shutdown.cancelled() => return,
                slot = self.slots.clone().acquire_owned() => {
                    slot.expect("the slot semaphore is never closed")
                }
            };
            let req = ClaimRequest {
                worker_id: &self.cfg.worker_id,
                caps: &self.cfg.caps,
                slots: self.cfg.slots,
                wait_s: CLAIM_WAIT_S,
            };
            let started = Instant::now();
            let claimed = tokio::select! {
                biased;
                () = shutdown.cancelled() => return, // drops the open long poll
                claimed = self.api.claim(&req) => claimed,
            };
            let wait = match claimed {
                Ok(Some(job)) => {
                    backoff.reset();
                    self.start(job, slot);
                    continue;
                }
                // The long poll waited and found nothing: claim again at once.
                Ok(None) if started.elapsed() >= EARLY_EMPTY_CLAIM => {
                    backoff.reset();
                    continue;
                }
                // An early 204: back off anyway, so no 204 path can ever spin.
                Ok(None) => backoff.next_delay(),
                Err(e) => {
                    let wait = retry_delay(&e, &mut backoff);
                    tracing::warn!(error = %e, wait_ms = wait.as_millis(), "claim failed");
                    wait
                }
            };
            drop(slot); // free while waiting
            tokio::select! {
                biased;
                () = shutdown.cancelled() => return,
                () = tokio::time::sleep(wait) => {}
            }
        }
    }

    fn start(&self, claimed: ClaimedJob<Job>, slot: OwnedSemaphorePermit) {
        let lease = match &claimed {
            ClaimedJob::Job(job) => JobLease::from(job.as_ref()),
            ClaimedJob::Unsupported(head) => JobLease::from(head),
        };
        let cancel = self.abandon.child_token();
        let registered = self.running.register(&lease, cancel.clone());
        let (api, runner, cfg) = (self.api.clone(), self.runner.clone(), self.cfg.clone());
        self.tracker.spawn(async move {
            run_job(&*api, &*runner, &cfg.caps, claimed, lease, &cancel).await;
            drop((registered, slot)); // leaves the heartbeat list, then frees the slot
        });
    }

    /// After `shutdown`: wait up to `bound` for running jobs, then abandon the rest
    /// (nothing is sent; the broker requeues them 30 s after the last heartbeat).
    pub async fn drain(&self, bound: Duration) {
        self.tracker.close();
        if tokio::time::timeout(bound, self.tracker.wait())
            .await
            .is_err()
        {
            tracing::warn!(jobs = self.tracker.len(), "abandoning jobs at shutdown");
            self.abandon.cancel(); // every job task drops its runner and returns
            self.tracker.wait().await;
        }
    }
}

/// The process: heartbeat and claims until SIGTERM or Ctrl-C, then `run_until`'s shutdown
/// (`run_until_sigterm` in the worker).
pub async fn run_until_stopped<A: WorkerApi, R: Runner>(
    worker: Arc<Worker<A, R>>,
) -> std::io::Result<()> {
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
/// return is at most `DRAIN`: the claim loop and the heartbeat drop a call in flight, and the
/// drain cuts every job at its bound, a `complete` call in flight included.
pub async fn run_until<A, R, S>(worker: Arc<Worker<A, R>>, stopped: S) -> std::io::Result<()>
where
    A: WorkerApi,
    R: Runner,
    S: Future<Output = std::io::Result<()>>,
{
    let (shutdown, stop_heartbeat) = (CancellationToken::new(), CancellationToken::new());
    let heartbeat = tokio::spawn({
        let (worker, stop) = (worker.clone(), stop_heartbeat.clone());
        async move { worker.heartbeat_loop(stop).await }
    });
    let claims = tokio::spawn({
        let (worker, shutdown) = (worker.clone(), shutdown.clone());
        async move { worker.claim_loop(shutdown).await }
    });
    stopped.await?;
    tracing::info!("stopping: no new claims, draining");
    shutdown.cancel(); // no new claims; an open long poll is dropped
    if let Err(e) = claims.await {
        tracing::error!(error = %e, "claim loop panicked");
    }
    worker.drain(DRAIN).await;
    stop_heartbeat.cancel();
    if let Err(e) = heartbeat.await {
        tracing::error!(error = %e, "heartbeat panicked");
    }
    Ok(())
}

/// The job task's body. A cancelled token (`cancel_job_ids`, `lease_lost`, the drain running
/// out) returns without sending anything.
async fn run_job<A: WorkerApi, R: Runner>(
    api: &A,
    runner: &R,
    caps: &[Capability],
    claimed: ClaimedJob<Job>,
    lease: JobLease,
    cancel: &CancellationToken,
) {
    let received = Instant::now();
    let job = match claimed {
        ClaimedJob::Job(job) => job,
        ClaimedJob::Unsupported(head) => {
            // Another `v`, or a `v: 1` job this build can't read (contracts §5, leases).
            let body = CompleteJob::failed(JobError::new(
                JobErrorCode::UnsupportedVersion,
                format!(
                    "job v {} (or its kind or payload) is not supported by this worker",
                    head.v
                ),
            ));
            let until = received + UNSUPPORTED_COMPLETE_WITHIN;
            complete_with_retry(api, &lease, &body, until, cancel).await;
            return;
        }
    };
    let max_runtime = Duration::from_secs(u64::from(job.max_runtime_s));
    let hard_deadline = received + max_runtime;
    let body = if caps.iter().any(|c| c.kinds().contains(&job.kind)) {
        let soft_deadline = received + max_runtime.saturating_sub(STOP_BEFORE_MAX_RUNTIME);
        let progress = Progress {
            api,
            lease: &lease,
            cancel: cancel.clone(),
        };
        let run = tokio::time::timeout_at(soft_deadline, runner.run(api, &job, &progress));
        let outcome = tokio::select! {
            () = cancel.cancelled() => return, // drops the runner's future: cancellation by drop
            outcome = run => outcome,
        };
        match outcome {
            Ok(Ok(body)) => body,
            Ok(Err(LeaseLost)) => return,
            Err(_elapsed) => CompleteJob::failed(JobError::new(
                JobErrorCode::MaxRuntimeExceeded,
                format!("stopped before max_runtime_s {}", job.max_runtime_s),
            )),
        }
    } else {
        // The claim filters by `caps`, so this is a bug: the adapter can't do the kind
        // (`ModelError::Unsupported` in the worker), a `model_error` that isn't retried.
        CompleteJob::failed(JobError {
            code: JobErrorCode::ModelError,
            message: format!("this worker does not run {:?} jobs", job.kind),
            retryable: false,
        })
    };
    complete_with_retry(api, &lease, &body, hard_deadline, cancel).await;
}

/// Repeating `complete` with the same lease token is safe: a job that already completed gets
/// 204. Stops at the hard deadline, since the lease can't outlive `max_runtime`.
async fn complete_with_retry<A: WorkerApi>(
    api: &A,
    lease: &JobLease,
    body: &CompleteJob,
    until: Instant,
    cancel: &CancellationToken,
) {
    let mut backoff = Backoff::new(Duration::from_millis(500), Duration::from_secs(8));
    loop {
        // Raced against the token, so an abandoned job's call in flight doesn't outlast the
        // drain. Dropping it is safe: either it landed, or the job is requeued.
        let sent = tokio::select! {
            () = cancel.cancelled() => return,
            sent = api.complete(lease, body) => sent,
        };
        let e = match sent {
            Ok(()) | Err(CallError::LeaseLost) => return,
            Err(CallError::Rejected(e)) => {
                tracing::error!(error = %e, job_id = %lease.job_id, "complete rejected");
                return;
            }
            Err(e) => e,
        };
        let wait = retry_delay(&e, &mut backoff);
        if Instant::now() + wait >= until {
            tracing::warn!(error = %e, job_id = %lease.job_id, "giving up on complete");
            return;
        }
        tokio::select! {
            () = cancel.cancelled() => return,
            () = tokio::time::sleep(wait) => {}
        }
    }
}

// copy of: rust-contract-types/unions.md JobHead
/// The fields needed to fail a claimed job, readable from any version: no
/// `deny_unknown_fields`, nothing but what `complete` needs (`domain::jobs::JobHead`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct JobHead {
    pub v: u64,
    pub job_id: String,
    pub lease_token: String,
}

/// What a worker makes of a `200` claim body (`domain::jobs::ClaimedJob`).
#[derive(Debug, Clone, PartialEq)]
pub enum ClaimedJob<J> {
    /// `v` is 1 and the whole job decoded.
    Job(Box<J>),
    /// Complete it as `failed` with `unsupported_version`, `retryable: false`: another `v`, or
    /// a `v: 1` job whose `kind` or payload this build can't read (contracts 5, leases).
    Unsupported(JobHead),
}

/// A claim body without a numeric `v`, a `job_id` or a `lease_token`: the job can't be
/// completed. Log it and let the lease expire (contracts 5, leases). The message never
/// includes the body.
#[derive(Debug, thiserror::Error)]
#[error("claim body has no readable job head")]
pub struct UnreadableHead(#[source] pub serde_json::Error);

/// Decodes a claim body: the head, then `v`, then the whole job. Only a body without even the
/// head is an error.
pub fn decode_claim<J: serde::de::DeserializeOwned>(
    body: &[u8],
) -> Result<ClaimedJob<J>, UnreadableHead> {
    let head: JobHead = serde_json::from_slice(body).map_err(UnreadableHead)?;
    if head.v != 1 {
        return Ok(ClaimedJob::Unsupported(head));
    }
    match serde_json::from_slice(body) {
        Ok(job) => Ok(ClaimedJob::Job(Box::new(job))),
        Err(error) => {
            tracing::error!(%error, job_id = %head.job_id, "unreadable job");
            Ok(ClaimedJob::Unsupported(head))
        }
    }
}
// end: JobHead

// copy of: rust-contract-types/unions.md JobErrorCode
/// `JobErrorCode` (contracts 5, complete): the codes a worker sends in `CompleteJob.error`,
/// typed `JobErrorCode | string`, so a code this build doesn't know (from a newer worker)
/// still decodes, as `Other`, and is stored as written (`domain::jobs::JobErrorCode`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum JobErrorCode {
    /// A payload or job `v` the worker doesn't support, a kind it doesn't know, or a payload
    /// it can't read; `retryable: false`.
    UnsupportedVersion,
    /// A `label_set_version` or `classifier_labels_version` the worker hasn't loaded;
    /// `retryable: false`.
    UnknownLabelVersion,
    /// Inference failed, or its output failed validation twice; `retryable: true`, except
    /// `false` when the adapter can't do the job's kind at all.
    ModelError,
    /// An image URL failed after retries; `retryable: true`.
    ImageFetchFailed,
    /// The worker stopped the job itself at its soft deadline; `retryable: true`.
    MaxRuntimeExceeded,
    /// Any other code, kept verbatim.
    Other(String),
}

impl JobErrorCode {
    /// The wire string.
    pub fn as_str(&self) -> &str {
        match self {
            Self::UnsupportedVersion => "unsupported_version",
            Self::UnknownLabelVersion => "unknown_label_version",
            Self::ModelError => "model_error",
            Self::ImageFetchFailed => "image_fetch_failed",
            Self::MaxRuntimeExceeded => "max_runtime_exceeded",
            Self::Other(code) => code,
        }
    }

    /// Whether the contract says a failure with this code may be retried; `None` for
    /// `Other`, whose `retryable` flag alone decides. `model_error` says `Some(true)`, the
    /// usual case: the worker sends `false` when the adapter can't do the kind.
    pub const fn retryable(&self) -> Option<bool> {
        match self {
            Self::UnsupportedVersion | Self::UnknownLabelVersion => Some(false),
            Self::ModelError | Self::ImageFetchFailed | Self::MaxRuntimeExceeded => Some(true),
            Self::Other(_) => None,
        }
    }
}

impl serde::Serialize for JobErrorCode {
    /// The bare string, `Other` as written.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for JobErrorCode {
    /// Any string: the five known codes, everything else as `Other`.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = <String as serde::Deserialize>::deserialize(deserializer)?;
        Ok(match code.as_str() {
            "unsupported_version" => Self::UnsupportedVersion,
            "unknown_label_version" => Self::UnknownLabelVersion,
            "model_error" => Self::ModelError,
            "image_fetch_failed" => Self::ImageFetchFailed,
            "max_runtime_exceeded" => Self::MaxRuntimeExceeded,
            _ => Self::Other(code),
        })
    }
}

/// `CompleteJob.error` (contracts 5): job errors travel here, never as HTTP status codes
/// (`domain::jobs::JobError`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct JobError {
    /// A `JobErrorCode`, or another string.
    pub code: JobErrorCode,
    /// Detail for the log: the error and its `source()` chain.
    pub message: String,
    /// Counts toward `attempts` when true.
    pub retryable: bool,
}

impl JobError {
    /// A failure with the contract's `retryable` for `code`. `Other` is never sent by this
    /// build, so it gets `false`.
    pub fn new(code: JobErrorCode, message: String) -> Self {
        let retryable = code.retryable().unwrap_or(false);
        Self {
            code,
            message,
            retryable,
        }
    }
}
// end: JobErrorCode

#[cfg(test)]
mod tests {
    use std::future::ready;

    use super::*;

    /// What a claim answers once the queued jobs are gone.
    #[derive(Default, Clone, Copy)]
    enum Empty {
        #[default]
        LongPoll, // 204 after wait_s
        AtOnce, // 204 at once
        Fails,  // connection refused, every claim
    }

    /// What every `events` call answers.
    #[derive(Default, Clone, Copy)]
    enum Events {
        #[default]
        Ok,
        LeaseLost,
        Unavailable,
    }

    #[derive(Default)]
    struct Fake {
        empty: Empty,
        job_v: u64,
        job_kind: &'static str,
        events: Events,
        complete_failures: Mutex<u32>,
        complete_retry_after: Option<Duration>,
        complete_calls: Mutex<Vec<Instant>>,
        claims: Mutex<u32>,
        jobs_left: Mutex<u32>,
        completes: Mutex<Vec<(JobLease, CompleteJob)>>,
        heartbeats: Mutex<Vec<Vec<JobLease>>>,
        cancel_job_ids: Vec<String>,
        hang_heartbeats: bool, // every heartbeat call stays in flight
        hang_completes: bool,  // every complete call stays in flight
        claim_retry_after: Option<Duration>, // with `Empty::Fails`: a 503 with retry_after_s
        complete_answers_lost: Mutex<u32>, // committed, but the answer times out
        complete_rejected: bool, // a 500 internal, retryable: false
    }

    impl Fake {
        /// The `200` body of claim number `n`, decoded as production decodes it.
        fn claim_body(&self, n: u32) -> ClaimedJob<Job> {
            let body = serde_json::json!({
                "v": self.job_v, "job_id": format!("job{n}"), "kind": self.job_kind,
                "scan_id": "scan1", "run": 1, "attempt": 1, "lease_token": format!("lease{n}"),
                "lease_until": "2026-10-18T14:05:22Z", "max_runtime_s": 120, "payload": {}
            });
            decode_claim(body.to_string().as_bytes()).expect("the fake sends a readable head")
        }
    }

    impl WorkerApi for Fake {
        fn heartbeat(
            &self,
            hb: &Heartbeat<'_>,
        ) -> impl Future<Output = Result<HeartbeatResponse, CallError>> {
            self.heartbeats
                .lock()
                .unwrap()
                .push(hb.current_jobs.clone());
            let (cancel_job_ids, hang) = (self.cancel_job_ids.clone(), self.hang_heartbeats);
            async move {
                if hang {
                    std::future::pending::<()>().await;
                }
                Ok(HeartbeatResponse { cancel_job_ids })
            }
        }

        fn claim(
            &self,
            _req: &ClaimRequest<'_>,
        ) -> impl Future<Output = Result<Option<ClaimedJob<Job>>, CallError>> {
            let mut claims = self.claims.lock().unwrap();
            *claims += 1;
            let mut jobs_left = self.jobs_left.lock().unwrap();
            let reply = if matches!(self.empty, Empty::Fails) {
                Some(Err(CallError::Unavailable {
                    reason: "connection refused".into(),
                    retry_after: self.claim_retry_after,
                }))
            } else if *jobs_left > 0 {
                *jobs_left -= 1;
                Some(Ok(Some(self.claim_body(*claims))))
            } else if matches!(self.empty, Empty::AtOnce) {
                Some(Ok(None)) // a 204 that didn't wait
            } else {
                None
            };
            async move {
                if let Some(reply) = reply {
                    reply
                } else {
                    // nothing queued: the long poll ends with 204 after wait_s
                    tokio::time::sleep(Duration::from_secs(25)).await;
                    Ok(None)
                }
            }
        }

        fn events(
            &self,
            _lease: &JobLease,
            _events: &serde_json::Value,
        ) -> impl Future<Output = Result<(), CallError>> {
            ready(match self.events {
                Events::Ok => Ok(()),
                Events::LeaseLost => Err(CallError::LeaseLost),
                Events::Unavailable => Err(CallError::unavailable("timed out")),
            })
        }

        fn complete(
            &self,
            lease: &JobLease,
            body: &CompleteJob,
        ) -> impl Future<Output = Result<(), CallError>> {
            self.complete_calls.lock().unwrap().push(Instant::now());
            let mut left = self.complete_failures.lock().unwrap();
            let mut completes = self.completes.lock().unwrap();
            let reply = if self.complete_rejected {
                Err(CallError::Rejected("500 internal".into()))
            } else if *left > 0 {
                *left -= 1;
                Err(CallError::Unavailable {
                    reason: "503".into(),
                    retry_after: self.complete_retry_after,
                })
            } else if self.hang_completes || completes.iter().any(|(l, _)| l == lease) {
                // never answered (the future below doesn't finish), or a repeat for a job
                // already completed: 204, nothing changes
                Ok(())
            } else {
                completes.push((lease.clone(), body.clone()));
                let mut lost = self.complete_answers_lost.lock().unwrap();
                if *lost > 0 {
                    *lost -= 1;
                    Err(CallError::unavailable("timed out")) // committed; the answer was lost
                } else {
                    Ok(())
                }
            };
            let hang = self.hang_completes;
            async move {
                if hang {
                    std::future::pending::<()>().await;
                }
                reply
            }
        }
    }

    type Log = Arc<Mutex<Vec<&'static str>>>;

    /// Models ort.md's `await_run`: a run that never finishes by itself and is terminated
    /// only when its future is dropped. `sends_event` posts one `partial_result` first.
    #[derive(Default)]
    struct Inferring {
        sends_event: bool,
        terminated: Log,
    }

    struct TerminateOnDrop(Log);

    impl Drop for TerminateOnDrop {
        fn drop(&mut self) {
            self.0.lock().unwrap().push("drop");
        }
    }

    impl Runner for Inferring {
        async fn run<A: WorkerApi>(
            &self,
            _api: &A,
            _job: &Job,
            progress: &Progress<'_, A>,
        ) -> Result<CompleteJob, LeaseLost> {
            let _guard = TerminateOnDrop(self.terminated.clone());
            if self.sends_event {
                // Ignores the result: the sink's own cancel must end the job.
                let _ = progress
                    .send(&serde_json::json!({ "v": 1, "events": [] }))
                    .await;
            }
            std::future::pending().await
        }
    }

    struct SendsEvent; // sends one partial_result, then finishes
    impl Runner for SendsEvent {
        async fn run<A: WorkerApi>(
            &self,
            _api: &A,
            _job: &Job,
            progress: &Progress<'_, A>,
        ) -> Result<CompleteJob, LeaseLost> {
            progress
                .send(&serde_json::json!({ "v": 1, "events": [] }))
                .await?;
            Ok(CompleteJob {
                v: 1,
                outcome: JobOutcome::Done {
                    result: serde_json::json!({ "regions": [] }),
                },
                model_versions: BTreeMap::new(),
                timings_ms: BTreeMap::new(),
            })
        }
    }

    /// A broker that hands out `jobs` perception jobs of `v: 1`, then has nothing queued.
    fn fake(jobs: u32) -> Fake {
        Fake {
            job_v: 1,
            job_kind: "perception",
            jobs_left: Mutex::new(jobs),
            ..Fake::default()
        }
    }

    fn worker<R: Runner>(fake: Fake, runner: R, slots: u32) -> Arc<Worker<Fake, R>> {
        let cfg = WorkerConfig {
            worker_id: "rig-3070-perception-1".into(),
            caps: vec![Capability::Perception],
            slots,
            versions: BTreeMap::new(),
        };
        Arc::new(Worker::new(fake, runner, cfg))
    }

    fn spawn_claims<R: Runner>(
        w: &Arc<Worker<Fake, R>>,
        shutdown: &CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        let (w, shutdown) = (w.clone(), shutdown.clone());
        tokio::spawn(async move { w.claim_loop(shutdown).await })
    }

    fn spawn_heartbeat<R: Runner>(w: &Arc<Worker<Fake, R>>, stop: &CancellationToken) {
        let (w, stop) = (w.clone(), stop.clone());
        tokio::spawn(async move { w.heartbeat_loop(stop).await });
    }

    /// Stops claiming and waits for the jobs, as `run_until` does.
    async fn stop<R: Runner>(
        w: &Worker<Fake, R>,
        shutdown: &CancellationToken,
        claims: tokio::task::JoinHandle<()>,
    ) {
        shutdown.cancel();
        claims.await.unwrap();
        w.drain(DRAIN).await;
    }

    /// The one `complete` sent, after the claim loop and the jobs have stopped.
    async fn only_complete<R: Runner>(fake: Fake, runner: R) -> (JobLease, CompleteJob) {
        let w = worker(fake, runner, 1);
        let shutdown = CancellationToken::new();
        let claims = spawn_claims(&w, &shutdown);
        tokio::time::sleep(Duration::from_millis(1)).await;
        stop(&w, &shutdown, claims).await;
        let mut completes = w.api.completes.lock().unwrap();
        assert_eq!(completes.len(), 1);
        completes.remove(0)
    }

    #[tokio::test(start_paused = true)]
    async fn holds_at_most_slots_jobs() {
        let w = worker(fake(10), Inferring::default(), 2);
        let shutdown = CancellationToken::new();
        let _claim_loop = spawn_claims(&w, &shutdown);
        tokio::time::sleep(Duration::from_secs(60)).await;
        assert_eq!(*w.api.claims.lock().unwrap(), 2); // the third claim waits for a slot
        assert_eq!(w.running.current_jobs().len(), 2);
        assert_eq!(w.slots.available_permits(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn lease_lost_from_a_progress_event_drops_the_job() {
        let fake = Fake {
            events: Events::LeaseLost,
            ..fake(2)
        };
        let runner = Inferring {
            sends_event: true,
            ..Inferring::default()
        };
        let w = worker(fake, runner, 1);
        let shutdown = CancellationToken::new();
        let claims = spawn_claims(&w, &shutdown);
        tokio::time::sleep(Duration::from_secs(1)).await;
        // One slot, yet both jobs were claimed: the sink cancelled the first job although its
        // runner ignored the error, so its task ended and freed the slot.
        assert_eq!(*w.api.claims.lock().unwrap(), 3); // the third is a long poll still waiting
        assert_eq!(*w.runner.terminated.lock().unwrap(), ["drop", "drop"]);
        stop(&w, &shutdown, claims).await;
        assert_eq!(w.api.completes.lock().unwrap().len(), 0); // no complete after lease_lost
        assert_eq!(w.running.current_jobs(), vec![]);
        assert_eq!(w.slots.available_permits(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn another_events_failure_does_not_stop_the_job() {
        let fake = Fake {
            events: Events::Unavailable,
            ..fake(1)
        };
        let (_, done) = only_complete(fake, SendsEvent).await;
        assert!(matches!(done.outcome, JobOutcome::Done { .. })); // complete repeats the events
    }

    #[tokio::test(start_paused = true)]
    async fn heartbeat_lists_jobs_and_cancels_the_listed_ones() {
        let fake = Fake {
            cancel_job_ids: vec!["job1".into()],
            ..fake(1)
        };
        let w = worker(fake, Inferring::default(), 1);
        let lease = JobLease {
            job_id: "job1".into(),
            lease_token: "lease1".into(),
        };
        let token = CancellationToken::new();
        let _registered = w.running.register(&lease, token.clone());
        let stop_heartbeat = CancellationToken::new();
        spawn_heartbeat(&w, &stop_heartbeat);
        tokio::time::sleep(Duration::from_millis(1)).await; // the first tick is immediate
        assert!(token.is_cancelled());
        let sent = w.api.heartbeats.lock().unwrap()[0].clone();
        assert_eq!(sent, vec![lease]);
        stop_heartbeat.cancel();
    }

    #[tokio::test(start_paused = true)]
    async fn a_cancelled_job_drops_its_run() {
        let fake = Fake {
            cancel_job_ids: vec!["job1".into()],
            ..fake(1)
        };
        let w = worker(fake, Inferring::default(), 1);
        let (shutdown, stop_heartbeat) = (CancellationToken::new(), CancellationToken::new());
        let claims = spawn_claims(&w, &shutdown);
        tokio::time::sleep(Duration::from_secs(1)).await; // job1 is running
        spawn_heartbeat(&w, &stop_heartbeat); // its first beat is immediate
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert_eq!(*w.runner.terminated.lock().unwrap(), ["drop"]); // at once, no grace
        assert_eq!(w.running.current_jobs(), vec![]);
        stop(&w, &shutdown, claims).await;
        stop_heartbeat.cancel();
        assert_eq!(w.api.completes.lock().unwrap().len(), 0); // nothing sent after a cancel
        assert_eq!(w.slots.available_permits(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn claim_errors_back_off_instead_of_spinning() {
        let fake = Fake {
            empty: Empty::Fails,
            ..fake(1)
        };
        let w = worker(fake, Inferring::default(), 1);
        let shutdown = CancellationToken::new();
        let _claim_loop = spawn_claims(&w, &shutdown);
        tokio::time::sleep(Duration::from_secs(60)).await;
        // Delays are at least 0.5, 1, 2, 4, 8, 15, 15 s and at most 1, 2, 4, 8, 16, 30 s.
        let n = *w.api.claims.lock().unwrap();
        assert!((6..=8).contains(&n), "{n} claims in 60 s");
        assert_eq!(w.slots.available_permits(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn an_early_204_backs_off_instead_of_spinning() {
        let fake = Fake {
            empty: Empty::AtOnce,
            ..fake(0)
        };
        let w = worker(fake, Inferring::default(), 1);
        let shutdown = CancellationToken::new();
        let _claim_loop = spawn_claims(&w, &shutdown);
        tokio::time::sleep(Duration::from_secs(60)).await;
        let n = *w.api.claims.lock().unwrap();
        assert!((6..=8).contains(&n), "{n} claims in 60 s"); // the same schedule as errors
    }

    #[tokio::test(start_paused = true)]
    async fn another_v_and_an_unknown_kind_fail_with_unsupported_version() {
        for (v, kind) in [(2, "perception"), (1, "soil_probe")] {
            let fake = Fake {
                job_v: v,
                job_kind: kind,
                ..fake(1)
            };
            let runner = Inferring::default();
            let terminated = runner.terminated.clone();
            let (lease, failed) = only_complete(fake, runner).await;
            assert_eq!(lease.lease_token, "lease1"); // read from the head
            let error = failed.error().expect("failed");
            assert_eq!(error.code, JobErrorCode::UnsupportedVersion);
            assert!(!error.retryable);
            assert!(
                terminated.lock().unwrap().is_empty(),
                "the runner never started"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_kind_the_worker_does_not_claim_is_a_model_error_not_retried() {
        let fake = Fake {
            job_kind: "vlm", // not a perception worker's kind
            ..fake(1)
        };
        let (_, failed) = only_complete(fake, Inferring::default()).await;
        let error = failed.error().expect("failed");
        assert_eq!(
            (&error.code, error.retryable),
            (&JobErrorCode::ModelError, false)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn complete_is_retried_after_a_timeout() {
        let fake = Fake {
            complete_failures: Mutex::new(1),
            ..fake(1)
        };
        let (_, done) = only_complete(fake, SendsEvent).await;
        assert!(matches!(done.outcome, JobOutcome::Done { .. }));
    }

    #[tokio::test(start_paused = true)]
    async fn a_repeat_complete_answered_204_is_success() {
        let fake = Fake {
            complete_answers_lost: Mutex::new(1),
            ..fake(1)
        };
        let w = worker(fake, SendsEvent, 1);
        let shutdown = CancellationToken::new();
        let claims = spawn_claims(&w, &shutdown);
        tokio::time::sleep(Duration::from_secs(5)).await;
        // The first call landed but its answer timed out; the repeat got 204 and ended the job.
        assert_eq!(w.api.complete_calls.lock().unwrap().len(), 2);
        assert_eq!(w.api.completes.lock().unwrap().len(), 1);
        assert_eq!(w.running.current_jobs(), vec![]);
        stop(&w, &shutdown, claims).await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_rejected_complete_is_not_retried() {
        let fake = Fake {
            complete_rejected: true,
            ..fake(1)
        };
        let w = worker(fake, SendsEvent, 1);
        let shutdown = CancellationToken::new();
        let claims = spawn_claims(&w, &shutdown);
        tokio::time::sleep(Duration::from_secs(60)).await;
        assert_eq!(w.api.complete_calls.lock().unwrap().len(), 1); // logged, never repeated
        assert_eq!(w.running.current_jobs(), vec![]); // the lease expires; the broker requeues
        stop(&w, &shutdown, claims).await;
    }

    #[tokio::test(start_paused = true)]
    async fn claim_errors_wait_for_retry_after() {
        let fake = Fake {
            empty: Empty::Fails,
            claim_retry_after: Some(Duration::from_secs(20)),
            ..fake(0)
        };
        let w = worker(fake, Inferring::default(), 1);
        let shutdown = CancellationToken::new();
        let _claim_loop = spawn_claims(&w, &shutdown);
        tokio::time::sleep(Duration::from_secs(45)).await;
        assert_eq!(*w.api.claims.lock().unwrap(), 3); // at 0, 20 and 40 s
    }

    #[tokio::test(start_paused = true)]
    async fn complete_waits_for_retry_after() {
        let fake = Fake {
            complete_failures: Mutex::new(1),
            complete_retry_after: Some(Duration::from_secs(7)), // a 503 with retry_after_s 7
            ..fake(1)
        };
        let w = worker(fake, SendsEvent, 1);
        let shutdown = CancellationToken::new();
        let claims = spawn_claims(&w, &shutdown);
        tokio::time::sleep(Duration::from_millis(1)).await;
        stop(&w, &shutdown, claims).await;
        let calls = w.api.complete_calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1] - calls[0], Duration::from_secs(7)); // not the 0.25–0.5 s backoff
    }

    #[tokio::test(start_paused = true)]
    async fn a_hung_model_is_stopped_before_max_runtime() {
        let w = worker(fake(1), Inferring::default(), 1);
        let shutdown = CancellationToken::new();
        let _claim_loop = spawn_claims(&w, &shutdown);
        // max_runtime_s 120 minus 15 s: the soft deadline is 105 s after receipt.
        tokio::time::sleep(Duration::from_secs(104)).await;
        assert!(
            w.api.completes.lock().unwrap().is_empty(),
            "not yet at 104 s"
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
        let completes = w.api.completes.lock().unwrap().clone();
        let error = completes[0].1.error().expect("failed");
        assert_eq!(error.code, JobErrorCode::MaxRuntimeExceeded);
        assert!(error.retryable);
        // the deadline dropped the runner's future, and its drop guard terminated the run
        assert_eq!(*w.runner.terminated.lock().unwrap(), ["drop"]);
    }

    #[tokio::test(start_paused = true)]
    async fn the_whole_shutdown_ends_within_the_drain_bound() {
        let fake = Fake {
            hang_heartbeats: true,
            hang_completes: true,
            ..fake(1)
        };
        let w = worker(fake, SendsEvent, 1);
        let started = Instant::now();
        // The signal after 1 s: the job is done and its `complete` hangs, as does a heartbeat.
        let signal = async {
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok(())
        };
        run_until(w.clone(), signal).await.unwrap();
        assert_eq!(started.elapsed(), Duration::from_secs(1) + DRAIN); // nothing after the drain
        assert_eq!(w.api.complete_calls.lock().unwrap().len(), 1);
        assert_eq!(w.slots.available_permits(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_abandons_jobs_after_the_drain_bound() {
        let w = worker(fake(1), Inferring::default(), 1);
        let shutdown = CancellationToken::new();
        let claims = spawn_claims(&w, &shutdown);
        tokio::time::sleep(Duration::from_secs(1)).await;
        let started = Instant::now();
        stop(&w, &shutdown, claims).await;
        assert_eq!(started.elapsed(), DRAIN); // the bound, then the runner is dropped at once
        assert_eq!(*w.runner.terminated.lock().unwrap(), ["drop"]);
        assert_eq!(w.api.completes.lock().unwrap().len(), 0); // abandoned: the broker requeues it
        assert_eq!(w.slots.available_permits(), 1);
    }
}
```

## 3. What the tests show

All run on paused time (`start_paused = true`): tokio advances the clock whenever every task is idle, so 106 virtual seconds take microseconds. The fake broker hands out a set number of jobs, each decoded by `decode_claim` from a JSON body as production decodes it, then answers each claim like an empty long poll (25 s, then `None`); a fake that answered at once forever would never let the clock advance. [tokio::test: start_paused](https://docs.rs/tokio/1.53.3/tokio/attr.test.html#configure-the-runtime-to-start-with-time-paused), [time::pause](https://docs.rs/tokio/1.53.3/tokio/time/fn.pause.html)

| Test | Shows |
|---|---|
| `holds_at_most_slots_jobs` | With 2 slots and models that never return, exactly 2 claims are made in 60 s |
| `lease_lost_from_a_progress_event_drops_the_job` | After `409 lease_lost` from `events`, the sink cancels the job's token, so the job ends although its runner ignored the error: no `complete`, the runner dropped, the job off the heartbeat list, and its slot lets the next job be claimed |
| `another_events_failure_does_not_stop_the_job` | An `events` call that fails for another reason is logged; the job runs on and lands its `complete` |
| `heartbeat_lists_jobs_and_cancels_the_listed_ones` | The heartbeat sends `{ job_id, lease_token }`, and `cancel_job_ids` cancels that job's token |
| `a_cancelled_job_drops_its_run` | A job named in `cancel_job_ids` has its runner dropped at once (its drop guard terminates the run), sends nothing, and frees its slot |
| `claim_errors_back_off_instead_of_spinning` | An unreachable API gets 6 to 8 claims in 60 s, and the slot is free while waiting |
| `an_early_204_backs_off_instead_of_spinning` | A `204` that comes back at once is retried on the same backoff, not in a tight loop |
| `another_v_and_an_unknown_kind_fail_with_unsupported_version` | `v: 2`, and a `v: 1` job of a kind this build doesn't know, are completed as `failed`, `unsupported_version`, `retryable: false`, with the head's lease token, and the runner never starts |
| `a_kind_the_worker_does_not_claim_is_a_model_error_not_retried` | A `vlm` job on a perception worker is completed as `failed`, `model_error`, `retryable: false`, without running |
| `claim_errors_wait_for_retry_after` | After claim errors with `retry_after_s: 20`, claims go out at 0, 20 and 40 s |
| `complete_is_retried_after_a_timeout` | A `complete` that failed with `Unavailable` is sent again and lands once |
| `a_repeat_complete_answered_204_is_success` | A `complete` that landed but whose answer timed out is repeated; the repeat's `204` ends the job |
| `a_rejected_complete_is_not_retried` | A `complete` answered with `retryable: false` (`500 internal`) is logged and never repeated; the job leaves the heartbeat, so its lease expires |
| `complete_waits_for_retry_after` | After a failure that carried `retry_after_s: 7`, the next `complete` goes out 7 s later, not after the shorter backoff |
| `a_hung_model_is_stopped_before_max_runtime` | A job with `max_runtime_s: 120` has no `complete` at 104 s and one at 106 s (`failed`, `max_runtime_exceeded`, `retryable: true`); the deadline dropped the run, and the drop guard terminated it |
| `the_whole_shutdown_ends_within_the_drain_bound` | With a `complete` and a heartbeat call that never return, the worker exits exactly 20 s after the signal: both calls are dropped |
| `shutdown_abandons_jobs_after_the_drain_bound` | The drain waits exactly its 20 s bound, then drops the runner; no `complete`, the slot is free |

The worker's own loop tests ([backend worker 15](../../../docs/backend/worker.md#15-testing)) overlap with these but are not the same list. Only here: `another_events_failure_does_not_stop_the_job`, `a_cancelled_job_drops_its_run`, `an_early_204_backs_off_instead_of_spinning` (worker 5 has no such guard), `a_kind_the_worker_does_not_claim_is_a_model_error_not_retried`, `a_repeat_complete_answered_204_is_success`, `a_rejected_complete_is_not_retried`, and the unknown-kind half of the `unsupported_version` test (the worker's is `another_version_fails_with_unsupported_version`). Only in the worker: `a_finished_job_completes_with_versions_and_timings` and `an_image_that_cannot_be_fetched_fails_the_job_retryably`, which need its model and image fetch; `a_complete_in_flight_does_not_hold_up_the_drain` and `stopping_the_heartbeat_drops_a_call_in_flight`, the two halves of what `the_whole_shutdown_ends_within_the_drain_bound` shows here at once; the `classify_body` tests, which are reqwest.md's `check` tests here; and its VLM adapter tests.

## 4. Sources

| Rule | Source |
|---|---|
| Heartbeat every 10 s with `current_jobs`; requeue after 30 s without one; lease 2 min, never past `max_runtime` | [contracts 5, leases](../../../docs/architecture/contracts.md#leases-runs-and-attempts), [heartbeat](../../../docs/architecture/contracts.md#post-internalworkersheartbeat) |
| Claim long poll, `wait_s` 25 (max 30), `204`, `slots`; the claim filters by the worker's kinds | [contracts 5, claim](../../../docs/architecture/contracts.md#post-internaljobsclaim), [leases](../../../docs/architecture/contracts.md#leases-runs-and-attempts) |
| `409 lease_lost` for any call about a requeued, expired or cancelled job; a repeated `complete` returns `204` | [contracts 5, leases](../../../docs/architecture/contracts.md#leases-runs-and-attempts) |
| Late `events` after `complete` with the same token: `204`, dropped | [backend api 15](../../../docs/backend/api.md#15-the-job-broker-and-the-worker-api) |
| `unsupported_version` for another `v` or an unreadable `v: 1` job, not retryable; `max_runtime_exceeded`, retryable | [contracts 1, versioning](../../../docs/architecture/contracts.md#versioning), [contracts 5, complete](../../../docs/architecture/contracts.md#post-internaljobsidcomplete), [contracts 12, questions 18, 19, 21](../../../docs/architecture/contracts.md#12-open-questions) |
| Cancellation by drop; the progress sink cancels on `lease_lost` | [backend domain 9, models](../../../docs/backend/domain.md#models), [backend worker 7](../../../docs/backend/worker.md#7-cancellation) |
| No lease hand-back at shutdown | [contracts 5, leases](../../../docs/architecture/contracts.md#leases-runs-and-attempts), [contracts 12, question 20](../../../docs/architecture/contracts.md#12-open-questions) |
| `MissedTickBehavior::Delay`; the first tick is immediate | [MissedTickBehavior::Delay](https://docs.rs/tokio/1.53.3/tokio/time/enum.MissedTickBehavior.html#variant.Delay), [interval](https://docs.rs/tokio/1.53.3/tokio/time/fn.interval.html) |
| `acquire_owned` gives a permit that can move into a task | [Semaphore::acquire_owned](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Semaphore.html#method.acquire_owned) |
| `child_token` is cancelled with its parent, and can be cancelled alone | [CancellationToken::child_token](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html#method.child_token) |
| `TaskTracker::wait` ends only once the tracker is closed and empty | [TaskTracker::wait](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html#method.wait) |
| `biased;` polls the branches in order | [select!: fairness](https://docs.rs/tokio/1.53.3/tokio/macro.select.html#fairness) |
| `timeout_at` against a `tokio::time::Instant` deadline | [timeout_at](https://docs.rs/tokio/1.53.3/tokio/time/fn.timeout_at.html) |
| A poisoned `Mutex` can still be used through `PoisonError::into_inner` | [PoisonError::into_inner](https://doc.rust-lang.org/std/sync/struct.PoisonError.html#method.into_inner) |
| `random_range` over a `Duration` range | [rand::random_range](https://docs.rs/rand/0.10.3/rand/fn.random_range.html), [UniformDuration](https://docs.rs/rand/0.10.3/rand/distr/uniform/struct.UniformDuration.html) |
| SIGTERM and Ctrl-C | [SignalKind::terminate](https://docs.rs/tokio/1.53.3/tokio/signal/unix/struct.SignalKind.html#method.terminate), [ctrl_c](https://docs.rs/tokio/1.53.3/tokio/signal/fn.ctrl_c.html), [rust-async, signals](../rust-async/shutdown.md#2-signals) |
| `retry_after_s` with `429` and `503`; `retryable` on every error body | [contracts 2](../../../docs/architecture/contracts.md#2-errors) |
| The drain budget against Compose's `stop_grace_period` (10 s by default, then SIGKILL) | [Compose file: stop_grace_period](https://docs.docker.com/reference/compose-file/services/#stop_grace_period), [backend worker 12](../../../docs/backend/worker.md#12-deployment), [rust-async, shutdown 1](../rust-async/shutdown.md#1-the-sequence) |
| Dropping a future runs the destructors of what it holds, which is how the drop guard terminates a run | [Reference: destructors](https://doc.rust-lang.org/reference/destructors.html) |
