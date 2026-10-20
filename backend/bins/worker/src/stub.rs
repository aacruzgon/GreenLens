//! Stub vision ([worker 15.1]): a `VisionModel` that answers `perception` jobs with canned,
//! valid results, so a scan can run from the app to a worker and back before any GPU or model
//! file exists. It reads only the payload; it opens no network, file or clock (the test
//! `stub_module_has_no_io` scans this file), and everything it makes carries the `stub` mark.
//!
//! [worker 15.1]: ../../../docs/backend/worker.md#151-stub-vision

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use domain::{
    events::{Skipped, Stage},
    id::{ProblemId, ProblemOrHealthy, RegionId, SpeciesId},
    jobs::{
        DiseaseResult, DiseaseSkipReason, DiseaseSkipped, GuidedPayload, GuidedResult,
        ModelVersionKey, PartialResult, PerceptionPayload, PerceptionRegion, PerceptionResult,
        PerceptionStage, ProblemScore, RegionClass, SpeciesLabel, SpeciesResult, VerifyPayload,
        VerifyResult, VlmPayload, VlmResult, WorkerEvent,
    },
    ports::{BoxError, JobContext, LoadedImage, ModelError, ProgressSink, VisionModel},
    scan::ImageKind,
    wire::{BBox, Confidence},
};

use crate::config::{StubArgs, StubRefused};

/// The model name in `ModelError`s.
const MODEL: &str = "stub";

/// The value of every `stub` mark ([worker 15.1.3]).
///
/// [worker 15.1.3]: ../../../docs/backend/worker.md#1513-how-a-stub-result-is-recognised
const MARK: &str = "stub";

/// The `model_versions` key that says the result is canned.
const STUB_KEY: &str = "stub";

/// Candidate scores, best first: the top one is a medium confidence.
const SPECIES_SCORES: [f64; 3] = [0.62, 0.21, 0.09];

/// `healthy`, then up to two problems.
const DISEASE_SCORES: [f64; 3] = [0.70, 0.20, 0.10];

/// What the stub was told to answer.
#[derive(Debug, Clone)]
pub struct StubConfig {
    label_set_version: String,
    classifier_labels_version: String,
    species: Vec<SpeciesId>,
    problems: Vec<ProblemId>,
    plant_present: bool,
    delay: Duration,
}

impl StubConfig {
    /// Reads the `STUB_*` settings. Slugs are checked here, so a typo stops the start-up
    /// instead of failing every job.
    pub fn new(args: &StubArgs) -> Result<Self, StubRefused> {
        fn slugs<T: std::str::FromStr>(
            variable: &'static str,
            items: &[String],
        ) -> Result<Vec<T>, StubRefused> {
            items
                .iter()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| {
                    s.parse().map_err(|_| StubRefused::BadSlug {
                        variable,
                        value: s.to_owned(),
                    })
                })
                .collect()
        }
        Ok(Self {
            label_set_version: args
                .label_set_version
                .clone()
                .ok_or(StubRefused::MissingVersion("STUB_LABEL_SET_VERSION"))?,
            classifier_labels_version: args.classifier_labels_version.clone().ok_or(
                StubRefused::MissingVersion("STUB_CLASSIFIER_LABELS_VERSION"),
            )?,
            species: slugs("STUB_SPECIES", &args.species)?,
            problems: slugs("STUB_PROBLEMS", &args.problems)?,
            plant_present: args.plant_present,
            delay: Duration::from_millis(args.stage_delay_ms),
        })
    }
}

/// The canned `perception` model.
#[derive(Debug)]
pub struct StubModel {
    config: StubConfig,
}

impl StubModel {
    /// A stub that answers as `config` says.
    pub const fn new(config: StubConfig) -> Self {
        Self { config }
    }

    /// One stage on the wire: `stage_started`, the partial result, a pause, then
    /// `stage_completed` (skipped for `skipped`). The pause is the only wait, and a cancelled
    /// job drops it like any other `.await`.
    async fn stage(
        &self,
        progress: &dyn ProgressSink,
        stage: Stage,
        partial: Option<PartialResult>,
        skipped: Option<&str>,
    ) {
        send(progress, WorkerEvent::StageStarted { stage }).await;
        if let Some(result) = partial {
            send(progress, WorkerEvent::PartialResult { stage, result }).await;
        }
        tokio::time::sleep(self.config.delay).await;
        send(
            progress,
            WorkerEvent::StageCompleted {
                stage,
                duration_ms: u64::try_from(self.config.delay.as_millis()).unwrap_or(u64::MAX),
                skipped: skipped.map(|reason| Skipped {
                    reason: reason.to_owned(),
                }),
            },
        )
        .await;
    }

