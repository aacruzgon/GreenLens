//! The `retrieve` step ([contracts 3]: measurement, context, retrieval and queueing the `vlm`
//! job; [architecture 6.3], [architecture 6.7]).

use std::{collections::BTreeMap, sync::Arc, time::Instant};

use chrono::DurationRound;
use domain::{
    events::{ScanEventBody, Skipped, Stage},
    id::{ImageId, ProblemId},
    jobs::{JobKind, JobSpec, RegionClass, WorkingSpecies},
    knowledge::ProblemSeed,
    ports::{
        CellRequest, GardenContext, IdGenExt, NewJob, ObjectKey, ObservationWrite, ScanSnapshot,
        StepClaim, StepOutput,
    },
    records::{
        measurement::MeasureQuantity,
        observation::{Observation, ObservationDetail, ObservationSource, ObservationStage},
        weather::WeatherSummary,
    },
    scan::{ScanFailureCode, ScanImage},
    url::Stored,
    wire::{Rated, Region, Timestamp},
};
use knowledge::{
    candidates::{ProfilePool, RankInput, rank_candidates},
    scoring::AnswerSet,
};

use super::{completed, run_of, started};
use crate::{
    Pipeline, PipelineError, findings,
    measure::{MeasureRegion, MeasureRequest, MeasuredQuantity, answer_indicator},
    payloads,
};

/// What measuring a set of photos produced.
pub(super) struct Measured {
    /// One observation per image and quantity.
    pub observations: Vec<Observation<Stored>>,
    /// Why the stage was skipped: `nothing_measurable` or `error`.
    pub skipped: Option<&'static str>,
}

fn quantity_label(q: MeasureQuantity) -> &'static str {
    match q {
        MeasureQuantity::PlantHeightCm => "Plant height",
        MeasureQuantity::CanopyWidthCm => "Canopy width",
        MeasureQuantity::LeafLengthCm => "Leaf length",
        MeasureQuantity::LeafAreaCm2 => "Leaf area",
        MeasureQuantity::LesionDiameterMm => "Spot diameter",
        MeasureQuantity::LesionCount => "Spot count",
        MeasureQuantity::AffectedAreaPct => "Affected area",
        MeasureQuantity::HoleDiameterMm => "Hole diameter",
        MeasureQuantity::HoleCount => "Hole count",
    }
}

impl Pipeline {
    /// The measurement stage over some photos with depth ([architecture 6.7]): each photo's
    /// bytes, depth and regions are read, then measured on the blocking pool behind the CPU
    /// semaphore. A failure never fails the scan: the stage is skipped with `error`.
    pub(super) async fn measure_images(
        &self,
        snap: &ScanSnapshot,
        images: &[&ScanImage<Stored>],
    ) -> Measured {
        let mut observations = Vec::new();
        let mut failed = false;
        for image in images {
            match self.measure_one(snap, image).await {
                Ok(mut obs) => observations.append(&mut obs),
                Err(e) => {
                    tracing::warn!(error = %e, image = %image.id, "measurement failed");
                    failed = true;
                }
            }
        }
        let skipped = if !observations.is_empty() {
            None
        } else if failed {
            Some(Skipped::ERROR)
        } else {
            Some(Skipped::NOTHING_MEASURABLE)
        };
        Measured {
            observations,
            skipped,
        }
    }

