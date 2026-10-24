//! Knowledge seed format ([contracts 10]): the reviewed YAML files and generated chunks. The
//! ingestion scripts validate every file against the JSON Schema generated from these types
//! (`schemars`), so every type here denies unknown fields: a misspelt key in a hand-written
//! profile fails validation instead of vanishing. The same types are the profile records the
//! `KnowledgeStore` returns and the agent's profile tools send to the model.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    events::ProblemKind,
    garden::species::SpeciesProfileKind,
    id::{
        ChunkId, IndicatorId, OrganismId, ProblemId, ProblemOrHealthy, SourceId, SpeciesId,
        TreatmentId,
    },
    jobs::IndicatorWhere,
    records::{diagnosis::Weight, measurement::MeasureQuantity},
    scan::ImageKind,
    vlm::{DamagePattern, SoilAspect},
    wire::{LocalDate, Timestamp},
};

/// `ProblemSeed.prevalence`: the tie-breaker in the candidate ranking.
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
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Prevalence {
    /// Common.
    Common,
    /// Occasional.
    Occasional,
    /// Rare.
    Rare,
}

/// `IndicatorMeasure.stat`: which value of the measurement the range applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MeasureStat {
    /// `value`.
    Median,
    /// `max`.
    Max,
    /// `min`.
    Min,
}

/// `IndicatorMeasure` ([contracts 10]): at least one of `min` and `max`, in the quantity's
/// unit, `min` < `max`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IndicatorMeasure {
    /// The quantity.
    pub quantity: MeasureQuantity,
    /// Which statistic.
    pub stat: MeasureStat,
    /// Lower bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// Upper bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

/// `ProblemSeed.indicators[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IndicatorSeed {
    /// `"<problem_id>.<slug>"`.
    pub id: IndicatorId,
    /// "Brown spots with concentric rings, like a target".
    pub text: String,
    /// Asked to the VLM verbatim.
    pub question: String,
    /// 1–3.
    pub weight: Weight,
    /// Exactly 3 per problem.
    pub discriminating: bool,
    /// Where it shows.
    pub r#where: IndicatorWhere,
    /// Which shots can show it.
    pub image_kinds: Vec<ImageKind>,
    /// Answered from `LiDAR` measurements when possible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measure: Option<IndicatorMeasure>,
}

/// `ProblemSeed.conditions.temp_c` and `SpeciesSeed.care.soil_ph`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Range {
    /// Lower bound.
    pub min: f64,
    /// Upper bound.
    pub max: f64,
}

/// `ProblemSeed.conditions`: the weather that favours the problem.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Conditions {
    /// °C.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temp_c: Option<Range>,
    /// Relative humidity, %.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rh_pct_min: Option<f64>,
    /// Hours with RH over 90% in the past 7 days (leaf-wetness proxy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub humid_hours_min: Option<u32>,
    /// Notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// `ProblemSeed.looks_like[]`, written once in either problem's file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LooksLikeSeed {
    /// The look-alike.
    pub problem: ProblemId,
    /// How to tell them apart.
    pub how_to_tell: String,
}

/// `ProblemSeed` ([contracts 10]): `problems/<problem_id>.yaml`, the `disease` table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProblemSeed {
    /// The slug.
    pub id: ProblemId,
    /// "Early blight".
    pub name: String,
    /// Its kind.
    pub kind: ProblemKind,
    /// Species → `susceptible_to` edges.
    pub affects: Vec<SpeciesId>,
    /// Organisms, for pest and wildlife problems.
    pub caused_by: Vec<OrganismId>,
    /// Patterns that point to this problem.
    pub damage_patterns: Vec<DamagePattern>,
    /// Tie-breaker in the ranking.
    pub prevalence: Prevalence,
    /// Indicators; exactly 3 discriminating.
    pub indicators: Vec<IndicatorSeed>,
    /// Favourable weather.
    pub conditions: Conditions,
    /// Look-alikes.
    pub looks_like: Vec<LooksLikeSeed>,
    /// `manages` edges.
    pub treatments: Vec<TreatmentId>,
    /// Its sources.
    pub sources: Vec<SourceId>,
    /// The teammate who reviewed it.
    pub reviewed_by: String,
    /// When.
    pub reviewed_at: LocalDate,
}

impl ProblemSeed {
    /// The 3 indicators that are asked.
    pub fn discriminating(&self) -> impl Iterator<Item = &IndicatorSeed> + '_ {
        self.indicators.iter().filter(|i| i.discriminating)
    }
}

