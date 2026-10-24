//! After decoding ([contracts 6]): parse the model's JSON into the raw shapes, validate it
//! against the call (aliases, enums from the payload, every indicator key, the `unsure`-only
//! rule), truncate strings and clamp boxes, drop repeated damage findings, map aliases back to
//! image ids, convert boxes from the adapter's format, and turn certainty words into confidences.

use domain::{
    id::{ImageId, IndicatorId, OrganismId, SpeciesId},
    jobs::{OrganismOption, SpeciesOption},
    scan::{Answer, GuidedKind},
    vlm::{
        BoxFormat, DamageFinding, DamagePattern, DamageRaw, GuidedTexture, ImageAlias,
        IndicatorAnswer, OTHER, OrganismFinding, OrganismRaw, SoilAspect, SoilFinding, SoilRaw,
        SoilReadingValue, SpeciesGuess, VlmFindings, VlmFindingsRaw, VlmGuidedRaw,
        VlmIndicatorAnswers, VlmIndicatorsRaw,
    },
    wire::{BBox, Region},
};
use serde::Serialize;

use super::{prepare::PreparedImage, profile::CertaintyMap, schema::Limits};

/// Why an answer failed validation. Each is retried once, then fails the job with
/// `model_error`, `retryable: true`.
#[derive(Debug, thiserror::Error)]
pub enum Invalid {
    /// Not JSON of the expected shape.
    #[error("unparseable: {0}")]
    Json(#[from] serde_json::Error),
    /// The model stopped at its token limit.
    #[error("cut off at the token limit")]
    Truncated,
    /// The model refused.
    #[error("refused")]
    Refused,
    /// An alias outside the call's images.
    #[error("unknown image alias {0}")]
    Alias(String),
    /// A species outside `species_options` and not `other`.
    #[error("unknown species {0}")]
    Species(String),
    /// An organism outside `organism_options` and not `other`.
    #[error("unknown organism {0}")]
    Organism(String),
    /// An indicator key is missing.
    #[error("missing answer {0}")]
    MissingKey(String),
    /// A key that isn't one of the call's questions.
    #[error("unexpected answer {0}")]
    ExtraKey(String),
    /// `yes` or `no` to a question no image of the call can show.
    #[error("{0} may only be unsure")]
    UnsureOnly(String),
    /// The guided answer's branch isn't the image's kind.
    #[error("guided answer for the wrong check")]
    WrongBranch,
}

/// The images of one call, by alias. `'c` borrows the job's prepared images.
#[derive(Debug, Clone)]
pub struct CallImages<'c> {
    images: Vec<&'c PreparedImage>,
}

impl<'c> CallImages<'c> {
    /// The call's images in alias order (`img1` first).
    pub const fn new(images: Vec<&'c PreparedImage>) -> Self {
        Self { images }
    }

    fn get(&self, alias: ImageAlias) -> Result<&'c PreparedImage, Invalid> {
        self.images
            .get(alias.index())
            .copied()
            .ok_or_else(|| Invalid::Alias(alias.to_string()))
    }
}

/// Cuts a string to `max` characters (not bytes, so a multi-byte character is never split).
/// A string that reaches `max` was most likely stopped there by the schema's `maxLength`, mid-word
/// and sometimes in another language (vision vlm.md 4), so it goes back to its last full sentence,
/// or failing that its last full word (the whole cut when it ends at the end of a word).
pub fn truncate(s: &str, max: usize) -> String {
    let cut: String = s.chars().take(max).collect();
    if s.chars().count() < max {
        return cut;
    }
    // The cut's last word is whole when a space ends the cut or follows it; a text of exactly
    // `max` was stopped there, so its last word may be cut.
    let word_ends =
        cut.ends_with(char::is_whitespace) || s.chars().nth(max).is_some_and(char::is_whitespace);
    let cut = cut.trim_end();
    if cut.ends_with(['.', '!', '?']) {
        return cut.to_owned();
    }
    let mut chars = cut.char_indices().peekable();
    let mut sentence_end = None;
    while let Some((i, c)) = chars.next() {
        if matches!(c, '.' | '!' | '?') && chars.peek().is_some_and(|&(_, n)| n.is_whitespace()) {
            sentence_end = Some(i + c.len_utf8());
        }
    }
    let kept = match (sentence_end, cut.rfind(char::is_whitespace)) {
        (Some(end), _) => &cut[..end],
        (None, _) if word_ends => cut,
        (None, Some(space)) => &cut[..space],
        (None, None) => cut,
    };
    kept.trim_end_matches([',', ';', ':', ' ']).to_owned()
}