    async fn measure_one(
        &self,
        snap: &ScanSnapshot,
        image: &ScanImage<Stored>,
    ) -> Result<Vec<Observation<Stored>>, PipelineError> {
        let Some((_, depth)) = snap.depth.iter().find(|(id, _)| *id == image.id) else {
            return Ok(Vec::new());
        };
        let blobs = &self.deps.blobs;
        let photo = blobs
            .get_bytes(&ObjectKey::scan_image(
                snap.scan.garden_id,
                snap.scan.id,
                image.id,
            ))
            .await?;
        let depth_png = blobs.get_bytes(&depth.depth_key).await?;
        let confidence_png = match &depth.confidence_key {
            Some(k) => Some(blobs.get_bytes(k).await?),
            None => None,
        };
        let regions = regions_of(snap, image.id);
        let request = MeasureRequest {
            image_id: image.id,
            kind: image.kind,
            photo,
            depth_png,
            confidence_png,
            meta: depth.meta.clone(),
            regions,
        };
        let measurer = Arc::clone(&self.deps.measurer);
        let version = format!("{}+{}", measurer.version(), depth.meta.device_model);
        // The permit bounds CPU work to one measurement per core; it moves into the blocking
        // task and is released when the measurement ends.
        let permit = Arc::clone(&self.deps.config.cpu_permits)
            .acquire_owned()
            .await
            .map_err(|_| PipelineError::Inconsistent("cpu semaphore closed"))?;
        let quantities = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            measurer.measure(&request)
        })
        .await?;
        let quantities = match quantities {
            Ok(q) => q,
            Err(e) => {
                tracing::warn!(error = %e, "measurement refused the photo");
                return Err(PipelineError::Inconsistent("measurement failed"));
            }
        };
        Ok(quantities
            .into_iter()
            .map(|q| self.measurement_observation(snap, image.id, &q, &version))
            .collect())
    }

    fn measurement_observation(
        &self,
        snap: &ScanSnapshot,
        image: ImageId,
        q: &MeasuredQuantity,
        version: &str,
    ) -> Observation<Stored> {
        Observation {
            id: self.deps.ids.id(),
            scan_id: snap.scan.id,
            run: snap.scan.run,
            stage: ObservationStage::Measurement,
            pipeline_stage: Stage::Measurement,
            label: quantity_label(q.measurement.quantity).to_owned(),
            knowledge_ref: None,
            role: None,
            detail: Some(ObservationDetail {
                measurement: Some(q.measurement),
                ..ObservationDetail::default()
            }),
            confidence: Rated(q.confidence),
            region: Some(Region::unmasked(image, q.bbox, None)),
            source: ObservationSource::Measurement,
            model_version: version.to_owned(),
            created_at: self.now(),
        }
    }

    /// The garden's weather: the cache cell for this hour, fetched from Open-Meteo on a miss
    /// and cached ([contracts 7], fetching and caching).
    pub(super) async fn weather_for(
        &self,
        garden: &GardenContext,
    ) -> Result<WeatherSummary, PipelineError> {
        let now = self.now();
        let hour = now
            .into_inner()
            .duration_trunc(chrono::TimeDelta::hours(1))
            .map_or(now, Timestamp::from);
        if let Some(cached) = self.deps.store.cached(garden.weather_cell, hour).await? {
            return Ok(cached.for_garden(garden.precision));
        }
        let cell = self
            .deps
            .weather
            .fetch_cell(&CellRequest {
                cell: garden.weather_cell,
                time_zone: garden.time_zone.clone(),
            })
            .await?;
        self.deps
            .store
            .put(garden.weather_cell, hour, cell.clone())
            .await?;
        Ok(cell.for_garden(garden.precision))
    }

    /// The profiles the ranking may select: the classifier hits', the working species'
    /// problems and their look-alikes'. Returns them with the species' problem ids and the
    /// look-alike map.
    pub(super) async fn retrieval_inputs(
        &self,
        snap: &ScanSnapshot,
        classifier: &[ProblemId],
    ) -> Result<
        (
            Vec<ProblemSeed>,
            Vec<ProblemId>,
            BTreeMap<ProblemId, Vec<ProblemId>>,
        ),
        PipelineError,
    > {
        let k = &self.deps.knowledge;
        let mut profiles = k.problems(classifier).await?;
        let species_problems = match &snap.working_species {
            Some(s) => k.problems_of_species(s).await?,
            None => Vec::new(),
        };
        let species_ids: Vec<ProblemId> = species_problems.iter().map(|p| p.id.clone()).collect();
        profiles.extend(species_problems);
        let mut lookalikes: BTreeMap<ProblemId, Vec<ProblemId>> = BTreeMap::new();
        let mut wanted: Vec<ProblemId> = Vec::new();
        for p in &profiles {
            let alikes = k.lookalikes(&p.id).await?;
            let ids: Vec<ProblemId> = alikes.into_iter().map(|l| l.problem_id).collect();
            wanted.extend(ids.iter().cloned());
            lookalikes.insert(p.id.clone(), ids);
        }
        wanted.retain(|id| !profiles.iter().any(|p| &p.id == id));
        wanted.sort();
        wanted.dedup();
        profiles.extend(k.problems(&wanted).await?);
        Ok((profiles, species_ids, lookalikes))
    }

    /// `retrieve`: measurement (when a run photo has depth and the run must measure), the
    /// weather, the candidate set, the measured answers and the `vlm` job, committed together.
    #[expect(
        clippy::too_many_lines,
        reason = "a step reads top to bottom in the order of its contract section"
    )]
    pub(super) async fn retrieve(
        &self,
        claim: StepClaim,
    ) -> Result<Option<StepClaim>, PipelineError> {
        let Some(snap) = self.snapshot_for(claim).await? else {
            return Ok(None);
        };
        let run = run_of(&snap);
        if findings::plant_present(&snap.observations) == Some(false) {
            self.fail_run(
                &snap,
                ScanFailureCode::NoPlantFound,
                "No plant was found in the photos.",
            )
            .await?;
            return Ok(None);
        }

        // Measurement.
        let mut new_obs: Vec<Observation<Stored>> = Vec::new();
        let mut measurement_write = None;
        let to_measure = images_to_measure(&snap);
        if !to_measure.is_empty() {
            let t = Instant::now();
            self.emit(snap.scan.id, run, vec![started(Stage::Measurement)])
                .await?;
            let measured = self.measure_images(&snap, &to_measure).await;
            self.emit(
                snap.scan.id,
                run,
                vec![completed(Stage::Measurement, t, measured.skipped)],
            )
            .await?;
            measurement_write = Some(ObservationWrite {
                stage: Stage::Measurement,
                run,
                images: to_measure.iter().map(|i| i.id).collect(),
                replace_imageless: false,
                observations: measured.observations.clone(),
                masks: Vec::new(),
            });
            new_obs = measured.observations;
        }

        // Context.
        let t = Instant::now();
        self.emit(snap.scan.id, run, vec![started(Stage::Context)])
            .await?;
        let weather = match self.weather_for(&snap.garden).await {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(error = %e, "weather unavailable");
                self.fail_run(
                    &snap,
                    ScanFailureCode::UpstreamUnavailable,
                    "The weather service is unavailable.",
                )
                .await?;
                return Ok(None);
            }
        };
        self.emit(
            snap.scan.id,
            run,
            vec![
                ScanEventBody::WeatherLoaded {
                    weather: weather.clone(),
                },
                completed(Stage::Context, t, None),
            ],
        )
        .await?;

        // Retrieve.
        let t = Instant::now();
        self.emit(snap.scan.id, run, vec![started(Stage::Retrieve)])
            .await?;
        let hits = findings::classifier_hits(&snap.observations);
        let hit_ids: Vec<ProblemId> = hits.iter().map(|h| h.problem.clone()).collect();
        let (profiles, species_ids, lookalikes) = self.retrieval_inputs(&snap, &hit_ids).await?;
        let pool = ProfilePool::new(&profiles);
        let set = rank_candidates(&RankInput {
            pool: &pool,
            classifier: &hits,
            species_problems: &species_ids,
            lookalikes: &lookalikes,
            weather: Some(&weather),
            cap: RankInput::CAP,
        });
        let all_obs: Vec<Observation<Stored>> =
            snap.observations.iter().chain(&new_obs).cloned().collect();
        let measured = findings::measured(&all_obs, &snap.scan.images, &snap.depth);
        let evidence: Vec<_> = set
            .as_slice()
            .iter()
            .flat_map(knowledge::candidates::Candidate::indicators)
            .filter_map(|ind| answer_indicator(ind, &measured))
            .collect();
        let mut stored = snap.evidence.clone();
        stored.extend(evidence.iter().cloned());
        let answers = AnswerSet::new(&stored);
        let prompts = payloads::prompts(set.as_slice(), &answers);
        let options = self.deps.knowledge.vlm_options().await?;
        let working = working_species(&snap);
        let job_id = self.deps.ids.id();
        let job = NewJob {
            id: job_id,
            scan: snap.scan.id,
            run: Some(run),
            question: None,
            spec: JobSpec::Vlm(payloads::vlm_payload(
                job_id,
                &snap.scan.images,
                prompts,
                working,
                &options,
            )),
            max_runtime: JobKind::Vlm.max_runtime(false),
            queued_at: self.now(),
        };
        let selected = set.selected();
        self.emit(snap.scan.id, run, vec![completed(Stage::Retrieve, t, None)])
            .await?;

        let next = self
            .commit(
                claim,
                StepOutput::Retrieve {
                    observations: measurement_write,
                    evidence,
                    candidates: selected.clone(),
                    weather: Some(weather),
                    job: Some(job),
                },
            )
            .await?;
        let mut events: Vec<ScanEventBody<Stored>> = new_obs
            .into_iter()
            .map(|observation| ScanEventBody::Finding { observation })
            .collect();
        events.push(ScanEventBody::CandidatesSelected {
            candidates: selected,
        });
        self.emit(snap.scan.id, run, events).await?;
        Ok(next)
    }
}