    /// One `plant`, `leaf` or `soil` region per photo, none for flower and fruit, ids
    /// `<image>.1`, no mask.
    fn regions(payload: &PerceptionPayload) -> Result<Vec<PerceptionRegion>, ModelError> {
        let mut regions = Vec::new();
        for image in &payload.images {
            let (class, [x, y, w, h]) = match image.kind {
                ImageKind::WholePlant => (RegionClass::Plant, [0.10, 0.10, 0.80, 0.80]),
                ImageKind::LeafCloseup | ImageKind::LeafUnderside => {
                    (RegionClass::Leaf, [0.15, 0.15, 0.70, 0.70])
                }
                ImageKind::SoilSurface => (RegionClass::Soil, [0.00, 0.50, 1.00, 0.50]),
                _ => continue,
            };
            regions.push(PerceptionRegion {
                region_id: RegionId {
                    image: image.image_id,
                    n: 1,
                },
                image_id: image.image_id,
                class,
                bbox: BBox::new(x, y, w, h).map_err(invalid)?,
                mask: None,
                confidence: Confidence::new(0.90).map_err(invalid)?,
            });
        }
        Ok(regions)
    }

    /// Up to three candidates from `STUB_SPECIES`, best first.
    fn species(&self) -> Result<SpeciesResult, ModelError> {
        let candidates = self
            .config
            .species
            .iter()
            .zip(SPECIES_SCORES)
            .map(|(id, score)| {
                Ok(SpeciesLabel {
                    species_id: Some(id.clone()),
                    label: format!("{id} (stub)"),
                    score: Confidence::new(score).map_err(invalid)?,
                })
            })
            .collect::<Result<_, ModelError>>()?;
        Ok(SpeciesResult { candidates })
    }

    /// One entry per leaf region on a close-up or underside, or one per such photo when there
    /// is no leaf region; `None` when there is neither.
    fn disease(
        &self,
        payload: &PerceptionPayload,
        regions: &[RegionRef],
    ) -> Result<Option<Vec<DiseaseResult>>, ModelError> {
        let leaf_photo = |image: &domain::id::ImageId| {
            payload.images.iter().any(|i| {
                i.image_id == *image
                    && matches!(i.kind, ImageKind::LeafCloseup | ImageKind::LeafUnderside)
            })
        };
        let top = self.top()?;
        let on_leaves: Vec<DiseaseResult> = regions
            .iter()
            .filter(|r| r.class == RegionClass::Leaf && leaf_photo(&r.image_id))
            .map(|r| DiseaseResult {
                image_id: r.image_id,
                region_id: Some(r.region_id),
                top: top.clone(),
            })
            .collect();
        if !on_leaves.is_empty() {
            return Ok(Some(on_leaves));
        }
        let whole: Vec<DiseaseResult> = payload
            .images
            .iter()
            .filter(|i| leaf_photo(&i.image_id))
            .map(|i| DiseaseResult {
                image_id: i.image_id,
                region_id: None,
                top: top.clone(),
            })
            .collect();
        Ok((!whole.is_empty()).then_some(whole))
    }

    /// `healthy` at 0.70, then the first two problems: sorted, at most three.
    fn top(&self) -> Result<Vec<ProblemScore>, ModelError> {
        std::iter::once(ProblemOrHealthy::Healthy)
            .chain(
                self.config
                    .problems
                    .iter()
                    .cloned()
                    .map(ProblemOrHealthy::Problem),
            )
            .zip(DISEASE_SCORES)
            .map(|(problem_id, score)| {
                Ok(ProblemScore {
                    problem_id,
                    confidence: Confidence::new(score).map_err(invalid)?,
                })
            })
            .collect()
    }
}

/// The part of a region the disease stage needs, from this job's or the payload's.
struct RegionRef {
    region_id: RegionId,
    image_id: domain::id::ImageId,
    class: RegionClass,
}

#[async_trait]
impl VisionModel for StubModel {
    fn model_versions(&self) -> BTreeMap<String, String> {
        let mark = |key: &str| (key.to_owned(), MARK.to_owned());
        BTreeMap::from([
            (STUB_KEY.to_owned(), "1".to_owned()),
            mark(ModelVersionKey::PERCEPTION_CONFIG),
            mark("leaf_detector"),
            mark("bioclip"),
            mark("disease_head"),
            (
                ModelVersionKey::LABEL_SET_VERSION.to_owned(),
                self.config.label_set_version.clone(),
            ),
            (
                ModelVersionKey::CLASSIFIER_LABELS_VERSION.to_owned(),
                self.config.classifier_labels_version.clone(),
            ),
        ])
    }

