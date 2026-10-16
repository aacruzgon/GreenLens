//! Scan REST API shapes ([contracts 3]): scans, images, plants as a scan sees them, questions,
//! guided checks, uploads with depth, answers, species confirmation and feedback.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::DomainError,
    id::{
        DiagnosisId, GardenId, ImageId, PlanId, PlantId, ProblemId, QuestionId, ScanId, SpeciesId,
        Table,
    },
    records::{
        diagnosis::Diagnosis, observation::Observation, plan::ActionPlan, weather::WeatherSummary,
    },
    url::{ImagePath, Sign, Signed, SignedUrl, Signer, Stored, UrlSlot, UrlState, url_absent},
    wire::{Confidence, EventSeq, False, Run, Timestamp, True, UserRef},
};

/// `ScanStatus` ([contracts 3], scan lifecycle). The transitions are in
/// [`crate::rules::scan_state`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScanStatus {
    /// Created; photos being uploaded.
    Pending,
    /// Submitted; the `perception` job waits for a worker.
    Queued,
    /// The run's jobs and steps are working.
    Analysing,
    /// A question is open; at most 2 per run.
    AwaitingAnswer,
    /// A diagnosis exists; the plan is being written.
    Diagnosed,
    /// A plan exists. Late work never changes this status.
    Planned,
    /// The run failed with a `ScanFailure`.
    Failed,
}

impl ScanStatus {
    /// The wire name, for `InvalidStateDetails.status`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Queued => "queued",
            Self::Analysing => "analysing",
            Self::AwaitingAnswer => "awaiting_answer",
            Self::Diagnosed => "diagnosed",
            Self::Planned => "planned",
            Self::Failed => "failed",
        }
    }
}

/// `ImageKind` ([contracts 3]). The kind limits are in [`crate::rules::uploads`].
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    ToSchema,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ImageKind {
    /// Required, at most 1.
    WholePlant,
    /// At most 2.
    LeafCloseup,
    /// At most 1.
    LeafUnderside,
    /// At most 1.
    SoilSurface,
    /// At most 1.
    FlowerFruit,
    /// Guided soil check: at most 1 before submit, any number after.
    SqueezeTest,
    /// Guided soil check: at most 1 before submit, any number after.
    JarTest,
}

impl ImageKind {
    /// Every kind, in contract order.
    pub const ALL: [Self; 7] = [
        Self::WholePlant,
        Self::LeafCloseup,
        Self::LeafUnderside,
        Self::SoilSurface,
        Self::FlowerFruit,
        Self::SqueezeTest,
        Self::JarTest,
    ];

    /// `squeeze_test` or `jar_test`: analysed alone in a `guided` job, never by the run.
    pub const fn guided(self) -> Option<GuidedKind> {
        match self {
            Self::SqueezeTest => Some(GuidedKind::SqueezeTest),
            Self::JarTest => Some(GuidedKind::JarTest),
            _ => None,
        }
    }

    /// A plant or soil shot: everything but the guided checks.
    pub const fn is_plant_or_soil(self) -> bool {
        self.guided().is_none()
    }

    /// The wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WholePlant => "whole_plant",
            Self::LeafCloseup => "leaf_closeup",
            Self::LeafUnderside => "leaf_underside",
            Self::SoilSurface => "soil_surface",
            Self::FlowerFruit => "flower_fruit",
            Self::SqueezeTest => "squeeze_test",
            Self::JarTest => "jar_test",
        }
    }
}

/// `ScanImage.depth`: present for `LiDAR` shots; the depth map itself is never served.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DepthInfo {
    /// Depth map width, pixels.
    pub width: u32,
    /// Depth map height, pixels.
    pub height: u32,
    /// Plant height and canopy width need gravity.
    pub has_gravity: bool,
}

/// `ScanImage` ([contracts 3]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = ScanImage)]
pub struct ScanImage<U: UrlState = Signed> {
    /// The image.
    pub id: ImageId,
    /// Its kind.
    pub kind: ImageKind,
    /// A photo sent to answer a question; doesn't count toward kind limits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub answers_question_id: Option<QuestionId>,
    /// Stored (upright) width, pixels.
    pub width: u32,
    /// Stored (upright) height, pixels.
    pub height: u32,
    /// Signed, short-lived.
    #[serde(default, skip_serializing_if = "UrlSlot::is_absent")]
    #[schema(value_type = SignedUrl, required = true)]
    pub url: U::Url,
    /// When it was uploaded.
    pub uploaded_at: Timestamp,
    /// `LiDAR` phones only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub depth: Option<DepthInfo>,
}

