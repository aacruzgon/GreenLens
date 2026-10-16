//! Scans, their images and masks ([contracts 3]).

use crate::{
    authz::{Editor, Membership, Reviewer, Viewer},
    events::{RunReason, SelectedCandidate},
    id::{
        FeedbackId, GuidedCheckId, ImageId, MaskId, PlantId, QuestionId, RegionId, ScanId,
        SpeciesId, UserId,
    },
    jobs::RleMask,
    ports::{GardenContext, NewJob, ObjectKey, ObjectRef, QuestionStepClaim, StoreError},
    records::{
        diagnosis::{Diagnosis, IndicatorEvidence},
        observation::Observation,
        plan::ActionPlan,
        weather::WeatherSummary,
    },
    rules::uploads::UploadRoute,
    scan::{
        DepthMeta, FeedbackRequest, ImageKind, NewPlantInput, Plant, Question, Scan, ScanDetail,
        ScanFailure, ScanImage, ScanStatus,
    },
    url::Stored,
    wire::{Run, Timestamp},
};

/// `POST /scans`, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewScan {
    /// Its id.
    pub id: ScanId,
    /// The plant, or the plant to create with its new id.
    pub plant: NewScanPlant,
    /// The creator.
    pub created_by: UserId,
    /// Now; also the UTC day the daily scan limit counts in.
    pub created_at: Timestamp,
}

/// The plant of a new scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewScanPlant {
    /// An existing, live plant of the garden.
    Existing(PlantId),
    /// A plant created in the same transaction.
    New {
        /// Its id.
        id: PlantId,
        /// Name and species.
        plant: NewPlantInput,
    },
}

/// A depth map stored with its photo ([contracts 3], depth; [database 3] `scan_image.depth_*`).
#[derive(Debug, Clone, PartialEq)]
pub struct StoredDepth {
    /// The 16-bit depth PNG.
    pub depth_key: ObjectKey,
    /// The 8-bit confidence PNG, when sent.
    pub confidence_key: Option<ObjectKey>,
    /// The metadata, transformed into the stored image's frame.
    pub meta: DepthMeta,
    /// The stored depth map's width.
    pub width: u32,
    /// Its height.
    pub height: u32,
}

/// The quality scores of a stored image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageQuality {
    /// Variance of the Laplacian.
    pub blur_score: f64,
    /// Mean exposure.
    pub exposure: f64,
}

/// An image the API processed and wrote to `RustFS`, to record ([storage 4]: the object first,
/// then the record).
#[derive(Debug, Clone, PartialEq)]
pub struct NewScanImage {
    /// Its id.
    pub id: ImageId,
    /// Its kind.
    pub kind: ImageKind,
    /// The question it answers.
    pub question: Option<QuestionId>,
    /// Stored width.
    pub width: u32,
    /// Stored height.
    pub height: u32,
    /// Its storage key.
    pub storage_key: ObjectKey,
    /// Its quality scores.
    pub quality: ImageQuality,
    /// Its depth, when stored.
    pub depth: Option<StoredDepth>,
    /// With `question`: the photo answer's first step, decided by the pipeline before the
    /// upload is recorded (`Pipeline::photo_answer_start`), so the question's step and its job
    /// are written in the same transaction as the image.
    pub photo_answer_start: Option<PhotoAnswerStart>,
    /// Now.
    pub uploaded_at: Timestamp,
}

/// A photo answer's first step ([contracts 3], a photo answer that stalls; [contracts 7],
/// photos sent for a question). Recorded with the image, so an `answering` question always
/// has either a job in flight (`segmenting`) or a step the sweep recovers (`measuring`).
#[derive(Debug, Clone, PartialEq)]
pub enum PhotoAnswerStart {
    /// `segmenting`: the segmentation-only `perception` job, queued in the same transaction.
    /// Boxed: a `NewJob` is about 290 bytes and `Measure` carries nothing, so the box keeps
    /// every `NewScanImage` small (`clippy::large_enum_variant`).
    Segment(Box<NewJob>),
    /// `measuring`: `api`'s own step, with no job; it measures what needs no mask (or nothing)
    /// and queues `verify`, or claims `score`.
    Measure,
}

