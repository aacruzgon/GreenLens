//! The per-call JSON Schema builder ([contracts 6], per-job schema): image aliases `img1`…,
//! indicator keys `q<n>_<slug>`, enums filled from the payload, one branch per guided kind.
//!
//! Rules for the grammar (xgrammar 0.2.7 on vLLM, llama.cpp's converter; [vision vlm 8]):
//! property order is generation order (the worker builds `serde_json` with `preserve_order`),
//! every object has `additionalProperties: false`, length-limited strings never carry a
//! `pattern`, and boxes are arrays of exactly 4 integers with bounds from the box format.
//!
//! [vision vlm 8]: ../../../docs/vision/vlm.md#8-schemas-from-the-models-side

use domain::{
    jobs::{Indicator, OrganismOption, SpeciesOption},
    scan::GuidedKind,
    vlm::{BoxFormat, ImageAlias, OTHER, indicator_key},
};
use serde_json::{Map, Value, json};

/// Text and list limits. They go into the schema, and decoding enforces them again for servers
/// that ignore them and for the hosted API, whose schema loses them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// `notes`.
    pub notes: usize,
    /// `other_description`.
    pub other_description: usize,
    /// `other_name`.
    pub other_name: usize,
    /// A damage finding's `trace`.
    pub trace: usize,
    /// An indicator's `reason`.
    pub reason: usize,
    /// `organisms` and `damage`.
    pub max_items: usize,
}

impl Limits {
    /// The contract's values (`other_name` is ours: the contract gives none).
    pub const CONTRACT: Self = Self {
        notes: 200,
        other_description: 100,
        other_name: 60,
        trace: 60,
        reason: 60,
        max_items: 10,
    };
}

/// The range of `bbox_2d` values for a box format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxRange {
    /// Integers from 0 to `max`: 1000 for `norm_1000`, the largest prepared side for pixels.
    Integers {
        /// The upper bound.
        max: u32,
    },
    /// Numbers from 0 to 1 (`norm_1`).
    Unit,
}

impl BoxRange {
    /// The range for a format and the call's prepared images.
    pub fn for_format(format: BoxFormat, largest_side: u32) -> Self {
        match format {
            BoxFormat::Norm1000 => Self::Integers { max: 1000 },
            BoxFormat::AbsPxPrepared => Self::Integers { max: largest_side },
            BoxFormat::Norm1 => Self::Unit,
        }
    }

    fn schema(self) -> Value {
        let item = match self {
            Self::Integers { max } => json!({ "type": "integer", "minimum": 0, "maximum": max }),
            Self::Unit => json!({ "type": "number", "minimum": 0, "maximum": 1 }),
        };
        json!({ "type": "array", "items": item, "minItems": 4, "maxItems": 4 })
    }
}

fn object(properties: Vec<(&str, Value)>) -> Value {
    let required: Vec<&str> = properties.iter().map(|(k, _)| *k).collect();
    let mut props = Map::new();
    for (k, v) in properties {
        props.insert(k.to_owned(), v);
    }
    json!({
        "type": "object",
        "properties": props,
        "required": required,
        "additionalProperties": false,
    })
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "every caller builds the schema inline"
)]
fn nullable(schema: Value) -> Value {
    json!({ "anyOf": [schema, { "type": "null" }] })
}

fn string_enum<'s>(values: impl IntoIterator<Item = &'s str>) -> Value {
    json!({ "type": "string", "enum": values.into_iter().collect::<Vec<_>>() })
}

fn text(max: usize) -> Value {
    json!({ "type": "string", "maxLength": max })
}

fn certainty() -> Value {
    string_enum(["low", "medium", "high"])
}

/// The aliases of a call with `n` images: `["img1", …]`.
pub fn aliases(n: usize) -> Vec<String> {
    (0..n)
        .filter_map(ImageAlias::for_index)
        .map(|a| a.to_string())
        .collect()
}

fn soil_reading(values: &[&str], images: &Value) -> Value {
    nullable(object(vec![
        ("value", string_enum(values.iter().copied())),
        ("image", images.clone()),
        ("certainty", certainty()),
    ]))
}

/// The damage patterns a findings call may name.
fn damage_enum() -> Value {
    string_enum([
        "shot_holes",
        "ragged_holes",
        "chewed_margins",
        "leaf_mines",
        "sticky_residue",
        "sooty_mould",
        "webbing",
        "slime_trails",
        "frass",
        "skeletonised_leaves",
        "cut_stems",
        "torn_stems",
        "digging",
        "tunnels_or_mounds",
        "tracks",
        "droppings",
        "pecked_fruit",
        "other",
    ])
}

/// The soil object of the findings schema: one nullable reading per aspect.
fn soil_schema(images: &Value) -> Value {
    object(vec![
        (
            "moisture",
            soil_reading(&["dry", "moist", "wet", "waterlogged"], images),
        ),
        (
            "surface",
            soil_reading(&["normal", "crusted", "cracked"], images),
        ),
        (
            "structure",
            soil_reading(&["crumbly", "cloddy", "compacted", "unclear"], images),
        ),
        (
            "colour",
            soil_reading(&["dark", "medium", "pale", "grey_mottled"], images),
        ),
        (
            "cover",
            soil_reading(&["bare", "mulched", "residue", "ground_cover"], images),
        ),
        (
            "deposits",
            soil_reading(&["none", "salt_crust", "moss", "algae", "mould"], images),
        ),
        (
            "erosion",
            soil_reading(&["none", "rills", "exposed_roots"], images),
        ),
        ("weeds", soil_reading(&["none", "few", "many"], images)),
    ])
}

