# The worker loop

Part of [rust-workers](SKILL.md). Versions: tokio 1.53.3, tokio-util 0.7.20, rand 0.10.3, serde_json 1.0.154. The rules are in [SKILL.md, section 2](SKILL.md#2-the-loop); the behaviour they implement is [contracts 5](../../../docs/architecture/contracts.md#5-worker-api-and-jobs), which wins over this file.

## Contents

1. [How the parts fit](#1-how-the-parts-fit)
2. [The loop, with its tests](#2-the-loop-with-its-tests)
3. [What the tests show](#3-what-the-tests-show)
4. [Sources](#4-sources)

## 1. How the parts fit

- **`WorkerApi`** is the boundary to the API: `heartbeat`, `claim`, `events`, `complete`. Production implements it with reqwest ([reqwest.md](reqwest.md)); the tests implement it with a fake. Its methods return `impl Future + Send` so job tasks can be spawned on the multi-thread runtime. [rust-async, traits](../rust-async/traits.md#3-what-the-project-does-with-its-boundary-traits)
- **`CallError`** is what a call can end in: `LeaseLost` (`409 lease_lost`), `Unavailable` (connect error, timeout, `429`, or a `5xx` that is retryable), carrying the API's `retry_after_s` when it sent one, and `Rejected` (anything else). [contracts 2](../../../docs/architecture/contracts.md#2-errors), [reqwest.md, reading errors](reqwest.md#4-reading-errors)
- **`Runner`** runs one job's stages (perception, VLM or fallback). It gets the job's `CancellationToken` and returns the `complete` body, or `Abandon` when the job must end without one: `LeaseLost` when a call got `409 lease_lost`, `Cancelled` when it stopped on its token. The perception runner's inference wait terminates the ONNX run on a cancel and when its future is dropped ([ort.md, the inference thread](ort.md#4-the-inference-thread)). [contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)
- **`send_events`** is how a runner posts a stage's events: `lease_lost` ends the job, any other failure is logged and the job goes on (rule 8). [contracts 5, events](../../../docs/architecture/contracts.md#post-internaljobsidevents)
- **`Running`** is the registry of held jobs: `job_id → (lease_token, CancellationToken)`. The heartbeat reads it for `current_jobs` and cancels the tokens listed in `cancel_job_ids`. A job task holds a `Registered` guard whose `Drop` removes the entry, so a job leaves the list however its task ends. The lock is a `std::sync::Mutex`, never held across an `.await`. [tokio Mutex: which kind of mutex](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Mutex.html#which-kind-of-mutex-should-you-use)
- **`Worker::claim_loop`** takes a slot permit, then claims. A job goes to `start`, which registers it and spawns `run_job` on the `TaskTracker` with the permit moved in. A `204` after the full long poll loops at once; an early `204` or an error backs off. [contracts 5, claim](../../../docs/architecture/contracts.md#post-internaljobsclaim)
- **`run_job`** checks `v` and `kind`, runs the `Runner` under the job's soft deadline, and sends `complete` with retries. On a cancel the runner gets `CANCEL_GRACE` (5 s) to stop by itself; after that its future is dropped. A cancelled job or an `Abandon` returns without sending anything. [contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)
- **`Worker::drain`** closes the tracker and waits for the jobs up to a bound, then cancels `abandon`, the parent of every job token. [TaskTracker::wait](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html#method.wait)
- **`run_until_stopped`** wires it together: the heartbeat on its own token, the claim loop on `shutdown` until SIGTERM or Ctrl-C, then drain, then stop the heartbeat. [rust-async, shutdown](../rust-async/shutdown.md#6-worker)

The `Job`, `Heartbeat` and `CompleteJob` structs below are **sketches of the `domain` types**: every contract field, with plain types (`String` for IDs and timestamps, `serde_json::Value` for the payload). The real types, with `V1`, ID newtypes and the tagged payloads, are in [rust-contract-types](../rust-contract-types/unions.md#2-jobs-kind-and-payload-are-adjacent-tags).

**Not in the contracts yet:** the loop fails a job it stopped at its soft deadline with the code `max_runtime_exceeded` (`retryable: true`), and a job of a kind its capabilities don't claim with the placeholder code `unsupported_kind` (`retryable: false`, a bug on one side) ([contracts 12, questions 18 and 21](../../../docs/architecture/contracts.md#12-open-questions)).

The fake broker hands the loop decoded `Job`s. In production the claim body is decoded head first with `decode_claim` ([rust-contract-types, the worker checks v first](../rust-contract-types/unions.md#3-the-worker-checks-v-first)): only a decoded `v != 1` reaches `run_job` as `unsupported_version`; a body whose head doesn't decode is a `Rejected` claim, logged, and its lease expires.

**Not in the contracts yet:** `unsupported_version` is sent with `retryable: false`: the same worker can't succeed on a retry ([contracts 12, question 19](../../../docs/architecture/contracts.md#12-open-questions)).

## 2. The loop, with its tests

```rust
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

// ---- Shapes: a sketch of the `domain` types (contracts §5), only what the loop reads ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Perception,
    Vlm,
    Fallback,
}

impl Capability {
    /// The job kinds each capability claims (contracts §5, leases).
    pub const fn kinds(self) -> &'static [&'static str] {
        match self {
            Self::Perception => &["perception"],
            Self::Vlm => &["vlm", "verify", "guided"],
            Self::Fallback => &["perception", "vlm", "verify", "guided"],
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Job {
    pub v: u64, // a plain number, so a job with another `v` still parses and can be failed
    pub job_id: String,
    pub kind: String, // a plain string, so an unknown kind still parses and can be failed
    pub scan_id: String,
    pub run: Option<u32>, // absent for guided jobs
    pub attempt: u32,
    pub lease_token: String,
    pub lease_until: String, // a `Timestamp` in `domain`; never compared with the local clock
    pub max_runtime_s: u64,
    pub payload: serde_json::Value, // parsed by kind only after `v` and `kind` are checked
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CurrentJob {
    pub job_id: String,
    pub lease_token: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Gpu {
    pub name: String,
    pub vram_used_mb: u64,
    pub vram_total_mb: u64,
}

#[derive(Debug, Serialize)]
pub struct Heartbeat<'a> {
    pub worker_id: &'a str,
    pub caps: &'a [Capability],
    pub slots: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu: Option<Gpu>, // left out until something measures VRAM
    pub current_jobs: Vec<CurrentJob>,
    pub versions: &'a HashMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct HeartbeatResponse {
    pub cancel_job_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ClaimRequest<'a> {
    pub worker_id: &'a str,
    pub caps: &'a [Capability],
    pub slots: usize,
    pub wait_s: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JobError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CompleteJob {
    pub v: u64,
    pub status: &'static str, // "done" | "failed"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JobError>,
    pub model_versions: HashMap<String, String>,
    pub timings_ms: HashMap<String, u64>,
}

impl CompleteJob {
    pub fn failed(code: &str, message: String, retryable: bool) -> Self {
        Self {
            v: 1,
            status: "failed",
            result: None,
            error: Some(JobError {
                code: code.to_owned(),
                message,
                retryable,
            }),
            model_versions: HashMap::new(),
            timings_ms: HashMap::new(),
        }
    }
}

// ---- The API client boundary: reqwest in production, a fake in tests ----

#[derive(Debug, thiserror::Error)]
pub enum CallError {
    /// `409 lease_lost`: the job was requeued, expired or cancelled. Drop it.
    #[error("lease lost")]
    LeaseLost,
    /// Connect error, timeout, `429`, or a `5xx` the body calls retryable: retry with
    /// backoff, and not before `retry_after` when the API sent `retry_after_s`.
    #[error("worker API unavailable: {reason}")]
    Unavailable {
        reason: String,
        retry_after: Option<Duration>,
    },
    /// Any other answer: a bug on one side, so the same call will fail again.
    #[error("worker API rejected the call: {0}")]
    Rejected(String),
}

impl CallError {
    pub fn unavailable(reason: &str) -> Self {
        Self::Unavailable {
            reason: reason.to_owned(),
            retry_after: None,
        }
    }
}

pub trait WorkerApi: Send + Sync + 'static {
    fn heartbeat(
        &self,
        hb: &Heartbeat<'_>,
    ) -> impl Future<Output = Result<HeartbeatResponse, CallError>> + Send;
    /// `Ok(None)` is the `204` of a long poll that found nothing.
    fn claim(
        &self,
        req: &ClaimRequest<'_>,
    ) -> impl Future<Output = Result<Option<Job>, CallError>> + Send;
    fn events(
        &self,
        job: &Job,
        events: &serde_json::Value,
    ) -> impl Future<Output = Result<(), CallError>> + Send;
    fn complete(
        &self,
        job: &Job,
        body: &CompleteJob,
    ) -> impl Future<Output = Result<(), CallError>> + Send;
}

/// Why a job ends without a `complete`: drop it and send nothing more.
#[derive(Debug, PartialEq, Eq)]
pub enum Abandon {
    /// A call about the job got `409 lease_lost`.
    LeaseLost,
    /// The job's token was cancelled: `cancel_job_ids`, or abandoned at shutdown.
    Cancelled,
}

/// Runs one job's stages: perception, VLM or fallback.
pub trait Runner: Send + Sync + 'static {
    /// `cancel` is the job's token: stop on it. The perception runner hands it to its
    /// inference wait, which terminates the ONNX run (ort.md §4).
    fn run<A: WorkerApi>(
        &self,
        api: &A,
        job: &Job,
        cancel: &CancellationToken,
    ) -> impl Future<Output = Result<CompleteJob, Abandon>> + Send;
}

/// Posts a stage's events. Only `lease_lost` ends the job; any other failure is logged and
/// the job goes on, because `complete` repeats every stage (contracts §5, events).
pub async fn send_events<A: WorkerApi>(
    api: &A,
    job: &Job,
    events: &serde_json::Value,
) -> Result<(), Abandon> {
    match api.events(job, events).await {
        Ok(()) => Ok(()),
        Err(CallError::LeaseLost) => Err(Abandon::LeaseLost),
        Err(e) => {
            tracing::warn!(error = %e, job_id = %job.job_id, "events failed; complete repeats them");
            Ok(())
        }
    }
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

    pub fn register(&self, job: &Job, cancel: CancellationToken) -> Registered {
        let entry = Entry {
            lease_token: job.lease_token.clone(),
            cancel,
        };
        self.lock().insert(job.job_id.clone(), entry);
        Registered {
            running: self.clone(),
            job_id: job.job_id.clone(),
        }
    }

    pub fn current_jobs(&self) -> Vec<CurrentJob> {
        self.lock()
            .iter()
            .map(|(id, e)| CurrentJob {
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
/// `retry_after_s` when it asked for longer.
fn retry_delay(e: &CallError, backoff: &mut Backoff) -> Duration {
    let delay = backoff.next_delay();
    match e {
        CallError::Unavailable {
            retry_after: Some(after),
            ..
        } => delay.max(*after),
        _ => delay,
    }
}

// ---- The worker ----

pub const HEARTBEAT_EVERY: Duration = Duration::from_secs(10); // contracts §5
pub const CLAIM_WAIT_S: u32 = 25; // contracts §5: default 25, max 30
/// A `204` sooner than half of `CLAIM_WAIT_S` didn't wait out the long poll: back off.
pub const EARLY_EMPTY_CLAIM: Duration = Duration::from_millis(12_500);
pub const STOP_BEFORE_MAX_RUNTIME: Duration = Duration::from_secs(15);
/// How long a cancelled job's runner gets to stop by itself before its future is dropped.
pub const CANCEL_GRACE: Duration = Duration::from_secs(5);
/// How long running jobs get after SIGTERM. Plus `CANCEL_GRACE`, 25 s: under the
/// `stop_grace_period: 30s` of the worker's compose service.
pub const DRAIN: Duration = Duration::from_secs(20);

pub struct WorkerConfig {
    pub worker_id: String,
    pub caps: Vec<Capability>,
    pub slots: usize,
    pub versions: HashMap<String, String>,
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
            slots: Arc::new(Semaphore::new(cfg.slots)),
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
            match self.api.heartbeat(&hb).await {
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
                // An early 204 (the API shutting down, say): don't spin on it.
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

    fn start(&self, job: Job, slot: OwnedSemaphorePermit) {
        let cancel = self.abandon.child_token();
        let registered = self.running.register(&job, cancel.clone());
        let (api, runner, cfg) = (self.api.clone(), self.runner.clone(), self.cfg.clone());
        self.tracker.spawn(async move {
            run_job(&*api, &*runner, &cfg.caps, &job, &cancel).await;
            drop((registered, slot)); // leaves the heartbeat list, then frees the slot
        });
    }

    /// After `shutdown`: wait up to `bound` for running jobs, then abandon the rest.
    pub async fn drain(&self, bound: Duration) {
        self.tracker.close();
        if tokio::time::timeout(bound, self.tracker.wait())
            .await
            .is_err()
        {
            tracing::warn!(jobs = self.tracker.len(), "abandoning jobs at shutdown");
            self.abandon.cancel();
            self.tracker.wait().await; // each runner stops within CANCEL_GRACE
        }
    }
}

/// The process: heartbeat and claims until SIGTERM or Ctrl-C, then stop claiming, drain,
/// stop heartbeating. The heartbeat outlives the claim loop so that draining jobs keep their leases.
pub async fn run_until_stopped<A: WorkerApi, R: Runner>(
    worker: Arc<Worker<A, R>>,
) -> std::io::Result<()> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut sigterm = signal(SignalKind::terminate())?;
    let (shutdown, stop_heartbeat) = (CancellationToken::new(), CancellationToken::new());
    let heartbeat = tokio::spawn({
        let (worker, stop) = (worker.clone(), stop_heartbeat.clone());
        async move { worker.heartbeat_loop(stop).await }
    });
    let claims = tokio::spawn({
        let (worker, shutdown) = (worker.clone(), shutdown.clone());
        async move { worker.claim_loop(shutdown).await }
    });
    tokio::select! {
        _ = sigterm.recv() => tracing::info!("SIGTERM: draining"),
        result = tokio::signal::ctrl_c() => {
            result?;
            tracing::info!("Ctrl-C: draining");
        }
    }
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

async fn run_job<A: WorkerApi, R: Runner>(
    api: &A,
    runner: &R,
    caps: &[Capability],
    job: &Job,
    cancel: &CancellationToken,
) {
    let received = Instant::now();
    let max_runtime = Duration::from_secs(job.max_runtime_s);
    let hard_deadline = received + max_runtime;
    let body = if job.v != 1 {
        // Not in the contracts yet: `retryable: false` (contracts 12, question 19).
        CompleteJob::failed(
            "unsupported_version",
            format!("job v {} is not supported", job.v),
            false,
        )
    } else if !caps.iter().any(|c| c.kinds().contains(&job.kind.as_str())) {
        // Not in the contracts yet: an unknown kind is a bug, failed with a placeholder
        // code (contracts 12, questions 18 and 21).
        CompleteJob::failed(
            "unsupported_kind",
            format!("job kind {} is not supported by this worker", job.kind),
            false,
        )
    } else {
        let soft_deadline = received + max_runtime.saturating_sub(STOP_BEFORE_MAX_RUNTIME);
        let run = tokio::time::timeout_at(soft_deadline, runner.run(api, job, cancel));
        // The runner stops on its token by itself; one that hasn't after CANCEL_GRACE is
        // dropped, and a drop guard in it terminates whatever it was waiting on.
        let gave_up = async {
            cancel.cancelled().await;
            tokio::time::sleep(CANCEL_GRACE).await;
        };
        let outcome = tokio::select! {
            outcome = run => outcome,
            () = gave_up => return,
        };
        if cancel.is_cancelled() {
            return; // cancel_job_ids, or abandoned at shutdown: send nothing more
        }
        match outcome {
            Ok(Ok(body)) => body,
            Ok(Err(Abandon::LeaseLost | Abandon::Cancelled)) => return,
            // Not in the contracts yet: the code `max_runtime_exceeded` (contracts 12, question 18).
            Err(_elapsed) => CompleteJob::failed(
                "max_runtime_exceeded",
                format!("stopped before max_runtime_s {}", job.max_runtime_s),
                true,
            ),
        }
    };
    complete_with_retry(api, job, &body, hard_deadline, cancel).await;
}

/// Repeating `complete` with the same lease token is safe: a job that already completed gets 204.
async fn complete_with_retry<A: WorkerApi>(
    api: &A,
    job: &Job,
    body: &CompleteJob,
    until: Instant,
    cancel: &CancellationToken,
) {
    let mut backoff = Backoff::new(Duration::from_millis(500), Duration::from_secs(8));
    loop {
        let e = match api.complete(job, body).await {
            Ok(()) | Err(CallError::LeaseLost) => return,
            Err(CallError::Rejected(e)) => {
                tracing::error!(error = %e, job_id = %job.job_id, "complete rejected");
                return;
            }
            Err(e) => e,
        };
        let wait = retry_delay(&e, &mut backoff);
        if Instant::now() + wait >= until {
            tracing::warn!(error = %e, job_id = %job.job_id, "giving up on complete");
            return;
        }
        tokio::select! {
            () = cancel.cancelled() => return,
            () = tokio::time::sleep(wait) => {}
        }
    }
}

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
        completes: Mutex<Vec<CompleteJob>>,
        heartbeats: Mutex<Vec<Vec<CurrentJob>>>,
        cancel_job_ids: Vec<String>,
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
            let cancel_job_ids = self.cancel_job_ids.clone();
            ready(Ok(HeartbeatResponse { cancel_job_ids }))
        }

        fn claim(
            &self,
            _req: &ClaimRequest<'_>,
        ) -> impl Future<Output = Result<Option<Job>, CallError>> {
            let mut claims = self.claims.lock().unwrap();
            *claims += 1;
            let mut jobs_left = self.jobs_left.lock().unwrap();
            let reply = if matches!(self.empty, Empty::Fails) {
                Some(Err(CallError::unavailable("connection refused")))
            } else if *jobs_left > 0 {
                *jobs_left -= 1;
                Some(Ok(Some(Job {
                    v: self.job_v,
                    job_id: format!("job{}", *claims),
                    kind: self.job_kind.into(),
                    scan_id: "scan1".into(),
                    run: Some(1),
                    attempt: 1,
                    lease_token: format!("lease{}", *claims),
                    lease_until: "2026-10-18T14:05:22Z".into(),
                    max_runtime_s: 120,
                    payload: serde_json::Value::Null,
                })))
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
            _job: &Job,
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
            _job: &Job,
            body: &CompleteJob,
        ) -> impl Future<Output = Result<(), CallError>> {
            self.complete_calls.lock().unwrap().push(Instant::now());
            let mut left = self.complete_failures.lock().unwrap();
            if *left > 0 {
                *left -= 1;
                return ready(Err(CallError::Unavailable {
                    reason: "503".into(),
                    retry_after: self.complete_retry_after,
                }));
            }
            self.completes.lock().unwrap().push(body.clone());
            ready(Ok(()))
        }
    }

    struct Hangs; // a model that never returns and ignores its token
    impl Runner for Hangs {
        async fn run<A: WorkerApi>(
            &self,
            _api: &A,
            _job: &Job,
            _cancel: &CancellationToken,
        ) -> Result<CompleteJob, Abandon> {
            std::future::pending().await
        }
    }

    struct SendsEvent; // sends one partial_result, then finishes
    impl Runner for SendsEvent {
        async fn run<A: WorkerApi>(
            &self,
            api: &A,
            job: &Job,
            _cancel: &CancellationToken,
        ) -> Result<CompleteJob, Abandon> {
            let events = serde_json::json!({ "v": 1, "events": [] });
            send_events(api, job, &events).await?;
            let mut done = CompleteJob::failed("unused", String::new(), false);
            done.status = "done";
            done.error = None;
            Ok(done)
        }
    }

    type Log = Arc<Mutex<Vec<&'static str>>>;

    /// Models ort.md's `await_run`: a run that ends only when it is terminated, by the
    /// job's token ("token") or by its future being dropped ("drop").
    #[derive(Default)]
    struct Inferring {
        terminated: Log,
    }

    struct TerminateOnDrop(Option<Log>);

    impl Drop for TerminateOnDrop {
        fn drop(&mut self) {
            if let Some(log) = self.0.take() {
                log.lock().unwrap().push("drop");
            }
        }
    }

    impl Runner for Inferring {
        async fn run<A: WorkerApi>(
            &self,
            _api: &A,
            _job: &Job,
            cancel: &CancellationToken,
        ) -> Result<CompleteJob, Abandon> {
            let mut guard = TerminateOnDrop(Some(self.terminated.clone()));
            cancel.cancelled().await; // the run never finishes by itself
            guard.0 = None;
            self.terminated.lock().unwrap().push("token");
            Err(Abandon::Cancelled)
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

    fn worker<R: Runner>(fake: Fake, runner: R, slots: usize) -> Arc<Worker<Fake, R>> {
        let cfg = WorkerConfig {
            worker_id: "rig-3070-perception-1".into(),
            caps: vec![Capability::Perception],
            slots,
            versions: HashMap::new(),
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

    /// Stops claiming and waits for the jobs, as `run_until_stopped` does.
    async fn stop<R: Runner>(
        w: &Worker<Fake, R>,
        shutdown: &CancellationToken,
        claims: tokio::task::JoinHandle<()>,
    ) {
        shutdown.cancel();
        claims.await.unwrap();
        w.drain(DRAIN).await;
    }

    #[tokio::test(start_paused = true)]
    async fn holds_at_most_slots_jobs() {
        let w = worker(fake(10), Hangs, 2);
        let shutdown = CancellationToken::new();
        let _claim_loop = spawn_claims(&w, &shutdown);
        tokio::time::sleep(Duration::from_secs(60)).await;
        assert_eq!(*w.api.claims.lock().unwrap(), 2); // the third claim waits for a slot
        assert_eq!(w.running.current_jobs().len(), 2);
        assert_eq!(w.slots.available_permits(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn lease_lost_drops_the_job_and_frees_the_slot() {
        let fake = Fake {
            events: Events::LeaseLost,
            ..fake(2)
        };
        let w = worker(fake, SendsEvent, 1);
        let shutdown = CancellationToken::new();
        let claims = spawn_claims(&w, &shutdown);
        tokio::time::sleep(Duration::from_secs(1)).await;
        // one slot, yet both jobs were claimed: the first freed its slot when it was dropped
        assert_eq!(*w.api.claims.lock().unwrap(), 3); // the third is a long poll still waiting
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
        let w = worker(fake, SendsEvent, 1);
        let shutdown = CancellationToken::new();
        let claims = spawn_claims(&w, &shutdown);
        tokio::time::sleep(Duration::from_millis(1)).await;
        stop(&w, &shutdown, claims).await;
        let completes = w.api.completes.lock().unwrap();
        assert_eq!(completes.len(), 1);
        assert_eq!(completes[0].status, "done"); // complete repeats the lost events
    }

    #[tokio::test(start_paused = true)]
    async fn heartbeat_lists_jobs_and_cancels_the_listed_ones() {
        let fake = Fake {
            cancel_job_ids: vec!["job1".into()],
            ..fake(1)
        };
        let w = worker(fake, Hangs, 1);
        let job: Job = serde_json::from_value(serde_json::json!({
            "v": 1, "job_id": "job1", "kind": "vlm", "scan_id": "scan1", "run": 1,
            "attempt": 1, "lease_token": "lease1", "lease_until": "2026-10-18T14:05:22Z",
            "max_runtime_s": 300, "payload": {}
        }))
        .unwrap();
        let token = CancellationToken::new();
        let _registered = w.running.register(&job, token.clone());
        let stop_heartbeat = CancellationToken::new();
        spawn_heartbeat(&w, &stop_heartbeat);
        tokio::time::sleep(Duration::from_millis(1)).await; // the first tick is immediate
        assert!(token.is_cancelled());
        let sent = w.api.heartbeats.lock().unwrap()[0].clone();
        let expected = CurrentJob {
            job_id: "job1".into(),
            lease_token: "lease1".into(),
        };
        assert_eq!(sent, vec![expected]);
        stop_heartbeat.cancel();
    }

    #[tokio::test(start_paused = true)]
    async fn a_cancelled_job_terminates_its_run() {
        let fake = Fake {
            cancel_job_ids: vec!["job1".into()],
            ..fake(1)
        };
        let w = worker(fake, Inferring::default(), 1);
        let (shutdown, stop_heartbeat) = (CancellationToken::new(), CancellationToken::new());
        let claims = spawn_claims(&w, &shutdown);
        spawn_heartbeat(&w, &stop_heartbeat);
        tokio::time::sleep(Duration::from_secs(11)).await; // a heartbeat lists job1 by now
        stop(&w, &shutdown, claims).await;
        stop_heartbeat.cancel();
        assert_eq!(*w.runner.terminated.lock().unwrap(), ["token"]);
        assert_eq!(w.api.completes.lock().unwrap().len(), 0); // nothing sent after a cancel
        assert_eq!(w.slots.available_permits(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn claim_errors_back_off_instead_of_spinning() {
        let fake = Fake {
            empty: Empty::Fails,
            ..fake(1)
        };
        let w = worker(fake, Hangs, 1);
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
        let w = worker(fake, Hangs, 1);
        let shutdown = CancellationToken::new();
        let _claim_loop = spawn_claims(&w, &shutdown);
        tokio::time::sleep(Duration::from_secs(60)).await;
        let n = *w.api.claims.lock().unwrap();
        assert!((6..=8).contains(&n), "{n} claims in 60 s"); // the same schedule as errors
    }

    #[tokio::test(start_paused = true)]
    async fn another_v_fails_with_unsupported_version() {
        let fake = Fake {
            job_v: 2,
            ..fake(1)
        };
        let w = worker(fake, Inferring::default(), 1);
        let shutdown = CancellationToken::new();
        let claims = spawn_claims(&w, &shutdown);
        tokio::time::sleep(Duration::from_millis(1)).await;
        stop(&w, &shutdown, claims).await;
        let completes = w.api.completes.lock().unwrap();
        let error = completes[0].error.as_ref().unwrap();
        assert_eq!(
            (completes[0].status, error.code.as_str()),
            ("failed", "unsupported_version")
        );
        assert!(!error.retryable);
        assert!(w.runner.terminated.lock().unwrap().is_empty()); // the runner never started
    }

    #[tokio::test(start_paused = true)]
    async fn an_unknown_kind_fails_without_running() {
        let fake = Fake {
            job_kind: "vlm", // not a perception worker's kind
            ..fake(1)
        };
        let w = worker(fake, Inferring::default(), 1);
        let shutdown = CancellationToken::new();
        let claims = spawn_claims(&w, &shutdown);
        tokio::time::sleep(Duration::from_millis(1)).await;
        stop(&w, &shutdown, claims).await;
        let completes = w.api.completes.lock().unwrap();
        let error = completes[0].error.as_ref().unwrap();
        assert_eq!(
            (error.code.as_str(), error.retryable),
            ("unsupported_kind", false)
        );
        assert!(w.runner.terminated.lock().unwrap().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn complete_is_retried_after_a_timeout() {
        let fake = Fake {
            complete_failures: Mutex::new(1),
            ..fake(1)
        };
        let w = worker(fake, SendsEvent, 1);
        let shutdown = CancellationToken::new();
        let claims = spawn_claims(&w, &shutdown);
        tokio::time::sleep(Duration::from_millis(1)).await;
        stop(&w, &shutdown, claims).await;
        let completes = w.api.completes.lock().unwrap();
        assert_eq!(completes.len(), 1);
        assert_eq!(completes[0].status, "done");
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
        let error = completes[0].error.clone().unwrap();
        assert_eq!(error.code, "max_runtime_exceeded");
        assert!(error.retryable);
        // the deadline dropped the runner's future, and its drop guard terminated the run
        assert_eq!(*w.runner.terminated.lock().unwrap(), ["drop"]);
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_abandons_jobs_after_the_drain_bound() {
        let w = worker(fake(1), Hangs, 1);
        let shutdown = CancellationToken::new();
        let claims = spawn_claims(&w, &shutdown);
        tokio::time::sleep(Duration::from_secs(1)).await;
        let started = Instant::now();
        stop(&w, &shutdown, claims).await;
        // The bound, then the grace a runner that ignores its token gets before it is dropped.
        assert_eq!(started.elapsed(), DRAIN + CANCEL_GRACE);
        assert_eq!(w.api.completes.lock().unwrap().len(), 0); // abandoned: the broker requeues it
        assert_eq!(w.slots.available_permits(), 1);
    }
}
```

## 3. What the tests show

All run on paused time (`start_paused = true`): tokio advances the clock whenever every task is idle, so 106 virtual seconds take microseconds. The fake broker hands out a set number of jobs, then answers each claim like an empty long poll (25 s, then `None`); a fake that answered at once forever would never let the clock advance. [tokio::test: start_paused](https://docs.rs/tokio/1.53.3/tokio/attr.test.html#configure-the-runtime-to-start-with-time-paused), [time::pause](https://docs.rs/tokio/1.53.3/tokio/time/fn.pause.html)

| Test | Shows |
|---|---|
| `holds_at_most_slots_jobs` | With 2 slots and models that never return, exactly 2 claims are made in 60 s |
| `lease_lost_drops_the_job_and_frees_the_slot` | After `409 lease_lost` from `events`, no `complete` is sent, the job leaves the heartbeat list, and its slot lets the next job be claimed |
| `another_events_failure_does_not_stop_the_job` | An `events` call that fails for another reason is logged; the job runs on and lands its `complete` |
| `heartbeat_lists_jobs_and_cancels_the_listed_ones` | The heartbeat sends `{ job_id, lease_token }`, and `cancel_job_ids` cancels that job's token |
| `a_cancelled_job_terminates_its_run` | A job named in `cancel_job_ids` terminates its run through the token the runner got, sends nothing, and frees its slot |
| `claim_errors_back_off_instead_of_spinning` | An unreachable API gets 6 to 8 claims in 60 s, and the slot is free while waiting |
| `an_early_204_backs_off_instead_of_spinning` | A `204` that comes back at once is retried on the same backoff, not in a tight loop |
| `another_v_fails_with_unsupported_version` | `v: 2` is completed as `failed`, `unsupported_version`, `retryable: false`, and the runner never starts |
| `an_unknown_kind_fails_without_running` | A `vlm` job on a perception worker is completed as `failed`, `unsupported_kind`, `retryable: false`, without running |
| `complete_is_retried_after_a_timeout` | A `complete` that failed with `Unavailable` is sent again and lands once |
| `complete_waits_for_retry_after` | After a failure that carried `retry_after_s: 7`, the next `complete` goes out 7 s later, not after the shorter backoff |
| `a_hung_model_is_stopped_before_max_runtime` | A job with `max_runtime_s: 120` has no `complete` at 104 s and one at 106 s (`failed`, `retryable: true`); the deadline dropped the run, and the drop guard terminated it |
| `shutdown_abandons_jobs_after_the_drain_bound` | The drain waits its 20 s bound, abandons the job, and a runner that ignores its token is dropped 5 s later; no `complete`, the slot is free |

## 4. Sources

| Rule | Source |
|---|---|
| Heartbeat every 10 s with `current_jobs`; requeue after 30 s without one; lease 2 min, never past `max_runtime` | [contracts 5, leases](../../../docs/architecture/contracts.md#leases-runs-and-attempts), [heartbeat](../../../docs/architecture/contracts.md#post-internalworkersheartbeat) |
| Claim long poll, `wait_s` 25 (max 30), `204`, `slots` | [contracts 5, claim](../../../docs/architecture/contracts.md#post-internaljobsclaim) |
| `409 lease_lost` for any call about a requeued, expired or cancelled job; a repeated `complete` returns `204` | [contracts 5, leases](../../../docs/architecture/contracts.md#leases-runs-and-attempts) |
| `unsupported_version`; which kinds each capability claims | [contracts 1, versioning](../../../docs/architecture/contracts.md#versioning), [contracts 5, leases](../../../docs/architecture/contracts.md#leases-runs-and-attempts) |
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
| The drain budget against Compose's `stop_grace_period` (10 s by default, then SIGKILL) | [Compose file: stop_grace_period](https://docs.docker.com/reference/compose-file/services/#stop_grace_period), [rust-async, shutdown 1](../rust-async/shutdown.md#1-the-sequence) |
| Dropping a future runs the destructors of what it holds, which is how the drop guard terminates a run | [Reference: destructors](https://doc.rust-lang.org/reference/destructors.html) |
