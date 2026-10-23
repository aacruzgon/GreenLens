//! Starting runs ([contracts 3], scan lifecycle, submit, species change; [contracts 4],
//! `run_started`): the run's `perception` job, which stages it repeats, and the label-version
//! rule of [contracts 5].

use domain::{
    authz::{Editor, Membership, Viewer},
    events::{ResolvedBy, RunReason, ScanEventBody, Stage},
    id::{PlantId, ScanId, SpeciesId},
    jobs::{Capability, JobKind, JobSpec, PriorRegion, RegionClass},
    ports::{IdGenExt, LiveWorker, NewJob, RunStart, ScanSnapshot},
    records::observation::ObservationStage,
    rules::{
        scan_state::{SpeciesFollow, SubmitDecision, decide_submit, species_follow},
        stages::{EarlierRun, perception_stages, run_stages},
    },
    scan::{Plant, Scan, ScanStatus, SpeciesChoice, SpeciesStatus},
    url::Stored,
};

use crate::{Pipeline, PipelineError, findings, payloads, steps::status_change};

/// What `submit` did: the scan as it now is, and whether a run started (`202`) or submit was
/// idempotent (`200`).
#[derive(Debug, Clone, PartialEq)]
pub struct SubmitOutcome {
    /// The scan.
    pub scan: Scan<Stored>,
    /// A run started.
    pub started: bool,
}

/// The `versions` keys of the label lists ([contracts 5], `model_versions` keys).
pub const LABEL_SET_KEY: &str = "label_set_version";
/// The classifier labels' key.
pub const CLASSIFIER_LABELS_KEY: &str = "classifier_labels_version";

/// Whether a `perception` job may name these versions ([contracts 5], versions a worker
/// hasn't loaded): some live perception or fallback worker reports both, or no such worker is
/// live at all (the job waits, and a worker that starts on other versions is a deployment
/// mismatch it reports as `unknown_label_version`).
///
/// A heartbeat reports every loaded version of a label key as a comma-separated list
/// (`"2026-10-14,2026-10-18"`), so each value is split before it is compared.
pub fn labels_loaded(workers: &[LiveWorker], label_set: &str, classifier: &str) -> bool {
    let relevant: Vec<&LiveWorker> = workers
        .iter()
        .filter(|w| {
            w.caps
                .iter()
                .any(|c| matches!(c, Capability::Perception | Capability::Fallback))
        })
        .collect();
    relevant.is_empty()
        || relevant.iter().any(|w| {
            reports(w, LABEL_SET_KEY, label_set) && reports(w, CLASSIFIER_LABELS_KEY, classifier)
        })
}

/// Whether the worker's comma-separated list under `key` holds `version`.
fn reports(worker: &LiveWorker, key: &str, version: &str) -> bool {
    worker
        .versions
        .get(key)
        .is_some_and(|list| list.split(',').map(str::trim).any(|v| v == version))
}

/// What the earlier run finished, for a species-change run: plant check and segmentation
/// observations exist, and measuring finished (or wasn't needed).
pub fn earlier_run(snap: &ScanSnapshot) -> EarlierRun {
    let has = |stage: ObservationStage| snap.observations.iter().any(|o| o.stage == stage);
    EarlierRun {
        perception_base_done: has(ObservationStage::PlantCheck)
            && has(ObservationStage::Segmentation),
        measured: !findings::has_depth(&snap.scan.images) || has(ObservationStage::Measurement),
    }
}

/// The earlier run's regions, sent with a species-change run that keeps segmentation.
pub fn prior_regions(snap: &ScanSnapshot) -> Vec<PriorRegion> {
    snap.observations
        .iter()
        .filter(|o| o.stage == ObservationStage::Segmentation)
        .filter_map(|o| {
            let r = o.region.as_ref()?;
            Some(PriorRegion {
                region_id: r.region_id?,
                image_id: r.image_id,
                bbox: r.bbox,
                class: o
                    .detail
                    .as_ref()
                    .and_then(|d| d.region_class)
                    .unwrap_or(RegionClass::Leaf),
            })
        })
        .collect()
}

