//! Worker API and jobs ([contracts 5]): heartbeat, claim, the job with its payload keyed by
//! kind, worker events with partial results, completion and results.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use utoipa::ToSchema;

use crate::{
    events::{Skipped, Stage},
    id::{
        ImageId, IndicatorId, JobId, LeaseToken, OrganismId, ProblemId, ProblemOrHealthy,
        QuestionId, RegionId, ScanId, SpeciesId, WorkerId,
    },
    scan::ImageKind,
    vlm::{GuidedTexture, VlmFindings, VlmIndicatorAnswers},
    wire::{BBox, Confidence, Run, Timestamp, V1},
};

/// `Capability` ([contracts 5]).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// The perception worker (RTX 3070): `perception` jobs.
    Perception,
    /// The VLM worker (RTX 5070 Ti): `vlm`, `verify` and `guided` jobs.
    Vlm,
    /// The fallback worker (LLM API): every kind, 15 seconds after `queued_at`.
    Fallback,
}

/// `Job.kind` ([contracts 5]).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    /// Plant check, segmentation, species, disease, image embeddings.
    Perception,
    /// The findings and indicators calls.
    Vlm,
    /// Indicators for a candidate outside the set, or a new photo.
    Verify,
    /// One guided soil check; belongs to the scan, not a run.
    Guided,
}

impl JobKind {
    /// Every kind.
    pub const ALL: [Self; 4] = [Self::Perception, Self::Vlm, Self::Verify, Self::Guided];

    /// The wire name and the `job.kind` value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Perception => "perception",
            Self::Vlm => "vlm",
            Self::Verify => "verify",
            Self::Guided => "guided",
        }
    }
}

/// `Heartbeat.gpu`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GpuInfo {
    /// "NVIDIA `GeForce` RTX 3070".
    pub name: String,
    /// VRAM in use, MB.
    pub vram_used_mb: u32,
    /// Total VRAM, MB.
    pub vram_total_mb: u32,
}

/// `Heartbeat.current_jobs[]`: the leases to extend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HeldJob {
    /// The job.
    pub job_id: JobId,
    /// Its lease token.
    pub lease_token: LeaseToken,
}

/// `POST /internal/workers/heartbeat` ([contracts 5]), every 10 seconds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Heartbeat {
    /// Stable per process.
    pub worker_id: WorkerId,
    /// What it claims.
    pub caps: Vec<Capability>,
    /// The most jobs it holds at once.
    pub slots: u32,
    /// The GPU, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub gpu: Option<GpuInfo>,
    /// Extends these leases, within `max_runtime`.
    pub current_jobs: Vec<HeldJob>,
    /// Model name → version, shown on `/status`.
    pub versions: BTreeMap<String, String>,
}

/// `200` of the heartbeat.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct HeartbeatResponse {
    /// Stop these jobs now.
    pub cancel_job_ids: Vec<JobId>,
}

/// `POST /internal/jobs/claim` ([contracts 5]), a long poll.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ClaimRequest {
    /// The worker.
    pub worker_id: WorkerId,
    /// Its capabilities.
    pub caps: Vec<Capability>,
    /// As in the heartbeat; a worker already holding `slots` jobs gets `204`.
    pub slots: u32,
    /// Default 25, at most 30.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub wait_s: Option<u32>,
}

impl ClaimRequest {
    /// The default long-poll wait.
    pub const DEFAULT_WAIT_S: u32 = 25;
    /// The longest long-poll wait.
    pub const MAX_WAIT_S: u32 = 30;

    /// The wait to use.
    pub fn wait_s(&self) -> u32 {
        self.wait_s
            .unwrap_or(Self::DEFAULT_WAIT_S)
            .min(Self::MAX_WAIT_S)
    }
}

/// `JobImage` ([contracts 5]). `url` is the worker route
/// `GET /internal/jobs/{job_id}/images/{image_id}`, authorised by the lease token; it is not
/// signed and never expires, so it is a [`WorkerImagePath`] and may be stored in the payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct JobImage {
    /// The image.
    pub image_id: ImageId,
    /// Its kind.
    pub kind: ImageKind,
    /// Stored width.
    pub width: u32,
    /// Stored height.
    pub height: u32,
    /// The worker route for its bytes.
    pub url: WorkerImagePath,
}

/// `/internal/jobs/{job_id}/images/{image_id}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
pub struct WorkerImagePath(String);

