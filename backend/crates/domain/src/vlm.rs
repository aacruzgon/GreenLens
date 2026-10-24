//! VLM structured output ([contracts 6]): the raw shapes the model writes under guided
//! decoding (aliases, certainty words, adapter box formats), and the shapes the worker converts
//! them to before the API stores them.
//!
//! The raw shapes are not in the `OpenAPI` document: the worker builds each call's JSON Schema
//! from the payload (allowed slugs, the indicator keys), so the types here serve decoding and
//! validation only.

use std::{collections::BTreeMap, fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use utoipa::ToSchema;

use crate::{
    id::{ImageId, IndicatorId, OrganismId, SpeciesId},
    scan::Answer,
    url::Stored,
    wire::{BBox, BBoxError, Certainty, Confidence, Region},
};

// ---------------------------------------------------------------------------------------------
// Shared vocabularies
// ---------------------------------------------------------------------------------------------

/// `DamagePattern` ([contracts 6]).
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
pub enum DamagePattern {
    /// Small round holes (flea beetles).
    ShotHoles,
    /// Ragged holes (slugs).
    RaggedHoles,
    /// Chewed leaf margins.
    ChewedMargins,
    /// Tunnels inside leaves (leaf miners).
    LeafMines,
    /// Honeydew (aphids).
    StickyResidue,
    /// Black mould on honeydew.
    SootyMould,
    /// Webbing (spider mites).
    Webbing,
    /// Slime trails (slugs, snails).
    SlimeTrails,
    /// Insect droppings.
    Frass,
    /// Only the veins left.
    SkeletonisedLeaves,
    /// Cleanly cut stems (rabbits).
    CutStems,
    /// Torn stems (deer).
    TornStems,
    /// Digging.
    Digging,
    /// Vole or mole tunnels and mounds.
    TunnelsOrMounds,
    /// Tracks.
    Tracks,
    /// Animal droppings.
    Droppings,
    /// Birds pecking fruit.
    PeckedFruit,
    /// Anything else, with `other_description`.
    Other,
}

/// `count` of an organism: `"one" | "few" | "many"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrganismCount {
    /// One.
    One,
    /// A few.
    Few,
    /// Many.
    Many,
}

/// `SoilAspect` ([contracts 6], after decoding). `texture` comes from guided checks.
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
pub enum SoilAspect {
    /// Moisture.
    Moisture,
    /// Surface.
    Surface,
    /// Structure.
    Structure,
    /// Colour.
    Colour,
    /// Cover.
    Cover,
    /// Deposits.
    Deposits,
    /// Erosion.
    Erosion,
    /// Weeds.
    Weeds,
    /// From guided checks.
    Texture,
}

/// `img1` … `img4`: an image as the model sees it in one call ([contracts 6], per-job schema).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ImageAlias(u8);

impl ImageAlias {
    /// Every call gets at most 4 images ([contracts 5], photo routing).
    pub const MAX: u8 = 4;

    /// The alias of the image at `index` (0-based) in the call's image list.
    pub fn for_index(index: usize) -> Option<Self> {
        u8::try_from(index + 1)
            .ok()
            .filter(|n| (1..=Self::MAX).contains(n))
            .map(Self)
    }

    /// The image's 0-based position in the call's image list.
    pub fn index(self) -> usize {
        usize::from(self.0 - 1)
    }
}
impl fmt::Display for ImageAlias {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "img{}", self.0)
    }
}
impl FromStr for ImageAlias {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.strip_prefix("img")
            .and_then(|n| n.parse::<u8>().ok())
            .filter(|n| (1..=Self::MAX).contains(n))
            .map(Self)
            .ok_or("expected img1 to img4")
    }
}
impl Serialize for ImageAlias {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}
impl<'de> Deserialize<'de> for ImageAlias {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        s.parse().map_err(de::Error::custom)
    }
}

/// The species and organism enums of a call end in this extra value ([contracts 6]).
pub const OTHER: &str = "other";