/// The plant's confirmed species: what a submitted run's `perception` job names, and the
/// working species that run starts with. Otherwise `None`, and the run's working species is
/// the best candidate of its own `species_suggested` ([architecture 6.2], step 4); a
/// resubmitted run doesn't keep the earlier run's suggestion, since it identifies the species
/// again.
fn confirmed_species(snap: &ScanSnapshot) -> Option<SpeciesId> {
    match snap.plant.species_status {
        SpeciesStatus::Confirmed => snap.plant.species.as_ref().map(|s| s.id.clone()),
        _ => None,
    }
}

impl Pipeline {
    /// The run's `perception` job for these stages, after checking the label versions.
    async fn perception_job(
        &self,
        snap: &ScanSnapshot,
        stages: &[Stage],
        choice: Option<&SpeciesChoice>,
        keep_regions: bool,
    ) -> Result<NewJob, PipelineError> {
        let options = self.deps.knowledge.vlm_options().await?;
        let labels = self.deps.knowledge.classifier_labels().await?;
        let workers = self.deps.store.live_workers(self.now()).await?;
        if !labels_loaded(&workers, &options.label_set_version, &labels.model_version) {
            return Err(PipelineError::LabelsNotLoaded {
                label_set_version: options.label_set_version,
                classifier_labels_version: labels.model_version,
            });
        }
        let (species_id, skip) = match choice {
            Some(SpeciesChoice::Known(id)) => (Some(id.clone()), false),
            Some(SpeciesChoice::Custom(_)) => (None, true),
            None => match confirmed_species(snap) {
                Some(id) => (Some(id), false),
                None => (None, snap.custom_species),
            },
        };
        let id = self.deps.ids.id();
        Ok(NewJob {
            id,
            scan: snap.scan.id,
            run: Some(snap.scan.run.next()),
            question: None,
            spec: JobSpec::Perception(payloads::perception_payload(
                id,
                &snap.scan.images,
                payloads::PerceptionSpec {
                    stages: perception_stages(stages),
                    species_id,
                    skip_species_identification: skip,
                    regions: keep_regions.then(|| prior_regions(snap)),
                    label_set_version: options.label_set_version,
                    classifier_labels_version: labels.model_version,
                },
            )),
            max_runtime: JobKind::Perception.max_runtime(false),
            queued_at: self.now(),
        })
    }

    /// `POST /scans/{id}/submit` ([contracts 3]): run 1, or a new run after a retryable
    /// failure; idempotent otherwise.
    pub async fn submit(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
    ) -> Result<SubmitOutcome, PipelineError> {
        let viewer: Membership<Viewer> = editor.at_least();
        let current = self.deps.store.scan(viewer, scan).await?;
        let decision = decide_submit(current.status, current.failure.as_ref().map(|f| f.code))
            .map_err(|e| domain::error::DomainError::InvalidState(e.details()))?;
        let SubmitDecision::StartRun(reason) = decision else {
            return Ok(SubmitOutcome {
                scan: current,
                started: false,
            });
        };
        domain::rules::uploads::check_required_shots(&findings::kinds(&current.images))?;
        let snap = self.deps.store.snapshot(scan).await?;
        let stages = run_stages(
            None,
            findings::has_depth(&snap.scan.images),
            snap.custom_species,
        );
        let job = self.perception_job(&snap, &stages, None, false).await?;
        let started = self
            .deps
            .store
            .start_run(
                editor,
                scan,
                RunStart {
                    reason,
                    expected_status: current.status,
                    expected_run: current.run,
                    working_species: confirmed_species(&snap),
                    first_job: job,
                    now: self.now(),
                },
            )
            .await?;
        let mut events = vec![ScanEventBody::RunStarted {
            reason,
            rerun_stages: stages,
        }];
        events.extend(status_change(started.from, started.to));
        self.emit(scan, started.run, events).await?;
        let scan = self.deps.store.scan(viewer, scan).await?;
        Ok(SubmitOutcome {
            scan,
            started: true,
        })
    }