/// Converts a raw `bbox_2d` to the stored image's `[x, y, w, h]` by the adapter's format,
/// clamped to the image. A box with no area after clamping is `None`.
pub fn convert_box(raw: [i64; 4], format: BoxFormat, prepared: (u32, u32)) -> Option<BBox> {
    #[allow(
        clippy::cast_precision_loss,
        reason = "box coordinates are at most a few thousand"
    )]
    let corners = raw.map(|v| v as f64);
    format
        .to_bbox(corners, prepared)
        .ok()
        .filter(|b| b.to_array()[2] > 0.0 && b.to_array()[3] > 0.0)
}

fn soil<V: Serialize>(
    out: &mut Vec<SoilFinding>,
    aspect: SoilAspect,
    reading: Option<SoilReadingValue<V>>,
    images: &CallImages<'_>,
    certainty: &CertaintyMap,
) -> Result<(), Invalid> {
    let Some(r) = reading else { return Ok(()) };
    let value = serde_json::to_value(&r.value)?;
    out.push(SoilFinding {
        aspect,
        value: value.as_str().unwrap_or_default().to_owned(),
        image_id: images.get(r.image)?.image_id,
        confidence: certainty.confidence(r.certainty),
    });
    Ok(())
}

/// Whether an `other_name` names a listed option by its id or its name. Case, surrounding spaces
/// and `_` for a space don't matter.
fn names(other_name: &str, id: &str, name: &str) -> bool {
    let norm = |s: &str| s.trim().replace('_', " ").to_lowercase();
    let other_name = norm(other_name);
    other_name == norm(id) || other_name == norm(name)
}

