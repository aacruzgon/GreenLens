//! Observations, questions, guided checks and indicator evidence ([contracts 3], [contracts 7]).

use crate::{
    authz::{Editor, Membership, Viewer},
    chat::SimilarCase,
    events::Stage,
    id::{ImageId, ObservationId, QuestionId, ScanId},
    ports::{ClaimAttempt, Lease, NewJob, NewMask, QuestionStepClaim, StepClaim, StoreError},
    records::{diagnosis::IndicatorEvidence, observation::Observation},
    rules::questions::QuestionStep,
    scan::{AnswerRequest, GuidedCheckStatus, ScanStatus},
    url::Stored,
    wire::{Run, Timestamp},
};

/// A stage's observations, replacing that stage's earlier ones for the images it covers and
/// those tied to no image ([database 5]: replacement per image; [contracts 3], observations
/// across runs).
#[derive(Debug, Clone, PartialEq)]
pub struct ObservationWrite {
    /// The pipeline stage that produced them.
    pub stage: Stage,
    /// The run.
    pub run: Run,
    /// The images the stage covered; their earlier observations of this stage are replaced.
    pub images: Vec<ImageId>,
    /// Also replace this stage's observations tied to no image (a run does; a question photo
    /// doesn't).
    pub replace_imageless: bool,
    /// The new observations.
    pub observations: Vec<Observation<Stored>>,
    /// Masks to record with them.
    pub masks: Vec<NewMask>,
}

/// What storing a stage's output changed, for `finding` and `finding_retracted` events
/// ([contracts 5], partial results).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ObservationDiff {
    /// New findings.
    pub added: Vec<Observation<Stored>>,
    /// Findings the final result didn't keep (`retracted_at` set).
    pub retracted: Vec<ObservationId>,
}

/// What answering an indicator question did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnswerOutcome {
    /// A late answer: the scan is `diagnosed` or `planned` and the scheduler had continued.
    pub late: bool,
    /// The `score` claim taken in the same transaction, or `Busy`.
    pub claim: ClaimAttempt,
}

/// A question due for continuation ([contracts 3]: 10 minutes without an answer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DueQuestion {
    /// The question.
    pub question: QuestionId,
    /// Its scan.
    pub scan: ScanId,
    /// Its run.
    pub run: Run,
}

/// What a photo answer's step does next ([contracts 3], a photo answer that stalls).
#[derive(Debug, Clone, PartialEq)]
#[expect(
    clippy::large_enum_variant,
    reason = "built once per step and moved straight into one transaction"
)]
pub enum QuestionStepOutput {
    /// Segmentation stored: move to `measuring` (in the result handler's transaction).
    Measure,
    /// Measured: store the measurements and evidence, then queue `verify` and move to
    /// `verifying`.
    Verify {
        /// The photo's measurements.
        observations: ObservationWrite,
        /// Indicators the measurements settled.
        evidence: Vec<IndicatorEvidence>,
        /// The `verify` job.
        job: NewJob,
    },
    /// Measured and nothing left to ask: store them and claim the scan's `score` step.
    Score {
        /// The photo's measurements.
        observations: ObservationWrite,
        /// Indicators the measurements settled.
        evidence: Vec<IndicatorEvidence>,
    },
}

/// What advancing a photo answer's step did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionAdvance {
    /// The question's next step, under a new stamp (`measuring` or `verifying`).
    Next(QuestionStepClaim),
    /// The measurements settled everything ([`QuestionStepOutput::Score`]): the question's
    /// step is cleared (it stays `answering` until `score` resolves it) and the scan's `score`
    /// step was claimed in the same transaction, or `rescore_pending` set (`Busy`), so a crash
    /// between the two can't strand the question.
    Scored(ClaimAttempt),
}

/// A finished guided check ([contracts 7], guided checks and the diagnosis). The store finds
/// the check from the leased job (its payload's image), so no id is passed.
#[derive(Debug, Clone, PartialEq)]
pub struct GuidedOutcome {
    /// `done` or `failed`.
    pub status: GuidedCheckStatus,
    /// The texture observation; none for a failed check or an unsettled jar.
    pub observation: Option<Observation<Stored>>,
}

