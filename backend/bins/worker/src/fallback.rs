//! The fallback worker's model ([architecture 8.4]; [vision 8]): every job kind on the hosted
//! LLM. `vlm`, `verify` and `guided` go through the same `VlmAdapter` as the rig (with the
//! hosted profile: `abs_px_prepared` boxes, images of at most 1568 pixels a side, schema limits
//! stripped and enforced after decoding). `perception` is its own small flow: a whole-image
//! `leaf` region per close-up that shows leaves, species and disease restricted to the payload's
//! label lists, no masks and no embeddings, and a job with `question_id` completed at once.
//!
//! [architecture 8.4]: ../../../docs/architecture/README.md#84-job-leasing-and-fallback
//! [vision 8]: ../../../docs/vision/README.md#8-the-fallback-path

use std::{collections::BTreeMap, fmt::Write as _, sync::Arc};

use async_trait::async_trait;
use domain::{
    id::{ProblemOrHealthy, RegionId, SpeciesId},
    jobs::{
        DiseaseResult, DiseaseSkipReason, DiseaseSkipped, GuidedPayload, GuidedResult,
        PerceptionPayload, PerceptionRegion, PerceptionResult, PerceptionStage, ProblemScore,
        RegionClass, SpeciesLabel, SpeciesResult, VerifyPayload, VerifyResult, VlmPayload,
        VlmResult,
    },
    ports::{JobContext, LoadedImage, ModelError, ProgressSink, VisionModel},
    rules::routing::{RoutableImage, findings_images},
    scan::ImageKind,
    wire::{BBox, Certainty},
};
use serde::Deserialize;
use serde_json::json;
use vision::{
    VisionError,
    manifest::Manifest,
    perception::heads::{DiseaseHead, LabelMatrix},
};

use crate::vlm::{
    VlmAdapter,
    backend::{AnthropicBackend, VlmCall, VlmImage},
    decode::Invalid,
    prepare::PreparedImage,
    prompt,
};

/// The label lists the fallback needs to answer `perception` jobs in the payload's terms.
#[derive(Debug, Default)]
pub struct FallbackLabels {
    species: BTreeMap<String, Arc<LabelMatrix>>,
    disease: BTreeMap<String, Arc<DiseaseHead>>,
}

impl FallbackLabels {
    /// Reads every `bioclip_labels*` and `disease_head*` entry of the manifest (hashes checked).
    pub fn load(manifest: &Manifest) -> Result<Self, VisionError> {
        let mut out = Self::default();
        for e in manifest.entries_with_prefix("bioclip_labels") {
            manifest.verify(&[e.name.as_str()])?;
            let m = LabelMatrix::load(&manifest.path(e))?;
            out.species.insert(m.version.clone(), Arc::new(m));
        }
        for e in manifest.entries_with_prefix("disease_head") {
            manifest.verify(&[e.name.as_str()])?;
            let h = DiseaseHead::load(&manifest.path(e))?;
            out.disease.insert(h.version.clone(), Arc::new(h));
        }
        Ok(out)
    }
}

/// The fallback model.
pub struct FallbackModel {
    vlm: VlmAdapter<AnthropicBackend>,
    labels: FallbackLabels,
}

impl FallbackModel {
    /// The hosted adapter and the label lists.
    pub const fn new(vlm: VlmAdapter<AnthropicBackend>, labels: FallbackLabels) -> Self {
        Self { vlm, labels }
    }