impl Sign for ScanImage<Stored> {
    type Signed = ScanImage<Signed>;
    type Scope = ScanId;
    fn sign(self, scan: ScanId, signer: &Signer<'_>) -> ScanImage<Signed> {
        ScanImage {
            url: signer.url(&ImagePath::ScanImage {
                scan,
                image: self.id,
            }),
            id: self.id,
            kind: self.kind,
            answers_question_id: self.answers_question_id,
            width: self.width,
            height: self.height,
            uploaded_at: self.uploaded_at,
            depth: self.depth,
        }
    }
}

/// `SpeciesRef` ([contracts 3]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SpeciesRef {
    /// The species.
    pub id: SpeciesId,
    /// "Solanum lycopersicum".
    pub scientific_name: String,
    /// "Tomato".
    pub common_name: String,
}

/// `SpeciesCandidate.source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CandidateSource {
    /// `BioCLIP` on the perception worker.
    Bioclip,
    /// The VLM's species guess.
    Vlm,
    /// The user's choice.
    User,
}

/// A species photo with its credit: `SpeciesCandidate.reference_image` and
/// `SpeciesSummary.reference_images[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = ReferenceImage)]
pub struct ReferenceImage<U: UrlState = Signed> {
    /// Signed.
    #[serde(default, skip_serializing_if = "UrlSlot::is_absent")]
    #[schema(value_type = SignedUrl, required = true)]
    pub url: U::Url,
    /// "Photo: A. Author, CC BY-SA 4.0"; shown with the photo.
    pub credit: String,
}

/// `SpeciesCandidate` ([contracts 3]). `reference_image` is computed from `species_id` when a
/// candidate is read or sent and never stored: a stored candidate always has `None`, and
/// [`Sign`] fills it from the species photo index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = SpeciesCandidate)]
pub struct SpeciesCandidate<U: UrlState = Signed> {
    /// `null` when the name isn't in the knowledge base.
    #[schema(required = true)]
    pub species_id: Option<SpeciesId>,
    /// The name shown.
    pub name: String,
    /// The candidate's score.
    pub score: Confidence,
    /// Who proposed it.
    pub source: CandidateSource,
    /// The species' first reference photo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false, value_type = Option<ReferenceImage>)]
    pub reference_image: Option<ReferenceImage<U>>,
}

impl Sign for SpeciesCandidate<Stored> {
    type Signed = SpeciesCandidate<Signed>;
    type Scope = ();
    fn sign(self, (): (), signer: &Signer<'_>) -> SpeciesCandidate<Signed> {
        let reference_image = self.species_id.as_ref().and_then(|species| {
            signer
                .photos()
                .photos(species)
                .into_iter()
                .next()
                .map(|photo| ReferenceImage {
                    url: signer.url(&ImagePath::SpeciesPhoto {
                        species: species.clone(),
                        hash: photo.hash,
                    }),
                    credit: photo.credit,
                })
        });
        SpeciesCandidate {
            species_id: self.species_id,
            name: self.name,
            score: self.score,
            source: self.source,
            reference_image,
        }
    }
}

/// `Plant.species_status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SpeciesStatus {
    /// Nothing identified yet.
    Unknown,
    /// Suggested by the models; the app shows the confirmation card.
    Suggested,
    /// Confirmed by the user; later scans skip identification.
    Confirmed,
}

/// `Plant.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlantStatus {
    /// No problem found.
    Healthy,
    /// Worth watching.
    Watch,
    /// A problem was diagnosed.
    Sick,
    /// Dead.
    Dead,
}

/// `Plant.position`: from the garden map ([contracts 9]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PlantPosition {
    /// The map version the pin is on.
    pub map_version: u32,
    /// Metres on the map's ground plane.
    pub x_m: f64,
    /// Metres on the map's ground plane.
    pub y_m: f64,
}

/// `Plant.latest_scan`: the plant's most recent scan, in any status, skipping scans being
/// deleted ([contracts 3], deleting a scan).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = LatestScan)]
pub struct LatestScan<U: UrlState = Signed> {
    /// The scan.
    pub scan_id: ScanId,
    /// When it was created.
    pub created_at: Timestamp,
    /// Its status.
    pub status: ScanStatus,
    /// Its `whole_plant` shot, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub thumbnail_image_id: Option<ImageId>,
    /// Signed URL of that shot.
    #[serde(default, skip_serializing_if = "url_absent")]
    #[schema(nullable = false, value_type = Option<SignedUrl>)]
    pub thumbnail_url: Option<U::Url>,
}

