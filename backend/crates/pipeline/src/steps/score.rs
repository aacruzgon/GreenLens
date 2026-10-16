//! The `score` step ([contracts 3]: scoring, fusion and the diagnosis, then a question or the
//! plan; [contracts 7], diagnosis and re-planning; [architecture 6.3]).

use std::time::Instant;

use domain::{
    events::{ResolvedBy, ScanEventBody, SelectionSource, Stage},
    id::{OrganismId, ProblemId, SourceId},
    jobs::{JobKind, JobSpec, VerifyReason},
    knowledge::{ProblemSeed, SeedOrganismRole},
    ports::{IdGenExt, NewJob, NewQuestion, ScanSnapshot, StepClaim, StepOutput},
    records::{
        diagnosis::{Diagnosis, DiagnosisOrganism, DiagnosisStatus, EvidenceBasis, EvidenceSource},
        observation::{KnowledgeRef, Observation, ObservationStage, OrganismRole},
    },
    rules::{limits::MAX_CANDIDATES, questions::may_ask},
    scan::{PlantStatus, QuestionStatus, ScanFailureCode, ScanStatus},
    url::Stored,
    vlm::DamagePattern,
};
use knowledge::{
    candidates::{CandidateSet, ProfilePool, outside_candidates},
    scoring::{AnswerSet, Scored, score_all},
    status::{StatusInput, choose_question, decide},
};

use super::{completed, run_of, started, status_change};
use crate::{
    Pipeline, PipelineError, findings,
    fusion::{FusionInput, risks},
    measure::{answer_indicator, clean_images},
    payloads,
    questions::build_question,
    replan::{PlanNeed, plan_need, plan_problems},
};

/// Where the scan is when `score` runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScoreMode {
    /// `analysing` or `awaiting_answer`: the run's own scoring; may ask a question.
    InRun,
    /// `diagnosed` or `planned`: late work; never asks, never moves the status.
    Late,
    /// Anything else claims nothing.
    Idle,
}

pub(crate) const fn mode(status: ScanStatus) -> ScoreMode {
    match status {
        ScanStatus::Analysing | ScanStatus::AwaitingAnswer => ScoreMode::InRun,
        ScanStatus::Diagnosed | ScanStatus::Planned => ScoreMode::Late,
        _ => ScoreMode::Idle,
    }
}

/// The VLM's organisms and damage patterns of the run at medium confidence or more.
fn vlm_pointers(snap: &ScanSnapshot) -> (Vec<OrganismId>, Vec<DamagePattern>) {
    let current = |o: &&Observation<Stored>| {
        o.run == snap.scan.run && o.confidence.confidence().at_least_medium()
    };
    let organisms = snap
        .observations
        .iter()
        .filter(current)
        .filter(|o| o.stage == ObservationStage::Organism)
        .filter_map(|o| match &o.knowledge_ref {
            Some(KnowledgeRef::Organism(id)) => Some(id.clone()),
            _ => None,
        })
        .collect();
    let patterns = snap
        .observations
        .iter()
        .filter(current)
        .filter(|o| o.stage == ObservationStage::Damage)
        .filter_map(|o| o.detail.as_ref().and_then(|d| d.pattern))
        .collect();
    (organisms, patterns)
}

/// A diagnosis equal to another apart from its identity: a re-run that finds nothing new
/// writes nothing.
fn same_result(a: &Diagnosis<Stored>, b: &Diagnosis<Stored>) -> bool {
    a.status == b.status
        && a.low_evidence == b.low_evidence
        && a.candidates == b.candidates
        && findings::texture(&a.soil) == findings::texture(&b.soil)
        && a.risks == b.risks
}

/// The plant's status after a diagnosis.
pub(crate) const fn plant_status(status: DiagnosisStatus) -> PlantStatus {
    match status {
        DiagnosisStatus::Healthy => PlantStatus::Healthy,
        DiagnosisStatus::Confirmed => PlantStatus::Sick,
        DiagnosisStatus::Uncertain => PlantStatus::Watch,
    }
}