/// `SourceSeed` ([contracts 10]): `sources.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceSeed {
    /// The slug.
    pub id: SourceId,
    /// Title.
    pub title: String,
    /// Publisher.
    pub publisher: String,
    /// Its page.
    pub url: String,
    /// Reuse terms; required for photo sources.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub licence: Option<String>,
    /// When it was retrieved.
    pub retrieved_at: LocalDate,
}

/// `SpeciesSeed.care.water`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WaterNeed {
    /// Low.
    Low,
    /// Medium.
    Medium,
    /// High.
    High,
}

/// `SpeciesSeed.care.light`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LightNeed {
    /// Full sun.
    FullSun,
    /// Part shade.
    PartShade,
    /// Shade.
    Shade,
}

/// `SpeciesSeed.care`: required when `profile` is `full`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Care {
    /// Water.
    pub water: WaterNeed,
    /// Light.
    pub light: LightNeed,
    /// Soil pH.
    pub soil_ph: Range,
    /// Canadian plant hardiness zones, such as `"annual"` or `"4-8"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hardiness_zones: Option<String>,
    /// Recommended distance between plants; the crowding rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing_cm: Option<f64>,
    /// The `growth_stalled` rule stops here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mature_height_cm: Option<f64>,
    /// Notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// `SpeciesSeed.reference_images[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReferenceImageSeed {
    /// Under `ml/knowledge/seed/images/`, e.g. `"species/tomato-1.jpg"`.
    pub file: String,
    /// A source whose licence allows reuse with credit.
    pub source_id: SourceId,
    /// "Photo: A. Author, CC BY-SA 4.0", shown with the photo.
    pub credit: String,
}

/// `SpeciesSeed` ([contracts 10]): `species/<species_id>.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpeciesSeed {
    /// The slug.
    pub id: SpeciesId,
    /// "Solanum lycopersicum".
    pub scientific_name: String,
    /// The first is the common name shown.
    pub common_names: Vec<String>,
    /// `label_only`: recognised by `BioCLIP`, no care profile or problems yet.
    pub profile: SpeciesProfileKind,
    /// Required when `full`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub care: Option<Care>,
    /// Checked against `classifier_labels.yaml` at ingestion.
    pub classifier_supported: bool,
    /// The text prompt `BioCLIP` scores against.
    pub bioclip_label: String,
    /// May be empty for `label_only`.
    pub sources: Vec<SourceId>,
    /// At most 3, for the confirmation card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_images: Option<Vec<ReferenceImageSeed>>,
    /// The reviewer.
    pub reviewed_by: String,
}

/// `OrganismSeed.taxon_group`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaxonGroup {
    /// Insects.
    Insect,
    /// Mites.
    Mite,
    /// Slugs and snails.
    Mollusc,
    /// Worms.
    Worm,
    /// Mammals.
    Mammal,
    /// Birds.
    Bird,
}

/// `OrganismSeed.role`: the seeded roles; `unknown` exists only for "other" findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SeedOrganismRole {
    /// Pest.
    Pest,
    /// Beneficial.
    Beneficial,
    /// Neutral.
    Neutral,
    /// Invasive: `reporting_url` required.
    Invasive,
}

impl From<SeedOrganismRole> for crate::records::observation::OrganismRole {
    fn from(r: SeedOrganismRole) -> Self {
        match r {
            SeedOrganismRole::Pest => Self::Pest,
            SeedOrganismRole::Beneficial => Self::Beneficial,
            SeedOrganismRole::Neutral => Self::Neutral,
            SeedOrganismRole::Invasive => Self::Invasive,
        }
    }
}

/// `OrganismSeed` ([contracts 10]): `organisms/<organism_id>.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganismSeed {
    /// The slug.
    pub id: OrganismId,
    /// "Ladybug larva".
    pub name: String,
    /// Scientific name.
    pub scientific_name: String,
    /// Group.
    pub taxon_group: TaxonGroup,
    /// Role.
    pub role: SeedOrganismRole,
    /// How to recognise it in a photo.
    pub recognition: String,
    /// Beneficial → pest edges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preys_on: Option<Vec<OrganismId>>,
    /// Required when invasive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reporting_url: Option<String>,
    /// `BioCLIP` prompt.
    pub bioclip_label: String,
    /// Sources.
    pub sources: Vec<SourceId>,
    /// Reviewer.
    pub reviewed_by: String,
}

/// `TreatmentSeed.kind`: a step that applies the treatment takes its category from this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TreatmentKind {
    /// Cultural.
    Cultural,
    /// Biological.
    Biological,
    /// Mechanical.
    Mechanical,
    /// Chemical.
    Chemical,
}