impl Sign for LatestScan<Stored> {
    type Signed = LatestScan<Signed>;
    type Scope = ();
    fn sign(self, (): (), signer: &Signer<'_>) -> LatestScan<Signed> {
        LatestScan {
            thumbnail_url: self.thumbnail_image_id.map(|image| {
                signer.url(&ImagePath::ScanImage {
                    scan: self.scan_id,
                    image,
                })
            }),
            scan_id: self.scan_id,
            created_at: self.created_at,
            status: self.status,
            thumbnail_image_id: self.thumbnail_image_id,
        }
    }
}

/// `Plant` ([contracts 3]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = Plant)]
pub struct Plant<U: UrlState = Signed> {
    /// The plant.
    pub id: PlantId,
    /// Its garden.
    pub garden_id: GardenId,
    /// "Tomato by the fence".
    pub name: String,
    /// The working species, suggested or confirmed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub species: Option<SpeciesRef>,
    /// "Something else" in the confirmation card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub custom_species_name: Option<String>,
    /// Whether the species is suggested or confirmed.
    pub species_status: SpeciesStatus,
    /// The candidates behind the suggestion.
    #[schema(value_type = Vec<SpeciesCandidate>)]
    pub species_candidates: Vec<SpeciesCandidate<U>>,
    /// Health status.
    pub status: PlantStatus,
    /// From the garden map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub position: Option<PlantPosition>,
    /// The most recent scan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false, value_type = Option<LatestScan>)]
    pub latest_scan: Option<LatestScan<U>>,
    /// When it was created.
    pub created_at: Timestamp,
}

impl Sign for Plant<Stored> {
    type Signed = Plant<Signed>;
    type Scope = ();
    fn sign(self, (): (), signer: &Signer<'_>) -> Plant<Signed> {
        Plant {
            id: self.id,
            garden_id: self.garden_id,
            name: self.name,
            species: self.species,
            custom_species_name: self.custom_species_name,
            species_status: self.species_status,
            species_candidates: self.species_candidates.sign((), signer),
            status: self.status,
            position: self.position,
            latest_scan: self.latest_scan.sign((), signer),
            created_at: self.created_at,
        }
    }
}

/// `ScanFailureCode` ([contracts 3]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScanFailureCode {
    /// Not retryable: start a new scan with new photos.
    NoPlantFound,
    /// Not retryable: start a new scan with new photos.
    PhotosUnusable,
    /// Retryable: job attempts exhausted.
    ProcessingError,
    /// Retryable: LLM API or weather down.
    UpstreamUnavailable,
}

impl ScanFailureCode {
    /// Only `processing_error` and `upstream_unavailable` can be resubmitted.
    pub const fn retryable(self) -> bool {
        matches!(self, Self::ProcessingError | Self::UpstreamUnavailable)
    }
}

/// `ScanFailure` ([contracts 3]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ScanFailure {
    /// The reason.
    pub code: ScanFailureCode,
    /// Safe to show.
    pub message: String,
    /// Equal to `code.retryable()`.
    pub retryable: bool,
}

impl ScanFailure {
    /// A failure whose `retryable` follows its code.
    pub fn new(code: ScanFailureCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: code.retryable(),
        }
    }
}

/// `"yes" | "no" | "unsure"`: question options, indicator answers and evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    /// The indicator is present.
    Yes,
    /// The indicator is absent.
    No,
    /// Can't tell; counts for nothing in scoring.
    Unsure,
}

/// `Question.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKind {
    /// Answered with an option.
    Indicator,
    /// Answered by uploading a photo with `question_id`.
    PhotoRequest,
}

/// `Question.options[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct QuestionOption {
    /// The option.
    pub id: Answer,
    /// Its label.
    pub label: String,
}

/// `Question.status`: what the app sees. The stored lifecycle has three more states, in
/// [`crate::rules::questions::QuestionState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuestionStatus {
    /// Waiting for an answer.
    Open,
    /// A photo for it is being analysed; further answers and photos get `409`.
    Answering,
}