    async fn perceive(
        &self,
        _job: JobContext,
        payload: &PerceptionPayload,
        _images: &[LoadedImage],
        progress: Arc<dyn ProgressSink>,
    ) -> Result<PerceptionResult, ModelError> {
        let progress = progress.as_ref();
        let wants = |s: PerceptionStage| payload.stages.contains(&s);
        let mut result = PerceptionResult {
            plant_present: None,
            regions: Vec::new(),
            species: None,
            disease: None,
            disease_skipped: None,
            image_embeddings: None,
        };
        // The regions the disease stage looks at: this job's, or the earlier run's.
        let mut known: Vec<RegionRef> = payload
            .regions
            .iter()
            .flatten()
            .map(|r| RegionRef {
                region_id: r.region_id,
                image_id: r.image_id,
                class: r.class,
            })
            .collect();

        for stage in PerceptionStage::ALL.into_iter().filter(|s| wants(*s)) {
            match stage {
                PerceptionStage::PlantCheck => {
                    let plant_present = self.config.plant_present;
                    result.plant_present = Some(plant_present);
                    let partial = PartialResult::PlantCheck { plant_present };
                    self.stage(progress, Stage::PlantCheck, Some(partial), None)
                        .await;
                }
                PerceptionStage::Segmentation => {
                    let regions = Self::regions(payload)?;
                    known = regions
                        .iter()
                        .map(|r| RegionRef {
                            region_id: r.region_id,
                            image_id: r.image_id,
                            class: r.class,
                        })
                        .collect();
                    result.regions.clone_from(&regions);
                    let partial = PartialResult::Segmentation { regions };
                    self.stage(progress, Stage::Segmentation, Some(partial), None)
                        .await;
                }
                PerceptionStage::Species => {
                    if payload.species_id.is_some() || payload.skip_species_identification {
                        continue;
                    }
                    let species = self.species()?;
                    result.species = Some(species.clone());
                    let partial = PartialResult::Species { species };
                    self.stage(progress, Stage::Species, Some(partial), None)
                        .await;
                }
                PerceptionStage::Disease => {
                    if let Some(disease) = self.disease(payload, &known)? {
                        result.disease = Some(disease.clone());
                        let partial = PartialResult::Disease { disease };
                        self.stage(progress, Stage::Disease, Some(partial), None)
                            .await;
                    } else {
                        result.disease_skipped = Some(DiseaseSkipped {
                            reason: DiseaseSkipReason::NoLeavesFound,
                        });
                        self.stage(progress, Stage::Disease, None, Some("no_leaves_found"))
                            .await;
                    }
                }
                // No vector: a made-up one stored under a model name would corrupt the
                // similar-case search the day a real one sits beside it.
                PerceptionStage::ImageEmbedding => {
                    self.stage(progress, Stage::ImageEmbedding, None, Some(MARK))
                        .await;
                }
            }
        }
        Ok(result)
    }

    async fn analyse(
        &self,
        _: JobContext,
        _: &VlmPayload,
        _: &[LoadedImage],
        _: Arc<dyn ProgressSink>,
    ) -> Result<VlmResult, ModelError> {
        Err(unsupported("vlm jobs"))
    }

    async fn verify(
        &self,
        _: JobContext,
        _: &VerifyPayload,
        _: &[LoadedImage],
        _: Arc<dyn ProgressSink>,
    ) -> Result<VerifyResult, ModelError> {
        Err(unsupported("verify jobs"))
    }

    async fn read_guided_check(
        &self,
        _: JobContext,
        _: &GuidedPayload,
        _: &LoadedImage,
    ) -> Result<GuidedResult, ModelError> {
        Err(unsupported("guided jobs"))
    }
}

const fn unsupported(what: &'static str) -> ModelError {
    ModelError::Unsupported { model: MODEL, what }
}

/// A canned value the domain types refuse: a bug in this file, reported as a failed job.
fn invalid(source: impl std::error::Error + Send + Sync + 'static) -> ModelError {
    ModelError::InvalidOutput {
        model: MODEL,
        source: BoxError::from(source),
    }
}

/// A failed send is logged and ignored, as in the real model: `complete` repeats every stage,
/// and a `lease_lost` is handled by the worker's sink, which cancels the job.
async fn send(progress: &dyn ProgressSink, event: WorkerEvent) {
    if let Err(e) = progress.send(vec![event]).await {
        tracing::debug!(error = %e, "progress not delivered");
    }
}

#[cfg(test)]
mod tests;