    /// `POST /plants/{id}/species` ([contracts 3]): confirms or corrects the species; the
    /// plant's most recent scan follows it, with a new run when it is past `queued` and the
    /// species differs from the run's working species. A `queued` scan's run takes the new
    /// species as its working species; a `pending` scan's run starts with it at submit.
    pub async fn species_confirmed(
        &self,
        editor: Membership<Editor>,
        plant: PlantId,
        choice: SpeciesChoice,
    ) -> Result<Plant<Stored>, PipelineError> {
        let confirmation = self
            .deps
            .store
            .confirm_species(editor, plant, choice.clone())
            .await?;
        let Some(recent) = confirmation.recent_scan else {
            return Ok(confirmation.plant);
        };
        let new_species = match &choice {
            SpeciesChoice::Known(id) => Some(id.clone()),
            SpeciesChoice::Custom(_) => None,
        };
        let custom_name = match &choice {
            SpeciesChoice::Custom(n) => Some(n.clone()),
            SpeciesChoice::Known(_) => None,
        };
        let follow = species_follow(
            recent.status,
            recent.created_at,
            self.now(),
            recent.working_species != new_species,
        );
        let snap = self.deps.store.snapshot(recent.scan).await?;
        if follow == SpeciesFollow::AtRunStart && recent.status == ScanStatus::Queued {
            // The run started before the confirmation, without it: its working species
            // becomes the plant's confirmed one now, before `retrieve` and the `vlm` job read
            // it. Its queued `perception` job keeps the payload it was queued with.
            self.deps
                .store
                .adopt_confirmed_species(recent.scan, snap.scan.run)
                .await?;
        }
        if follow != SpeciesFollow::NewRun {
            if snap.scan.run.0 > 0 {
                self.emit(
                    recent.scan,
                    snap.scan.run,
                    vec![ScanEventBody::SpeciesConfirmed {
                        species_id: new_species,
                        custom_name,
                        rerun: false,
                    }],
                )
                .await?;
            }
            return Ok(confirmation.plant);
        }
        let earlier = earlier_run(&snap);
        let stages = run_stages(
            Some(earlier),
            findings::has_depth(&snap.scan.images),
            matches!(choice, SpeciesChoice::Custom(_)),
        );
        let job = self
            .perception_job(&snap, &stages, Some(&choice), earlier.perception_base_done)
            .await?;
        let started = self
            .deps
            .store
            .start_run(
                editor,
                recent.scan,
                RunStart {
                    reason: RunReason::SpeciesChanged,
                    expected_status: snap.scan.status,
                    expected_run: snap.scan.run,
                    working_species: new_species.clone(),
                    first_job: job,
                    now: self.now(),
                },
            )
            .await?;
        let mut events = vec![
            ScanEventBody::SpeciesConfirmed {
                species_id: new_species,
                custom_name,
                rerun: true,
            },
            ScanEventBody::RunStarted {
                reason: RunReason::SpeciesChanged,
                rerun_stages: stages,
            },
        ];
        if let Some(q) = started.superseded_question {
            events.push(ScanEventBody::QuestionResolved {
                question_id: q,
                by: ResolvedBy::Superseded,
            });
        }
        events.extend(status_change(started.from, ScanStatus::Analysing));
        self.emit(recent.scan, started.run, events).await?;
        Ok(confirmation.plant)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn worker(caps: Vec<Capability>, set: &str, cls: &str) -> LiveWorker {
        LiveWorker {
            caps,
            versions: BTreeMap::from([
                (LABEL_SET_KEY.to_owned(), set.to_owned()),
                (CLASSIFIER_LABELS_KEY.to_owned(), cls.to_owned()),
            ]),
        }
    }

    #[test]
    fn label_versions_rule() {
        assert!(labels_loaded(&[], "a", "b"), "nobody live: queue and wait");
        let vlm_only = [worker(vec![Capability::Vlm], "x", "y")];
        assert!(
            labels_loaded(&vlm_only, "a", "b"),
            "VLM workers don't count"
        );
        let old = [worker(vec![Capability::Perception], "old", "b")];
        assert!(!labels_loaded(&old, "a", "b"));
        let both = [
            worker(vec![Capability::Perception], "old", "b"),
            worker(vec![Capability::Fallback], "a", "b"),
        ];
        assert!(labels_loaded(&both, "a", "b"));
    }

    #[test]
    fn heartbeat_label_lists_are_split_on_commas() {
        let two_sets = [worker(vec![Capability::Perception], "old,a", "b, c")];
        assert!(
            labels_loaded(&two_sets, "a", "c"),
            "each listed version counts"
        );
        assert!(labels_loaded(&two_sets, "old", "b"));
        assert!(!labels_loaded(&two_sets, "ol", "b"), "whole values only");
        assert!(
            !labels_loaded(&two_sets, "old,a", "b"),
            "the list itself is not a version"
        );
    }
}