/// The findings call's schema for `n_images` images.
pub fn findings(
    n_images: usize,
    species: &[SpeciesOption],
    organisms: &[OrganismOption],
    boxes: BoxRange,
    limits: Limits,
) -> Value {
    let alias_list = aliases(n_images);
    let images = string_enum(alias_list.iter().map(String::as_str));
    let species_enum = string_enum(
        species
            .iter()
            .map(|s| s.id.as_str())
            .chain(std::iter::once(OTHER)),
    );
    let organism_enum = string_enum(
        organisms
            .iter()
            .map(|o| o.id.as_str())
            .chain(std::iter::once(OTHER)),
    );
    let damage_enum = damage_enum();
    let soil = soil_schema(&images);
    let organism = object(vec![
        ("label", organism_enum),
        ("other_name", nullable(text(limits.other_name))),
        ("count", string_enum(["one", "few", "many"])),
        ("image", images.clone()),
        ("bbox_2d", boxes.schema()),
        ("certainty", certainty()),
    ]);
    // `trace` comes first, so the model names the animal or the trace it left before it picks a
    // pattern; decoding drops a finding with none (task P11).
    let damage = object(vec![
        ("trace", text(limits.trace)),
        ("pattern", damage_enum),
        (
            "other_description",
            nullable(text(limits.other_description)),
        ),
        ("image", images.clone()),
        ("bbox_2d", nullable(boxes.schema())),
        ("certainty", certainty()),
    ]);
    object(vec![
        (
            "species_guess",
            object(vec![
                ("species", species_enum),
                ("other_name", nullable(text(limits.other_name))),
                ("certainty", certainty()),
            ]),
        ),
        ("soil", soil),
        (
            "organisms",
            json!({ "type": "array", "items": organism, "maxItems": limits.max_items }),
        ),
        (
            "damage",
            json!({ "type": "array", "items": damage, "maxItems": limits.max_items }),
        ),
        ("notes", text(limits.notes)),
    ])
}

/// One question of an indicators call, with its key and the aliases of the call's images whose
/// kind can show it. `'p` borrows the payload's indicator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question<'p> {
    /// `q3_concentric_rings`.
    pub key: String,
    /// The payload's indicator.
    pub indicator: &'p Indicator,
    /// Aliases of the images that can show it; empty means the answer is `unsure` only.
    pub images: Vec<ImageAlias>,
}

/// The questions of a call, numbered in payload order, with their images by kind.
pub fn questions<'p>(
    indicators: impl IntoIterator<Item = &'p Indicator>,
    image_kinds: &[domain::scan::ImageKind],
) -> Vec<Question<'p>> {
    indicators
        .into_iter()
        .enumerate()
        .map(|(i, indicator)| Question {
            key: indicator_key(i + 1, &indicator.indicator_id),
            indicator,
            images: image_kinds
                .iter()
                .enumerate()
                .filter(|(_, k)| indicator.image_kinds.contains(k))
                .filter_map(|(j, _)| ImageAlias::for_index(j))
                .collect(),
        })
        .collect()
}