// ---------------------------------------------------------------------------------------------
// Raw output, findings call
// ---------------------------------------------------------------------------------------------

/// `soil.moisture` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SoilMoisture {
    /// Dry.
    Dry,
    /// Moist.
    Moist,
    /// Wet.
    Wet,
    /// Waterlogged.
    Waterlogged,
}

/// `soil.surface` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SoilSurface {
    /// Normal.
    Normal,
    /// Crusted.
    Crusted,
    /// Cracked.
    Cracked,
}

/// `soil.structure` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SoilStructure {
    /// Crumbly.
    Crumbly,
    /// Cloddy.
    Cloddy,
    /// Compacted.
    Compacted,
    /// Unclear.
    Unclear,
}

/// `soil.colour` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SoilColour {
    /// Dark.
    Dark,
    /// Medium.
    Medium,
    /// Pale.
    Pale,
    /// Grey and mottled (waterlogging).
    GreyMottled,
}

/// `soil.cover` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SoilCover {
    /// Bare.
    Bare,
    /// Mulched.
    Mulched,
    /// Crop residue.
    Residue,
    /// Ground cover plants.
    GroundCover,
}

/// `soil.deposits` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SoilDeposits {
    /// None.
    None,
    /// Salt crust.
    SaltCrust,
    /// Moss.
    Moss,
    /// Algae.
    Algae,
    /// Mould.
    Mould,
}

/// `soil.erosion` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SoilErosion {
    /// None.
    None,
    /// Rills.
    Rills,
    /// Exposed roots.
    ExposedRoots,
}

/// `soil.weeds` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SoilWeeds {
    /// None.
    None,
    /// A few.
    Few,
    /// Many.
    Many,
}

/// One soil reading: `{ value, image, certainty }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoilReadingValue<V> {
    /// The value.
    pub value: V,
    /// The shot that showed it.
    pub image: ImageAlias,
    /// How sure the model is.
    pub certainty: Certainty,
}

/// `SoilReading<V>`: `null` when not visible. Always present in the object.
pub type SoilReading<V> = Option<SoilReadingValue<V>>;

/// `VlmFindingsRaw.species_guess`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeciesGuessRaw {
    /// A `species_options` slug, or `"other"`.
    pub species: String,
    /// With `"other"`; `null` otherwise.
    pub other_name: Option<String>,
    /// How sure.
    pub certainty: Certainty,
}

/// `VlmFindingsRaw.soil`: every aspect present, `null` when not visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoilRaw {
    /// Moisture.
    pub moisture: SoilReading<SoilMoisture>,
    /// Surface.
    pub surface: SoilReading<SoilSurface>,
    /// Structure.
    pub structure: SoilReading<SoilStructure>,
    /// Colour.
    pub colour: SoilReading<SoilColour>,
    /// Cover.
    pub cover: SoilReading<SoilCover>,
    /// Deposits.
    pub deposits: SoilReading<SoilDeposits>,
    /// Erosion.
    pub erosion: SoilReading<SoilErosion>,
    /// Weeds.
    pub weeds: SoilReading<SoilWeeds>,
}

/// `VlmFindingsRaw.organisms[]` (at most 10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganismRaw {
    /// An `organism_options` slug, or `"other"`.
    pub label: String,
    /// With `"other"`.
    pub other_name: Option<String>,
    /// How many.
    pub count: OrganismCount,
    /// The shot.
    pub image: ImageAlias,
    /// Integers, in the adapter's `box_format`.
    pub bbox_2d: [i64; 4],
    /// How sure.
    pub certainty: Certainty,
}