impl WorkerImagePath {
    /// The route of one image of one job.
    pub fn new(job: JobId, image: ImageId) -> Self {
        Self(format!("/internal/jobs/{job}/images/{image}"))
    }

    /// The path.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// `PerceptionPayload.stages[]`: the perception stages only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PerceptionStage {
    /// Plant present?
    PlantCheck,
    /// Regions and masks.
    Segmentation,
    /// `BioCLIP`.
    Species,
    /// The disease classifier.
    Disease,
    /// Image vectors.
    ImageEmbedding,
}

impl PerceptionStage {
    /// Every perception stage, in order.
    pub const ALL: [Self; 5] = [
        Self::PlantCheck,
        Self::Segmentation,
        Self::Species,
        Self::Disease,
        Self::ImageEmbedding,
    ];

    /// The same stage in the full list.
    pub const fn stage(self) -> Stage {
        match self {
            Self::PlantCheck => Stage::PlantCheck,
            Self::Segmentation => Stage::Segmentation,
            Self::Species => Stage::Species,
            Self::Disease => Stage::Disease,
            Self::ImageEmbedding => Stage::ImageEmbedding,
        }
    }
}

/// `RegionClass` ([contracts 5]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RegionClass {
    /// The whole plant.
    Plant,
    /// A leaf (the reliable class).
    Leaf,
    /// Soil.
    Soil,
    /// Surroundings.
    Surroundings,
}

/// `PerceptionPayload.regions[]`: `Region & { region_id: string; class: RegionClass }` from an
/// earlier run, when segmentation isn't re-run. The optional `mask_url` of `Region` is never
/// sent: workers have no route to masks, and species and disease work from the boxes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PriorRegion {
    /// Always present here.
    pub region_id: RegionId,
    /// The image.
    pub image_id: ImageId,
    /// The box.
    pub bbox: BBox,
    /// The class.
    pub class: RegionClass,
}

/// `PerceptionPayload` ([contracts 5]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PerceptionPayload {
    /// Every plant and soil image of the scan, or only a question's photo; never guided-check
    /// images, never depth.
    pub images: Vec<JobImage>,
    /// The stages to run.
    pub stages: Vec<PerceptionStage>,
    /// Confirmed or user-chosen species: skip identification, use this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub species_id: Option<SpeciesId>,
    /// True when the user chose "something else" (no `species_id`). Omitted when false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip_species_identification: bool,
    /// From an earlier run, when segmentation isn't re-run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub regions: Option<Vec<PriorRegion>>,
    /// `BioCLIP` label list version.
    pub label_set_version: String,
    /// Classifier class → problem mapping version.
    pub classifier_labels_version: String,
    /// Set on a question photo's segmentation-only job: `stages` is `["segmentation"]`,
    /// `images` holds that photo alone, and nothing it returns can fail the scan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub question_id: Option<QuestionId>,
}

/// `IndicatorWhere` ([contracts 10]): where on the plant an indicator shows.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum IndicatorWhere {
    /// Lower leaves.
    LowerLeaves,
    /// Upper leaves.
    UpperLeaves,
    /// The whole plant.
    WholePlant,
    /// The leaf underside.
    LeafUnderside,
    /// The stem.
    Stem,
    /// The fruit.
    Fruit,
    /// The soil.
    Soil,
}

/// `Indicator` ([contracts 5]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Indicator {
    /// `"tomato_early_blight.concentric_rings"`.
    pub indicator_id: IndicatorId,
    /// Asked to the VLM verbatim.
    pub question: String,
    /// Where it shows.
    pub r#where: IndicatorWhere,
    /// Which shots can show it.
    pub image_kinds: Vec<ImageKind>,
}

/// `CandidatePrompt` ([contracts 5]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CandidatePrompt {
    /// The problem.
    pub problem_id: ProblemId,
    /// Its name.
    pub name: String,
    /// Its 3 discriminating indicators, minus those a measurement settled; a candidate with
    /// none left is not sent.
    pub indicators: Vec<Indicator>,
}

/// `VlmPayload.working_species`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct WorkingSpecies {
    /// `null` for a name outside the knowledge base.
    #[schema(required = true)]
    pub id: Option<SpeciesId>,
    /// Its name.
    pub name: String,
}

