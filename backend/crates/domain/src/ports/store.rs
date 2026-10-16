//! The store's repository traits, one per area ([database 3], [database 4]), and [`Store`],
//! the supertrait of all of them that `api` holds as `Arc<dyn Store>`.
//!
//! Rules every method follows:
//! - A garden-scoped call takes a [`Membership<R>`](crate::authz::Membership) proof with the
//!   role the role matrix asks for; the store scopes its query to `membership.garden()`, so a
//!   record of another garden is [`StoreError::NotFound`](crate::ports::StoreError).
//! - A call that continues `api`'s own step takes the [`StepClaim`] it got from
//!   [`StepRepo::claim_step`]; a worker call takes the [`Lease`] from [`JobRepo::claim_job`].
//!   The store re-checks either in the write's transaction ([database 4]).
//! - Reads skip records being deleted (`deleting_at`), and writes that need a live record check
//!   it in the same transaction.
//! - Transactions are sent as one query and retried on conflict up to 5 times inside the store
//!   ([database 4]); a caller sees [`Conflict::Retries`](crate::ports::Conflict) only when that
//!   fails.
//! - Records come back in their stored URL state (`X<Stored>`); `api` signs them.

mod access;
mod analysis;
mod chat;
mod deletion;
mod events;
mod gardens;
mod idempotency;
mod identity;
mod jobs;
mod knowledge;
mod map;
mod plans;
mod plants;
mod scans;
mod steps;
mod weather;

pub use access::*;
pub use analysis::*;
pub use chat::*;
pub use deletion::*;
pub use events::*;
pub use gardens::*;
pub use idempotency::*;
pub use identity::*;
pub use jobs::*;
pub use knowledge::*;
pub use map::*;
pub use plans::*;
pub use plants::*;
pub use scans::*;
pub use steps::*;
pub use weather::*;

use crate::{
    id::{JobId, LeaseToken, QuestionId, ScanId, WorkerId},
    ports::{ContentType, ObjectKey},
    rules::{questions::QuestionStep, steps::PipelineStep},
    wire::{Run, Timestamp},
};

/// A worker's hold on a job: what every later call about the job carries
/// (`X-Lease-Token`, [contracts 5]). The store compares all three in the write's transaction;
/// a mismatch, an expired lease or a cancelled job is `Conflict::LeaseLost`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
    /// The job.
    pub job: JobId,
    /// Its current token.
    pub token: LeaseToken,
    /// The worker holding it.
    pub worker: WorkerId,
}

/// A scan's step claim (`pipeline_step`, `pipeline_step_run`, `pipeline_step_at`,
/// [database 4], steps between jobs). Produced only by the store's claim and re-claim; a commit
/// goes through only if all four still hold, the scan's `run` is unchanged and neither the plant
/// nor the garden is being deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepClaim {
    /// The scan.
    pub scan: ScanId,
    /// The step.
    pub step: PipelineStep,
    /// The run it belongs to.
    pub run: Run,
    /// When it was claimed: the compare-and-set stamp.
    pub stamp: Timestamp,
}

/// The result of an entry point's claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimAttempt {
    /// This caller holds the step now.
    Claimed(StepClaim),
    /// Another step holds the claim; `rescore_pending` was set instead, and the running step
    /// hands over to `score` when it commits.
    Busy,
    /// Nothing to claim: the scan's status claims nothing (a guided check on a `pending`,
    /// `queued` or `failed` scan), or it is being deleted.
    NotNeeded,
}

/// A photo answer's step claim (`question.step`, `step_at` and its `run`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuestionStepClaim {
    /// The question.
    pub question: QuestionId,
    /// Its scan.
    pub scan: ScanId,
    /// The step.
    pub step: QuestionStep,
    /// The run it belongs to.
    pub run: Run,
    /// The compare-and-set stamp.
    pub stamp: Timestamp,
}

/// Where an image's bytes are, for the routes that stream them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectRef {
    /// The storage key from the record.
    pub key: ObjectKey,
    /// Its content type.
    pub content_type: ContentType,
}

/// Every repository, for `Arc<dyn Store>`. Blanket-implemented, so the store crate implements
/// each area trait and gets this for free. Trait upcasting (stable since Rust 1.86) turns an
/// `Arc<dyn Store>` into an `Arc<dyn PlanRepo>` for a component that needs one area only.
pub trait Store:
    AccessRepo
    + IdentityRepo
    + GardenRepo
    + PlantRepo
    + ScanRepo
    + EventRepo
    + AnalysisRepo
    + StepRepo
    + PlanRepo
    + ChatRepo
    + KnowledgeRepo
    + JobRepo
    + IdempotencyRepo
    + UsageRepo
    + WeatherCacheRepo
    + DeletionRepo
    + MapRepo
{
}

impl<T> Store for T where
    T: AccessRepo
        + IdentityRepo
        + GardenRepo
        + PlantRepo
        + ScanRepo
        + EventRepo
        + AnalysisRepo
        + StepRepo
        + PlanRepo
        + ChatRepo
        + KnowledgeRepo
        + JobRepo
        + IdempotencyRepo
        + UsageRepo
        + WeatherCacheRepo
        + DeletionRepo
        + MapRepo
{
}