/// `VlmFindingsRaw.damage[]` (at most 10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DamageRaw {
    /// The animal seen, or the trace it left, that shows an animal made the damage (a larva in
    /// the mine, frass, webbing, bite marks), at most 60 characters; empty when none is visible.
    /// Written first, so the pattern follows from it. A finding without one is dropped in
    /// decoding (task P11): a disease lesion has no trace. Required: a reply without it is
    /// invalid output, retried once like any other ([contracts 6], after decoding).
    pub trace: String,
    /// The pattern.
    pub pattern: DamagePattern,
    /// With `"other"`, at most 100 characters.
    pub other_description: Option<String>,
    /// The shot.
    pub image: ImageAlias,
    /// Integers in the adapter's `box_format`, or `null`.
    pub bbox_2d: Option<[i64; 4]>,
    /// How sure.
    pub certainty: Certainty,
}

/// `VlmFindingsRaw` ([contracts 6], findings call).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VlmFindingsRaw {
    /// The model's own species guess.
    pub species_guess: SpeciesGuessRaw,
    /// Soil, every aspect.
    pub soil: SoilRaw,
    /// Organisms.
    pub organisms: Vec<OrganismRaw>,
    /// Damage.
    pub damage: Vec<DamageRaw>,
    /// At most 200 characters.
    pub notes: String,
}

/// One value of `VlmIndicatorsRaw`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndicatorAnswerRaw {
    /// At most 100 characters (the prompt asks for at most 60); written first, so the answer
    /// follows from it.
    pub reason: String,
    /// The answer; `"unsure"` only when none of the indicator's kinds is in the call.
    pub answer: Answer,
    /// The shot that decided it.
    pub image: Option<ImageAlias>,
}

/// `VlmIndicatorsRaw`: one required key per indicator, `q<n>_<slug cut to 24 characters>`.
pub type VlmIndicatorsRaw = BTreeMap<String, IndicatorAnswerRaw>;

/// The key of the `n`-th indicator (1-based) in an indicators call: `q3_concentric_rings`.
pub fn indicator_key(n: usize, indicator: &IndicatorId) -> String {
    let slug: String = indicator.slug().chars().take(24).collect();
    format!("q{n}_{slug}")
}

/// `squeeze_test.result`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SqueezeResult {
    /// Sandy.
    Crumbles,
    /// Loamy.
    HoldsBall,
    /// Clayey.
    StickyRibbon,
    /// Can't tell.
    Unclear,
}

/// `jar_test.texture`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum JarTexture {
    /// Sandy.
    Sandy,
    /// Loamy.
    Loamy,
    /// Silty.
    Silty,
    /// Clayey.
    Clayey,
    /// Can't tell.
    Unclear,
}

/// `VlmGuidedRaw` ([contracts 6], guided-check call): the schema has only the branch for the
/// image's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VlmGuidedRaw {
    /// A squeeze test.
    SqueezeTest {
        /// What the soil did.
        result: SqueezeResult,
        /// How sure.
        certainty: Certainty,
    },
    /// A jar test.
    JarTest {
        /// Whether the layers had settled.
        settled: bool,
        /// The texture the layers show.
        texture: JarTexture,
        /// How sure.
        certainty: Certainty,
    },
}

/// How a model adapter writes boxes ([contracts 6], box formats per model adapter).
/// Every format is corners `[x1, y1, x2, y2]` in `bbox_2d`. The format belongs to the adapter
/// instance (model, server and preparation together), not to the model family alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoxFormat {
    /// Pixels of the prepared image: divide by its size. The Qwen2.5-VL baseline on vLLM, and
    /// the fallback worker's hosted LLM (sent images it won't resize), confirmed by a live run
    /// on 2026-10-23 (contracts open question 26).
    AbsPxPrepared,
    /// 0–1000: divide by 1000. The chosen Qwen3-VL-class models (Qwen3-VL, Qwen3.5) on vLLM or
    /// llama.cpp; the `vlm_box_format` default.
    Norm1000,
    /// 0–1: no conversion. Models that return 0–1 coordinates.
    #[serde(rename = "norm_1")]
    Norm1,
}