impl Pipeline {
    /// `score`. In a run: waits for the VLM stage, fails an unreadable plant, adds the
    /// candidates the VLM found outside the set (a `verify` job), then scores; an ambiguous
    /// result asks a question while the run may, otherwise the diagnosis is stored and the
    /// plan step follows. Late work stores a new diagnosis when the result changed, and
    /// re-plans by the re-planning rules.
    #[expect(
        clippy::too_many_lines,
        reason = "a step reads top to bottom in the order of its contract section"
    )]
    pub(super) async fn score(&self, claim: StepClaim) -> Result<Option<StepClaim>, PipelineError> {
        let Some(snap) = self.snapshot_for(claim).await? else {
            return Ok(None);
        };
        let run = run_of(&snap);
        let mode = mode(snap.scan.status);
        if mode == ScoreMode::Idle
            || (mode == ScoreMode::InRun && !findings::vlm_done(&snap.observations, run))
        {
            return self.commit(claim, StepOutput::Release).await;
        }
        if mode == ScoreMode::InRun && !findings::readable(&snap.observations) {
            self.fail_run(
                &snap,
                ScanFailureCode::PhotosUnusable,
                "Nothing could be read from the photos.",
            )
            .await?;
            return Ok(None);
        }

        let k = &self.deps.knowledge;
        let stored_ids: Vec<ProblemId> = snap
            .candidates
            .iter()
            .map(|c| c.problem_id.clone())
            .collect();
        let mut profiles = k.problems(&stored_ids).await?;
        let hits = findings::classifier_hits(&snap.observations);
        let weather = snap.weather.as_ref();

        // Candidates outside the set: one verify job, before any diagnosis of the run.
        if mode == ScoreMode::InRun
            && let Some(next) = self.verify_outside(claim, &snap, &profiles, &hits).await?
        {
            return Ok(next);
        }

        let t = Instant::now();
        self.emit(snap.scan.id, run, vec![started(Stage::Scoring)])
            .await?;
        profiles.retain(|p| stored_ids.contains(&p.id));
        let pool = ProfilePool::new(&profiles);
        let set = CandidateSet::from_selected(&snap.candidates, &pool, &hits, weather);
        let answers = AnswerSet::new(&snap.evidence);
        let ranked: Vec<Scored<'_>> = score_all(set.as_slice(), &answers);
        let scores: Vec<_> = ranked.iter().map(|s| s.score.clone()).collect();
        let measured = findings::measured(&snap.observations, &snap.scan.images, &snap.depth);
        let clean = clean_images(&measured);

        // The open question: a photo answer whose verify finished is resolved by this score.
        let open = snap.scan.open_question.as_ref();
        let resolved_question = open
            .filter(|q| q.status == QuestionStatus::Answering && snap.open_question_claim.is_none())
            .map(|q| q.id);
        let still_open = open.filter(|q| Some(q.id) != resolved_question);
        let continued = still_open.is_some_and(|q| q.status == QuestionStatus::Open);
        let decision = decide(&StatusInput {
            scores: &scores,
            findings: findings::finding_summary(&snap.observations),
            clean_images: &clean,
            question_open: continued,
            margin: self.deps.config.diagnosis_margin,
        });
        self.emit(snap.scan.id, run, vec![completed(Stage::Scoring, t, None)])
            .await?;

        let resolved_event = resolved_question.map(|question_id| ScanEventBody::QuestionResolved {
            question_id,
            by: ResolvedBy::Photo,
        });

        // Ambiguous, still in the run, under the cap, and not a continuation: ask.
        if let Some(ambiguity) = decision.ambiguity
            && mode == ScoreMode::InRun
            && !continued
            && may_ask(snap.questions_asked, true)
            && let Some(choice) = choose_question(ambiguity, &ranked, &answers)
        {
            let question = build_question(&choice, self.deps.ids.id(), run, self.now());
            let indicator = match &choice {
                knowledge::status::QuestionChoice::Indicator { indicator, .. } => {
                    Some(indicator.id.clone())
                }
                knowledge::status::QuestionChoice::Photo { .. } => None,
            };
            let next = self
                .commit(
                    claim,
                    StepOutput::Ask {
                        question: NewQuestion {
                            question: question.clone(),
                            indicator,
                        },
                        resolved_question,
                    },
                )
                .await?;
            let mut events: Vec<_> = resolved_event.into_iter().collect();
            events.push(ScanEventBody::Question { question });
            events.extend(status_change(snap.scan.status, ScanStatus::AwaitingAnswer));
            self.emit(snap.scan.id, run, events).await?;
            return Ok(next);
        }

        // Fusion and the diagnosis.
        let diagnosis = self.diagnose(&snap, decision, &scores, &profiles).await?;

        if mode == ScoreMode::Late
            && resolved_question.is_none()
            && snap
                .diagnosis
                .as_ref()
                .is_some_and(|d| same_result(d, &diagnosis))
            && snap
                .plan
                .as_ref()
                .is_none_or(|p| p.open_question.as_ref().map(|q| q.id) == still_open.map(|q| q.id))
        {
            return self.commit(claim, StepOutput::Release).await;
        }

        let need = plan_need(
            snap.plan.as_ref(),
            snap.plan_diagnosis.as_ref(),
            &diagnosis,
            still_open,
        );
        let status = (mode == ScoreMode::InRun).then_some(ScanStatus::Diagnosed);
        let next = self
            .commit(
                claim,
                StepOutput::Score {
                    diagnosis: diagnosis.clone(),
                    status,
                    resolved_question,
                    needs_plan: matches!(need, PlanNeed::Write(_)),
                },
            )
            .await?;
        if let Err(e) = self
            .deps
            .store
            .set_plant_status(snap.plant.id, plant_status(diagnosis.status))
            .await
        {
            tracing::warn!(error = %e, "plant status not updated");
        }
        let mut events: Vec<_> = resolved_event.into_iter().collect();
        events.push(ScanEventBody::Diagnosis { diagnosis });
        if let Some(to) = status {
            events.extend(status_change(snap.scan.status, to));
        }
        self.emit(snap.scan.id, run, events).await?;
        Ok(next)
    }

    /// Fusion and the diagnosis ([contracts 7], diagnosis): the ranked scores (at most 9), the
    /// organisms, the soil (latest guided check of each kind), fusion's risks, the evidence
    /// basis and the citations of the problems the diagnosis points to.
    async fn diagnose(
        &self,
        snap: &ScanSnapshot,
        decision: knowledge::status::StatusDecision,
        scores: &[domain::records::diagnosis::CandidateScore],
        profiles: &[ProblemSeed],
    ) -> Result<Diagnosis<Stored>, PipelineError> {
        let k = &self.deps.knowledge;
        let run = run_of(snap);
        let weather = snap.weather.as_ref();
        let t = Instant::now();
        self.emit(snap.scan.id, run, vec![started(Stage::Fusion)])
            .await?;
        let mut diagnosis = Diagnosis {
            id: self.deps.ids.id(),
            scan_id: snap.scan.id,
            run,
            supersedes: snap.diagnosis.as_ref().map(|d| d.id),
            status: decision.status,
            low_evidence: decision.low_evidence,
            primary: (decision.status != DiagnosisStatus::Healthy)
                .then(|| scores.first().cloned())
                .flatten(),
            candidates: scores.iter().take(MAX_CANDIDATES).cloned().collect(),
            organisms: self.organisms(snap).await?,
            soil: findings::soil(&snap.observations),
            risks: Vec::new(),
            evidence_basis: EvidenceBasis::default(),
            citations: Vec::new(),
            model_versions: findings::model_versions(&snap.observations),
            created_at: self.now(),
        };
        let problem_ids = plan_problems(&diagnosis);
        let plan_profiles: Vec<&ProblemSeed> = profiles
            .iter()
            .filter(|p| problem_ids.contains(&p.id))
            .collect();
        let species = match &snap.working_species {
            Some(s) => k.species(s).await?,
            None => None,
        };
        let care = species.as_ref().and_then(|s| s.care.as_ref());
        let heights = self.deps.store.heights(snap.plant.id).await?;
        let map = if snap.garden.map_version > 0 {
            self.deps.store.current_map(snap.garden.id).await?
        } else {
            None
        };
        diagnosis.risks = risks(&FusionInput {
            problems: &plan_profiles,
            weather,
            soil: &diagnosis.soil,
            heights: &heights,
            mature_height_cm: care.and_then(|c| c.mature_height_cm),
            spacing_cm: care.and_then(|c| c.spacing_cm),
            plant: snap.plant.id,
            pins: map.as_ref().map_or(&[], |m| m.pins.as_slice()),
            now: self.now(),
            thresholds: self.deps.config.weather,
        });
        diagnosis.evidence_basis = self.evidence_basis(snap, &diagnosis).await?;
        let mut sources: Vec<SourceId> = plan_profiles
            .iter()
            .flat_map(|p| p.sources.iter().cloned())
            .collect();
        if plan_profiles.is_empty()
            && let Some(s) = &species
        {
            sources.extend(s.sources.iter().cloned());
        }
        diagnosis.citations = k.citations(&sources).await?;
        self.emit(snap.scan.id, run, vec![completed(Stage::Fusion, t, None)])
            .await?;
        Ok(diagnosis)
    }

    /// The VLM found organisms or damage pointing to problems outside the run's set
    /// ([architecture 6.3], step 2): adds them, answers what measurements settle, and queues one
    /// `verify` job for the rest. `Some(next)` when it committed; `None` when nothing is outside.
    async fn verify_outside(
        &self,
        claim: StepClaim,
        snap: &ScanSnapshot,
        profiles: &[ProblemSeed],
        hits: &[knowledge::candidates::ClassifierHit],
    ) -> Result<Option<Option<StepClaim>>, PipelineError> {
        let k = &self.deps.knowledge;
        let run = run_of(snap);
        let weather = snap.weather.as_ref();
        let (organisms, patterns) = vlm_pointers(snap);
        let pointed: Vec<ProblemSeed> = if organisms.is_empty() && patterns.is_empty() {
            Vec::new()
        } else {
            k.problems_pointed_to(&organisms, &patterns).await?
        };
        let pool = ProfilePool::new(profiles);
        let set = CandidateSet::from_selected(&snap.candidates, &pool, hits, weather);
        let tagged: Vec<(&ProblemSeed, SelectionSource)> = pointed
            .iter()
            .map(|p| {
                let by_organism = p.caused_by.iter().any(|o| organisms.contains(o));
                (
                    p,
                    if by_organism {
                        SelectionSource::Organism
                    } else {
                        SelectionSource::Damage
                    },
                )
            })
            .collect();
        let added = outside_candidates(&set, &tagged, weather);
        if !added.is_empty() {
            let measured = findings::measured(&snap.observations, &snap.scan.images, &snap.depth);
            let evidence: Vec<_> = added
                .iter()
                .flat_map(knowledge::candidates::Candidate::indicators)
                .filter_map(|i| answer_indicator(i, &measured))
                .collect();
            let mut all = snap.evidence.clone();
            all.extend(evidence.iter().cloned());
            let prompts = payloads::prompts(&added, &AnswerSet::new(&all));
            let added_candidates: Vec<_> = added
                .iter()
                .map(knowledge::candidates::Candidate::selected)
                .collect();
            let job = (!prompts.is_empty()).then(|| {
                let id = self.deps.ids.id();
                NewJob {
                    id,
                    scan: snap.scan.id,
                    run: Some(run),
                    question: None,
                    spec: JobSpec::Verify(payloads::verify_payload(
                        id,
                        VerifyReason::OutsideCandidateSet,
                        None,
                        &snap.scan.images,
                        None,
                        prompts,
                    )),
                    max_runtime: JobKind::Verify.max_runtime(false),
                    queued_at: self.now(),
                }
            });
            // The store signals the claim long poll when the commit queues the job.
            let next = self
                .commit(
                    claim,
                    StepOutput::Verify {
                        job,
                        evidence,
                        added_candidates: added_candidates.clone(),
                    },
                )
                .await?;
            let mut all_selected = snap.candidates.clone();
            all_selected.extend(added_candidates);
            self.emit(
                snap.scan.id,
                run,
                vec![ScanEventBody::CandidatesSelected {
                    candidates: all_selected,
                }],
            )
            .await?;
            return Ok(Some(next));
        }
        Ok(None)
    }

    /// `Diagnosis.organisms`: the run's organism findings, with reporting guidance for invasive
    /// ones (`preys_on` is filled when read, never stored).
    async fn organisms(
        &self,
        snap: &ScanSnapshot,
    ) -> Result<Vec<DiagnosisOrganism>, PipelineError> {
        let mut out = Vec::new();
        for o in snap
            .observations
            .iter()
            .filter(|o| o.stage == ObservationStage::Organism && o.run == snap.scan.run)
        {
            let organism_id = match &o.knowledge_ref {
                Some(KnowledgeRef::Organism(id)) => Some(id.clone()),
                _ => None,
            };
            let seed = match &organism_id {
                Some(id) => self.deps.knowledge.organism(id).await?,
                None => None,
            };
            let role = o
                .role
                .or_else(|| seed.as_ref().map(|s| s.role.into()))
                .unwrap_or(OrganismRole::Unknown);
            out.push(DiagnosisOrganism {
                observation_id: o.id,
                organism_id,
                name: o.label.clone(),
                role,
                reporting_url: seed
                    .filter(|s| s.role == SeedOrganismRole::Invasive)
                    .and_then(|s| s.reporting_url),
                preys_on: None,
            });
        }
        Ok(out)
    }

    /// `Diagnosis.evidence_basis` (architecture 6.4): shared memories, then the photo
    /// (measurements and soil findings), then modelled data.
    async fn evidence_basis(
        &self,
        snap: &ScanSnapshot,
        diagnosis: &Diagnosis<Stored>,
    ) -> Result<EvidenceBasis, PipelineError> {
        let memories = self
            .deps
            .memory
            .shared_for_plan(snap.garden.id, snap.plant.id)
            .await?;
        let user_input = memories
            .into_iter()
            .filter(|m| m.source == domain::chat::MemorySource::User)
            .take(5)
            .map(|m| m.text)
            .collect();
        let mut photo: Vec<String> = snap
            .evidence
            .iter()
            .filter(|e| e.source == EvidenceSource::Measurement)
            .map(|e| e.reason.clone())
            .collect();
        photo.extend(diagnosis.soil.iter().map(|o| o.label.clone()));
        let mut modelled = Vec::new();
        if let Some(w) = &snap.weather {
            modelled.push(format!(
                "{:.0} °C, {:.0}% humidity now; {} hours over 90% in the past week",
                w.current.temp_c, w.current.rh_pct, w.past_7_days.hours_rh_over_90
            ));
            modelled.push(format!(
                "Soil moisture {:.2} m³/m³ in the top 9 cm",
                w.soil.moisture_0_9cm_m3m3
            ));
        }
        if let Some(depth) = snap
            .garden
            .soil_profile
            .as_ref()
            .and_then(|p| p.depths.first())
            && let Some(ph) = depth.ph_h2o
        {
            modelled.push(format!("SoilGrids pH {ph:.1} near the surface"));
        }
        Ok(EvidenceBasis {
            user_input,
            photo,
            modelled,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes() {
        assert_eq!(mode(ScanStatus::Analysing), ScoreMode::InRun);
        assert_eq!(mode(ScanStatus::Planned), ScoreMode::Late);
        assert_eq!(mode(ScanStatus::Failed), ScoreMode::Idle);
        assert_eq!(plant_status(DiagnosisStatus::Uncertain), PlantStatus::Watch);
    }
}
