//! The three library APIs `api` calls that are not `domain` ports: the pipeline's entry points
//! and its `ScanWriter` port, the agent's chat turn, and the vision crate's upload pipeline.
//!
//! Each is held as an `Arc<dyn …>` in [`AppState`](crate::state::AppState), so a handler test
//! swaps it for a fake exactly like a domain port. The traits here are `api`'s own view of those
//! APIs; `wiring.rs` implements them for `pipeline::Pipeline`, `agent::ChatAgent` and
//! `vision::upload::UploadProcessor` (a local trait on a foreign type, which the orphan rule
//! allows). The types that cross are the crates' own, re-exported. The signatures were agreed by
//! message with the crates' authors.

use async_trait::async_trait;
use futures::stream::BoxStream;
use tokio::sync::oneshot;

use domain::{
    authz::{Editor, Membership},
    chat::{ChatEvent, Preview},
    id::{PendingActionId, PlantId, QuestionId, ScanId},
    jobs::{CompleteJob, WorkerEvent},
    ports::{CompletionOutcome, Lease, LeasedJob, PhotoAnswerStart, PlanWrite, QuestionStepClaim},
    records::plan::ActionPlan,
    scan::{AnswerRequest, Plant, Scan, SpeciesChoice},
    url::Stored,
    wire::Timestamp,
};

pub use agent::{
    TurnError, TurnOutcome, TurnRequest,
    tools::revise::{PreviewError, RevisedPlan},
};
pub use pipeline::{
    FollowUp, Handled, PhotoInfo, PipelineError, PlanGuard, ScanWriter, SubmitOutcome,
};
pub use vision::{
    depth::{DepthInput, ProcessedDepth},
    upload::{ProcessedImage, ProcessedUpload, UploadError, UploadInput},
};

/// The pipeline's entry points (`pipeline::Pipeline`). All return quickly except
/// [`follow_up`](Self::follow_up), which `api` spawns.
#[async_trait]
pub trait PipelineApi: Send + Sync {
    /// Runs a step or question step and its hand-overs.
    async fn follow_up(&self, follow_up: FollowUp) -> Result<(), PipelineError>;
    /// `POST /scans/{id}/submit`.
    async fn submit(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
    ) -> Result<SubmitOutcome, PipelineError>;
    /// `POST /plants/{id}/species`.
    async fn species_confirmed(
        &self,
        editor: Membership<Editor>,
        plant: PlantId,
        choice: SpeciesChoice,
    ) -> Result<Plant<Stored>, PipelineError>;
    /// `POST /scans/{id}/answers`.
    async fn answer(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
        answer: AnswerRequest,
    ) -> Result<Handled<Scan<Stored>>, PipelineError>;
    /// Before `add_image`, for an upload with `question_id`.
    async fn photo_answer_start(
        &self,
        scan: ScanId,
        question: QuestionId,
        photo: PhotoInfo,
    ) -> Result<PhotoAnswerStart, PipelineError>;
    /// After `add_image`, with `AddedImage.photo_answer`: a `measuring` start comes back as a
    /// follow-up; a `segmenting` start has its job queued already.
    fn photo_answer_added(&self, claim: QuestionStepClaim) -> Option<FollowUp>;
    /// `POST /internal/jobs/{id}/events`.
    async fn worker_events(
        &self,
        lease: &Lease,
        job: LeasedJob,
        events: Vec<WorkerEvent>,
    ) -> Result<(), PipelineError>;
    /// `POST /internal/jobs/{id}/complete`.
    async fn job_completed(
        &self,
        lease: &Lease,
        job: LeasedJob,
        complete: CompleteJob,
    ) -> Result<Handled<CompletionOutcome>, PipelineError>;
    /// A job out of attempts (from `complete` or the requeue sweep).
    async fn job_failed_for_good(&self, job: LeasedJob) -> Result<Option<FollowUp>, PipelineError>;
    /// The sweep: stale steps, stalled question steps, and the 10-minute continuation of
    /// unanswered questions (its one owner; the scheduler's `sweep` task calls it).
    async fn sweep(&self) -> Result<Vec<FollowUp>, PipelineError>;
    /// A confirmed `revise_plan`: plan lock, `write_revision`, then `plan_superseded`.
    async fn store_revision(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
        write: PlanWrite,
    ) -> Result<ActionPlan, PipelineError>;
}

/// `TurnStream::into_parts()`, with the events boxed: `'static` and `Send`, so they can move
/// into the response body.
pub struct TurnParts {
    /// The events. Dropping this stream stops the LLM call.
    pub events: BoxStream<'static, ChatEvent>,
    /// Resolved by the turn task once it stored the message.
    pub outcome: oneshot::Receiver<TurnOutcome>,
}

/// The agent's chat turn and the pure preview application (`agent::ChatAgent`,
/// `agent::tools::revise::apply_preview`).
#[async_trait]
pub trait ChatTurns: Send + Sync {
    /// Validation, budget, routing, storing the user message; then the stream.
    async fn start_turn(&self, request: TurnRequest) -> Result<TurnParts, TurnError>;

    /// The plan a confirmed `revise_plan` card stores.
    fn apply_preview(
        &self,
        base: &ActionPlan,
        preview: &Preview,
        action: PendingActionId,
        now: Timestamp,
    ) -> Result<RevisedPlan, PreviewError>;
}

/// The CPU upload pipeline (`vision::upload::UploadProcessor`). Synchronous and blocking: `api`
/// calls it inside `spawn_blocking`, behind a `Semaphore`.
pub trait ImageProcessing: Send + Sync {
    /// A scan photo with its optional depth.
    fn process(&self, input: UploadInput<'_>) -> Result<ProcessedUpload, UploadError>;
    /// A chat image: no quality check, no depth.
    fn process_chat(&self, file: &[u8]) -> Result<ProcessedImage, UploadError>;
}