impl BoxFormat {
    /// Converts corners in this format to a normalised `[x, y, w, h]`, clamping to the image.
    /// `prepared` is the prepared image's width and height, used by `abs_px_prepared`.
    pub fn to_bbox(self, corners: [f64; 4], prepared: (u32, u32)) -> Result<BBox, BBoxError> {
        let (sx, sy) = match self {
            Self::AbsPxPrepared => (f64::from(prepared.0), f64::from(prepared.1)),
            Self::Norm1000 => (1000.0, 1000.0),
            Self::Norm1 => (1.0, 1.0),
        };
        let [x1, y1, x2, y2] = corners;
        let (x1, x2) = (x1.min(x2) / sx, x1.max(x2) / sx);
        let (y1, y2) = (y1.min(y2) / sy, y1.max(y2) / sy);
        let (x1, y1) = (x1.clamp(0.0, 1.0), y1.clamp(0.0, 1.0));
        let (x2, y2) = (x2.clamp(0.0, 1.0), y2.clamp(0.0, 1.0));
        BBox::new(x1, y1, x2 - x1, y2 - y1)
    }
}

// ---------------------------------------------------------------------------------------------
// After decoding: what the worker returns and the API stores
// ---------------------------------------------------------------------------------------------

/// `VlmFindings.species_guess`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SpeciesGuess {
    /// `null` for `"other"`.
    #[schema(required = true)]
    pub species_id: Option<SpeciesId>,
    /// With `"other"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub other_name: Option<String>,
    /// From the certainty word.
    pub confidence: Confidence,
}

/// `VlmFindings.soil[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SoilFinding {
    /// The aspect.
    pub aspect: SoilAspect,
    /// Its value, such as `"dry"`.
    pub value: String,
    /// The shot.
    pub image_id: ImageId,
    /// From the certainty word.
    pub confidence: Confidence,
}

/// `VlmFindings.organisms[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct OrganismFinding {
    /// `null` for `"other"`.
    #[schema(required = true)]
    pub organism_id: Option<OrganismId>,
    /// With `"other"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub other_name: Option<String>,
    /// How many.
    pub count: OrganismCount,
    /// The box; VLM regions never have a mask.
    #[schema(value_type = Region)]
    pub region: Region<Stored>,
    /// From the certainty word.
    pub confidence: Confidence,
}

/// `VlmFindings.damage[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct DamageFinding {
    /// The pattern.
    pub pattern: DamagePattern,
    /// With `"other"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub other_description: Option<String>,
    /// The box, when the model gave one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false, value_type = Option<Region>)]
    pub region: Option<Region<Stored>>,
    /// The shot.
    pub image_id: ImageId,
    /// From the certainty word.
    pub confidence: Confidence,
}

/// `VlmFindings` ([contracts 6]): aliases mapped back to ids, certainty words to numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct VlmFindings {
    /// The species guess.
    pub species_guess: SpeciesGuess,
    /// Soil readings that were visible.
    pub soil: Vec<SoilFinding>,
    /// Organisms.
    pub organisms: Vec<OrganismFinding>,
    /// Damage.
    pub damage: Vec<DamageFinding>,
    /// Notes.
    pub notes: String,
}

/// `VlmIndicatorAnswers.answers[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IndicatorAnswer {
    /// The indicator.
    pub indicator_id: IndicatorId,
    /// The answer.
    pub answer: Answer,
    /// Why.
    pub reason: String,
    /// The shot that decided it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub image_id: Option<ImageId>,
}

/// `VlmIndicatorAnswers` ([contracts 6]).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct VlmIndicatorAnswers {
    /// One answer per indicator of the payload.
    pub answers: Vec<IndicatorAnswer>,
}

/// `GuidedTexture` ([contracts 6]), tagged by `kind`. A jar test with `settled: false` makes no
/// texture observation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GuidedTexture {
    /// A squeeze test.
    SqueezeTest {
        /// What the soil did.
        result: SqueezeResult,
        /// From the certainty word.
        confidence: Confidence,
    },
    /// A jar test.
    JarTest {
        /// Whether the layers had settled.
        settled: bool,
        /// The texture.
        texture: JarTexture,
        /// From the certainty word.
        confidence: Confidence,
    },
}