/// The run's photos with depth that must be measured ([architecture 6.7], runs): all of
/// them when the run has no measurement yet, or when segmentation re-ran after the last
/// measurement; none when an earlier run's measurements still stand.
pub(crate) fn images_to_measure(snap: &ScanSnapshot) -> Vec<&ScanImage<Stored>> {
    let with_depth: Vec<&ScanImage<Stored>> = snap
        .scan
        .images
        .iter()
        .filter(|i| {
            i.depth.is_some() && i.kind.is_plant_or_soil() && i.answers_question_id.is_none()
        })
        .collect();
    if with_depth.is_empty() {
        return with_depth;
    }
    let last = |stage: ObservationStage| {
        snap.observations
            .iter()
            .filter(|o| {
                o.stage == stage
                    && o.region
                        .as_ref()
                        .is_some_and(|r| with_depth.iter().any(|i| i.id == r.image_id))
            })
            .map(|o| o.run)
            .max()
    };
    match (
        last(ObservationStage::Measurement),
        last(ObservationStage::Segmentation),
    ) {
        (Some(m), Some(s)) if m >= s => Vec::new(),
        (Some(_), None) => Vec::new(),
        _ => with_depth,
    }
}

/// A photo's segmentation regions with their masks, for measurement.
pub(crate) fn regions_of(snap: &ScanSnapshot, image: ImageId) -> Vec<MeasureRegion> {
    snap.observations
        .iter()
        .filter(|o| o.stage == ObservationStage::Segmentation)
        .filter_map(|o| {
            let region = o.region.as_ref().filter(|r| r.image_id == image)?;
            let region_id = region.region_id?;
            let class = o
                .detail
                .as_ref()
                .and_then(|d| d.region_class)
                .unwrap_or(RegionClass::Leaf);
            // By the region's own `mask_id`: region ids repeat across runs (`<image_id>.<n>`).
            let mask = region
                .mask
                .mask_id
                .and_then(|id| snap.masks.iter().find(|m| m.id == id))
                .map(|m| m.rle.clone());
            Some(MeasureRegion {
                region_id,
                class,
                bbox: region.bbox,
                mask,
            })
        })
        .collect()
}

/// The VLM payload's working species: the confirmed or best candidate, or the custom name.
pub(crate) fn working_species(snap: &ScanSnapshot) -> Option<WorkingSpecies> {
    if let Some(id) = &snap.working_species {
        let name = snap
            .plant
            .species
            .as_ref()
            .filter(|s| &s.id == id)
            .map_or_else(|| id.to_string(), |s| s.common_name.clone());
        return Some(WorkingSpecies {
            id: Some(id.clone()),
            name,
        });
    }
    snap.plant
        .custom_species_name
        .as_ref()
        .map(|name| WorkingSpecies {
            id: None,
            name: name.clone(),
        })
}