/// The indicators call's schema: one required key per question, `reason` first; a question no
/// image can show allows only `unsure`.
pub fn indicators(n_images: usize, questions: &[Question<'_>], limits: Limits) -> Value {
    let alias_list = aliases(n_images);
    let images = nullable(string_enum(alias_list.iter().map(String::as_str)));
    let props: Vec<(&str, Value)> = questions
        .iter()
        .map(|q| {
            let answers = if q.images.is_empty() {
                string_enum(["unsure"])
            } else {
                string_enum(["yes", "no", "unsure"])
            };
            (
                q.key.as_str(),
                object(vec![
                    ("reason", text(limits.reason)),
                    ("answer", answers),
                    ("image", images.clone()),
                ]),
            )
        })
        .collect();
    object(props)
}

/// The guided-check call's schema: only the branch for the image's kind.
pub fn guided(kind: GuidedKind) -> Value {
    match kind {
        GuidedKind::SqueezeTest => object(vec![
            ("kind", string_enum(["squeeze_test"])),
            (
                "result",
                string_enum(["crumbles", "holds_ball", "sticky_ribbon", "unclear"]),
            ),
            ("certainty", certainty()),
        ]),
        GuidedKind::JarTest => object(vec![
            ("kind", string_enum(["jar_test"])),
            ("settled", json!({ "type": "boolean" })),
            (
                "texture",
                string_enum(["sandy", "loamy", "silty", "clayey", "unclear"]),
            ),
            ("certainty", certainty()),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use domain::{jobs::IndicatorWhere, scan::ImageKind};

    use super::*;

    fn indicator(id: &str, kinds: &[ImageKind]) -> Indicator {
        Indicator {
            indicator_id: id.parse().unwrap(),
            question: "?".to_owned(),
            r#where: IndicatorWhere::LowerLeaves,
            image_kinds: kinds.to_vec(),
        }
    }

    #[test]
    fn reason_is_generated_before_the_answer() {
        let ind = indicator(
            "tomato_early_blight.concentric_rings",
            &[ImageKind::LeafCloseup],
        );
        let qs = questions([&ind], &[ImageKind::WholePlant, ImageKind::LeafCloseup]);
        assert_eq!(qs[0].key, "q1_concentric_rings");
        assert_eq!(qs[0].images, vec![ImageAlias::for_index(1).unwrap()]);
        let schema = indicators(2, &qs, Limits::CONTRACT);
        let text = serde_json::to_string(&schema).unwrap();
        assert!(text.find("\"reason\"").unwrap() < text.find("\"answer\"").unwrap());
        assert_eq!(
            schema["properties"]["q1_concentric_rings"]["properties"]["answer"]["enum"],
            json!(["yes", "no", "unsure"])
        );
        assert_eq!(schema["additionalProperties"], json!(false));
    }

    #[test]
    fn a_question_no_image_can_show_allows_only_unsure() {
        let ind = indicator("aphids.honeydew", &[ImageKind::LeafUnderside]);
        let qs = questions([&ind], &[ImageKind::WholePlant]);
        let schema = indicators(1, &qs, Limits::CONTRACT);
        assert_eq!(
            schema["properties"]["q1_honeydew"]["properties"]["answer"]["enum"],
            json!(["unsure"])
        );
    }

    #[test]
    fn findings_enums_come_from_the_payload_and_boxes_from_the_format() {
        let species = [SpeciesOption {
            id: "tomato".parse().unwrap(),
            name: "Tomato".to_owned(),
        }];
        let schema = findings(
            3,
            &species,
            &[],
            BoxRange::for_format(BoxFormat::Norm1000, 0),
            Limits::CONTRACT,
        );
        assert_eq!(
            schema["properties"]["species_guess"]["properties"]["species"]["enum"],
            json!(["tomato", "other"])
        );
        let bbox = &schema["properties"]["organisms"]["items"]["properties"]["bbox_2d"];
        assert_eq!(bbox["items"]["maximum"], json!(1000));
        assert_eq!(bbox["minItems"], json!(4));
        assert_eq!(
            schema["properties"]["soil"]["properties"]["moisture"]["anyOf"][0]["properties"]["image"]
                ["enum"],
            json!(["img1", "img2", "img3"])
        );
    }

    fn tomato_findings(boxes: BoxRange) -> Value {
        let species = [SpeciesOption {
            id: "tomato".parse().unwrap(),
            name: "Tomato".to_owned(),
        }];
        let organisms = [OrganismOption {
            id: "aphid".parse().unwrap(),
            name: "Aphid".to_owned(),
        }];
        findings(4, &species, &organisms, boxes, Limits::CONTRACT)
    }

    #[test]
    fn a_damage_finding_names_its_trace_before_its_pattern() {
        let schema = tomato_findings(BoxRange::for_format(BoxFormat::Norm1000, 0));
        let damage = &schema["properties"]["damage"]["items"];
        assert_eq!(
            damage["properties"]["trace"],
            json!({ "type": "string", "maxLength": 60 })
        );
        assert_eq!(damage["required"][0], json!("trace"));
        let keys: Vec<&String> = damage["properties"].as_object().unwrap().keys().collect();
        assert_eq!(keys[..2], ["trace", "pattern"]);
    }

    /// Union-typed parameters (`anyOf` or a type array) in a schema.
    fn unions(v: &Value) -> usize {
        match v {
            Value::Object(o) => {
                usize::from(o.contains_key("anyOf") || o.get("type").is_some_and(Value::is_array))
                    + o.values().map(unions).sum::<usize>()
            }
            Value::Array(a) => a.iter().map(unions).sum(),
            _ => 0,
        }
    }

    #[test]
    fn the_hosted_findings_schema_keeps_the_trace_within_the_union_limit() {
        // The fallback sends the schema through the hosted API's sanitiser, which strips
        // `maxLength` and allows at most 16 union-typed parameters a request (integrations
        // anthropic::schema). `trace` is a plain string, so it adds none.
        let schema = tomato_findings(BoxRange::for_format(BoxFormat::AbsPxPrepared, 1568));
        let sanitized = integrations::anthropic::sanitize_schema(&schema);
        let damage = &sanitized.schema["properties"]["damage"]["items"];
        assert_eq!(damage["properties"]["trace"], json!({ "type": "string" }));
        assert_eq!(damage["required"][0], json!("trace"));
        assert_eq!(damage["additionalProperties"], json!(false));
        assert!(unions(&sanitized.schema) <= 16);
    }

    #[test]
    fn guided_schemas_have_one_branch() {
        let s = guided(GuidedKind::JarTest);
        assert_eq!(s["properties"]["kind"]["enum"], json!(["jar_test"]));
        assert!(s["properties"].get("result").is_none());
    }
}