/// `VlmPayload.species_options[]`: allowed `species_guess` values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SpeciesOption {
    /// The species.
    pub id: SpeciesId,
    /// Its name.
    pub name: String,
}

/// `VlmPayload.organism_options[]`: allowed organism labels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct OrganismOption {
    /// The organism.
    pub id: OrganismId,
    /// Its name.
    pub name: String,
}

/// `VlmPayload` ([contracts 5]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct VlmPayload {
    /// At most 4, fixed order ([`crate::rules::routing`]).
    pub findings_images: Vec<JobImage>,
    /// At most 4, chosen from the indicators; empty when measurements answered all.
    pub indicator_images: Vec<JobImage>,
    /// The working species.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub working_species: Option<WorkingSpecies>,
    /// Allowed `species_guess` values.
    pub species_options: Vec<SpeciesOption>,
    /// Allowed organism labels.
    pub organism_options: Vec<OrganismOption>,
    /// At most 9; empty when measurements answered every indicator.
    pub candidates: Vec<CandidatePrompt>,
}

/// `VerifyPayload.reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerifyReason {
    /// The VLM found an organism or damage pattern outside the candidate set.
    OutsideCandidateSet,
    /// A photo sent for a question.
    NewPhoto,
}

/// `VerifyPayload` ([contracts 5]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct VerifyPayload {
    /// Why.
    pub reason: VerifyReason,
    /// With `new_photo`: the question the photo answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub question_id: Option<QuestionId>,
    /// At most 4; for `new_photo`, the new shot first.
    pub images: Vec<JobImage>,
    /// The candidates to ask about.
    pub candidates: Vec<CandidatePrompt>,
}

/// `GuidedPayload` ([contracts 5]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GuidedPayload {
    /// One `squeeze_test` or `jar_test` image, alone.
    pub image: JobImage,
}

/// `kind` and `payload` side by side ([contracts 5], `Job`): adjacently tagged, flattened into
/// [`Job`]. Stored as `job.kind` and `job.payload`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum JobSpec {
    /// A `perception` job.
    Perception(PerceptionPayload),
    /// A `vlm` job.
    Vlm(VlmPayload),
    /// A `verify` job.
    Verify(VerifyPayload),
    /// A `guided` job.
    Guided(GuidedPayload),
}

impl JobSpec {
    /// The job's kind.
    pub const fn kind(&self) -> JobKind {
        match self {
            Self::Perception(_) => JobKind::Perception,
            Self::Vlm(_) => JobKind::Vlm,
            Self::Verify(_) => JobKind::Verify,
            Self::Guided(_) => JobKind::Guided,
        }
    }

    /// The question a photo answer's job belongs to (`job.question`, [database 3]).
    pub fn question(&self) -> Option<QuestionId> {
        match self {
            Self::Perception(p) => p.question_id,
            Self::Verify(p) => p.question_id,
            Self::Vlm(_) | Self::Guided(_) => None,
        }
    }
}

/// `Job` ([contracts 5]): `200` of the claim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Job {
    /// `1`.
    pub v: V1,
    /// The job.
    pub job_id: JobId,
    /// Its scan.
    pub scan_id: ScanId,
    /// Absent for guided jobs, which don't belong to a run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub run: Option<Run>,
    /// 1 on the first try.
    pub attempt: u32,
    /// Sent as `X-Lease-Token` on every later call.
    pub lease_token: LeaseToken,
    /// When the lease ends unless a heartbeat extends it.
    pub lease_until: Timestamp,
    /// The kind's `max_runtime`, doubled on the fallback worker.
    pub max_runtime_s: u32,
    /// `kind` and `payload`.
    #[serde(flatten)]
    pub spec: JobSpec,
}

/// The fields a worker needs to fail a job, readable from any version: the worker decodes this
/// first, and the full [`Job`] only when `v` is 1 ([contracts 1], versioning).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct JobHead {
    /// The payload's version.
    pub v: u64,
    /// The job.
    pub job_id: JobId,
    /// Its lease token.
    pub lease_token: LeaseToken,
}

/// What a worker makes of a claimed body.
#[derive(Debug, Clone, PartialEq)]
pub enum ClaimedJob {
    /// A job this worker can read.
    Job(Box<Job>),
    /// Complete it as failed with `unsupported_version`.
    Unsupported(JobHead),
}