fn known<'o, T, I: PartialEq + 'o>(
    value: &str,
    options: impl IntoIterator<Item = (&'o I, T)>,
    id: impl Fn(&I) -> &str,
) -> Option<T> {
    options
        .into_iter()
        .find(|(i, _)| id(i) == value)
        .map(|(_, t)| t)
}

/// The soil readings of a findings answer, in aspect order.
fn soil_findings(
    s: SoilRaw,
    images: &CallImages<'_>,
    certainty: &CertaintyMap,
) -> Result<Vec<SoilFinding>, Invalid> {
    let mut soil_findings = Vec::new();
    soil(
        &mut soil_findings,
        SoilAspect::Moisture,
        s.moisture,
        images,
        certainty,
    )?;
    soil(
        &mut soil_findings,
        SoilAspect::Surface,
        s.surface,
        images,
        certainty,
    )?;
    soil(
        &mut soil_findings,
        SoilAspect::Structure,
        s.structure,
        images,
        certainty,
    )?;
    soil(
        &mut soil_findings,
        SoilAspect::Colour,
        s.colour,
        images,
        certainty,
    )?;
    soil(
        &mut soil_findings,
        SoilAspect::Cover,
        s.cover,
        images,
        certainty,
    )?;
    soil(
        &mut soil_findings,
        SoilAspect::Deposits,
        s.deposits,
        images,
        certainty,
    )?;
    soil(
        &mut soil_findings,
        SoilAspect::Erosion,
        s.erosion,
        images,
        certainty,
    )?;
    soil(
        &mut soil_findings,
        SoilAspect::Weeds,
        s.weeds,
        images,
        certainty,
    )?;
    Ok(soil_findings)
}

/// The organisms of a findings answer: known or `other`, boxed, at most `max_items`.
fn organism_findings(
    raw: Vec<OrganismRaw>,
    options: &[OrganismOption],
    images: &CallImages<'_>,
    format: BoxFormat,
    certainty: &CertaintyMap,
    limits: Limits,
) -> Result<Vec<OrganismFinding>, Invalid> {
    let mut organism_findings = Vec::new();
    for o in raw.into_iter().take(limits.max_items) {
        let organism_id: Option<OrganismId> = if o.label == OTHER {
            // `other` naming a listed organism is that organism, as for species.
            o.other_name.as_deref().and_then(|n| {
                options
                    .iter()
                    .find(|x| names(n, x.id.as_str(), &x.name))
                    .map(|x| x.id.clone())
            })
        } else {
            Some(
                known(
                    &o.label,
                    options.iter().map(|x| (&x.id, x.id.clone())),
                    |i| i.as_str(),
                )
                .ok_or_else(|| Invalid::Organism(o.label.clone()))?,
            )
        };
        let image = images.get(o.image)?;
        // An organism needs its box; one with no area after clamping is dropped.
        let Some(bbox) = convert_box(o.bbox_2d, format, (image.width, image.height)) else {
            continue;
        };
        organism_findings.push(OrganismFinding {
            other_name: organism_id
                .is_none()
                .then(|| {
                    o.other_name
                        .as_deref()
                        .map(|n| truncate(n, limits.other_name))
                })
                .flatten(),
            organism_id,
            count: o.count,
            region: Region::unmasked(image.image_id, bbox, None),
            confidence: certainty.confidence(o.certainty),
        });
    }
    Ok(organism_findings)
}

/// Words that say a trace is absent: a trace with one of them is no trace.
const NEGATIONS: &str = "no none not nothing without unknown unclear unsure n/a na";

/// Words that describe a disease sign, its colour, shape or place, or join other words, by group.
/// A trace made only of these names no animal and nothing an animal left (task P11): "brown spots
/// with a yellow halo", "concentric rings on the lower leaves".
const NOT_A_TRACE: [&str; 5] = [
    // Disease signs, and words that only say something is there.
    "spot spots spotting speck specks lesion lesions blight ring rings concentric target halo \
     halos streak streaks stripe stripes blotch blotches patch patches discolouration \
     discoloration discoloured discolored yellowing browning wilting wilt chlorosis chlorotic \
     necrosis necrotic rot rotting rotten mould mold mildew powdery downy rust fungus fungal \
     fungi bacterial viral virus disease diseased infection infected canker cankers scab \
     sunscald scorch scorched dieback dead dying dried dry tissue mark marks dot dots pustule \
     pustules spores sign signs symptom symptoms damage damaged visible seen trace traces",
    // Colours.
    "brown dark black yellow pale white grey gray tan red purple orange light green brownish \
     yellowish blackish",
    // Shapes, sizes and amounts.
    "small large tiny big round circular irregular elongated long angular many few several some \
     area areas surface sunken raised",
    // Plant parts.
    "leaf leaves leaflet leaflets stem stems fruit fruits plant plants margin margins edge edges \
     vein veins tip tips lower upper older",
    // Joining words.
    "a an the of on in with and or along across between inside around its their at to by from \
     has have is are",
];

/// Whether `word` is one of the space-separated words of `list`.
fn listed(list: &str, word: &str) -> bool {
    list.split_whitespace().any(|w| w == word)
}

/// Whether a damage finding's `trace` names an animal or something an animal left. Empty, negated
/// ("no insect seen") or made only of disease, colour, shape and place words, it doesn't.
fn names_a_trace(trace: &str) -> bool {
    let lower = trace.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !(c.is_alphanumeric() || c == '/'))
        .filter(|w| !w.is_empty())
        .collect();
    !words.is_empty()
        && !words.iter().any(|w| listed(NEGATIONS, w))
        && words
            .iter()
            .any(|w| !NOT_A_TRACE.iter().any(|group| listed(group, w)))
}