impl From<TreatmentKind> for crate::records::plan::StepCategory {
    fn from(k: TreatmentKind) -> Self {
        match k {
            TreatmentKind::Cultural => Self::Cultural,
            TreatmentKind::Biological => Self::Biological,
            TreatmentKind::Mechanical => Self::Mechanical,
            TreatmentKind::Chemical => Self::Chemical,
        }
    }
}

/// `TreatmentSeed` ([contracts 10]): no doses, ever; checked like plan steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TreatmentSeed {
    /// The slug.
    pub id: TreatmentId,
    /// Category.
    pub kind: TreatmentKind,
    /// The advice.
    pub text: String,
    /// Sources.
    pub sources: Vec<SourceId>,
}

/// `ProductsSeed.products[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProductSeed {
    /// Matched case-insensitively; a step naming one becomes chemical.
    pub name: String,
    /// Harms beneficial insects too.
    pub broad_spectrum: bool,
}

/// `ProductsSeed` ([contracts 10]): `products.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProductsSeed {
    /// Every pest-control product name.
    pub products: Vec<ProductSeed>,
}

/// `ClassifierLabels.labels[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassifierLabel {
    /// The classifier's output index.
    pub index: u32,
    /// The training dataset's class name.
    pub label: String,
    /// The species.
    pub species_id: SpeciesId,
    /// The problem, or `"healthy"`.
    pub problem_id: ProblemOrHealthy,
}

/// `ClassifierLabels` ([contracts 10]): `classifier_labels.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassifierLabels {
    /// The classifier version these labels belong to.
    pub model_version: String,
    /// One per output.
    pub labels: Vec<ClassifierLabel>,
}

/// `ChunkSeed.topic`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChunkTopic {
    /// Symptoms.
    Symptoms,
    /// Conditions.
    Conditions,
    /// Management.
    Management,
    /// Care.
    Care,
    /// Recognition.
    Recognition,
}

/// `ChunkSeed` ([contracts 10]): one line of `chunks/<source_id>.jsonl`. Embeddings are
/// computed at load time with the pinned model, never stored in the seed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChunkSeed {
    /// `"<source_id>.<n>"`.
    pub id: ChunkId,
    /// The source.
    pub source_id: SourceId,
    /// The crop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop: Option<SpeciesId>,
    /// The problem.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<ProblemId>,
    /// The organism.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organism: Option<OrganismId>,
    /// The topic.
    pub topic: ChunkTopic,
    /// A short excerpt, at most about 200 words.
    pub text: String,
}

/// One entry of `soil_conditions.yaml`: a description for each `SoilAspect` and value, with
/// what to do and its sources ([contracts 10]; the file's shape is not spelled out there, so
/// this follows `schema.surql`'s `soil_condition` table).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SoilConditionSeed {
    /// The aspect.
    pub aspect: SoilAspect,
    /// The value, such as `"compacted"`.
    pub value: String,
    /// What it means.
    pub description: String,
    /// What to do.
    pub what_to_do: String,
    /// Sources.
    pub sources: Vec<SourceId>,
}

/// `SoilProfile.depths[].depth_cm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub enum DepthBand {
    /// 0–5 cm.
    #[serde(rename = "0-5")]
    D0To5,
    /// 5–15 cm.
    #[serde(rename = "5-15")]
    D5To15,
    /// 15–30 cm.
    #[serde(rename = "15-30")]
    D15To30,
    /// 30–60 cm.
    #[serde(rename = "30-60")]
    D30To60,
    /// 60–100 cm.
    #[serde(rename = "60-100")]
    D60To100,
    /// 100–200 cm.
    #[serde(rename = "100-200")]
    D100To200,
}

/// `SoilProfile.depths[]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SoilDepth {
    /// The depth band.
    pub depth_cm: DepthBand,
    /// pH in water.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub ph_h2o: Option<f64>,
    /// %.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub clay_pct: Option<f64>,
    /// %.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub sand_pct: Option<f64>,
    /// %.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub silt_pct: Option<f64>,
    /// g/kg.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub organic_carbon_g_kg: Option<f64>,
    /// g/kg.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub nitrogen_g_kg: Option<f64>,
    /// cmol/kg.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub cec_cmol_kg: Option<f64>,
    /// kg/dm³.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub bulk_density_kg_dm3: Option<f64>,
}

