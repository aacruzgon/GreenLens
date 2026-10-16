//! `Diagnosis` and its parts ([contracts 7], diagnosis).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    events::ProblemKind,
    id::{
        DiagnosisId, ImageId, IndicatorId, ObservationId, OrganismId, ProblemId, ScanId, SourceId,
    },
    records::{
        measurement::MeasureQuantity,
        observation::{Observation, OrganismRole},
    },
    scan::Answer,
    url::{Sign, Signed, Signer, Stored, UrlState},
    wire::{Run, Timestamp},
};

/// An indicator's weight, 1 to 3 ([contracts 7], [contracts 10]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, ToSchema)]
#[serde(into = "u8")]
#[schema(value_type = u8, minimum = 1, maximum = 3)]
pub struct Weight(u8);

impl Weight {
    /// Checks 1–3.
    pub const fn new(w: u8) -> Option<Self> {
        if w >= 1 && w <= 3 {
            Some(Self(w))
        } else {
            None
        }
    }

    /// The number.
    pub const fn get(self) -> u8 {
        self.0
    }
}
impl From<Weight> for u8 {
    fn from(w: Weight) -> Self {
        w.0
    }
}
impl<'de> Deserialize<'de> for Weight {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let w = u8::deserialize(deserializer)?;
        Self::new(w).ok_or_else(|| serde::de::Error::custom("weight must be 1, 2 or 3"))
    }
}
impl schemars::JsonSchema for Weight {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Weight")
    }
    fn inline_schema() -> bool {
        true
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "integer", "enum": [1, 2, 3] })
    }
}

/// `IndicatorEvidence.source`, ranked `user` > `measurement` > `vlm`: each indicator keeps one
/// answer ([contracts 7], measurable indicators).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSource {
    /// The VLM; lowest rank.
    Vlm,
    /// A `LiDAR` measurement.
    Measurement,
    /// The user; highest rank.
    User,
}

/// `IndicatorEvidence.measured`: with source `measurement`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MeasuredValue {
    /// The quantity.
    pub quantity: MeasureQuantity,
    /// The value the indicator's `stat` picked.
    pub value: f64,
    /// The image it came from.
    pub image_id: ImageId,
}

/// `IndicatorEvidence` ([contracts 7]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct IndicatorEvidence {
    /// The indicator.
    pub indicator_id: IndicatorId,
    /// "Concentric rings in the spots".
    pub text: String,
    /// 1–3.
    pub weight: Weight,
    /// The answer.
    pub answer: Answer,
    /// Why.
    pub reason: String,
    /// Who answered.
    pub source: EvidenceSource,
    /// With source `measurement`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub measured: Option<MeasuredValue>,
}

/// `CandidateScore.weather_fit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WeatherFit {
    /// The weather matches the profile's favourable conditions.
    Favourable,
    /// Neither.
    Neutral,
    /// The weather works against it.
    Unfavourable,
}

/// `CandidateScore` ([contracts 7]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CandidateScore {
    /// The problem.
    pub problem_id: ProblemId,
    /// Its name.
    pub name: String,
    /// Its kind.
    pub kind: ProblemKind,
    /// Sum of matched weights minus contradicted weights.
    pub score: i32,
    /// Answered `yes`.
    pub matched: Vec<IndicatorEvidence>,
    /// Answered `no`.
    pub contradicted: Vec<IndicatorEvidence>,
    /// Answered `unsure`.
    pub unsure: Vec<IndicatorEvidence>,
    /// The weather against the profile.
    pub weather_fit: WeatherFit,
}

/// `Citation` ([contracts 7]): a curated source. Its `url` is the source's public page, not an
/// image, so it is a plain string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Citation {
    /// The source.
    pub source_id: SourceId,
    /// Its title.
    pub title: String,
    /// Its publisher.
    pub publisher: String,
    /// Its page.
    pub url: String,
}

/// `Diagnosis.status`, decided in the order of contracts 7's table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosisStatus {
    /// A clear winner.
    Confirmed,
    /// Ambiguous; the plan is provisional.
    Uncertain,
    /// Only on positive evidence; a short care plan.
    Healthy,
}

/// `Diagnosis.organisms[].preys_on[]`: computed from the `preys_on` graph when read; never
/// stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Prey {
    /// The pest.
    pub organism_id: OrganismId,
    /// Its name.
    pub name: String,
}

