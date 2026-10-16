//! The perception worker's models as one `VisionModel` ([contracts 5], results; [vision
//! models]): plant check, segmentation (RF-DETR boxes, then SAM 2.1 masks as COCO RLE),
//! species (`BioCLIP` 2 zero-shot), disease (a head on `BioCLIP` 2 features of each leaf crop),
//! and image embeddings. Each model runs once per job over a batch, split at its own cap under
//! `PERCEPTION_MAX_BATCH`; each finished stage sends its partial result.
//!
//! [contracts 5]: ../../../docs/architecture/contracts.md#results
//! [vision models]: ../../../docs/vision/models.md

use std::{collections::BTreeMap, sync::Arc, time::Instant};

use async_trait::async_trait;
use domain::{
    events::{Skipped, Stage},
    id::{ImageId, RegionId, SpeciesId},
    jobs::{
        DiseaseResult, DiseaseSkipReason, DiseaseSkipped, GuidedPayload, GuidedResult,
        ImageEmbedding, PartialResult, PerceptionPayload, PerceptionRegion, PerceptionResult,
        PerceptionStage, RegionClass, SpeciesResult, VerifyPayload, VerifyResult, VlmPayload,
        VlmResult, WorkerEvent,
    },
    ports::{JobContext, LoadedImage, ModelError, ProgressSink, VisionModel},
    scan::ImageKind,
    wire::{BBox, Confidence},
};
use image::RgbImage;
use ndarray::{Array3, ArrayD, Axis, IxDyn};
use tokio::sync::Semaphore;

use crate::{
    VisionError,
    imaging::{self, ImageView, PixelRect},
    manifest::{Manifest, ModelInput},
    rle,
    runtime::{OrtRuntime, SessionSettings},
};

pub mod detector;
pub mod heads;
pub mod inference;
pub mod preprocess;
pub mod sam;

use detector::{Detection, DetectorConfig};
use heads::{DiseaseHead, LabelMatrix, PlantProbe};
use inference::{Inference, Inputs, ModelSlot, SessionSpec};
use sam::SamConfig;

/// The name in `ModelError`s from this adapter.
const MODEL: &str = "perception";

/// Batch caps ([contracts 5], batching; [vision 4]): the job-wide `PERCEPTION_MAX_BATCH` and
/// each model's own cap below it.
///
/// [vision 4]: ../../../docs/vision/README.md#4-placement-and-vram-budgets
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchCaps {
    /// `PERCEPTION_MAX_BATCH` (16).
    pub job: usize,
    /// Leaf detector images (8).
    pub detector: usize,
    /// SAM encoder images (2).
    pub sam_encoder: usize,
    /// SAM decoder boxes per call (16).
    pub sam_boxes: usize,
    /// `BioCLIP` images or crops (16).
    pub bioclip: usize,
}

impl Default for BatchCaps {
    fn default() -> Self {
        Self {
            job: 16,
            detector: 8,
            sam_encoder: 2,
            sam_boxes: 16,
            bioclip: 16,
        }
    }
}

/// The perception settings, from the manifest's `[model.config]` tables and the environment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PerceptionConfig {
    /// Batch caps.
    pub caps: BatchCaps,
    /// Detector thresholds.
    pub detector: DetectorConfig,
    /// SAM thresholds.
    pub sam: SamConfig,
    /// `τ_leaf` of the plant check.
    pub tau_leaf: f32,
    /// `τ_plant` of the plant check.
    pub tau_plant: f32,
    /// `T_species`.
    pub species_temperature: f32,
    /// Leaf boxes grow by this share on each side before the disease crop.
    pub crop_grow: f64,
}

impl Default for PerceptionConfig {
    fn default() -> Self {
        Self {
            caps: BatchCaps::default(),
            detector: DetectorConfig::default(),
            sam: SamConfig::default(),
            tau_leaf: 0.5,
            tau_plant: 0.5,
            species_temperature: 1.0,
            crop_grow: 0.10,
        }
    }
}

/// One decoded image of the job.
#[derive(Debug, Clone)]
struct Shot {
    image_id: ImageId,
    kind: ImageKind,
    pixels: RgbImage,
}