impl ClaimedJob {
    /// Decodes the head, checks `v`, then decodes the job.
    pub fn decode(body: &[u8]) -> Result<Self, serde_json::Error> {
        let head: JobHead = serde_json::from_slice(body)?;
        if head.v != 1 {
            return Ok(Self::Unsupported(head));
        }
        serde_json::from_slice(body).map(|job| Self::Job(Box::new(job)))
    }
}

// ---------------------------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------------------------

/// `RleMask` ([contracts 5]): COCO-style compressed run-length encoding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RleMask {
    /// Height, width of the stored image.
    pub size: [u32; 2],
    /// The compressed counts.
    pub counts: String,
}

/// `PerceptionResult.regions[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PerceptionRegion {
    /// `"<image_id>.<n>"`.
    pub region_id: RegionId,
    /// The image.
    pub image_id: ImageId,
    /// The class.
    pub class: RegionClass,
    /// The box.
    pub bbox: BBox,
    /// The API stores it as a PNG and serves it through a signed URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub mask: Option<RleMask>,
    /// The detector's confidence.
    pub confidence: Confidence,
}

/// `PerceptionResult.species.candidates[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SpeciesLabel {
    /// `null` for a label outside the knowledge base.
    #[schema(required = true)]
    pub species_id: Option<SpeciesId>,
    /// The `BioCLIP` label.
    pub label: String,
    /// Its score.
    pub score: Confidence,
}

/// `PerceptionResult.species`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SpeciesResult {
    /// Top candidates.
    pub candidates: Vec<SpeciesLabel>,
}

/// `PerceptionResult.disease[].top[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ProblemScore {
    /// A problem, or `"healthy"`.
    pub problem_id: ProblemOrHealthy,
    /// Its confidence.
    pub confidence: Confidence,
}

/// `PerceptionResult.disease[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct DiseaseResult {
    /// The image.
    pub image_id: ImageId,
    /// Absent when the classifier ran on the whole close-up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub region_id: Option<RegionId>,
    /// Top 3, restricted to the working species' labels.
    pub top: Vec<ProblemScore>,
}

/// `PerceptionResult.disease_skipped.reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiseaseSkipReason {
    /// The classifier doesn't cover the species.
    SpeciesNotSupported,
    /// No leaf region and no close-up.
    NoLeavesFound,
}

/// `PerceptionResult.disease_skipped`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DiseaseSkipped {
    /// Why.
    pub reason: DiseaseSkipReason,
}

/// `PerceptionResult.image_embeddings[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ImageEmbedding {
    /// The image.
    pub image_id: ImageId,
    /// The embedding model.
    pub model: String,
    /// The vector.
    pub vector: Vec<f32>,
}

/// `PerceptionResult` ([contracts 5]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PerceptionResult {
    /// Present when `plant_check` was requested; `false` fails the scan with `no_plant_found`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plant_present: Option<bool>,
    /// Regions found.
    pub regions: Vec<PerceptionRegion>,
    /// `BioCLIP`'s candidates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub species: Option<SpeciesResult>,
    /// The classifier's results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub disease: Option<Vec<DiseaseResult>>,
    /// Why disease classification didn't run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub disease_skipped: Option<DiseaseSkipped>,
    /// Image vectors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub image_embeddings: Option<Vec<ImageEmbedding>>,
}

/// `VlmResult` ([contracts 5]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct VlmResult {
    /// Aliases already mapped back to ids.
    pub findings: VlmFindings,
    /// Empty when the indicators call was skipped.
    pub indicators: VlmIndicatorAnswers,
}

/// `VerifyResult` ([contracts 5]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct VerifyResult {
    /// The answers.
    pub indicators: VlmIndicatorAnswers,
}

/// `GuidedResult` ([contracts 5]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct GuidedResult {
    /// The texture reading.
    pub texture: GuidedTexture,
}

/// A decoded job result. contracts 5 sends `result` with no tag
/// (`PerceptionResult | VlmResult | VerifyResult | GuidedResult`), and an untagged decoder would
/// take a `VlmResult` for a `VerifyResult` (a subset of its fields), so the API keeps the raw
/// JSON and decodes it for the kind it has on record ([`JobResult::decode`]).
#[derive(Debug, Clone, PartialEq)]
pub enum JobResult {
    /// From a `perception` job.
    Perception(PerceptionResult),
    /// From a `vlm` job.
    Vlm(VlmResult),
    /// From a `verify` job.
    Verify(VerifyResult),
    /// From a `guided` job.
    Guided(GuidedResult),
}

