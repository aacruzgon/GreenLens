//! The job broker's storage ([contracts 5]; [database 4], job claim with `slots`).

use std::time::Duration;

use crate::{
    garden::ops::Status,
    id::{ImageId, JobId, QuestionId, ScanId},
    jobs::{Heartbeat, HeartbeatResponse, Job, JobError, JobKind, JobSpec},
    ports::{ClaimAttempt, Lease, ObjectRef, ObservationWrite, StoreError},
    records::diagnosis::IndicatorEvidence,
    rules::{jobs::ClaimPolicy, steps::PipelineStep},
    wire::{Run, Timestamp},
};

/// A job to queue.
#[derive(Debug, Clone, PartialEq)]
pub struct NewJob {
    /// Its id (the payload's image URLs contain it).
    pub id: JobId,
    /// Its scan.
    pub scan: ScanId,
    /// Its run; `None` for guided jobs.
    pub run: Option<Run>,
    /// A photo answer's job: top-level `job.question`, so the fallback's claim can hold it back.
    pub question: Option<QuestionId>,
    /// Kind and payload.
    pub spec: JobSpec,
    /// The kind's `max_runtime` on a GPU worker (doubled at claim for the fallback).
    pub max_runtime: Duration,
    /// `queued_at`.
    pub queued_at: Timestamp,
}

/// A leased job, as the claim returns it.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimedJob {
    /// The job body for the worker.
    pub job: Job,
    /// The scan moved from `queued` to `analysing` in the claim's transaction (the run's first
    /// job), for the `status_changed` event.
    pub started_analysing: bool,
}

/// What the lease check found about a job, for `events` and `complete`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeasedJob {
    /// The job.
    pub job: JobId,
    /// Its kind.
    pub kind: JobKind,
    /// Its scan.
    pub scan: ScanId,
    /// Its run.
    pub run: Option<Run>,
    /// Its question.
    pub question: Option<QuestionId>,
}

/// A job's completion, stored under its lease with everything it produced and the step claim
/// it hands to, in one transaction ([contracts 3], claiming).
#[derive(Debug, Clone, PartialEq)]
pub enum JobCompletion {
    /// `status: done`.
    Done {
        /// The raw result, kept on the job row.
        result: serde_json::Value,
        /// The result's observations, per stage.
        observations: Vec<ObservationWrite>,
        /// The VLM's indicator answers.
        evidence: Vec<IndicatorEvidence>,
        /// The step to claim: `retrieve` after a run's perception result, `score` after `vlm`
        /// or a run `verify`; `None` for guided jobs and question photos (handled by their own
        /// writes).
        claim: Option<PipelineStep>,
    },
    /// `status: failed`.
    Failed {
        /// The worker's error.
        error: JobError,
    },
}

/// What completing did.
#[derive(Debug, Clone, PartialEq)]
pub enum CompletionOutcome {
    /// Stored; the claim taken, when one was asked.
    Stored {
        /// The claim.
        claim: ClaimAttempt,
        /// What the final result changed against the job's partial results, for `finding`
        /// and `finding_retracted` events ([contracts 5], partial results).
        diff: crate::ports::ObservationDiff,
    },
    /// The same lease token completed this job before: `204`, nothing changed.
    AlreadyCompleted,
    /// A retryable failure with attempts left: requeued.
    Requeued,
    /// Failed for good: the caller applies [`crate::rules::jobs::final_failure`].
    FailedForGood {
        /// The job.
        job: LeasedJob,
    },
}

/// A live worker as its last heartbeat described it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveWorker {
    /// Its capabilities.
    pub caps: Vec<crate::jobs::Capability>,
    /// Its `versions` (contracts 5, `model_versions` keys), including `label_set_version` and
    /// `classifier_labels_version` on perception and fallback workers.
    pub versions: std::collections::BTreeMap<String, String>,
}

/// A job the scheduler requeued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequeuedJob {
    /// The job.
    pub job: LeasedJob,
    /// Out of attempts after this requeue: failed for good instead.
    pub exhausted: bool,
}