/// The input recipes of the GPU models.
#[derive(Debug, Clone)]
struct Recipes {
    detector: ModelInput,
    sam: ModelInput,
    bioclip: ModelInput,
}

/// The perception adapter: the inference thread, the recipes and the CPU heads.
#[derive(Debug)]
pub struct PerceptionPipeline {
    inference: Inference,
    recipes: Arc<Recipes>,
    labels: BTreeMap<String, Arc<LabelMatrix>>,
    disease: BTreeMap<String, Arc<DiseaseHead>>,
    probe: Option<PlantProbe>,
    config: PerceptionConfig,
    versions: BTreeMap<String, String>,
    bioclip_version: String,
    cpu: Arc<Semaphore>,
}

impl PerceptionPipeline {
    /// The manifest entries this adapter loads. Label matrices are every entry named
    /// `bioclip_labels*`, disease heads every `disease_head*`.
    pub const GPU_MODELS: [(&'static str, ModelSlot); 4] = [
        ("leaf_detector", ModelSlot::Detector),
        ("sam_encoder", ModelSlot::SamEncoder),
        ("sam_decoder", ModelSlot::SamDecoder),
        ("bioclip", ModelSlot::Bioclip),
    ];

    /// Verifies every file's hash, loads the sessions on the inference thread (with warm-up at
    /// the largest batch), and reads the heads. `cpu_permits` bounds image decoding and
    /// pre-processing on the blocking pool.
    pub async fn load(
        runtime: OrtRuntime,
        manifest: &Manifest,
        config: PerceptionConfig,
        cpu_permits: usize,
    ) -> Result<Self, VisionError> {
        let names: Vec<&str> = Self::GPU_MODELS.iter().map(|(n, _)| *n).collect();
        manifest.verify(&names)?;
        let mut versions = BTreeMap::new();
        versions.insert(
            domain::jobs::ModelVersionKey::ORT_EP.to_owned(),
            runtime.ep().as_str().to_owned(),
        );
        versions.insert(
            domain::jobs::ModelVersionKey::PERCEPTION_CONFIG.to_owned(),
            manifest.perception_config(),
        );
        let mut specs = Vec::new();
        for (name, slot) in Self::GPU_MODELS {
            let entry = manifest.entry(name)?;
            versions.insert(name.to_owned(), entry.version.clone());
            specs.push(SessionSpec {
                slot,
                path: manifest.path(entry),
                settings: SessionSettings {
                    gpu_mem_limit_mb: entry.config_usize("gpu_mem_limit_mb").ok(),
                },
                warm_up: None,
            });
        }
        let input = |name: &str| -> Result<ModelInput, VisionError> {
            manifest
                .entry(name)?
                .input
                .clone()
                .ok_or_else(|| VisionError::Shape {
                    model: "manifest",
                    detail: format!("{name} has no [model.input]"),
                })
        };
        let recipes = Recipes {
            detector: input("leaf_detector")?,
            sam: input("sam_encoder")?,
            bioclip: input("bioclip")?,
        };
        let mut labels = BTreeMap::new();
        for entry in manifest.entries_with_prefix("bioclip_labels") {
            manifest.verify(&[entry.name.as_str()])?;
            let m = LabelMatrix::load(&manifest.path(entry))?;
            labels.insert(m.version.clone(), Arc::new(m));
        }
        let mut disease = BTreeMap::new();
        for entry in manifest.entries_with_prefix("disease_head") {
            manifest.verify(&[entry.name.as_str()])?;
            let h = DiseaseHead::load(&manifest.path(entry))?;
            versions.insert(entry.name.clone(), entry.version.clone());
            disease.insert(h.version.clone(), Arc::new(h));
        }
        let probe = match manifest.entry("plant_probe") {
            Ok(entry) => {
                manifest.verify(&["plant_probe"])?;
                versions.insert("plant_probe".to_owned(), entry.version.clone());
                Some(PlantProbe::load(&manifest.path(entry))?)
            }
            Err(_) => None,
        };
        let join = |keys: Vec<&String>| keys.into_iter().cloned().collect::<Vec<_>>().join(",");
        versions.insert(
            domain::jobs::ModelVersionKey::LABEL_SET_VERSION.to_owned(),
            join(labels.keys().collect()),
        );
        versions.insert(
            domain::jobs::ModelVersionKey::CLASSIFIER_LABELS_VERSION.to_owned(),
            join(disease.keys().collect()),
        );
        let bioclip_version = format!("bioclip-2@{}", manifest.entry("bioclip")?.version);
        let inference = Inference::start(runtime, specs).await?;
        Ok(Self {
            inference,
            recipes: Arc::new(recipes),
            labels,
            disease,
            probe,
            config,
            versions,
            bioclip_version,
            cpu: Arc::new(Semaphore::new(cpu_permits.max(1))),
        })
    }

    /// Decodes the job's images on the blocking pool. The closure must be `'static` and `Send`:
    /// it owns its inputs (a `Vec` of cheap `Bytes` clones) and returns owned pixels; the
    /// semaphore permit moves in with it, so it is held until the work ends even if the job is
    /// dropped meanwhile.
    async fn decode(&self, images: &[LoadedImage]) -> Result<Arc<Vec<Shot>>, VisionError> {
        let owned: Vec<(ImageId, ImageKind, bytes::Bytes)> = images
            .iter()
            .map(|i| (i.image.image_id, i.image.kind, i.bytes.clone()))
            .collect();
        let permit = Arc::clone(&self.cpu)
            .acquire_owned()
            .await
            .map_err(|_| VisionError::InferenceGone)?;
        let shots = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            owned
                .into_iter()
                .map(|(image_id, kind, bytes)| {
                    Ok(Shot {
                        image_id,
                        kind,
                        pixels: imaging::decode_stored(&bytes)?,
                    })
                })
                .collect::<Result<Vec<_>, VisionError>>()
        })
        .await??;
        Ok(Arc::new(shots))
    }

    /// Pre-processes some shots (or crops) for one model on the blocking pool and stacks them
    /// into batches of the model's effective cap.
    async fn tensors(
        &self,
        images: Vec<RgbImage>,
        recipe: ModelInput,
        cap: usize,
    ) -> Result<Vec<ndarray::Array4<f32>>, VisionError> {
        let permit = Arc::clone(&self.cpu)
            .acquire_owned()
            .await
            .map_err(|_| VisionError::InferenceGone)?;
        let cap = preprocess::effective_cap(cap, self.config.caps.job);
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let t: Vec<Array3<f32>> = images
                .iter()
                .map(|i| preprocess::to_tensor(ImageView::from(i), &recipe))
                .collect::<Result<_, _>>()?;
            preprocess::batches(&t, cap)
        })
        .await?
    }

    /// The leaf detector over the given shots, at most `detector` images per run.
    async fn detect(&self, shots: &[&Shot]) -> Result<Vec<Vec<Detection>>, VisionError> {
        let images = shots.iter().map(|s| s.pixels.clone()).collect();
        let batches = self
            .tensors(
                images,
                self.recipes.detector.clone(),
                self.config.caps.detector,
            )
            .await?;
        let mut out = Vec::new();
        let mut i = 0;
        for batch in batches {
            let n = batch.len_of(Axis(0));
            let res = self
                .inference
                .run(
                    ModelSlot::Detector,
                    vec![("input".to_owned(), batch.into_dyn())],
                    &["dets", "labels"],
                )
                .await?;
            let (dets, logits) = (&res[0], &res[1]);
            let classes = logits.shape().get(2).copied().unwrap_or(1);
            for b in 0..n {
                let d = dets.index_axis(Axis(0), b);
                let l = logits.index_axis(Axis(0), b);
                let size = shots[i].pixels.dimensions();
                out.push(detector::decode(
                    &d.iter().copied().collect::<Vec<_>>(),
                    &l.iter().copied().collect::<Vec<_>>(),
                    classes,
                    size,
                    &self.config.detector,
                ));
                i += 1;
            }
        }
        Ok(out)
    }

    /// SAM over one image's boxes: encoder once, decoder in chunks of `sam_boxes`, one prompt per
    /// box. Returns, per box, the mask when its predicted `IoU` reaches `τ_mask`.
    async fn masks(
        &self,
        shot: &Shot,
        boxes: &[BBox],
    ) -> Result<Vec<Option<crate::mask::MaskBuf>>, VisionError> {
        if boxes.is_empty() {
            return Ok(Vec::new());
        }
        let enc_batches = self
            .tensors(vec![shot.pixels.clone()], self.recipes.sam.clone(), 1)
            .await?;
        let Some(batch) = enc_batches.into_iter().next() else {
            return Ok(vec![None; boxes.len()]);
        };
        let feats = self
            .inference
            .run(
                ModelSlot::SamEncoder,
                vec![("images".to_owned(), batch.into_dyn())],
                &["high_res_feats_0", "high_res_feats_1", "image_embed"],
            )
            .await?;
        let mut out = Vec::with_capacity(boxes.len());
        for chunk in boxes.chunks(preprocess::effective_cap(
            self.config.caps.sam_boxes,
            self.config.caps.job,
        )) {
            let inputs: Inputs = vec![
                ("high_res_feats_0".to_owned(), feats[0].clone()),
                ("high_res_feats_1".to_owned(), feats[1].clone()),
                ("image_embed".to_owned(), feats[2].clone()),
                ("boxes".to_owned(), sam::boxes_input(chunk).into_dyn()),
            ];
            let res = self
                .inference
                .run(
                    ModelSlot::SamDecoder,
                    inputs,
                    &["mask_single", "iou_single"],
                )
                .await?;
            let (logits, iou) = (&res[0], &res[1]);
            for (k, _) in chunk.iter().enumerate() {
                let score = iou.get(IxDyn(&[k, 0])).copied().unwrap_or(0.0);
                if score < self.config.sam.min_iou {
                    out.push(None);
                    continue;
                }
                let per_box = logits.index_axis(Axis(0), k);
                let map = per_box
                    .index_axis(Axis(0), 0)
                    .into_dimensionality::<ndarray::Ix2>()
                    .map_err(|e| VisionError::Shape {
                        model: "sam_decoder",
                        detail: e.to_string(),
                    })?;
                out.push(Some(sam::upsample(&map, shot.pixels.dimensions())));
            }
        }
        Ok(out)
    }

    /// `BioCLIP` 2 over images or crops: L2-normalised 768-value embeddings.
    async fn embed(&self, images: Vec<RgbImage>) -> Result<Vec<Vec<f32>>, VisionError> {
        let batches = self
            .tensors(
                images,
                self.recipes.bioclip.clone(),
                self.config.caps.bioclip,
            )
            .await?;
        let mut out = Vec::new();
        for batch in batches {
            let res = self
                .inference
                .run(
                    ModelSlot::Bioclip,
                    vec![("image".to_owned(), batch.into_dyn())],
                    &["embedding"],
                )
                .await?;
            out.extend(rows(&res[0]));
        }
        Ok(out)
    }

    /// Segmentation of every plant shot: leaf boxes, their masks, and a `plant` region from the
    /// leaves on a whole-plant shot. Region ids are `<image_id>.<n>`, numbered from 1 per image.
    async fn segment(
        &self,
        shots: &[Shot],
    ) -> Result<(Vec<PerceptionRegion>, BTreeMap<ImageId, f32>), VisionError> {
        let plant_shots: Vec<&Shot> = shots
            .iter()
            .filter(|s| heads::shot_weight(s.kind) > 0.0)
            .collect();
        let detections = self.detect(&plant_shots).await?;
        let mut regions = Vec::new();
        let mut top_scores = BTreeMap::new();
        for (shot, found) in plant_shots.iter().zip(detections) {
            top_scores.insert(shot.image_id, detector::top_score(&found));
            let mut boxes: Vec<BBox> = found.iter().map(|d| d.bbox).collect();
            let plant_box = (shot.kind == ImageKind::WholePlant
                && boxes.len() >= self.config.sam.plant_min_leaves)
                .then(|| sam::enclosing(&boxes, self.config.sam.plant_grow))
                .flatten();
            if let Some(b) = plant_box {
                boxes.push(b);
            }
            let masks = self.masks(shot, &boxes).await?;
            let mut n = 0u32;
            for (i, d) in found.iter().enumerate() {
                n += 1;
                regions.push(PerceptionRegion {
                    region_id: RegionId {
                        image: shot.image_id,
                        n,
                    },
                    image_id: shot.image_id,
                    class: RegionClass::Leaf,
                    bbox: d.bbox,
                    mask: masks
                        .get(i)
                        .cloned()
                        .flatten()
                        .map(|m| rle::encode(m.view())),
                    confidence: Confidence::saturating(f64::from(d.score)),
                });
            }
            if let Some(b) = plant_box {
                let leaf_area: f64 = found.iter().map(|d| d.bbox.area()).sum();
                let mask = masks.last().cloned().flatten().filter(|m| {
                    let covered = m.count() as f64 / (f64::from(m.width()) * f64::from(m.height()));
                    covered >= leaf_area * self.config.sam.plant_min_cover
                });
                n += 1;
                regions.push(PerceptionRegion {
                    region_id: RegionId {
                        image: shot.image_id,
                        n,
                    },
                    image_id: shot.image_id,
                    class: RegionClass::Plant,
                    bbox: b,
                    mask: mask.map(|m| rle::encode(m.view())),
                    confidence: Confidence::saturating(f64::from(detector::top_score(&found))),
                });
            }
        }
        Ok((regions, top_scores))
    }

    fn label_set(&self, version: &str) -> Result<Arc<LabelMatrix>, VisionError> {
        self.labels
            .get(version)
            .cloned()
            .ok_or_else(|| VisionError::UnknownLabelVersion {
                what: "label_set_version",
                version: version.to_owned(),
            })
    }

    fn disease_head(&self, version: &str) -> Result<Arc<DiseaseHead>, VisionError> {
        self.disease
            .get(version)
            .cloned()
            .ok_or_else(|| VisionError::UnknownLabelVersion {
                what: "classifier_labels_version",
                version: version.to_owned(),
            })
    }

    /// The whole job, with `VisionError`s; the trait method maps them.
    #[allow(
        clippy::too_many_lines,
        reason = "one stage after another, in the contract's order; splitting it would hide the order"
    )]
    async fn run(
        &self,
        payload: &PerceptionPayload,
        images: &[LoadedImage],
        progress: &dyn ProgressSink,
    ) -> Result<PerceptionResult, VisionError> {
        let stages: std::collections::HashSet<PerceptionStage> =
            payload.stages.iter().copied().collect();
        let wants = |s| stages.contains(&s);
        let identify = wants(PerceptionStage::Species)
            && payload.species_id.is_none()
            && !payload.skip_species_identification;
        // Fail fast, before any GPU time, on versions this worker hasn't loaded.
        let labels = identify
            .then(|| self.label_set(&payload.label_set_version))
            .transpose()?;
        let head = wants(PerceptionStage::Disease)
            .then(|| self.disease_head(&payload.classifier_labels_version))
            .transpose()?;
        let shots = self.decode(images).await?;
        let mut result = PerceptionResult {
            plant_present: None,
            regions: Vec::new(),
            species: None,
            disease: None,
            disease_skipped: None,
            image_embeddings: None,
        };

        // Segmentation (also gives the plant check's leaf scores).
        let mut leaf_scores = BTreeMap::new();
        if wants(PerceptionStage::Segmentation) {
            let clock = StageClock::start(progress, Stage::Segmentation).await;
            let (regions, scores) = self.segment(&shots).await?;
            leaf_scores = scores;
            result.regions = regions;
            clock
                .finish(Some(PartialResult::Segmentation {
                    regions: result.regions.clone(),
                }))
                .await;
        } else if let Some(prior) = &payload.regions {
            result.regions = prior
                .iter()
                .map(|r| PerceptionRegion {
                    region_id: r.region_id,
                    image_id: r.image_id,
                    class: r.class,
                    bbox: r.bbox,
                    mask: None,
                    confidence: Confidence::saturating(1.0),
                })
                .collect();
        }

        // BioCLIP pass 1: every image, for the plant probe, species and embeddings.
        let needs_pass1 = identify
            || wants(PerceptionStage::ImageEmbedding)
            || (wants(PerceptionStage::PlantCheck) && self.probe.is_some());
        let pass1 = if needs_pass1 {
            self.embed(shots.iter().map(|s| s.pixels.clone()).collect())
                .await?
        } else {
            Vec::new()
        };

        if wants(PerceptionStage::PlantCheck) {
            let clock = StageClock::start(progress, Stage::PlantCheck).await;
            let per_shot: Vec<(ImageKind, f32, f32)> = shots
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let leaf = leaf_scores.get(&s.image_id).copied().unwrap_or(0.0);
                    let plant = match (&self.probe, pass1.get(i)) {
                        (Some(p), Some(e)) => p.probability(e),
                        _ => 0.0,
                    };
                    (s.kind, leaf, plant)
                })
                .collect();
            let present =
                heads::plant_present(&per_shot, self.config.tau_leaf, self.config.tau_plant);
            result.plant_present = Some(present);
            clock
                .finish(Some(PartialResult::PlantCheck {
                    plant_present: present,
                }))
                .await;
        }

        let mut working: Option<SpeciesId> = payload.species_id.clone();
        if let Some(labels) = &labels {
            let clock = StageClock::start(progress, Stage::Species).await;
            let per_shot: Vec<(ImageKind, Vec<f32>)> = shots
                .iter()
                .zip(&pass1)
                .map(|(s, e)| {
                    (
                        s.kind,
                        labels.probabilities(e, self.config.species_temperature),
                    )
                })
                .collect();
            let species = SpeciesResult {
                candidates: heads::combine_species(labels, &per_shot, 3),
            };
            working = working.or_else(|| {
                species
                    .candidates
                    .first()
                    .and_then(|c| c.species_id.clone())
            });
            result.species = Some(species.clone());
            clock.finish(Some(PartialResult::Species { species })).await;
        }

        if let Some(head) = &head {
            let clock = StageClock::start(progress, Stage::Disease).await;
            match self
                .classify(&shots, &result.regions, head, working.as_ref())
                .await?
            {
                Ok(disease) => {
                    result.disease = Some(disease.clone());
                    clock.finish(Some(PartialResult::Disease { disease })).await;
                }
                Err(reason) => {
                    result.disease_skipped = Some(DiseaseSkipped { reason });
                    let why = match reason {
                        DiseaseSkipReason::SpeciesNotSupported => "species_not_supported",
                        DiseaseSkipReason::NoLeavesFound => "no_leaves_found",
                    };
                    clock.skip(why).await;
                }
            }
        }

        if wants(PerceptionStage::ImageEmbedding) {
            let clock = StageClock::start(progress, Stage::ImageEmbedding).await;
            result.image_embeddings = Some(
                shots
                    .iter()
                    .zip(pass1)
                    .map(|(s, vector)| ImageEmbedding {
                        image_id: s.image_id,
                        model: self.bioclip_version.clone(),
                        vector,
                    })
                    .collect(),
            );
            clock.finish(None).await;
        }
        Ok(result)
    }

    /// The disease stage: `BioCLIP` pass 2 over the leaf crops of close-ups and undersides (or
    /// whole close-ups without leaf regions), then the head restricted to the working species.
    async fn classify(
        &self,
        shots: &[Shot],
        regions: &[PerceptionRegion],
        head: &DiseaseHead,
        working: Option<&SpeciesId>,
    ) -> Result<Result<Vec<DiseaseResult>, DiseaseSkipReason>, VisionError> {
        let Some(species) = working.filter(|s| head.supports(s)) else {
            return Ok(Err(DiseaseSkipReason::SpeciesNotSupported));
        };
        let leafy = |k: ImageKind| matches!(k, ImageKind::LeafCloseup | ImageKind::LeafUnderside);
        let mut crops: Vec<(ImageId, Option<RegionId>, RgbImage)> = Vec::new();
        for shot in shots.iter().filter(|s| leafy(s.kind)) {
            let leaves: Vec<&PerceptionRegion> = regions
                .iter()
                .filter(|r| r.image_id == shot.image_id && r.class == RegionClass::Leaf)
                .collect();
            if leaves.is_empty() {
                if shot.kind == ImageKind::LeafCloseup {
                    crops.push((shot.image_id, None, shot.pixels.clone()));
                }
                continue;
            }
            for leaf in leaves {
                let [bx, by, bw, bh] = leaf.bbox.to_array();
                let g = self.config.crop_grow;
                let grown = [
                    bx - bw * g,
                    by - bh * g,
                    bw * (1.0 + 2.0 * g),
                    bh * (1.0 + 2.0 * g),
                ];
                let rect = PixelRect::from_normalised(grown, shot.pixels.dimensions());
                crops.push((
                    shot.image_id,
                    Some(leaf.region_id),
                    imaging::crop(ImageView::from(&shot.pixels), rect),
                ));
            }
        }
        if crops.is_empty() {
            return Ok(Err(DiseaseSkipReason::NoLeavesFound));
        }
        let features = self
            .embed(crops.iter().map(|c| c.2.clone()).collect())
            .await?;
        Ok(Ok(crops
            .into_iter()
            .zip(features)
            .map(|((image_id, region_id, _), f)| DiseaseResult {
                image_id,
                region_id,
                top: head.top(&f, species, 3),
            })
            .collect()))
    }
}