impl JobResult {
    /// Decodes `result` for the job's recorded kind.
    pub fn decode(kind: JobKind, result: serde_json::Value) -> Result<Self, serde_json::Error> {
        fn parse<T: DeserializeOwned>(v: serde_json::Value) -> Result<T, serde_json::Error> {
            serde_json::from_value(v)
        }
        Ok(match kind {
            JobKind::Perception => Self::Perception(parse(result)?),
            JobKind::Vlm => Self::Vlm(parse(result)?),
            JobKind::Verify => Self::Verify(parse(result)?),
            JobKind::Guided => Self::Guided(parse(result)?),
        })
    }

    /// The kind this result belongs to.
    pub const fn kind(&self) -> JobKind {
        match self {
            Self::Perception(_) => JobKind::Perception,
            Self::Vlm(_) => JobKind::Vlm,
            Self::Verify(_) => JobKind::Verify,
            Self::Guided(_) => JobKind::Guided,
        }
    }

    /// The JSON the worker sends.
    pub fn to_value(&self) -> Result<serde_json::Value, serde_json::Error> {
        match self {
            Self::Perception(r) => serde_json::to_value(r),
            Self::Vlm(r) => serde_json::to_value(r),
            Self::Verify(r) => serde_json::to_value(r),
            Self::Guided(r) => serde_json::to_value(r),
        }
    }
}

/// `JobErrorCode` ([contracts 5], complete): the codes a worker sends in `CompleteJob.error`.
/// The contract types the field as `JobErrorCode | string`, so a code this build doesn't know
/// (from a newer worker) still decodes, as [`JobErrorCode::Other`], and is stored as written.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum JobErrorCode {
    /// A payload or job `v` the worker doesn't support, a job kind it doesn't know, or a payload
    /// it can't read; `retryable: false`.
    UnsupportedVersion,
    /// A `label_set_version` or `classifier_labels_version` the worker hasn't loaded;
    /// `retryable: false`.
    UnknownLabelVersion,
    /// Inference failed, or its output failed validation twice; `retryable: true`, except
    /// `false` when the worker's adapter can't do the job's kind at all.
    ModelError,
    /// An image URL failed after retries; `retryable: true`.
    ImageFetchFailed,
    /// The worker stopped the job itself at its soft deadline, before `max_runtime_s` (a hung
    /// or slow model); `retryable: true`. Settled by contracts 12, question 18.
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
    /// [`JobErrorCode::Other`], whose `retryable` flag alone decides.
    pub const fn retryable(&self) -> Option<bool> {
        match self {
            Self::UnsupportedVersion | Self::UnknownLabelVersion => Some(false),
            Self::ModelError | Self::ImageFetchFailed | Self::MaxRuntimeExceeded => Some(true),
            Self::Other(_) => None,
        }
    }
}

impl From<&str> for JobErrorCode {
    fn from(code: &str) -> Self {
        match code {
            "unsupported_version" => Self::UnsupportedVersion,
            "unknown_label_version" => Self::UnknownLabelVersion,
            "model_error" => Self::ModelError,
            "image_fetch_failed" => Self::ImageFetchFailed,
            "max_runtime_exceeded" => Self::MaxRuntimeExceeded,
            other => Self::Other(other.to_owned()),
        }
    }
}

impl std::fmt::Display for JobErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for JobErrorCode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for JobErrorCode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        Ok(Self::from(code.as_ref()))
    }
}

crate::schema_util::string_schema!(
    JobErrorCode,
    "JobErrorCode",
    None,
    "unsupported_version | unknown_label_version | model_error | image_fetch_failed | max_runtime_exceeded, or another string"
);

/// The keys of `CompleteJob.model_versions` and the heartbeat's `versions` ([contracts 5],
/// `model_versions` keys). Plain constants: the map stays `BTreeMap<String, String>`, because
/// each model a job used adds its own `<model name>` key beside these.
pub struct ModelVersionKey;