/// What recording an upload did. The transaction re-checks the scan's status and `deleting_at`,
/// the kind limit and the question's state, so a racing delete or a second photo for the same
/// question loses.
#[derive(Debug, Clone, PartialEq)]
pub struct AddedImage {
    /// The image.
    pub image: ScanImage<Stored>,
    /// Where it went.
    pub route: UploadRoute,
    /// The new guided check, with its `guided` job queued in the same transaction.
    pub guided_check: Option<GuidedCheckId>,
    /// A question photo: the question is now `answering`, in its first step.
    pub photo_answer: Option<QuestionStepClaim>,
}

/// A run to start: submit, resubmit or species change ([contracts 3]). One transaction moves
/// the status, increments `run`, cancels the old run's queued and leased run jobs, supersedes
/// the open question, releases the step claim and queues the first job.
#[derive(Debug, Clone, PartialEq)]
pub struct RunStart {
    /// Why.
    pub reason: RunReason,
    /// The status the decision was made from; the transaction refuses any other.
    pub expected_status: ScanStatus,
    /// The run being replaced (the scan's current `run`).
    pub expected_run: Run,
    /// The working species, when known.
    pub working_species: Option<SpeciesId>,
    /// The run's first job (the `perception` job).
    pub first_job: NewJob,
    /// Now.
    pub now: Timestamp,
}

/// What starting a run did, for the events the per-scan writer sends next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedRun {
    /// The new run.
    pub run: Run,
    /// The status before.
    pub from: ScanStatus,
    /// The status now (`queued` or `analysing`).
    pub to: ScanStatus,
    /// The question superseded, if one was open or `answering`.
    pub superseded_question: Option<QuestionId>,
    /// Jobs cancelled; workers learn it from their next heartbeat.
    pub cancelled_jobs: Vec<crate::id::JobId>,
}

/// What failing a scan did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedScan {
    /// The status before.
    pub from: ScanStatus,
    /// The question superseded in the same transaction: a failed scan never holds one.
    pub superseded_question: Option<QuestionId>,
}

/// A mask to store: the API encoded the worker's RLE as a PNG and wrote it to `RustFS`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMask {
    /// Its id.
    pub id: MaskId,
    /// The image.
    pub image: ImageId,
    /// The region it outlines.
    pub region_id: RegionId,
    /// Its storage key.
    pub storage_key: ObjectKey,
    /// The worker's mask, kept for measurement.
    pub rle: RleMask,
}

/// Everything a step reads when it starts ([contracts 3]: every step re-reads its inputs).
#[derive(Debug, Clone, PartialEq)]
pub struct ScanSnapshot {
    /// The scan, its images, guided checks and open question.
    pub scan: Scan<Stored>,
    /// Its plant.
    pub plant: Plant<Stored>,
    /// Its garden.
    pub garden: GardenContext,
    /// The species the run works with.
    pub working_species: Option<SpeciesId>,
    /// "Something else" was chosen.
    pub custom_species: bool,
    /// The current observations ([contracts 3], observations across runs).
    pub observations: Vec<Observation<Stored>>,
    /// Every stored indicator answer of the run, of every source (`user`, `measurement`,
    /// `vlm`); the scoring keeps one per indicator, ranked user > measurement > vlm.
    pub evidence: Vec<IndicatorEvidence>,
    /// The run's candidate set as `retrieve` selected it, plus any a `verify` job added
    /// (`StepOutput::Retrieve.candidates`, `StepOutput::Score.added_candidates`).
    pub candidates: Vec<SelectedCandidate>,
    /// The run's weather snapshot (`scan.weather`), stored by `retrieve`.
    pub weather: Option<WeatherSummary>,
    /// The open question's photo-answer step claim (`question.step`, `step_at`, its `run`)
    /// while it is `answering` with a step. `None` with `answering` means its `verify` job has
    /// completed: the store clears the step in the job's completion transaction, and the next
    /// `score` resolves the question. The broker path uses the claim to move or drop a photo
    /// answer whose job failed for good.
    pub open_question_claim: Option<QuestionStepClaim>,
    /// The indicator the open question asks (`question.indicator`), for an `indicator`
    /// question.
    pub open_question_indicator: Option<crate::id::IndicatorId>,
    /// The masks of the scan's regions, for measurement.
    pub masks: Vec<NewMask>,
    /// Each image's stored depth.
    pub depth: Vec<(ImageId, StoredDepth)>,
    /// Questions asked in the current run.
    pub questions_asked: u32,
    /// Every question of the run, open or resolved.
    pub questions: Vec<Question>,
    /// The current diagnosis.
    pub diagnosis: Option<Diagnosis<Stored>>,
    /// The current plan.
    pub plan: Option<ActionPlan>,
    /// The diagnosis the current plan was written from (`plan.diagnosis_id`), for the
    /// re-planning rules: its run (a species change) and its soil texture (a guided check).
    pub plan_diagnosis: Option<Diagnosis<Stored>>,
    /// New input arrived while a step held the claim.
    pub rescore_pending: bool,
}

