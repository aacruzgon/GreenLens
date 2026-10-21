//! The stage events a job's `complete` stands in for ([contracts 5], partial results; backend
//! roadmap gap 9).
//!
//! A worker's `POST /internal/jobs/{id}/events` calls are best effort: while `api` is down the
//! worker drops them and goes on, because `complete` repeats every stage's output. The findings
//! then come from `complete`, but a stage whose `stage_completed` was lost would stay open on
//! the stream, and the app would show it running forever. So when a job completes, `api` folds
//! the run's stored stage events as the app's reducer does, and for each stage of the job's kind
//! sends what the stream is missing, before the job's findings:
//!
//! - a stage left started (its last stage event is a `stage_started`) gets its
//!   `stage_completed`, with no repeated `stage_started`;
//! - a stage with no stage event in the run gets `stage_started` and `stage_completed`, when the
//!   result has output for it and, for a perception stage, the run's `run_started` lists it (a
//!   species-change run passes the earlier run's regions through without segmenting again);
//! - a completed stage gets nothing, so when every event arrived `complete` adds none.
//!
//! `duration_ms` is 0: `api` never timed the stage. `skipped` is what the result shows (the
//! disease classifier's `disease_skipped`, `all_measured` for a `vlm` job with no indicator
//! answers), and nothing otherwise.
//!
//! A `guided` job gets no catch-up. Its `guided` stage is one stage shared by every guided
//! check of the run, and two checks can run at once, so the fold can't tell which check's
//! `stage_started` is still open: one check's completion would close another's stage, and two
//! completions could each send a start and a completion. `guided_check_done` already tells
//! the app the check finished.

use std::collections::BTreeMap;

use domain::{
    events::{ScanEventBody, Skipped, Stage},
    jobs::{DiseaseSkipReason, JobResult},
    ports::ObservationDiff,
    url::Stored,
};

/// What a job's result says about its stages.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResultStages {
    /// The stages a job of this kind reports, in pipeline order.
    kind: &'static [Stage],
    /// The stages the result has output for, with the `skipped` their `stage_completed` carries.
    shown: BTreeMap<Stage, Option<Skipped>>,
}

const PERCEPTION: [Stage; 5] = [
    Stage::PlantCheck,
    Stage::Segmentation,
    Stage::Species,
    Stage::Disease,
    Stage::ImageEmbedding,
];

fn skipped(reason: &str) -> Skipped {
    Skipped {
        reason: reason.to_owned(),
    }
}