impl ModelVersionKey {
    /// Every worker: `rig-9070xt-vlm-1`.
    pub const WORKER_ID: &'static str = "worker_id";
    /// VLM and fallback workers: `Qwen/Qwen3-VL-8B-Instruct-FP8@<revision>`.
    pub const VLM_MODEL: &'static str = "vlm_model";
    /// VLM workers: `fp8_block`, `awq_w4a16`.
    pub const VLM_QUANT: &'static str = "vlm_quant";
    /// VLM and fallback workers: `vllm@v0.31.0`, `vllm-rocm@v0.31.0`, `anthropic`.
    pub const VLM_SERVER: &'static str = "vlm_server";
    /// VLM and fallback workers: `norm_1000`.
    pub const VLM_BOX_FORMAT: &'static str = "vlm_box_format";
    /// VLM and fallback workers: `1003520`.
    pub const VLM_MAX_PIXELS: &'static str = "vlm_max_pixels";
    /// VLM and fallback workers: `findings@3,indicators@2,guided@1`.
    pub const VLM_PROMPTS: &'static str = "vlm_prompts";
    /// VLM and fallback workers: `0.4/0.65/0.85`.
    pub const VLM_CERTAINTY: &'static str = "vlm_certainty";
    /// Perception workers: the first 16 hex digits of the manifest's configuration hash.
    pub const PERCEPTION_CONFIG: &'static str = "perception_config";
    /// Workers that run ONNX models: `cuda`, `migraphx`, `cpu`.
    pub const ORT_EP: &'static str = "ort_ep";
    /// The label list a perception job used (in the heartbeat: every loaded one, comma-separated).
    pub const LABEL_SET_VERSION: &'static str = "label_set_version";
    /// The classifier mapping a perception job used (in the heartbeat: every loaded one).
    pub const CLASSIFIER_LABELS_VERSION: &'static str = "classifier_labels_version";
}

/// `CompleteJob.error` ([contracts 5]): job errors travel here, never as HTTP status codes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct JobError {
    /// A [`JobErrorCode`], or another string.
    pub code: JobErrorCode,
    /// Detail for the log.
    pub message: String,
    /// Counts toward `attempts` when true.
    pub retryable: bool,
}

impl JobError {
    /// The code for a payload version the worker doesn't support.
    pub const UNSUPPORTED_VERSION: &'static str = "unsupported_version";

    /// A failure whose `retryable` follows the code ([`JobErrorCode::retryable`]); an
    /// [`JobErrorCode::Other`] code is not retryable.
    pub fn new(code: JobErrorCode, message: impl Into<String>) -> Self {
        let retryable = code.retryable().unwrap_or(false);
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }
}

/// `status` with `result` (done) or `error` (failed), flattened into [`CompleteJob`]. The tag
/// makes the other combinations unrepresentable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum JobOutcome {
    /// The job finished.
    Done {
        /// The kind's result, decoded with [`JobResult::decode`].
        #[schema(value_type = Object)]
        result: serde_json::Value,
    },
    /// The job failed.
    Failed {
        /// Why.
        error: JobError,
    },
}

/// `POST /internal/jobs/{id}/complete` ([contracts 5]). A repeat with the same lease token for
/// a completed job returns `204` and changes nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CompleteJob {
    /// `1`.
    pub v: V1,
    /// `status`, with `result` or `error`.
    #[serde(flatten)]
    pub outcome: JobOutcome,
    /// Model name → version.
    pub model_versions: BTreeMap<String, String>,
    /// Stage → milliseconds.
    pub timings_ms: BTreeMap<String, u64>,
}

// ---------------------------------------------------------------------------------------------
// Worker events
// ---------------------------------------------------------------------------------------------

/// `PartialResult` ([contracts 5]): the complete output of one finished stage, tagged by its
/// own `stage`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum PartialResult {
    /// The plant check.
    PlantCheck {
        /// Plant present?
        plant_present: bool,
    },
    /// Segmentation.
    Segmentation {
        /// Its regions.
        regions: Vec<PerceptionRegion>,
    },
    /// `BioCLIP`.
    Species {
        /// Its candidates.
        species: SpeciesResult,
    },
    /// The classifier.
    Disease {
        /// Its results.
        disease: Vec<DiseaseResult>,
    },
    /// The VLM's findings call.
    VlmFindings {
        /// The findings.
        findings: VlmFindings,
    },
}

impl PartialResult {
    /// The stage it belongs to; the API checks it equals the event's `stage`.
    pub const fn stage(&self) -> Stage {
        match self {
            Self::PlantCheck { .. } => Stage::PlantCheck,
            Self::Segmentation { .. } => Stage::Segmentation,
            Self::Species { .. } => Stage::Species,
            Self::Disease { .. } => Stage::Disease,
            Self::VlmFindings { .. } => Stage::VlmFindings,
        }
    }
}