    fn unknown(what: &'static str, version: &str) -> ModelError {
        VisionError::UnknownLabelVersion {
            what,
            version: version.to_owned(),
        }
        .into_model_error("llm_fallback")
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the stages in the contract's order, as on the rig"
    )]
    async fn perceive_inner(
        &self,
        payload: &PerceptionPayload,
        images: &[LoadedImage],
    ) -> Result<PerceptionResult, ModelError> {
        let mut result = PerceptionResult {
            plant_present: None,
            regions: Vec::new(),
            species: None,
            disease: None,
            disease_skipped: None,
            image_embeddings: None,
        };
        // A question photo's segmentation job exists only for masks, which the fallback can't
        // make: complete at once, with no regions and no LLM call ([contracts 5], results).
        if payload.question_id.is_some() {
            return Ok(result);
        }
        let wants = |s| payload.stages.contains(&s);
        let identify = wants(PerceptionStage::Species)
            && payload.species_id.is_none()
            && !payload.skip_species_identification;
        let species_list = identify
            .then(|| {
                self.labels
                    .species
                    .get(&payload.label_set_version)
                    .cloned()
                    .ok_or_else(|| Self::unknown("label_set_version", &payload.label_set_version))
            })
            .transpose()?;
        let head = wants(PerceptionStage::Disease)
            .then(|| {
                self.labels
                    .disease
                    .get(&payload.classifier_labels_version)
                    .cloned()
                    .ok_or_else(|| {
                        Self::unknown(
                            "classifier_labels_version",
                            &payload.classifier_labels_version,
                        )
                    })
            })
            .transpose()?;

        // At most 4 images, in the findings order (plant shots first).
        let routable: Vec<RoutableImage<usize>> = images
            .iter()
            .enumerate()
            .map(|(i, img)| RoutableImage {
                kind: img.image.kind,
                nth_of_kind: u32::try_from(
                    images[..i]
                        .iter()
                        .filter(|o| o.image.kind == img.image.kind)
                        .count(),
                )
                .unwrap_or(0),
                item: i,
            })
            .collect();
        let chosen: Vec<LoadedImage> = findings_images(&routable)
            .into_iter()
            .map(|i| images[i].clone())
            .collect();
        let prepared = self.vlm.prepare(&chosen).await?;
        let refs: Vec<&PreparedImage> = prepared.iter().collect();

        let survey = self
            .survey(
                &refs,
                wants(PerceptionStage::PlantCheck),
                species_list.as_deref(),
            )
            .await?;
        if wants(PerceptionStage::PlantCheck) {
            result.plant_present = Some(survey.plant_present.unwrap_or(true));
        }
        for (i, p) in refs.iter().enumerate() {
            if p.kind == ImageKind::LeafCloseup && survey.leaves.get(i).copied().unwrap_or(false) {
                result.regions.push(PerceptionRegion {
                    region_id: RegionId {
                        image: p.image_id,
                        n: 1,
                    },
                    image_id: p.image_id,
                    class: RegionClass::Leaf,
                    bbox: BBox::FULL,
                    mask: None,
                    confidence: self.vlm.profile().certainty.confidence(Certainty::Medium),
                });
            }
        }
        let mut working = payload.species_id.clone();
        if let Some(top) = survey.species {
            working = working.or_else(|| top.first().and_then(|c| c.species_id.clone()));
            result.species = Some(SpeciesResult { candidates: top });
        }
        if let Some(head) = head {
            let classes = working
                .as_ref()
                .map(|s| head.classes(s))
                .unwrap_or_default();
            let leafy: Vec<&PreparedImage> = refs
                .iter()
                .copied()
                .filter(|p| matches!(p.kind, ImageKind::LeafCloseup))
                .collect();
            if classes.is_empty() {
                result.disease_skipped = Some(DiseaseSkipped {
                    reason: DiseaseSkipReason::SpeciesNotSupported,
                });
            } else if leafy.is_empty() {
                result.disease_skipped = Some(DiseaseSkipped {
                    reason: DiseaseSkipReason::NoLeavesFound,
                });
            } else {
                result.disease = Some(self.classify(&leafy, &classes).await?);
            }
        }
        Ok(result)
    }

    /// One call: plant present, which close-ups show leaves, and the species top 3 from the
    /// label set (or `other`).
    async fn survey(
        &self,
        images: &[&PreparedImage],
        plant_check: bool,
        species: Option<&LabelMatrix>,
    ) -> Result<Survey, ModelError> {
        let mut props = serde_json::Map::new();
        if plant_check {
            props.insert("plant_present".to_owned(), json!({ "type": "boolean" }));
        }
        props.insert(
            "leaves_visible".to_owned(),
            json!({ "type": "array", "items": { "type": "boolean" }, "minItems": images.len(), "maxItems": images.len() }),
        );
        if let Some(list) = species {
            let mut ids: Vec<&str> = list.species.iter().map(|(id, _)| id.as_str()).collect();
            ids.push("other");
            props.insert(
                "species".to_owned(),
                json!({ "type": "array", "maxItems": 3, "items": {
                    "type": "object", "additionalProperties": false,
                    "required": ["species", "certainty"],
                    "properties": { "species": { "type": "string", "enum": ids },
                                    "certainty": { "type": "string", "enum": ["low", "medium", "high"] } } } }),
            );
        }
        let required: Vec<String> = props.keys().cloned().collect();
        let schema = json!({ "type": "object", "additionalProperties": false,
                             "required": required, "properties": props });
        let mut instructions = String::from(
            "Look at the photos of one garden plant.\n- leaves_visible: for each image in order, true when it clearly shows leaves of the plant.\n",
        );
        if plant_check {
            instructions.push_str("- plant_present: true when any photo shows a living plant.\n");
        }
        if let Some(list) = species {
            instructions.push_str(
                "- species: up to 3 species from this list, most likely first, or other:\n",
            );
            for (id, label) in &list.species {
                let _ = writeln!(instructions, "  - {id}: {label}");
            }
        }
        let labels: Vec<String> = images
            .iter()
            .enumerate()
            .map(|(i, p)| prompt::image_label(i, p.kind, false))
            .collect();
        let call = VlmCall {
            system: &prompt::system(self.vlm.profile().box_format),
            images: labels
                .iter()
                .zip(images)
                .map(|(label, p)| VlmImage {
                    label,
                    jpeg: &p.jpeg,
                })
                .collect(),
            instructions: &instructions,
            schema_name: "perception_survey",
            schema: &schema,
            max_output_tokens: self.vlm.profile().budget.findings,
        };
        let certainty = self.vlm.profile().certainty;
        self.vlm
            .call_validated(&call, |text| {
                let raw: SurveyRaw = serde_json::from_str(text)?;
                let species = species.map(|list| {
                    raw.species
                        .iter()
                        .map(|s| SpeciesLabel {
                            species_id: s
                                .species
                                .parse::<SpeciesId>()
                                .ok()
                                .filter(|id| list.species.iter().any(|(known, _)| known == id)),
                            label: list
                                .species
                                .iter()
                                .find(|(id, _)| id.as_str() == s.species)
                                .map_or_else(|| s.species.clone(), |(_, l)| l.clone()),
                            score: certainty.confidence(s.certainty),
                        })
                        .collect()
                });
                Ok(Survey {
                    plant_present: raw.plant_present,
                    leaves: raw.leaves_visible,
                    species,
                })
            })
            .await
    }

    /// One call per job: each close-up's top 3 among the working species' classes, confidences
    /// from certainty words, never a softmax.
    async fn classify(
        &self,
        images: &[&PreparedImage],
        classes: &[ProblemOrHealthy],
    ) -> Result<Vec<DiseaseResult>, ModelError> {
        let names: Vec<String> = classes.iter().map(ToString::to_string).collect();
        let item = json!({ "type": "object", "additionalProperties": false,
            "required": ["problem", "certainty"],
            "properties": { "problem": { "type": "string", "enum": names },
                            "certainty": { "type": "string", "enum": ["low", "medium", "high"] } } });
        let schema = json!({ "type": "object", "additionalProperties": false, "required": ["images"],
            "properties": { "images": { "type": "array", "minItems": images.len(), "maxItems": images.len(),
                "items": { "type": "array", "maxItems": 3, "items": item } } } });
        let instructions = format!(
            "For each image in order, the up to 3 most likely conditions of the leaf from this list, most likely first: {}. Choose healthy when the leaf looks healthy.",
            names.join(", ")
        );
        let labels: Vec<String> = images
            .iter()
            .enumerate()
            .map(|(i, p)| prompt::image_label(i, p.kind, false))
            .collect();
        let call = VlmCall {
            system: &prompt::system(self.vlm.profile().box_format),
            images: labels
                .iter()
                .zip(images)
                .map(|(label, p)| VlmImage {
                    label,
                    jpeg: &p.jpeg,
                })
                .collect(),
            instructions: &instructions,
            schema_name: "perception_disease",
            schema: &schema,
            max_output_tokens: self.vlm.profile().budget.indicators,
        };
        let certainty = self.vlm.profile().certainty;
        self.vlm
            .call_validated(&call, |text| {
                let raw: DiseaseRaw = serde_json::from_str(text)?;
                if raw.images.len() != images.len() {
                    return Err(Invalid::MissingKey("images".to_owned()));
                }
                Ok(images
                    .iter()
                    .zip(raw.images)
                    .map(|(p, top)| DiseaseResult {
                        image_id: p.image_id,
                        region_id: None,
                        top: top
                            .into_iter()
                            .filter_map(|t| {
                                Some(ProblemScore {
                                    problem_id: t.problem.parse().ok()?,
                                    confidence: certainty.confidence(t.certainty),
                                })
                            })
                            .collect(),
                    })
                    .collect())
            })
            .await
    }
}