/// Splits a `[n, d]` output into rows.
fn rows(a: &ArrayD<f32>) -> Vec<Vec<f32>> {
    a.outer_iter()
        .map(|r| r.iter().copied().collect())
        .collect()
}

/// Sends `stage_started` on creation and `stage_completed` (with the partial result first) on
/// finish. A failed send is logged and ignored: `complete` repeats every stage, and a
/// `lease_lost` is handled by the worker's sink, which cancels the job.
struct StageClock<'p> {
    progress: &'p dyn ProgressSink,
    stage: Stage,
    started: Instant,
}

impl<'p> StageClock<'p> {
    async fn start(progress: &'p dyn ProgressSink, stage: Stage) -> Self {
        send(progress, vec![WorkerEvent::StageStarted { stage }]).await;
        Self {
            progress,
            stage,
            started: Instant::now(),
        }
    }

    fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    async fn finish(self, partial: Option<PartialResult>) {
        let mut events = Vec::new();
        if let Some(result) = partial {
            events.push(WorkerEvent::PartialResult {
                stage: self.stage,
                result,
            });
        }
        events.push(WorkerEvent::StageCompleted {
            stage: self.stage,
            duration_ms: self.elapsed_ms(),
            skipped: None,
        });
        send(self.progress, events).await;
    }