/// `WorkerEvents.events[]` ([contracts 5]), tagged by `type`. An event type from a newer
/// worker decodes as `Unknown` and is ignored; an unknown value inside a known event is still
/// an error, because the API must store it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerEvent {
    /// A stage started.
    StageStarted {
        /// The stage.
        stage: Stage,
    },
    /// A stage completed.
    StageCompleted {
        /// The stage.
        stage: Stage,
        /// How long it took.
        duration_ms: u64,
        /// Why it was skipped.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schema(nullable = false)]
        skipped: Option<Skipped>,
    },
    /// One finished stage's output.
    PartialResult {
        /// The stage.
        stage: Stage,
        /// Its output.
        result: PartialResult,
    },
    /// An event type this API doesn't know. Ignored; never sent.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// `POST /internal/jobs/{id}/events` ([contracts 5]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct WorkerEvents {
    /// `1`.
    pub v: V1,
    /// The events, in order.
    pub events: Vec<WorkerEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(job: &str, id: &str, kind: ImageKind) -> JobImage {
        JobImage {
            image_id: id.parse().unwrap(),
            kind,
            width: 2048,
            height: 1536,
            url: WorkerImagePath::new(job.parse().unwrap(), id.parse().unwrap()),
        }
    }

    const JOB: &str = "01JAX5AAAAAAAAAAAAAAAAAAAA";
    const IMG: &str = "01JAX6BBBBBBBBBBBBBBBBBBBB";

    fn job(spec: JobSpec, run: Option<u32>) -> Job {
        Job {
            v: V1,
            job_id: JOB.parse().unwrap(),
            scan_id: "01JAX3K9T6R2B8V0QZ4M7D1E5F".parse().unwrap(),
            run: run.map(Run),
            attempt: 1,
            lease_token: "01JAX7CCCCCCCCCCCCCCCCCCCC".parse().unwrap(),
            lease_until: "2026-10-18T14:05:22Z".parse().unwrap(),
            max_runtime_s: 120,
            spec,
        }
    }

    fn round_trip(job: &Job, kind: &str) {
        let value = serde_json::to_value(job).unwrap();
        assert_eq!(value["kind"], kind);
        assert!(value["payload"].is_object());
        assert_eq!(value["v"], 1);
        let back: Job = serde_json::from_value(value).unwrap();
        assert_eq!(&back, job);
        assert_eq!(back.spec.kind().as_str(), kind);
    }

    #[test]
    fn job_with_each_payload_kind() {
        let perception = job(
            JobSpec::Perception(PerceptionPayload {
                images: vec![image(JOB, IMG, ImageKind::WholePlant)],
                stages: PerceptionStage::ALL.to_vec(),
                species_id: None,
                skip_species_identification: false,
                regions: None,
                label_set_version: "2026-10-20".to_owned(),
                classifier_labels_version: "plantdoc-v3".to_owned(),
                question_id: None,
            }),
            Some(1),
        );
        round_trip(&perception, "perception");
        let json = serde_json::to_value(&perception).unwrap();
        assert!(json["payload"].get("skip_species_identification").is_none());
        assert_eq!(
            json["payload"]["images"][0]["url"],
            format!("/internal/jobs/{JOB}/images/{IMG}")
        );

        let candidate = CandidatePrompt {
            problem_id: "tomato_early_blight".parse().unwrap(),
            name: "Early blight".to_owned(),
            indicators: vec![Indicator {
                indicator_id: "tomato_early_blight.concentric_rings".parse().unwrap(),
                question: "Do the brown spots have rings inside them, like a target?".to_owned(),
                r#where: IndicatorWhere::LowerLeaves,
                image_kinds: vec![ImageKind::LeafCloseup],
            }],
        };
        let vlm = job(
            JobSpec::Vlm(VlmPayload {
                findings_images: vec![image(JOB, IMG, ImageKind::WholePlant)],
                indicator_images: vec![image(JOB, IMG, ImageKind::WholePlant)],
                working_species: Some(WorkingSpecies {
                    id: Some("tomato".parse().unwrap()),
                    name: "Tomato".to_owned(),
                }),
                species_options: vec![],
                organism_options: vec![],
                candidates: vec![candidate.clone()],
            }),
            Some(1),
        );
        round_trip(&vlm, "vlm");
        let json = serde_json::to_value(&vlm).unwrap();
        assert_eq!(
            json["payload"]["candidates"][0]["indicators"][0]["where"],
            "lower_leaves"
        );

        let verify = job(
            JobSpec::Verify(VerifyPayload {
                reason: VerifyReason::NewPhoto,
                question_id: Some("01JAX8DDDDDDDDDDDDDDDDDDDD".parse().unwrap()),
                images: vec![image(JOB, IMG, ImageKind::LeafCloseup)],
                candidates: vec![candidate],
            }),
            Some(2),
        );
        round_trip(&verify, "verify");

        let guided = job(
            JobSpec::Guided(GuidedPayload {
                image: image(JOB, IMG, ImageKind::JarTest),
            }),
            None,
        );
        round_trip(&guided, "guided");
        assert!(serde_json::to_value(&guided).unwrap().get("run").is_none());
    }

    #[test]
    fn unsupported_version_keeps_the_head() {
        let body = format!(
            r#"{{"v":2,"job_id":"{JOB}","lease_token":"01JAX7CCCCCCCCCCCCCCCCCCCC","task":{{}}}}"#
        );
        match ClaimedJob::decode(body.as_bytes()).unwrap() {
            ClaimedJob::Unsupported(head) => assert_eq!(head.v, 2),
            ClaimedJob::Job(_) => panic!("decoded a v2 job"),
        }
    }

    #[test]
    fn complete_is_tagged_by_status_and_results_decode_by_kind() {
        let body = r#"{"v":1,"status":"done","result":{"indicators":{"answers":[]}},
            "model_versions":{"qwen2.5-vl-7b":"awq-1"},"timings_ms":{"vlm_indicators":900}}"#;
        let done: CompleteJob = serde_json::from_str(body).unwrap();
        let JobOutcome::Done { result } = done.outcome else {
            panic!("expected done")
        };
        assert!(matches!(
            JobResult::decode(JobKind::Verify, result.clone()).unwrap(),
            JobResult::Verify(_)
        ));
        assert!(JobResult::decode(JobKind::Vlm, result).is_err());

        let failed = r#"{"v":1,"status":"failed","error":{"code":"unsupported_version",
            "message":"v2","retryable":false},"model_versions":{},"timings_ms":{}}"#;
        let failed: CompleteJob = serde_json::from_str(failed).unwrap();
        assert!(matches!(failed.outcome, JobOutcome::Failed { .. }));
        let neither = r#"{"v":1,"status":"done","model_versions":{},"timings_ms":{}}"#;
        assert!(serde_json::from_str::<CompleteJob>(neither).is_err());
    }

    #[test]
    fn job_error_codes_keep_unknown_strings() {
        let e: JobError = serde_json::from_str(
            r#"{"code":"image_fetch_failed","message":"404","retryable":true}"#,
        )
        .unwrap();
        assert_eq!(e.code, JobErrorCode::ImageFetchFailed);
        let e: JobError =
            serde_json::from_str(r#"{"code":"gpu_on_fire","message":"","retryable":false}"#)
                .unwrap();
        assert_eq!(e.code, JobErrorCode::Other("gpu_on_fire".to_owned()));
        assert_eq!(serde_json::to_value(&e).unwrap()["code"], "gpu_on_fire");
        assert!(JobError::new(JobErrorCode::ModelError, "x").retryable);
        assert!(!JobError::new(JobErrorCode::UnknownLabelVersion, "x").retryable);
    }

    #[test]
    fn worker_events_ignore_unknown_types() {
        let body = r#"{"v":1,"events":[
            {"type":"stage_started","stage":"plant_check"},
            {"type":"gpu_temperature","celsius":71},
            {"type":"partial_result","stage":"plant_check",
             "result":{"stage":"plant_check","plant_present":true}}]}"#;
        let batch: WorkerEvents = serde_json::from_str(body).unwrap();
        assert_eq!(batch.events[1], WorkerEvent::Unknown);
        let WorkerEvent::PartialResult { stage, result } = &batch.events[2] else {
            panic!("expected a partial result")
        };
        assert_eq!(*stage, result.stage());
        assert!(serde_json::to_string(&WorkerEvent::Unknown).is_err());
    }
}