/// The `job` and `worker` tables.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait JobRepo: Send + Sync {
    /// Queues a job on its own (a guided check's job; run jobs are queued in step commits and
    /// run starts). Signals the broker's `Notify` after commit; so does every other write that
    /// queues or requeues a job (step commits, run starts, `add_image`, a retryable failure in
    /// `complete_job`, `requeue_stale`).
    async fn enqueue_job(&self, job: NewJob) -> Result<JobId, StoreError>;

    /// The claim ([database 4]): counts the worker's leased jobs against `slots`, then leases
    /// the oldest eligible queued job of its kinds with a fresh `lease_token`, re-checking
    /// `status = 'queued'`, in one transaction. `None` when nothing is eligible.
    async fn claim_job(
        &self,
        worker: &crate::id::WorkerId,
        slots: u32,
        policy: ClaimPolicy,
        now: Timestamp,
    ) -> Result<Option<ClaimedJob>, StoreError>;

    /// A heartbeat: upserts `worker:[worker_id]`, extends each listed lease whose token matches
    /// and isn't cancelled (never past `claimed_at + max_runtime`), and returns the jobs to
    /// cancel.
    async fn heartbeat(
        &self,
        heartbeat: Heartbeat,
        now: Timestamp,
    ) -> Result<HeartbeatResponse, StoreError>;

    /// Checks a lease for `POST /internal/jobs/{id}/events` and `POST /internal/jobs/{id}/complete`
    /// and returns the job's head. Accepts a live lease (`leased`, same token and worker,
    /// `lease_until >= now`) **and** a job already `done` or `failed` with the same token and
    /// worker, so a retried `complete` reaches `complete_job`, which answers
    /// `CompletionOutcome::AlreadyCompleted` (contracts 5: `204`, nothing changes). Anything else
    /// (requeued, expired, cancelled, another token) is `Conflict::LeaseLost`.
    async fn check_lease(&self, lease: &Lease, now: Timestamp) -> Result<LeasedJob, StoreError>;

    /// Checks a lease for `POST /internal/jobs/{id}/events`: `Some` for a live lease, `None` for
    /// a job already `done` or `failed` with the same token and worker (late events after
    /// `complete`: `api` answers `204` and drops them, never forwarding them to the pipeline),
    /// and `Conflict::LeaseLost` for anything else, as [`check_lease`](Self::check_lease).
    ///
    /// The default body falls back to `check_lease` and treats every accepted job as live; the
    /// store overrides it with one query that reads the job's status.
    async fn live_lease(
        &self,
        lease: &Lease,
        now: Timestamp,
    ) -> Result<Option<LeasedJob>, StoreError> {
        self.check_lease(lease, now).await.map(Some)
    }

    /// `GET /internal/jobs/{job_id}/images/{image_id}`: the image, if it is in the leased job's
    /// payload.
    async fn job_image(
        &self,
        lease: &Lease,
        image: ImageId,
        now: Timestamp,
    ) -> Result<Option<ObjectRef>, StoreError>;

    /// `POST /internal/jobs/{id}/complete`.
    async fn complete_job(
        &self,
        lease: &Lease,
        completion: JobCompletion,
        now: Timestamp,
    ) -> Result<CompletionOutcome, StoreError>;

    /// Requeues leases past `lease_until` (index `job_lease`) and the jobs of workers silent for
    /// 30 seconds, resetting `queued_at` and counting attempts ([`crate::rules::jobs`]). Signals
    /// the broker's `Notify` after commit when any job went back to `queued`, like every other
    /// write that queues a job: the store's `Notify` is the claim long poll's only wake-up.
    async fn requeue_stale(&self, now: Timestamp) -> Result<Vec<RequeuedJob>, StoreError>;

    /// The capabilities and `versions` of every worker that heartbeated in the last 30 seconds,
    /// so a `perception` job never names a `label_set_version` or `classifier_labels_version`
    /// that no live perception or fallback worker has loaded ([contracts 5], versions a worker
    /// hasn't loaded).
    async fn live_workers(&self, now: Timestamp) -> Result<Vec<LiveWorker>, StoreError>;

    /// Whether a worker with the `perception` capability heartbeated in the last 30 seconds.
    async fn perception_alive(&self, now: Timestamp) -> Result<bool, StoreError>;

    /// `GET /status`.
    async fn status(&self, now: Timestamp) -> Result<Status, StoreError>;

    /// Cleanup: finished jobs older than the cutoff, in batches of 500.
    async fn delete_finished_jobs(&self, before: Timestamp) -> Result<u32, StoreError>;
}