/// `Diagnosis.organisms[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DiagnosisOrganism {
    /// The finding.
    pub observation_id: ObservationId,
    /// `null` for "other".
    #[schema(required = true)]
    pub organism_id: Option<OrganismId>,
    /// Its name.
    pub name: String,
    /// Its role.
    pub role: OrganismRole,
    /// Invasive only: the app shows "How to report".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub reporting_url: Option<String>,
    /// Beneficial only: the pests it eats. Filled in when read, never stored: the store drops it
    /// on write and `api` fills it from `KnowledgeStore::prey_of`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub preys_on: Option<Vec<Prey>>,
}

/// `Diagnosis.risks[].level`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    /// Low.
    Low,
    /// Medium.
    Medium,
    /// High.
    High,
}

impl RiskLevel {
    /// One step up, at most `high` (the crowding rule, [contracts 9]).
    #[must_use]
    pub const fn raised(self) -> Self {
        match self {
            Self::Low => Self::Medium,
            Self::Medium | Self::High => Self::High,
        }
    }
}

/// `Diagnosis.risks[]`. `risk` is an open vocabulary in contracts 7; the known values are the
/// constants below.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Risk {
    /// Such as `"fungal_spread"`.
    pub risk: String,
    /// Its level.
    pub level: RiskLevel,
    /// "4 days over 90% humidity forecast", "Crowded: nearest plant 30 cm away".
    pub drivers: Vec<String>,
}

impl Risk {
    /// Fungal spread (humidity, crowding).
    pub const FUNGAL_SPREAD: &'static str = "fungal_spread";
    /// Root rot.
    pub const ROOT_ROT: &'static str = "root_rot";
    /// Frost damage.
    pub const FROST_DAMAGE: &'static str = "frost_damage";
    /// The growth curve stalled ([contracts 7], growth).
    pub const GROWTH_STALLED: &'static str = "growth_stalled";
}

/// `Diagnosis.evidence_basis` (architecture 6.4: user input, then photo, then modelled data).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct EvidenceBasis {
    /// From shared memories only ([contracts 8]).
    pub user_input: Vec<String>,
    /// Measurements included: "Median spot 2.1 mm".
    pub photo: Vec<String>,
    /// Weather and soil models.
    pub modelled: Vec<String>,
}

/// `Diagnosis` ([contracts 7]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = Diagnosis)]
pub struct Diagnosis<U: UrlState = Signed> {
    /// The diagnosis.
    pub id: DiagnosisId,
    /// Its scan.
    pub scan_id: ScanId,
    /// Its run.
    pub run: Run,
    /// The earlier diagnosis of the same scan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub supersedes: Option<DiagnosisId>,
    /// The status.
    pub status: DiagnosisStatus,
    /// `uncertain`, and nothing points to any problem.
    pub low_evidence: bool,
    /// Absent when healthy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub primary: Option<CandidateScore>,
    /// Ranked, at most 9.
    pub candidates: Vec<CandidateScore>,
    /// Organisms found.
    pub organisms: Vec<DiagnosisOrganism>,
    /// The soil observations used, guided checks included.
    #[schema(value_type = Vec<Observation>)]
    pub soil: Vec<Observation<U>>,
    /// Fusion's risks.
    pub risks: Vec<Risk>,
    /// What the diagnosis rests on.
    pub evidence_basis: EvidenceBasis,
    /// Every source cited.
    pub citations: Vec<Citation>,
    /// Model name → version.
    pub model_versions: BTreeMap<String, String>,
    /// When it was stored.
    pub created_at: Timestamp,
}

impl Sign for Diagnosis<Stored> {
    type Signed = Diagnosis<Signed>;
    type Scope = ();
    fn sign(self, (): (), signer: &Signer<'_>) -> Diagnosis<Signed> {
        Diagnosis {
            soil: self.soil.sign((), signer),
            id: self.id,
            scan_id: self.scan_id,
            run: self.run,
            supersedes: self.supersedes,
            status: self.status,
            low_evidence: self.low_evidence,
            primary: self.primary,
            candidates: self.candidates,
            organisms: self.organisms,
            risks: self.risks,
            evidence_basis: self.evidence_basis,
            citations: self.citations,
            model_versions: self.model_versions,
            created_at: self.created_at,
        }
    }
}
