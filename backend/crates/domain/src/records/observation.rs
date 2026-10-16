//! `Observation` ([contracts 7]).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    events::Stage,
    id::{ObservationId, OrganismId, ProblemId, ScanId, SpeciesId},
    jobs::RegionClass,
    records::measurement::Measurement,
    scan::GuidedKind,
    url::{Sign, Signed, Signer, Stored, UrlState},
    vlm::{DamagePattern, OrganismCount, SoilAspect},
    wire::{Rated, Region, Run, Timestamp},
};

/// `Observation.stage`: what kind of finding it is (not the pipeline stage that produced it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObservationStage {
    /// Plant present or not.
    PlantCheck,
    /// A segmentation region.
    Segmentation,
    /// A species candidate or guess.
    Species,
    /// A disease classifier result.
    Disease,
    /// A soil aspect, from the VLM or a guided check.
    Soil,
    /// An organism.
    Organism,
    /// A damage pattern.
    Damage,
    /// A `LiDAR` measurement.
    Measurement,
}

/// `Observation.ref`: the knowledge-base record, when there is one. Adjacently tagged, so it
/// serialises as `{ "table": "disease", "id": "tomato_early_blight" }`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(tag = "table", content = "id", rename_all = "snake_case")]
pub enum KnowledgeRef {
    /// A species.
    Species(SpeciesId),
    /// A problem (the `disease` table).
    Disease(ProblemId),
    /// An organism.
    Organism(OrganismId),
}

/// `Observation.role` and `Diagnosis.organisms[].role`: organisms only; `unknown` for "other".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrganismRole {
    /// Harms the plant.
    Pest,
    /// Eats pests or pollinates; never targeted.
    Beneficial,
    /// Neither.
    Neutral,
    /// An invasive species, with reporting guidance.
    Invasive,
    /// Reported as "other": cannot be targeted.
    Unknown,
}

/// `Observation.detail`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ToSchema)]
pub struct ObservationDetail {
    /// Soil observations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub aspect: Option<SoilAspect>,
    /// The soil aspect's value, such as `"dry"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub value: Option<String>,
    /// Damage observations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub pattern: Option<DamagePattern>,
    /// Organism observations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub count: Option<OrganismCount>,
    /// Segmentation observations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub region_class: Option<RegionClass>,
    /// Guided-check observations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub guided_kind: Option<GuidedKind>,
    /// Measurement observations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub measurement: Option<Measurement>,
    /// Organisms on a photo with depth; shown, not scored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub length_mm: Option<f64>,
}

/// `Observation.source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObservationSource {
    /// The segmentation models.
    Segmentation,
    /// `BioCLIP`.
    Bioclip,
    /// The disease classifier.
    Classifier,
    /// The VLM.
    Vlm,
    /// The fallback worker's LLM.
    LlmFallback,
    /// The user.
    User,
    /// The measurement stage.
    Measurement,
}

/// `Observation` ([contracts 7]). `confidence` and `confidence_band` travel together as the
/// flattened [`Rated`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = Observation)]
pub struct Observation<U: UrlState = Signed> {
    /// The observation.
    pub id: ObservationId,
    /// Its scan.
    pub scan_id: ScanId,
    /// The run that produced it.
    pub run: Run,
    /// What kind of finding it is.
    pub stage: ObservationStage,
    /// The pipeline stage that produced it; runs replace observations by this, per image.
    pub pipeline_stage: Stage,
    /// Display text: "Early blight", "Ladybug larva", "Soil: dry", "Leaf".
    pub label: String,
    /// The knowledge-base record, when there is one.
    #[serde(rename = "ref", default, skip_serializing_if = "Option::is_none")]
    pub knowledge_ref: Option<KnowledgeRef>,
    /// Organisms only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub role: Option<OrganismRole>,
    /// Kind-specific detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub detail: Option<ObservationDetail>,
    /// `confidence` and its computed `confidence_band`.
    #[serde(flatten)]
    #[schema(inline)]
    pub confidence: Rated,
    /// Where on which image; always present for measurements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false, value_type = Option<Region>)]
    pub region: Option<Region<U>>,
    /// Who produced it.
    pub source: ObservationSource,
    /// For measurements: the algorithm version and `depth_meta.device_model`.
    pub model_version: String,
    /// When it was stored.
    pub created_at: Timestamp,
}

impl Sign for Observation<Stored> {
    type Signed = Observation<Signed>;
    type Scope = ();
    fn sign(self, (): (), signer: &Signer<'_>) -> Observation<Signed> {
        Observation {
            region: self.region.sign(self.scan_id, signer),
            id: self.id,
            scan_id: self.scan_id,
            run: self.run,
            stage: self.stage,
            pipeline_stage: self.pipeline_stage,
            label: self.label,
            knowledge_ref: self.knowledge_ref,
            role: self.role,
            detail: self.detail,
            confidence: self.confidence,
            source: self.source,
            model_version: self.model_version,
            created_at: self.created_at,
        }
    }
}