impl From<VlmGuidedRaw> for GuidedTexture {
    fn from(raw: VlmGuidedRaw) -> Self {
        match raw {
            VlmGuidedRaw::SqueezeTest { result, certainty } => Self::SqueezeTest {
                result,
                confidence: certainty.confidence(),
            },
            VlmGuidedRaw::JarTest {
                settled,
                texture,
                certainty,
            } => Self::JarTest {
                settled,
                texture,
                confidence: certainty.confidence(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases() {
        assert_eq!(ImageAlias::for_index(0).unwrap().to_string(), "img1");
        assert!(ImageAlias::for_index(4).is_none());
        assert_eq!("img3".parse::<ImageAlias>().unwrap().index(), 2);
        assert!("img5".parse::<ImageAlias>().is_err());
    }

    #[test]
    fn indicator_keys_carry_the_slug() {
        let id: IndicatorId = "tomato_early_blight.concentric_rings".parse().unwrap();
        assert_eq!(indicator_key(3, &id), "q3_concentric_rings");
        let long: IndicatorId = "p.a_very_long_indicator_slug_beyond_limit".parse().unwrap();
        assert_eq!(indicator_key(1, &long), "q1_a_very_long_indicator_sl");
    }

    #[test]
    fn soil_reading_null_is_kept() {
        let raw = r#"{"moisture":{"value":"dry","image":"img2","certainty":"high"},
            "surface":null,"structure":null,"colour":null,"cover":null,
            "deposits":null,"erosion":null,"weeds":null}"#;
        let soil: SoilRaw = serde_json::from_str(raw).unwrap();
        assert_eq!(soil.moisture.unwrap().value, SoilMoisture::Dry);
        assert!(
            serde_json::to_string(&soil)
                .unwrap()
                .contains("\"surface\":null")
        );
    }

    #[test]
    fn a_damage_finding_carries_its_trace_first() {
        let raw = r#"{"trace":"larva in the tunnel","pattern":"leaf_mines","other_description":null,"image":"img2","bbox_2d":null,"certainty":"medium"}"#;
        let d: DamageRaw = serde_json::from_str(raw).unwrap();
        assert_eq!(d.trace, "larva in the tunnel");
        assert_eq!(serde_json::to_string(&d).unwrap(), raw);
        // A server that ignores the schema and leaves it out sent invalid output.
        assert!(
            serde_json::from_str::<DamageRaw>(
                r#"{"pattern":"other","other_description":"x","image":"img1","bbox_2d":null,"certainty":"low"}"#,
            )
            .is_err()
        );
    }

    #[test]
    fn box_formats() {
        let b = BoxFormat::Norm1000
            .to_bbox([100.0, 200.0, 400.0, 600.0], (0, 0))
            .unwrap();
        let expected = [0.1, 0.2, 0.3, 0.4];
        assert!(
            b.to_array()
                .iter()
                .zip(expected)
                .all(|(got, want)| (got - want).abs() < 1e-9)
        );
        let b = BoxFormat::AbsPxPrepared
            .to_bbox([0.0, 0.0, 560.0, 280.0], (1120, 560))
            .unwrap();
        assert_eq!(b.to_array(), [0.0, 0.0, 0.5, 0.5]);
    }

    #[test]
    fn guided_texture_tags_by_kind() {
        let t: GuidedTexture = VlmGuidedRaw::JarTest {
            settled: true,
            texture: JarTexture::Loamy,
            certainty: Certainty::Medium,
        }
        .into();
        assert_eq!(
            serde_json::to_value(t).unwrap(),
            serde_json::json!({"kind": "jar_test", "settled": true, "texture": "loamy", "confidence": 0.65})
        );
    }
}