#[derive(Debug)]
struct Survey {
    plant_present: Option<bool>,
    leaves: Vec<bool>,
    species: Option<Vec<SpeciesLabel>>,
}

#[derive(Debug, Deserialize)]
struct SurveyRaw {
    #[serde(default)]
    plant_present: Option<bool>,
    leaves_visible: Vec<bool>,
    #[serde(default)]
    species: Vec<RankedSpecies>,
}

#[derive(Debug, Deserialize)]
struct RankedSpecies {
    species: String,
    certainty: Certainty,
}

#[derive(Debug, Deserialize)]
struct DiseaseRaw {
    images: Vec<Vec<RankedProblem>>,
}

#[derive(Debug, Deserialize)]
struct RankedProblem {
    problem: String,
    certainty: Certainty,
}

#[async_trait]
impl VisionModel for FallbackModel {
    fn model_versions(&self) -> BTreeMap<String, String> {
        self.vlm.profile().model_versions()
    }

    async fn perceive(
        &self,
        _job: JobContext,
        payload: &PerceptionPayload,
        images: &[LoadedImage],
        _progress: Arc<dyn ProgressSink>,
    ) -> Result<PerceptionResult, ModelError> {
        self.perceive_inner(payload, images).await
    }

    async fn analyse(
        &self,
        job: JobContext,
        payload: &VlmPayload,
        images: &[LoadedImage],
        progress: Arc<dyn ProgressSink>,
    ) -> Result<VlmResult, ModelError> {
        self.vlm.analyse(job, payload, images, progress).await
    }

    async fn verify(
        &self,
        job: JobContext,
        payload: &VerifyPayload,
        images: &[LoadedImage],
        progress: Arc<dyn ProgressSink>,
    ) -> Result<VerifyResult, ModelError> {
        self.vlm.verify(job, payload, images, progress).await
    }

    async fn read_guided_check(
        &self,
        job: JobContext,
        payload: &GuidedPayload,
        image: &LoadedImage,
    ) -> Result<GuidedResult, ModelError> {
        self.vlm.read_guided_check(job, payload, image).await
    }
}