/// `POST /scans/{id}/feedback`, with its author.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFeedback {
    /// Its id.
    pub id: FeedbackId,
    /// The author; deleted with their account.
    pub user: UserId,
    /// The body.
    pub request: FeedbackRequest,
}

/// The `scan`, `scan_image` and `mask` tables.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait ScanRepo: Send + Sync {
    /// `POST /scans`: creates the scan in `pending` (and the new plant), and counts it in
    /// `usage_daily` in the same transaction ([database 4], limits): over 30 is
    /// `Rejected(BudgetExceeded)` and nothing is stored.
    async fn create_scan(
        &self,
        editor: Membership<Editor>,
        scan: NewScan,
    ) -> Result<Scan<Stored>, StoreError>;

    /// The scan, skipping one being deleted.
    async fn scan(
        &self,
        member: Membership<Viewer>,
        scan: ScanId,
    ) -> Result<Scan<Stored>, StoreError>;

    /// `GET /scans/{id}`.
    async fn scan_detail(
        &self,
        member: Membership<Viewer>,
        scan: ScanId,
    ) -> Result<ScanDetail<Stored>, StoreError>;

    /// `POST /scans/{id}/images`, after the object is written.
    async fn add_image(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
        image: NewScanImage,
    ) -> Result<AddedImage, StoreError>;

    /// `DELETE /scans/{id}/images/{image_id}` while `pending`: deletes the record and returns
    /// the keys (photo, depth, confidence) to delete in the background.
    async fn remove_image(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
        image: ImageId,
    ) -> Result<Vec<ObjectKey>, StoreError>;

    /// Submit, resubmit and species change: see [`RunStart`].
    async fn start_run(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
        start: RunStart,
    ) -> Result<StartedRun, StoreError>;

    /// Fails the scan's run `run` (job attempts exhausted, `no_plant_found`,
    /// `photos_unusable`, `upstream_unavailable`): releases the claim and supersedes its
    /// question in the same transaction. Refused when the run is no longer current.
    async fn fail_scan(
        &self,
        scan: ScanId,
        run: Run,
        failure: ScanFailure,
        now: Timestamp,
    ) -> Result<FailedScan, StoreError>;

    /// `POST /scans/{id}/feedback`; refused while `pending`.
    async fn record_feedback(
        &self,
        reviewer: Membership<Reviewer>,
        scan: ScanId,
        feedback: NewFeedback,
    ) -> Result<FeedbackId, StoreError>;

    /// `GET /scans/{id}/images/{image_id}`: the object behind a signed URL (the signature is
    /// the authorization). Depth maps are never returned.
    async fn image_object(
        &self,
        scan: ScanId,
        image: ImageId,
    ) -> Result<Option<ObjectRef>, StoreError>;

    /// `GET /scans/{id}/masks/{mask_id}`.
    async fn mask_object(
        &self,
        scan: ScanId,
        mask: MaskId,
    ) -> Result<Option<ObjectRef>, StoreError>;

    /// Everything a step reads, in one consistent read.
    async fn snapshot(&self, scan: ScanId) -> Result<ScanSnapshot, StoreError>;
}