impl ResultStages {
    /// The stages of a decoded result.
    pub(crate) fn of(result: &JobResult) -> Self {
        let mut shown = BTreeMap::new();
        let kind: &'static [Stage] = match result {
            JobResult::Perception(r) => {
                if r.plant_present.is_some() {
                    shown.insert(Stage::PlantCheck, None);
                }
                if !r.regions.is_empty() {
                    shown.insert(Stage::Segmentation, None);
                }
                if r.species.is_some() {
                    shown.insert(Stage::Species, None);
                }
                if r.disease.is_some() {
                    shown.insert(Stage::Disease, None);
                } else if let Some(s) = r.disease_skipped {
                    let why = match s.reason {
                        DiseaseSkipReason::SpeciesNotSupported => "species_not_supported",
                        DiseaseSkipReason::NoLeavesFound => "no_leaves_found",
                    };
                    shown.insert(Stage::Disease, Some(skipped(why)));
                }
                if r.image_embeddings.is_some() {
                    shown.insert(Stage::ImageEmbedding, None);
                }
                &PERCEPTION
            }
            JobResult::Vlm(r) => {
                shown.insert(Stage::VlmFindings, None);
                // contracts 5: the answers are empty only when the indicators call was skipped.
                let indicators = if r.indicators.answers.is_empty() {
                    Some(skipped(Skipped::ALL_MEASURED))
                } else {
                    None
                };
                shown.insert(Stage::VlmIndicators, indicators);
                &[Stage::VlmFindings, Stage::VlmIndicators]
            }
            JobResult::Verify(_) => {
                shown.insert(Stage::Verify, None);
                &[Stage::Verify]
            }
            // The `guided` stage is shared by every check of the run (module docs).
            JobResult::Guided(_) => &[],
        };
        Self { kind, shown }
    }

    /// The stage events the stream is missing for this job, given the run's stored
    /// `run_started` and stage events in `seq` order (other events are ignored).
    pub(crate) fn missing(
        &self,
        run_events: &[ScanEventBody<Stored>],
    ) -> Vec<ScanEventBody<Stored>> {
        let mut listed: &[Stage] = &[];
        // Per stage: `true` while its last stage event is a `stage_started`.
        let mut open = BTreeMap::new();
        for event in run_events {
            match event {
                ScanEventBody::RunStarted { rerun_stages, .. } => listed = rerun_stages,
                ScanEventBody::StageStarted { stage } => {
                    open.insert(*stage, true);
                }
                ScanEventBody::StageCompleted { stage, .. } => {
                    open.insert(*stage, false);
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for &stage in self.kind {
            let completed = |skipped| ScanEventBody::StageCompleted {
                stage,
                duration_ms: 0,
                skipped,
            };
            match open.get(&stage) {
                Some(true) => out.push(completed(self.shown.get(&stage).cloned().flatten())),
                Some(false) => {}
                None => {
                    let in_run = !PERCEPTION.contains(&stage) || listed.contains(&stage);
                    if let Some(skip) = self.shown.get(&stage).filter(|_| in_run) {
                        out.push(ScanEventBody::StageStarted { stage });
                        out.push(completed(skip.clone()));
                    }
                }
            }
        }
        out
    }
}

/// Everything a stored completion sends, in order: the stage events the stream is missing
/// (none without a decoded result), then the `finding` and `finding_retracted` events of the
/// result's diff.
pub(crate) fn completion_events(
    stages: Option<&ResultStages>,
    run_events: &[ScanEventBody<Stored>],
    diff: &ObservationDiff,
) -> Vec<ScanEventBody<Stored>> {
    let mut events = stages.map_or_else(Vec::new, |s| s.missing(run_events));
    events.extend(
        diff.added
            .iter()
            .cloned()
            .map(|observation| ScanEventBody::Finding { observation }),
    );
    events.extend(
        diff.retracted
            .iter()
            .map(|&observation_id| ScanEventBody::FindingRetracted { observation_id }),
    );
    events
}

#[cfg(test)]
mod tests {
    use domain::{
        events::RunReason,
        jobs::JobKind,
        records::observation::{ObservationSource, ObservationStage},
        rules::stages::run_stages,
        scan::ScanStatus,
    };

    use super::*;
    use crate::testkit;

    /// The stub worker's full result for a five-photo scan: plant check, four regions, species
    /// and disease, no image embeddings.
    const STUB_RESULT: &str = include_str!("fixtures/stub_perception_result.json");

    fn stub_result() -> JobResult {
        JobResult::decode(
            JobKind::Perception,
            serde_json::from_str(STUB_RESULT).unwrap(),
        )
        .unwrap()
    }

    fn run_started(stages: Vec<Stage>) -> ScanEventBody<Stored> {
        ScanEventBody::RunStarted {
            reason: RunReason::Submit,
            rerun_stages: stages,
        }
    }

    fn status(from: ScanStatus, to: ScanStatus) -> ScanEventBody<Stored> {
        ScanEventBody::StatusChanged { from, to }
    }

    fn started(stage: Stage) -> ScanEventBody<Stored> {
        ScanEventBody::StageStarted { stage }
    }

    fn completed(stage: Stage, ms: u64) -> ScanEventBody<Stored> {
        ScanEventBody::StageCompleted {
            stage,
            duration_ms: ms,
            skipped: None,
        }
    }

    /// The start of a submitted run, up to `queued → analysing`.
    fn submitted() -> Vec<ScanEventBody<Stored>> {
        vec![
            run_started(run_stages(None, false, false)),
            status(ScanStatus::Pending, ScanStatus::Queued),
            status(ScanStatus::Queued, ScanStatus::Analysing),
        ]
    }

    /// Two findings of the stub's result, as the store's diff returns them.
    fn findings() -> ObservationDiff {
        ObservationDiff {
            added: vec![
                testkit::segmentation_leaf(),
                testkit::obs(
                    ObservationStage::PlantCheck,
                    Stage::PlantCheck,
                    ObservationSource::Segmentation,
                    0.9,
                ),
            ],
            retracted: Vec::new(),
        }
    }

    /// Per stage, as the app's reducer folds them: how often it started, how often it
    /// completed, and whether it is left open.
    fn fold(events: &[ScanEventBody<Stored>]) -> BTreeMap<Stage, (u32, u32, bool)> {
        let mut out: BTreeMap<Stage, (u32, u32, bool)> = BTreeMap::new();
        for e in events {
            match e {
                ScanEventBody::StageStarted { stage } => {
                    let s = out.entry(*stage).or_default();
                    s.0 += 1;
                    s.2 = true;
                }
                ScanEventBody::StageCompleted { stage, .. } => {
                    let s = out.entry(*stage).or_default();
                    s.1 += 1;
                    s.2 = false;
                }
                _ => {}
            }
        }
        out
    }

    fn first_finding(events: &[ScanEventBody<Stored>]) -> usize {
        events
            .iter()
            .position(|e| matches!(e, ScanEventBody::Finding { .. }))
            .expect("a finding")
    }

    fn last_stage_event(events: &[ScanEventBody<Stored>]) -> usize {
        events
            .iter()
            .rposition(|e| {
                matches!(
                    e,
                    ScanEventBody::StageStarted { .. } | ScanEventBody::StageCompleted { .. }
                )
            })
            .expect("a stage event")
    }

    /// The laptop run of gap 9: `plant_check` started, then `api` went down and every later
    /// stage event of the job was lost. `complete` closes `plant_check` without starting it
    /// again, starts and completes the other stages its result shows, and sends all of them
    /// before the findings.
    #[test]
    fn lost_stage_events_are_sent_once_before_the_findings() {
        let mut stored = submitted();
        stored.push(started(Stage::PlantCheck));
        let stages = ResultStages::of(&stub_result());
        let sent = completion_events(Some(&stages), &stored, &findings());

        assert!(
            last_stage_event(&sent) < first_finding(&sent),
            "stage events before the findings"
        );
        assert_eq!(
            sent[..=last_stage_event(&sent)],
            [
                completed(Stage::PlantCheck, 0),
                started(Stage::Segmentation),
                completed(Stage::Segmentation, 0),
                started(Stage::Species),
                completed(Stage::Species, 0),
                started(Stage::Disease),
                completed(Stage::Disease, 0),
            ]
        );
        let mut run = stored;
        run.extend(sent);
        let folded = fold(&run);
        for stage in [
            Stage::PlantCheck,
            Stage::Segmentation,
            Stage::Species,
            Stage::Disease,
        ] {
            assert_eq!(
                folded[&stage],
                (1, 1, false),
                "{stage:?}: one start, one completion"
            );
        }
        assert!(
            !folded.contains_key(&Stage::ImageEmbedding),
            "no embeddings in the result, and the worker never started it"
        );
    }

    /// Every stage event arrived: `complete` adds none and sends only its findings.
    #[test]
    fn when_every_event_arrived_complete_adds_no_stage_events() {
        let mut stored = submitted();
        for stage in PERCEPTION {
            stored.push(started(stage));
            stored.push(completed(stage, 40));
        }
        let stages = ResultStages::of(&stub_result());
        assert_eq!(stages.missing(&stored), []);
        let sent = completion_events(Some(&stages), &stored, &findings());
        assert_eq!(sent.len(), 2);
        assert!(
            sent.iter()
                .all(|e| matches!(e, ScanEventBody::Finding { .. }))
        );
    }

    /// A requeued job starts a stage again after its first attempt completed it; if the second
    /// attempt's events are lost, the stage is open again and gets one more completion.
    #[test]
    fn a_stage_started_again_after_a_requeue_is_completed() {
        let mut stored = submitted();
        stored.extend([
            started(Stage::PlantCheck),
            completed(Stage::PlantCheck, 30),
            started(Stage::Segmentation),
            started(Stage::PlantCheck),
            completed(Stage::PlantCheck, 30),
            started(Stage::Segmentation),
            completed(Stage::Segmentation, 50),
            started(Stage::Species),
            completed(Stage::Species, 50),
            started(Stage::Disease),
        ]);
        let sent = ResultStages::of(&stub_result()).missing(&stored);
        assert_eq!(sent, [completed(Stage::Disease, 0)]);
    }

    /// A species-change run that keeps the earlier run's segmentation passes its regions
    /// through the result; that is no segmentation stage of this run.
    #[test]
    fn regions_passed_through_are_not_a_segmentation_stage() {
        let done = domain::rules::stages::EarlierRun {
            perception_base_done: true,
            measured: true,
        };
        let stored = vec![run_started(run_stages(Some(done), false, false))];
        let sent = ResultStages::of(&stub_result()).missing(&stored);
        let stages: Vec<_> = sent
            .iter()
            .filter_map(|e| match e {
                ScanEventBody::StageStarted { stage } => Some(*stage),
                _ => None,
            })
            .collect();
        assert_eq!(stages, [Stage::Species, Stage::Disease]);
    }

    /// A `guided` job's stage is shared by the run's checks, so even one left open gets no
    /// catch-up from a check's completion.
    #[test]
    fn a_guided_result_sends_no_stage_events() {
        let mut stored = submitted();
        stored.extend([started(Stage::Guided), started(Stage::Guided)]);
        let result = JobResult::decode(
            JobKind::Guided,
            serde_json::json!({"texture": {"kind": "jar_test", "settled": true, "texture": "loamy", "confidence": 0.65}}),
        )
        .unwrap();
        let stages = ResultStages::of(&result);
        assert_eq!(stages.missing(&stored), []);
        assert_eq!(stages.missing(&submitted()), []);
    }

    /// A `vlm` job with no indicator answers skipped its indicators call: the missing
    /// `stage_completed` says so, as the worker's own would have.
    #[test]
    fn a_vlm_job_without_answers_completes_vlm_indicators_as_all_measured() {
        let mut stored = submitted();
        stored.extend([
            started(Stage::VlmFindings),
            completed(Stage::VlmFindings, 900),
        ]);
        let result = JobResult::decode(
            JobKind::Vlm,
            serde_json::json!({
                "findings": {
                    "species_guess": {"species_id": null, "confidence": 0.4},
                    "soil": [], "organisms": [], "damage": [], "notes": ""
                },
                "indicators": {"answers": []}
            }),
        )
        .unwrap();
        let sent = ResultStages::of(&result).missing(&stored);
        assert_eq!(
            sent,
            [
                started(Stage::VlmIndicators),
                ScanEventBody::StageCompleted {
                    stage: Stage::VlmIndicators,
                    duration_ms: 0,
                    skipped: Some(skipped(Skipped::ALL_MEASURED)),
                },
            ]
        );
    }
}