/// The damage of a findings answer, at most `max_items`. A finding whose `trace` names no animal
/// and nothing an animal left is dropped, so a disease lesion reported as damage never reaches the
/// API (task P11). A finding that repeats an earlier one (pattern, image, box and description) is
/// dropped too: a model in a loop can fill the list with one.
fn damage_findings(
    raw: Vec<DamageRaw>,
    images: &CallImages<'_>,
    format: BoxFormat,
    certainty: &CertaintyMap,
    limits: Limits,
) -> Result<Vec<DamageFinding>, Invalid> {
    let mut damage_findings = Vec::new();
    let traced = raw.into_iter().filter(|d| {
        let keep = names_a_trace(&d.trace);
        if !keep {
            tracing::debug!(pattern = ?d.pattern, trace = %d.trace, "damage finding without a trace dropped");
        }
        keep
    });
    for d in traced.take(limits.max_items) {
        let image = images.get(d.image)?;
        let region = d
            .bbox_2d
            .and_then(|b| convert_box(b, format, (image.width, image.height)))
            .map(|bbox| Region::unmasked(image.image_id, bbox, None));
        let finding = DamageFinding {
            pattern: d.pattern,
            // Only `other` keeps its description, as `other_name` with a known id.
            other_description: (d.pattern == DamagePattern::Other)
                .then(|| {
                    d.other_description
                        .as_deref()
                        .map(|t| truncate(t, limits.other_description))
                })
                .flatten(),
            region,
            image_id: image.image_id,
            confidence: certainty.confidence(d.certainty),
        };
        let repeated = damage_findings.iter().any(|f: &DamageFinding| {
            f.pattern == finding.pattern
                && f.image_id == finding.image_id
                && f.region == finding.region
                && f.other_description == finding.other_description
        });
        if !repeated {
            damage_findings.push(finding);
        }
    }
    Ok(damage_findings)
}

/// The findings call's answer.
pub fn findings(
    text: &str,
    images: &CallImages<'_>,
    species: &[SpeciesOption],
    organisms: &[OrganismOption],
    format: BoxFormat,
    certainty: &CertaintyMap,
    limits: Limits,
) -> Result<VlmFindings, Invalid> {
    let raw: VlmFindingsRaw = serde_json::from_str(text)?;
    let species_id: Option<SpeciesId> = if raw.species_guess.species == OTHER {
        // `other` naming a listed species is that species: the day-1 check got `other` with
        // `other_name: "apple"` while `apple` was listed (vision vlm.md 4).
        raw.species_guess.other_name.as_deref().and_then(|n| {
            species
                .iter()
                .find(|x| names(n, x.id.as_str(), &x.name))
                .map(|x| x.id.clone())
        })
    } else {
        Some(
            known(
                &raw.species_guess.species,
                species.iter().map(|s| (&s.id, s.id.clone())),
                |i| i.as_str(),
            )
            .ok_or_else(|| Invalid::Species(raw.species_guess.species.clone()))?,
        )
    };
    let soil_findings = soil_findings(raw.soil, images, certainty)?;
    let organism_findings =
        organism_findings(raw.organisms, organisms, images, format, certainty, limits)?;
    let damage_findings = damage_findings(raw.damage, images, format, certainty, limits)?;
    Ok(VlmFindings {
        species_guess: SpeciesGuess {
            other_name: species_id
                .is_none()
                .then(|| {
                    raw.species_guess
                        .other_name
                        .as_deref()
                        .map(|n| truncate(n, limits.other_name))
                })
                .flatten(),
            species_id,
            confidence: certainty.confidence(raw.species_guess.certainty),
        },
        soil: soil_findings,
        organisms: organism_findings,
        damage: damage_findings,
        notes: truncate(&raw.notes, limits.notes),
    })
}

/// One question as decoding needs it: its key, its indicator, whether only `unsure` is allowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expected {
    /// `q1_concentric_rings`.
    pub key: String,
    /// The indicator.
    pub indicator_id: IndicatorId,
    /// No image of the call can show it.
    pub unsure_only: bool,
}

/// The indicators call's answer: exactly one answer per question, in question order.
pub fn indicators(
    text: &str,
    images: &CallImages<'_>,
    expected: &[Expected],
    limits: Limits,
) -> Result<VlmIndicatorAnswers, Invalid> {
    let mut raw: VlmIndicatorsRaw = serde_json::from_str(text)?;
    let mut answers = Vec::with_capacity(expected.len());
    for e in expected {
        let a = raw
            .remove(&e.key)
            .ok_or_else(|| Invalid::MissingKey(e.key.clone()))?;
        if e.unsure_only && a.answer != Answer::Unsure {
            return Err(Invalid::UnsureOnly(e.key.clone()));
        }
        let image_id: Option<ImageId> = a
            .image
            .map(|al| images.get(al))
            .transpose()?
            .map(|p| p.image_id);
        answers.push(IndicatorAnswer {
            indicator_id: e.indicator_id.clone(),
            answer: a.answer,
            reason: truncate(&a.reason, limits.reason),
            image_id,
        });
    }
    if let Some(extra) = raw.keys().next() {
        return Err(Invalid::ExtraKey(extra.clone()));
    }
    Ok(VlmIndicatorAnswers { answers })
}