/// What finishing a guided check did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuidedFinish {
    /// The scan's status at that moment.
    pub scan_status: ScanStatus,
    /// The `score` claim on a `diagnosed` or `planned` scan; `NotNeeded` otherwise.
    pub claim: ClaimAttempt,
}

/// The `observation`, `question` and `guided_check` tables, and the indicator evidence.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait AnalysisRepo: Send + Sync {
    /// A `partial_result` from a worker: stores the stage's output once, under the lease.
    async fn store_partial(
        &self,
        lease: &Lease,
        write: ObservationWrite,
    ) -> Result<ObservationDiff, StoreError>;

    /// `api`'s own observations (measurement, organism lengths, VLM findings it converts),
    /// under the step claim.
    async fn store_observations(
        &self,
        claim: &StepClaim,
        write: ObservationWrite,
    ) -> Result<ObservationDiff, StoreError>;

    /// The scan's current observations.
    async fn observations(&self, scan: ScanId) -> Result<Vec<Observation<Stored>>, StoreError>;

    /// `POST /scans/{id}/answers`: stores the user's evidence, resolves the question and claims
    /// `score` (or flags `rescore_pending`) in one transaction. A question that is answered,
    /// superseded or `answering` is `Rejected(InvalidState)`.
    async fn answer_question(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
        answer: AnswerRequest,
        evidence: Vec<IndicatorEvidence>,
        now: Timestamp,
    ) -> Result<AnswerOutcome, StoreError>;

    /// Moves a photo answer to its next step under its claim; refused (`QuestionStepLost`)
    /// unless the question is still `answering` with the same step and stamp, its run is the
    /// scan's, and the plant isn't being deleted.
    async fn advance_question_step(
        &self,
        claim: &QuestionStepClaim,
        output: QuestionStepOutput,
        now: Timestamp,
    ) -> Result<QuestionAdvance, StoreError>;

    /// A photo answer that couldn't be analysed: `dropped` when late, reopened otherwise.
    async fn drop_photo_answer(
        &self,
        claim: &QuestionStepClaim,
        now: Timestamp,
    ) -> Result<(), StoreError>;

    /// `measuring` steps older than 2 minutes (index `question_stalled`).
    async fn stalled_question_steps(
        &self,
        older_than: Timestamp,
    ) -> Result<Vec<QuestionStepClaim>, StoreError>;

    /// The sweep's re-claim of a stalled step, by its old stamp.
    async fn reclaim_question_step(
        &self,
        stale: &QuestionStepClaim,
        step: QuestionStep,
        now: Timestamp,
    ) -> Result<Option<QuestionStepClaim>, StoreError>;

    /// Open questions whose `continue_at` has passed (index `question_sweep`), skipping those
    /// whose photo is being analysed.
    async fn questions_due(&self, now: Timestamp) -> Result<Vec<DueQuestion>, StoreError>;

    /// The scheduler continues without an answer: sets `continued_at` and claims `score`.
    async fn continue_without_answer(
        &self,
        due: DueQuestion,
        now: Timestamp,
    ) -> Result<ClaimAttempt, StoreError>;

    /// `find_similar_cases` ([contracts 8]): an exact cosine search over `scan_image.embedding`
    /// within the proof's garden ([database 5]), excluding the query image's own scan; at most
    /// `k` (5 or fewer), best first. `NotFound` when the image isn't a scan image of the garden;
    /// an empty list when it has no embedding.
    async fn similar_cases(
        &self,
        member: Membership<Viewer>,
        image: ImageId,
        k: u32,
    ) -> Result<Vec<SimilarCase>, StoreError>;

    /// A `guided` job's result, under its lease: updates the check, stores its observation and
    /// claims `score` when the scan is `diagnosed` or `planned`.
    async fn finish_guided_check(
        &self,
        lease: &Lease,
        outcome: GuidedOutcome,
    ) -> Result<GuidedFinish, StoreError>;
}