    async fn skip(self, reason: &str) {
        let event = WorkerEvent::StageCompleted {
            stage: self.stage,
            duration_ms: self.elapsed_ms(),
            skipped: Some(Skipped {
                reason: reason.to_owned(),
            }),
        };
        send(self.progress, vec![event]).await;
    }
}

async fn send(progress: &dyn ProgressSink, events: Vec<WorkerEvent>) {
    if let Err(e) = progress.send(events).await {
        tracing::debug!(error = %e, "progress not delivered");
    }
}

#[async_trait]
impl VisionModel for PerceptionPipeline {
    fn model_versions(&self) -> BTreeMap<String, String> {
        self.versions.clone()
    }

    async fn perceive(
        &self,
        job: JobContext,
        payload: &PerceptionPayload,
        images: &[LoadedImage],
        progress: Arc<dyn ProgressSink>,
    ) -> Result<PerceptionResult, ModelError> {
        tracing::debug!(job_id = %job.job_id, images = images.len(), "perception");
        self.run(payload, images, progress.as_ref())
            .await
            .map_err(|e| e.into_model_error(MODEL))
    }

    async fn analyse(
        &self,
        _job: JobContext,
        _payload: &VlmPayload,
        _images: &[LoadedImage],
        _progress: Arc<dyn ProgressSink>,
    ) -> Result<VlmResult, ModelError> {
        Err(ModelError::Unsupported {
            model: MODEL,
            what: "vlm jobs",
        })
    }

    async fn verify(
        &self,
        _job: JobContext,
        _payload: &VerifyPayload,
        _images: &[LoadedImage],
        _progress: Arc<dyn ProgressSink>,
    ) -> Result<VerifyResult, ModelError> {
        Err(ModelError::Unsupported {
            model: MODEL,
            what: "verify jobs",
        })
    }

    async fn read_guided_check(
        &self,
        _job: JobContext,
        _payload: &GuidedPayload,
        _image: &LoadedImage,
    ) -> Result<GuidedResult, ModelError> {
        Err(ModelError::Unsupported {
            model: MODEL,
            what: "guided jobs",
        })
    }
}