/// The guided call's answer; its branch must be the image's kind.
pub fn guided(
    text: &str,
    kind: GuidedKind,
    certainty: &CertaintyMap,
) -> Result<GuidedTexture, Invalid> {
    let raw: VlmGuidedRaw = serde_json::from_str(text)?;
    match (kind, raw) {
        (
            GuidedKind::SqueezeTest,
            VlmGuidedRaw::SqueezeTest {
                result,
                certainty: c,
            },
        ) => Ok(GuidedTexture::SqueezeTest {
            result,
            confidence: certainty.confidence(c),
        }),
        (
            GuidedKind::JarTest,
            VlmGuidedRaw::JarTest {
                settled,
                texture,
                certainty: c,
            },
        ) => Ok(GuidedTexture::JarTest {
            settled,
            texture,
            confidence: certainty.confidence(c),
        }),
        _ => Err(Invalid::WrongBranch),
    }
}

#[cfg(test)]
mod tests {
    use domain::scan::ImageKind;

    use super::*;

    fn prepared(id: &str, w: u32, h: u32) -> PreparedImage {
        PreparedImage {
            image_id: id.parse().unwrap(),
            kind: ImageKind::LeafCloseup,
            width: w,
            height: h,
            jpeg: Vec::new(),
        }
    }

    const IMG1: &str = "01JAX6AAAAAAAAAAAAAAAAAAAA";
    const IMG2: &str = "01JAX6BBBBBBBBBBBBBBBBBBBB";