/// `Question` ([contracts 3]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Question {
    /// The question.
    pub id: QuestionId,
    /// The run that asked it.
    pub run: Run,
    /// How it is answered.
    pub kind: QuestionKind,
    /// "Do the spots have rings, like a target?"
    pub text: String,
    /// Indicator questions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub options: Option<Vec<QuestionOption>>,
    /// `photo_request`: which shot to take.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub photo_kind: Option<ImageKind>,
    /// "Turn the leaf over and photograph the underside".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub photo_hint: Option<String>,
    /// The problems this question separates.
    pub candidates: Vec<ProblemId>,
    /// When it was asked.
    pub asked_at: Timestamp,
    /// When the scheduler continues without an answer.
    pub continue_at: Timestamp,
    /// `open` or `answering`.
    pub status: QuestionStatus,
}

/// `GuidedCheck.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GuidedKind {
    /// Squeeze a handful of moist soil.
    SqueezeTest,
    /// Shake soil with water in a jar and let it settle.
    JarTest,
}

impl GuidedKind {
    /// The image kind of the check's photo.
    pub const fn image_kind(self) -> ImageKind {
        match self {
            Self::SqueezeTest => ImageKind::SqueezeTest,
            Self::JarTest => ImageKind::JarTest,
        }
    }
}

/// `GuidedCheck.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GuidedCheckStatus {
    /// Its `guided` job is queued or running.
    Analysing,
    /// Finished; `result` is set unless a jar hadn't settled.
    Done,
    /// The job failed for good; the check is photographed again as a new check.
    Failed,
}

/// `GuidedCheck` ([contracts 3]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = GuidedCheck)]
pub struct GuidedCheck<U: UrlState = Signed> {
    /// The check's photo.
    pub image_id: ImageId,
    /// Which check.
    pub kind: GuidedKind,
    /// Its progress.
    pub status: GuidedCheckStatus,
    /// A soil observation with aspect `texture` ([contracts 7]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false, value_type = Option<Observation>)]
    pub result: Option<Observation<U>>,
}

impl Sign for GuidedCheck<Stored> {
    type Signed = GuidedCheck<Signed>;
    type Scope = ();
    fn sign(self, (): (), signer: &Signer<'_>) -> GuidedCheck<Signed> {
        GuidedCheck {
            image_id: self.image_id,
            kind: self.kind,
            status: self.status,
            result: self.result.sign((), signer),
        }
    }
}

/// `Scan` ([contracts 3]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = Scan)]
pub struct Scan<U: UrlState = Signed> {
    /// The scan.
    pub id: ScanId,
    /// Its garden.
    pub garden_id: GardenId,
    /// Its plant.
    pub plant_id: PlantId,
    /// Its status.
    pub status: ScanStatus,
    /// 0 until submitted.
    pub run: Run,
    /// Every plant, soil and guided-check photo.
    #[schema(value_type = Vec<ScanImage>)]
    pub images: Vec<ScanImage<U>>,
    /// Every guided check.
    #[schema(value_type = Vec<GuidedCheck>)]
    pub guided_checks: Vec<GuidedCheck<U>>,
    /// When it was created.
    pub created_at: Timestamp,
    /// Absent once that user is deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub created_by: Option<UserRef>,
    /// When it was first submitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub submitted_at: Option<Timestamp>,
    /// Present while a question is unanswered, in any status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub open_question: Option<Question>,
    /// The current diagnosis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub diagnosis_id: Option<DiagnosisId>,
    /// The current plan version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plan_id: Option<PlanId>,
    /// With status `failed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub failure: Option<ScanFailure>,
    /// The highest `scan_event.seq` so far.
    pub last_event_seq: EventSeq,
}

impl Sign for Scan<Stored> {
    type Signed = Scan<Signed>;
    type Scope = ();
    fn sign(self, (): (), signer: &Signer<'_>) -> Scan<Signed> {
        Scan {
            images: self.images.sign(self.id, signer),
            guided_checks: self.guided_checks.sign((), signer),
            id: self.id,
            garden_id: self.garden_id,
            plant_id: self.plant_id,
            status: self.status,
            run: self.run,
            created_at: self.created_at,
            created_by: self.created_by,
            submitted_at: self.submitted_at,
            open_question: self.open_question,
            diagnosis_id: self.diagnosis_id,
            plan_id: self.plan_id,
            failure: self.failure,
            last_event_seq: self.last_event_seq,
        }
    }
}