/// `SoilProfile` ([contracts 10]): `garden.soil_profile`, from `SoilGrids`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SoilProfile {
    /// When it was fetched.
    pub fetched_at: Timestamp,
    /// Per depth band.
    pub depths: Vec<SoilDepth>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// contracts 10's example, as the JSON its YAML parses to.
    const EARLY_BLIGHT: &str = r#"{
      "id": "tomato_early_blight",
      "name": "Early blight",
      "kind": "fungal",
      "affects": ["tomato", "potato"],
      "caused_by": [],
      "damage_patterns": [],
      "prevalence": "common",
      "indicators": [
        {"id": "tomato_early_blight.concentric_rings",
         "text": "Brown spots with concentric rings, like a target",
         "question": "Do the brown spots have rings inside them, like a target?",
         "weight": 3, "discriminating": true, "where": "lower_leaves",
         "image_kinds": ["leaf_closeup"]},
        {"id": "tomato_early_blight.lower_leaves_first",
         "text": "Spots started on the oldest, lowest leaves",
         "question": "Are the spots mostly on the lowest leaves of the plant?",
         "weight": 2, "discriminating": true, "where": "whole_plant",
         "image_kinds": ["whole_plant"]},
        {"id": "tomato_early_blight.yellow_halo",
         "text": "Yellowing around each spot",
         "question": "Is there a yellow ring around the brown spots?",
         "weight": 1, "discriminating": true, "where": "lower_leaves",
         "image_kinds": ["leaf_closeup"]}
      ],
      "conditions": {
        "temp_c": {"min": 24, "max": 29},
        "rh_pct_min": 90,
        "humid_hours_min": 24,
        "notes": "Spreads in warm, humid weather with long periods of wet leaves."
      },
      "looks_like": [
        {"problem": "tomato_septoria_leaf_spot",
         "how_to_tell": "Septoria spots are smaller, with no rings, and have tiny black dots in a pale centre."}
      ],
      "treatments": ["remove_lower_infected_leaves", "mulch_soil_surface", "avoid_overhead_watering"],
      "sources": ["example_extension_tomato_diseases"],
      "reviewed_by": "teammate",
      "reviewed_at": "2026-10-20"
    }"#;

    #[test]
    fn problem_seed_example_round_trips() {
        let seed: ProblemSeed = serde_json::from_str(EARLY_BLIGHT).unwrap();
        assert_eq!(seed.discriminating().count(), 3);
        assert_eq!(seed.indicators[0].r#where, IndicatorWhere::LowerLeaves);
        let again = serde_json::to_value(&seed).unwrap();
        assert_eq!(
            serde_json::from_value::<ProblemSeed>(again.clone()).unwrap(),
            seed
        );
        let expected: serde_json::Value = serde_json::from_str(EARLY_BLIGHT).unwrap();
        let keys =
            |v: &serde_json::Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
        assert_eq!(keys(&again), keys(&expected));
        assert_eq!(again["indicators"], expected["indicators"]);
    }

    #[test]
    fn measurable_indicator_and_unknown_fields() {
        let ind = r#"{"id": "tomato_septoria_leaf_spot.small_spots",
            "text": "Spots about 1.5-3 mm across",
            "question": "Are the spots small, about the size of a pinhead or a little bigger?",
            "weight": 3, "discriminating": true, "where": "lower_leaves",
            "image_kinds": ["leaf_closeup"],
            "measure": {"quantity": "lesion_diameter_mm", "stat": "median", "min": 1.5, "max": 3}}"#;
        let seed: IndicatorSeed = serde_json::from_str(ind).unwrap();
        assert_eq!(
            seed.measure.unwrap().quantity,
            MeasureQuantity::LesionDiameterMm
        );
        let typo = EARLY_BLIGHT.replace("\"prevalence\"", "\"prevalance\"");
        assert!(serde_json::from_str::<ProblemSeed>(&typo).is_err());
    }

    #[test]
    fn seed_schema_is_strict() {
        let schema = serde_json::to_value(schemars::schema_for!(ProblemSeed)).unwrap();
        assert_eq!(schema["additionalProperties"], serde_json::json!(false));
        assert_eq!(schema["$defs"]["LocalDate"]["format"], "date");
    }

    /// The seed's JSON Schemas and the `measure` rule, as `ml/knowledge/validate.py` reads them.
    fn seed_schema_file() -> String {
        use std::collections::BTreeMap;

        use schemars::schema_for;

        let schemas: BTreeMap<&str, serde_json::Value> = [
            ("ChunkSeed", schema_for!(ChunkSeed)),
            ("ClassifierLabels", schema_for!(ClassifierLabels)),
            ("OrganismSeed", schema_for!(OrganismSeed)),
            ("ProblemSeed", schema_for!(ProblemSeed)),
            ("ProductsSeed", schema_for!(ProductsSeed)),
            ("SoilConditionSeed", schema_for!(SoilConditionSeed)),
            ("SourceSeed", schema_for!(SourceSeed)),
            ("SpeciesSeed", schema_for!(SpeciesSeed)),
            ("TreatmentSeed", schema_for!(TreatmentSeed)),
        ]
        .into_iter()
        .map(|(name, s)| (name, serde_json::to_value(s).unwrap()))
        .collect();
        let measurable_from: BTreeMap<MeasureQuantity, Vec<ImageKind>> = MeasureQuantity::ALL
            .into_iter()
            .map(|q| {
                let kinds = ImageKind::ALL
                    .into_iter()
                    .filter(|&k| q.measurable_from(k))
                    .collect();
                (q, kinds)
            })
            .collect();
        let file = serde_json::json!({
            "about": "Generated from the domain crate's seed types by its test \
                      the_seed_schema_file_is_current; set UPDATE_SEED_SCHEMA=1 to rewrite it.",
            "measurable_from": measurable_from,
            "schemas": schemas,
            "soil_values": soil_values(),
        });
        serde_json::to_string_pretty(&sorted(file)).unwrap() + "\n"
    }

    /// The values each `SoilAspect` can take, for `soil_conditions.yaml` (`value` is a string
    /// there). `texture` takes the squeeze test's and the jar test's.
    fn soil_values() -> std::collections::BTreeMap<SoilAspect, Vec<String>> {
        use std::collections::BTreeMap;

        use schemars::{Schema, schema_for};

        use crate::vlm::{
            JarTexture, SoilColour, SoilCover, SoilDeposits, SoilErosion, SoilMoisture,
            SoilStructure, SoilSurface, SoilWeeds, SqueezeResult,
        };

        // A unit enum's schema lists its values under `oneOf`, one `const` each, or as `enum`.
        fn values(s: Schema) -> Vec<String> {
            let v = s.to_value();
            let consts = v["oneOf"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|b| &b["const"]);
            consts
                .chain(v["enum"].as_array().into_iter().flatten())
                .filter_map(|c| c.as_str().map(str::to_owned))
                .collect()
        }
        let mut texture = values(schema_for!(SqueezeResult));
        for v in values(schema_for!(JarTexture)) {
            if !texture.contains(&v) {
                texture.push(v);
            }
        }
        BTreeMap::from([
            (SoilAspect::Moisture, values(schema_for!(SoilMoisture))),
            (SoilAspect::Surface, values(schema_for!(SoilSurface))),
            (SoilAspect::Structure, values(schema_for!(SoilStructure))),
            (SoilAspect::Colour, values(schema_for!(SoilColour))),
            (SoilAspect::Cover, values(schema_for!(SoilCover))),
            (SoilAspect::Deposits, values(schema_for!(SoilDeposits))),
            (SoilAspect::Erosion, values(schema_for!(SoilErosion))),
            (SoilAspect::Weeds, values(schema_for!(SoilWeeds))),
            (SoilAspect::Texture, texture),
        ])
    }

    #[test]
    fn every_soil_aspect_has_values() {
        let v = soil_values();
        assert_eq!(v.len(), 9);
        assert!(v.values().all(|vs| !vs.is_empty()));
        assert_eq!(
            v[&SoilAspect::Deposits],
            ["none", "salt_crust", "moss", "algae", "mould"]
        );
        assert_eq!(
            v[&SoilAspect::Texture],
            [
                "crumbles",
                "holds_ball",
                "sticky_ribbon",
                "unclear",
                "sandy",
                "loamy",
                "silty",
                "clayey"
            ]
        );
    }

    /// Object keys in sorted order at every depth. `serde_json` keeps insertion order when another
    /// crate in the build turns on `preserve_order`, so the file's bytes would depend on how cargo
    /// is invoked.
    fn sorted(v: serde_json::Value) -> serde_json::Value {
        use serde_json::Value;
        match v {
            Value::Object(map) => {
                let mut entries: Vec<_> = map.into_iter().collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                Value::Object(entries.into_iter().map(|(k, v)| (k, sorted(v))).collect())
            }
            Value::Array(items) => Value::Array(items.into_iter().map(sorted).collect()),
            other => other,
        }
    }

    /// `ml/knowledge/schema/seed.schema.json` follows these types, as `backend/openapi.json`
    /// follows the API's.
    #[test]
    fn the_seed_schema_file_is_current() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../ml/knowledge/schema/seed.schema.json");
        let expected = seed_schema_file();
        if std::env::var_os("UPDATE_SEED_SCHEMA").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &expected).unwrap();
        }
        let committed = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert!(
            committed == expected,
            "ml/knowledge/schema/seed.schema.json is stale: run \
             UPDATE_SEED_SCHEMA=1 cargo test -p domain the_seed_schema_file_is_current"
        );
    }
}