    fn findings_json(organism_box: &str) -> String {
        format!(
            r#"{{"species_guess":{{"species":"tomato","other_name":null,"certainty":"high"}},
            "soil":{{"moisture":{{"value":"dry","image":"img2","certainty":"medium"}},
              "surface":null,"structure":null,"colour":null,"cover":null,"deposits":null,"erosion":null,"weeds":null}},
            "organisms":[{{"label":"other","other_name":"{}","count":"few","image":"img1",
              "bbox_2d":{organism_box},"certainty":"low"}}],
            "damage":[{{"trace":"flea beetles on the leaf","pattern":"shot_holes","other_description":null,"image":"img2","bbox_2d":[0,0,0,0],"certainty":"high"}}],
            "notes":"ok"}}"#,
            "x".repeat(80)
        )
    }

    #[test]
    #[allow(clippy::many_single_char_names, reason = "x, y, w, h of a box")]
    fn findings_map_aliases_boxes_and_words() {
        let (a, b) = (prepared(IMG1, 1152, 864), prepared(IMG2, 1152, 864));
        let images = CallImages::new(vec![&a, &b]);
        let species = [SpeciesOption {
            id: "tomato".parse().unwrap(),
            name: "Tomato".to_owned(),
        }];
        let f = findings(
            &findings_json("[100,200,400,600]"),
            &images,
            &species,
            &[],
            BoxFormat::Norm1000,
            &CertaintyMap::default(),
            Limits::CONTRACT,
        )
        .unwrap();
        assert_eq!(f.species_guess.species_id.unwrap().as_str(), "tomato");
        assert_eq!(f.soil[0].image_id.to_string(), IMG2);
        assert_eq!(f.soil[0].value, "dry");
        let o = &f.organisms[0];
        assert_eq!(o.region.image_id.to_string(), IMG1);
        let [x, y, w, h] = o.region.bbox.to_array();
        assert!(
            (x - 0.1).abs() < 1e-9
                && (y - 0.2).abs() < 1e-9
                && (w - 0.3).abs() < 1e-9
                && (h - 0.4).abs() < 1e-9
        );
        assert_eq!(o.other_name.as_ref().unwrap().chars().count(), 60); // truncated
        assert!((o.confidence.get() - 0.4).abs() < 1e-12);
        assert!(f.damage[0].region.is_none()); // a zero-area box is dropped, the finding kept
    }

    #[test]
    fn a_degenerate_organism_box_drops_the_organism() {
        let a = prepared(IMG1, 100, 100);
        let b = prepared(IMG2, 100, 100);
        let f = findings(
            &findings_json("[500,500,500,900]"),
            &CallImages::new(vec![&a, &b]),
            &[SpeciesOption {
                id: "tomato".parse().unwrap(),
                name: "Tomato".to_owned(),
            }],
            &[],
            BoxFormat::Norm1000,
            &CertaintyMap::default(),
            Limits::CONTRACT,
        )
        .unwrap();
        assert_eq!(f.organisms.len(), 0);
    }

    #[test]
    fn unknown_values_and_aliases_fail_validation() {
        let a = prepared(IMG1, 100, 100);
        let images = CallImages::new(vec![&a]);
        let err = findings(
            &findings_json("[0,0,10,10]"),
            &images,
            &[],
            &[],
            BoxFormat::Norm1000,
            &CertaintyMap::default(),
            Limits::CONTRACT,
        )
        .unwrap_err();
        assert!(matches!(err, Invalid::Species(_)));
    }

    #[test]
    fn only_other_damage_keeps_its_description() {
        let a = prepared(IMG1, 100, 100);
        let json = findings_json("[0,0,10,10]").replace(
            r#""damage":[{"trace":"flea beetles on the leaf","pattern":"shot_holes","other_description":null,"#,
            r#""damage":[{"trace":"larva in the tunnel","pattern":"leaf_mines","other_description":"pale tunnel","image":"img1","bbox_2d":null,"certainty":"medium"},
              {"trace":"gnawed by teeth","pattern":"other","other_description":"bark stripped","#,
        );
        let b = prepared(IMG2, 100, 100);
        let f = findings(
            &json,
            &CallImages::new(vec![&a, &b]),
            &[SpeciesOption {
                id: "tomato".parse().unwrap(),
                name: "Tomato".to_owned(),
            }],
            &[],
            BoxFormat::Norm1000,
            &CertaintyMap::default(),
            Limits::CONTRACT,
        )
        .unwrap();
        assert_eq!(f.damage.len(), 2);
        assert_eq!(f.damage[0].other_description, None);
        assert_eq!(
            f.damage[1].other_description.as_deref(),
            Some("bark stripped")
        );
    }

    #[test]
    fn text_at_its_cap_goes_back_to_a_full_sentence_or_word() {
        // Under the cap: unchanged, even mid-word.
        assert_eq!(truncate("dry soil, few weeds", 60), "dry soil, few weeds");
        // At the cap: Qwen3.5's tomato notes with findings@4, stopped at 200 mid-word.
        let notes = "The provided images appear to be a composite of different plants. The first image shows a leaf with small spots, the second shows a tomato plant with severe blight and rot, and the third shows a corn/";
        assert_eq!(notes.chars().count(), 200);
        assert_eq!(
            truncate(notes, 200),
            "The provided images appear to be a composite of different plants."
        );
        // Over the cap with no full sentence: back to the last full word, or the whole cut
        // when it ends at the end of a word (Sam's review of #45).
        assert_eq!(
            truncate("yellow halo around brown spots", 22),
            "yellow halo around"
        );
        assert_eq!(
            truncate("yellow halo around brown spots", 18),
            "yellow halo around"
        );
        assert_eq!(
            truncate("yellow halo around brown spots", 19),
            "yellow halo around"
        );
        assert_eq!(truncate("lower leaves, yellowing", 13), "lower leaves");
        assert_eq!(truncate("lower leaves, yellowing", 16), "lower leaves");
        // A sentence that ends at the cap is kept whole; one word longer than the cap is cut.
        assert_eq!(truncate("Soil is dry.", 12), "Soil is dry.");
        assert_eq!(truncate("Growing西红柿", 9), "Growing西红");
    }

    #[test]
    fn other_naming_a_listed_option_takes_its_id() {
        let a = prepared(IMG1, 100, 100);
        let b = prepared(IMG2, 100, 100);
        let images = CallImages::new(vec![&a, &b]);
        let species = [SpeciesOption {
            id: "bell_pepper".parse().unwrap(),
            name: "Bell pepper".to_owned(),
        }];
        let organisms = [OrganismOption {
            id: "aphid".parse().unwrap(),
            name: "Aphid".to_owned(),
        }];
        let decode = |species_other_name: &str, organism_other_name: &str| {
            let json = findings_json("[0,0,10,10]")
                .replace(
                    r#""species":"tomato","other_name":null"#,
                    &format!(r#""species":"other","other_name":"{species_other_name}""#),
                )
                .replace(&"x".repeat(80), organism_other_name);
            findings(
                &json,
                &images,
                &species,
                &organisms,
                BoxFormat::Norm1000,
                &CertaintyMap::default(),
                Limits::CONTRACT,
            )
            .unwrap()
        };
        // By name or by id, whatever the case and with a space for `_`.
        for (species_name, organism_name) in [("Bell Pepper", " aphid "), ("bell pepper", "APHID")]
        {
            let f = decode(species_name, organism_name);
            assert_eq!(f.species_guess.species_id.unwrap().as_str(), "bell_pepper");
            assert_eq!(f.species_guess.other_name, None);
            assert_eq!(
                f.organisms[0].organism_id.as_ref().unwrap().as_str(),
                "aphid"
            );
            assert_eq!(f.organisms[0].other_name, None);
        }
        // Anything else stays `other` with its name.
        let f = decode("chilli pepper", "aphid mummy");
        assert_eq!(f.species_guess.species_id, None);
        assert_eq!(f.species_guess.other_name.as_deref(), Some("chilli pepper"));
        assert_eq!(f.organisms[0].organism_id, None);
        assert_eq!(f.organisms[0].other_name.as_deref(), Some("aphid mummy"));
    }

    #[test]
    fn repeated_damage_is_dropped() {
        let a = prepared(IMG1, 100, 100);
        let b = prepared(IMG2, 100, 100);
        // Like Qwen3.5's corn answer with `findings@3`, one finding repeated up to the cap of 10.
        // The same pattern in another box, or in no box, is a finding of its own.
        let mine = r#"{"trace":"larva in the tunnel","pattern":"leaf_mines","other_description":"pale tunnels","image":"img2","bbox_2d":[138,333,422,422],"certainty":"high"}"#;
        let other_box = mine.replace("[138,333,422,422]", "[500,500,600,600]");
        let no_box = mine.replace("[138,333,422,422]", "null");
        let damage = [mine; 7]
            .into_iter()
            .chain([other_box.as_str(), no_box.as_str(), no_box.as_str()])
            .collect::<Vec<_>>()
            .join(",");
        let f = findings_with_damage(&damage, &a, &b);
        assert_eq!(f.damage.len(), 3);
        assert!(
            f.damage
                .iter()
                .all(|d| d.pattern == DamagePattern::LeafMines)
        );
        assert!(f.damage[0].region.is_some() && f.damage[1].region.is_some());
        assert_ne!(f.damage[0].region, f.damage[1].region);
        assert!(f.damage[2].region.is_none());
    }

    /// `findings_json` with its damage list replaced by `damage` (items joined by commas).
    fn findings_with_damage(damage: &str, a: &PreparedImage, b: &PreparedImage) -> VlmFindings {
        let json = findings_json("[0,0,10,10]").replace(
            r#""damage":[{"trace":"flea beetles on the leaf","pattern":"shot_holes","other_description":null,"image":"img2","bbox_2d":[0,0,0,0],"certainty":"high"}]"#,
            &format!(r#""damage":[{damage}]"#),
        );
        findings(
            &json,
            &CallImages::new(vec![a, b]),
            &[SpeciesOption {
                id: "tomato".parse().unwrap(),
                name: "Tomato".to_owned(),
            }],
            &[],
            BoxFormat::Norm1000,
            &CertaintyMap::default(),
            Limits::CONTRACT,
        )
        .unwrap()
    }

    #[test]
    fn damage_without_a_trace_is_dropped() {
        let a = prepared(IMG1, 100, 100);
        let b = prepared(IMG2, 100, 100);
        let item = |trace: &str, pattern: &str| {
            format!(
                r#"{{"trace":"{trace}","pattern":"{pattern}","other_description":null,"image":"img2","bbox_2d":null,"certainty":"high"}}"#
            )
        };
        let damage = [
            // Kept: an animal, or something an animal left.
            item("larva visible inside the tunnel", "leaf_mines"),
            item("frass", "frass"),
            item("bite marks along the chewed edge", "chewed_margins"),
            // Dropped: no trace, a negated one, or a disease sign only. The first two are
            // Qwen3.5's early-blight spots as `leaf_mines` and FP8's corn blight as
            // `chewed_margins` (vision vlm.md 4), as they would read with a trace field.
            item("brown spots with a yellow halo", "leaf_mines"),
            item("long tan lesions along the leaf", "chewed_margins"),
            item("concentric rings on the lower leaves", "other"),
            item("no insect visible", "shot_holes"),
            item("", "leaf_mines"),
            item("  ", "webbing"),
            // A server that ignores the schema may leave the field out: that is no trace either.
            r#"{"pattern":"sooty_mould","other_description":null,"image":"img2","bbox_2d":null,"certainty":"low"}"#.to_owned(),
        ]
        .join(",");
        let f = findings_with_damage(&damage, &a, &b);
        let patterns: Vec<DamagePattern> = f.damage.iter().map(|d| d.pattern).collect();
        assert_eq!(
            patterns,
            [
                DamagePattern::LeafMines,
                DamagePattern::Frass,
                DamagePattern::ChewedMargins
            ]
        );
    }

    #[test]
    fn a_trace_names_an_animal_or_what_it_left() {
        for kept in [
            "aphids",
            "Caterpillar on the leaf",
            "fine silk webbing",
            "dried slime",
            "dark frass line inside the mine",
            "holes chewed by beetles",
            "deer footprints",
        ] {
            assert!(names_a_trace(kept), "{kept}");
        }
        for dropped in [
            "",
            "none",
            "N/A",
            "not visible",
            "no animal seen",
            "without any insect",
            "Brown spots",
            "dark brown lesions with concentric rings",
            "yellowing leaf margins",
            "grey powdery mould on the upper leaves",
            "necrotic tissue",
        ] {
            assert!(!names_a_trace(dropped), "{dropped}");
        }
    }

    #[test]
    fn indicators_need_every_key_and_respect_unsure_only() {
        let a = prepared(IMG1, 100, 100);
        let images = CallImages::new(vec![&a]);
        let expected = [
            Expected {
                key: "q1_rings".to_owned(),
                indicator_id: "p.rings".parse().unwrap(),
                unsure_only: false,
            },
            Expected {
                key: "q2_honeydew".to_owned(),
                indicator_id: "p.honeydew".parse().unwrap(),
                unsure_only: true,
            },
        ];
        let ok = r#"{"q1_rings":{"reason":"rings on img1","answer":"yes","image":"img1"},
                     "q2_honeydew":{"reason":"not shown","answer":"unsure","image":null}}"#;
        let answers = indicators(ok, &images, &expected, Limits::CONTRACT).unwrap();
        assert_eq!(answers.answers.len(), 2);
        assert_eq!(answers.answers[0].image_id.unwrap().to_string(), IMG1);
        let missing = r#"{"q1_rings":{"reason":"","answer":"yes","image":null}}"#;
        assert!(matches!(
            indicators(missing, &images, &expected, Limits::CONTRACT),
            Err(Invalid::MissingKey(_))
        ));
        let not_unsure = ok.replace(r#""answer":"unsure""#, r#""answer":"no""#);
        assert!(matches!(
            indicators(&not_unsure, &images, &expected, Limits::CONTRACT),
            Err(Invalid::UnsureOnly(_))
        ));
    }

    #[test]
    fn guided_answers_must_match_the_check() {
        let jar = r#"{"kind":"jar_test","settled":true,"texture":"loamy","certainty":"medium"}"#;
        assert!(guided(jar, GuidedKind::JarTest, &CertaintyMap::default()).is_ok());
        assert!(matches!(
            guided(jar, GuidedKind::SqueezeTest, &CertaintyMap::default()),
            Err(Invalid::WrongBranch)
        ));
    }
}