/// `ScanDetail extends Scan` ([contracts 3]): `GET /scans/{id}`. `extends` is a flattened
/// `scan` field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = ScanDetail)]
pub struct ScanDetail<U: UrlState = Signed> {
    /// Every field of `Scan`.
    #[serde(flatten)]
    #[schema(value_type = Scan)]
    pub scan: Scan<U>,
    /// The scan's plant.
    #[schema(value_type = Plant)]
    pub plant: Plant<U>,
    /// The latest observations of each stage, across runs ([contracts 3], observations across
    /// runs).
    #[schema(value_type = Vec<Observation>)]
    pub observations: Vec<Observation<U>>,
    /// The run's weather snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub weather: Option<WeatherSummary>,
    /// The current diagnosis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false, value_type = Option<Diagnosis>)]
    pub diagnosis: Option<Diagnosis<U>>,
    /// The current plan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plan: Option<ActionPlan>,
}

impl Sign for ScanDetail<Stored> {
    type Signed = ScanDetail<Signed>;
    type Scope = ();
    fn sign(self, (): (), signer: &Signer<'_>) -> ScanDetail<Signed> {
        ScanDetail {
            scan: self.scan.sign((), signer),
            plant: self.plant.sign((), signer),
            observations: self.observations.sign((), signer),
            weather: self.weather,
            diagnosis: self.diagnosis.sign((), signer),
            plan: self.plan,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Requests and responses
// ---------------------------------------------------------------------------------------------

/// `new_plant` of `CreateScanRequest` and `GardenMapInput.pins[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewPlantInput {
    /// The plant's name.
    pub name: String,
    /// Its species, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub species_id: Option<SpeciesId>,
}

/// `POST /scans` ([contracts 3]). Exactly one of `plant_id` and `new_plant`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateScanRequest {
    /// The garden.
    pub garden_id: GardenId,
    /// An existing plant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plant_id: Option<PlantId>,
    /// A plant to create.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub new_plant: Option<NewPlantInput>,
}

/// The plant a new scan is for, once "exactly one of" is checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanPlant {
    /// An existing plant of the garden.
    Existing(PlantId),
    /// A plant created with the scan.
    New(NewPlantInput),
}

impl CreateScanRequest {
    /// Checks "exactly one of `plant_id` and `new_plant`".
    pub fn plant(self) -> Result<ScanPlant, DomainError> {
        match (self.plant_id, self.new_plant) {
            (Some(id), None) => Ok(ScanPlant::Existing(id)),
            (None, Some(new)) => Ok(ScanPlant::New(new)),
            _ => Err(DomainError::invalid(
                "plant_id",
                "exactly one of plant_id and new_plant is required",
            )),
        }
    }
}

/// `DepthMeta.source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DepthSource {
    /// The `LiDAR` depth camera or `ARKit` scene depth.
    Lidar,
}

/// `DepthMeta.intrinsics`: pixels of a reference size, in the sensor frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Intrinsics {
    /// Focal length x, pixels.
    pub fx: f64,
    /// Focal length y, pixels.
    pub fy: f64,
    /// Principal point x, pixels.
    pub cx: f64,
    /// Principal point y, pixels.
    pub cy: f64,
    /// The reference width the values are in.
    pub ref_width: u32,
    /// The reference height the values are in.
    pub ref_height: u32,
}

/// `DepthMeta.distortion`: only if the day-1 check finds the photo and depth distorted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Distortion {
    /// Reference-size pixels, sensor frame.
    pub center: [f64; 2],
    /// The calibration's inverse lens distortion table.
    pub inverse_lookup: Vec<f64>,
}

/// `DepthMeta` ([contracts 3], depth): the `depth_meta` multipart part, JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DepthMeta {
    /// Always `lidar`.
    pub source: DepthSource,
    /// Camera intrinsics.
    pub intrinsics: Intrinsics,
    /// Lens distortion, when sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub distortion: Option<Distortion>,
    /// Unit vector towards the ground, camera coordinates (x right, y down, z forward), sensor
    /// frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub gravity: Option<[f64; 3]>,
    /// "iPhone16,1".
    pub device_model: String,
    /// The phone filled holes in the depth map.
    pub filtered: bool,
}

/// `UploadImageResponse.quality`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct QualityReport {
    /// Variance of the Laplacian.
    pub blur_score: f64,
    /// Mean exposure, 0–1.
    pub exposure: f64,
    /// Always `true`: a failing shot gets `422 image_quality` instead.
    pub passed: True,
}

/// Why depth sent with a photo was dropped ([contracts 3]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DepthProblem {
    /// Not the photo's aspect ratio within 1%.
    AspectMismatch,
    /// Fewer than 20% of the pixels have depth.
    TooSparse,
    /// `depth_meta` is invalid, or the orientation is mirrored.
    InvalidMeta,
    /// Not a 16-bit grayscale PNG.
    UnsupportedFormat,
    /// Over 4 MB.
    TooLarge,
    /// Sent with a `squeeze_test` or `jar_test`.
    NotForThisKind,
    /// Writing the depth or confidence map failed after the photo was stored.
    StorageError,
}

/// `UploadImageResponse.depth`: `{ stored: true } | { stored: false, problem }`. The union is
/// tagged by a boolean, which serde can't tag on, so it is untagged with [`True`] and [`False`]
/// literal fields that only deserialise from their own value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum DepthOutcome {
    /// The depth (and confidence map) were stored.
    Stored {
        /// `true`.
        stored: True,
    },
    /// The photo was stored without its depth.
    Dropped {
        /// `false`.
        stored: False,
        /// Why.
        problem: DepthProblem,
    },
}

/// `201` of `POST /scans/{id}/images` ([contracts 3]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = UploadImageResponse)]
pub struct UploadImageResponse<U: UrlState = Signed> {
    /// The stored image.
    #[schema(value_type = ScanImage)]
    pub image: ScanImage<U>,
    /// The quality scores.
    pub quality: QualityReport,
    /// Present when depth parts were sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub depth: Option<DepthOutcome>,
}

impl Sign for UploadImageResponse<Stored> {
    type Signed = UploadImageResponse<Signed>;
    type Scope = ScanId;
    fn sign(self, scan: ScanId, signer: &Signer<'_>) -> UploadImageResponse<Signed> {
        UploadImageResponse {
            image: self.image.sign(scan, signer),
            quality: self.quality,
            depth: self.depth,
        }
    }
}

/// `POST /scans/{id}/answers` ([contracts 3]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AnswerRequest {
    /// An `indicator` question.
    pub question_id: QuestionId,
    /// The answer.
    pub option_id: Answer,
}

/// `POST /plants/{id}/species` ([contracts 3]). Exactly one of the two fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfirmSpeciesRequest {
    /// A knowledge-base species.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub species_id: Option<SpeciesId>,
    /// "Something else", at most 80 characters: the plant is treated as unsupported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub custom_name: Option<String>,
}

/// The user's species choice, once "exactly one of" and the length are checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeciesChoice {
    /// A knowledge-base species.
    Known(SpeciesId),
    /// "Something else": skip species identification.
    Custom(String),
}

impl SpeciesChoice {
    /// The longest custom name.
    pub const CUSTOM_NAME_MAX_CHARS: usize = 80;
}

impl ConfirmSpeciesRequest {
    /// Checks the request.
    pub fn choice(self) -> Result<SpeciesChoice, DomainError> {
        match (self.species_id, self.custom_name) {
            (Some(id), None) => Ok(SpeciesChoice::Known(id)),
            (None, Some(name)) => {
                let name = name.trim().to_owned();
                if name.is_empty() || name.chars().count() > SpeciesChoice::CUSTOM_NAME_MAX_CHARS {
                    Err(DomainError::invalid("custom_name", "1-80 characters"))
                } else {
                    Ok(SpeciesChoice::Custom(name))
                }
            }
            _ => Err(DomainError::invalid(
                "species_id",
                "exactly one of species_id and custom_name is required",
            )),
        }
    }
}

/// `FeedbackRequest.verdict`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// The diagnosis was right.
    Correct,
    /// It was wrong.
    Wrong,
    /// Can't say.
    Unsure,
}

/// `POST /scans/{id}/feedback` ([contracts 3]): stored with the author's user id; deleted with
/// their account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FeedbackRequest {
    /// The verdict.
    pub verdict: Verdict,
    /// Free text, or a slug when picked from a list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub corrected_label: Option<String>,
    /// Notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub notes: Option<String>,
}

/// `{ id: Id }`, the `201` of `POST /scans/{id}/feedback`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "", as = Created)]
pub struct Created<T: Table> {
    /// The new record.
    #[schema(value_type = String)]
    pub id: crate::id::Id<T>,
}

/// The `201` of `POST /scans/{id}/feedback`.
pub type FeedbackCreated = Created<crate::id::table::ScanFeedback>;
