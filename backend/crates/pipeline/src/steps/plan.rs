//! The `plan` step ([contracts 3]: the plan writer; [contracts 7], action plan, re-planning,
//! neighbours, steps to tasks; [database 4], plan writes: compare-and-set).

use std::time::Instant;

use agent::{
    beliefs::current_beliefs,
    plan_writer::{FARM_SEARCH_TIMEOUT, PlanInput, SharedMemories, farm_query, plan_farm_records},
};
use domain::{
    chat::{Belief, MemoryKind, MemoryTopic},
    connectors::FarmRecordHit,
    events::{ScanEventBody, Stage},
    id::{OrganismId, SourceId, SpeciesId},
    knowledge::{OrganismSeed, SpeciesSeed},
    ports::{
        Conflict, IdGenExt, NewMemory, PlanWrite, ScanSnapshot, StepClaim, StepOutput, StoreError,
    },
    records::{
        diagnosis::{Diagnosis, DiagnosisStatus},
        plan::{ActionPlan, PlanReason, PlanTarget},
    },
    scan::{ScanFailureCode, ScanStatus},
    url::Stored,
};

use super::{completed, run_of, started, status_change};
use crate::{
    Pipeline, PipelineError, local_today,
    replan::{NeighbourPlant, PlanNeed, neighbours, plan_need, plan_problems, tasks_for},
};

/// Automatic re-plans that lose the compare-and-set start again from the new current plan;
/// after this many losses the step releases its claim (or hands over to `score` when
/// `rescore_pending` is set) instead of holding it until the 5-minute sweep.
const MAX_PLAN_ATTEMPTS: u32 = 3;

/// What the plan writer reads, owned by the step so [`PlanInput<'_>`] can borrow it.
struct PlanContext {
    problems: Vec<domain::knowledge::ProblemSeed>,
    treatments: Vec<domain::knowledge::TreatmentSeed>,
    organisms: Vec<OrganismSeed>,
    species: Option<SpeciesSeed>,
    memories: Vec<domain::chat::Memory>,
    /// The plant's and its garden's current beliefs: shared memories only.
    beliefs: Vec<Belief>,
    constraints: Vec<domain::records::plan::PlanConstraint>,
    neighbours: Vec<domain::records::plan::PlanNeighbour>,
    known_sources: Vec<SourceId>,
    /// The garden's farm records for this plan: empty without connectors.
    farm_records: Vec<FarmRecordHit>,
}

impl Pipeline {
    /// `plan`: holds the scan's plan lock from reading the current plan to storing the new
    /// one. Writes nothing when the plan already follows the diagnosis.
    pub(super) async fn plan(&self, claim: StepClaim) -> Result<Option<StepClaim>, PipelineError> {
        let _guard = self.deps.writer.plan_lock(claim.scan).await;
        for _ in 0..MAX_PLAN_ATTEMPTS {
            let Some(snap) = self.snapshot_for(claim).await? else {
                return Ok(None);
            };
            let Some(diagnosis) = snap.diagnosis.clone() else {
                return self.commit(claim, StepOutput::Release).await;
            };
            let reason = match plan_need(
                snap.plan.as_ref(),
                snap.plan_diagnosis.as_ref(),
                &diagnosis,
                snap.scan.open_question.as_ref(),
            ) {
                PlanNeed::UpToDate => return self.commit(claim, StepOutput::Release).await,
                PlanNeed::Write(r) => r,
            };
            match self.write_plan(claim, &snap, &diagnosis, reason).await {
                Err(PipelineError::Store(StoreError::Conflict(Conflict::StalePlan { .. }))) => {
                    tracing::info!(
                        "plan write lost the compare-and-set; starting from the new plan"
                    );
                }
                other => return other,
            }
        }
        // Every attempt lost to a plan written outside this lock (the plan lock serialises the
        // writers of one process, so another writer is another process). The newest plan stays
        // current; holding the claim would keep the stream open and block the scan's steps for
        // 5 minutes, so the claim is released now (or handed to `score` when an input arrived
        // meanwhile). The next diagnosis change re-plans from that plan.
        tracing::warn!(scan = %claim.scan, attempts = MAX_PLAN_ATTEMPTS, "plan writes kept losing the compare-and-set; releasing the step");
        self.commit(claim, StepOutput::Release).await
    }

    #[expect(
        clippy::too_many_lines,
        reason = "a step reads top to bottom in the order of its contract section"
    )]
    async fn write_plan(
        &self,
        claim: StepClaim,
        snap: &ScanSnapshot,
        diagnosis: &Diagnosis<Stored>,
        reason: PlanReason,
    ) -> Result<Option<StepClaim>, PipelineError> {
        let run = run_of(snap);
        let k = &self.deps.knowledge;
        let t = Instant::now();
        self.emit(snap.scan.id, run, vec![started(Stage::Plan)])
            .await?;

        let ctx = self.plan_context(snap, diagnosis).await?;
        let memories = SharedMemories::new(&ctx.memories)
            .map_err(|_| PipelineError::Inconsistent("a private memory reached the plan writer"))?;
        let input = PlanInput {
            plant_name: &snap.plant.name,
            species: ctx.species.as_ref(),
            diagnosis,
            problems: &ctx.problems,
            treatments: &ctx.treatments,
            organisms: &ctx.organisms,
            memories,
            beliefs: &ctx.beliefs,
            constraints: &ctx.constraints,
            neighbours: &ctx.neighbours,
            open_question: snap.scan.open_question.as_ref(),
            weather: snap.weather.as_ref(),
            known_sources: &ctx.known_sources,
            farm_records: &ctx.farm_records,
        };
        let written = match self.deps.plan_author.write_plan(&input).await {
            Ok(w) => w,
            Err(e) if snap.plan.is_none() && snap.scan.status == ScanStatus::Diagnosed => {
                tracing::warn!(error = %e, "plan writer unavailable; the run fails");
                self.fail_run(
                    snap,
                    ScanFailureCode::UpstreamUnavailable,
                    "The plan writer is unavailable.",
                )
                .await?;
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        };
        for d in &written.dropped {
            tracing::info!(failure = ?d, "plan step dropped by validation");
        }

        let now = self.now();
        let plan_id = self.deps.ids.id();
        let today = local_today(now, &snap.garden.time_zone.0);
        let (tasks, triggers) = tasks_for(
            plan_id,
            snap.plant.id,
            &written.steps,
            today,
            now,
            || self.deps.ids.id(),
            || self.deps.ids.id(),
        );
        let citations = k.citations(&written.citations).await?;
        let plan = ActionPlan {
            id: plan_id,
            scan_id: snap.scan.id,
            plant_id: snap.plant.id,
            version: snap.plan.as_ref().map_or(1, |p| p.version + 1),
            supersedes: snap.plan.as_ref().map(|p| p.id),
            provisional: diagnosis.status == DiagnosisStatus::Uncertain,
            diagnosis_id: diagnosis.id,
            open_question: snap.scan.open_question.clone(),
            constraints: ctx.constraints.clone(),
            summary: written.summary,
            steps: written.steps,
            watch_for: written.watch_for,
            citations,
            farm_records: written.farm_records,
            task_ids: tasks.iter().map(|t| t.id).collect(),
            llm_model: written.llm_model,
            prompt_version: written.prompt_version,
            created_at: now,
        };
        let old = snap.plan.as_ref().map(|p| p.id);
        let provisional = plan.provisional;
        let next = self
            .commit(
                claim,
                StepOutput::Plan(Box::new(PlanWrite {
                    plan: plan.clone(),
                    reason,
                    expected: old,
                    tasks,
                    triggers,
                })),
            )
            .await?;

        let mut events = vec![completed(Stage::Plan, t, None)];
        if snap.scan.status == ScanStatus::Diagnosed {
            events.extend(status_change(ScanStatus::Diagnosed, ScanStatus::Planned));
        }
        if let (Some(old_plan_id), Some(why)) = (old, reason.supersede_reason()) {
            events.push(ScanEventBody::PlanSuperseded {
                old_plan_id,
                new_plan_id: plan_id,
                reason: why,
            });
        }
        if snap.scan.status == ScanStatus::Diagnosed {
            events.push(ScanEventBody::PlanReady {
                plan_id,
                provisional,
            });
        }
        self.emit(snap.scan.id, run, events).await?;

        if matches!(reason, PlanReason::Initial | PlanReason::SpeciesChanged) {
            self.remember_scan(snap, diagnosis, &plan).await;
        }
        Ok(next)
    }

    /// Everything the plan writer reads, owned for the step: the plan's problems with their
    /// treatments, the organisms seen and targeted, the species, the shared memories, the
    /// constraints carried from the current plan, the neighbours, the sources a step may cite
    /// and the garden's farm records. [`PlanInput`] then borrows it.
    async fn plan_context(
        &self,
        snap: &ScanSnapshot,
        diagnosis: &Diagnosis<Stored>,
    ) -> Result<PlanContext, PipelineError> {
        let k = &self.deps.knowledge;
        let problems = k.problems(&plan_problems(diagnosis)).await?;
        let treatment_ids: Vec<_> = problems
            .iter()
            .flat_map(|p| p.treatments.iter().cloned())
            .collect();
        let treatments = k.treatments(&treatment_ids).await?;
        let mut organism_ids: Vec<OrganismId> = diagnosis
            .organisms
            .iter()
            .filter_map(|o| o.organism_id.clone())
            .chain(problems.iter().flat_map(|p| p.caused_by.iter().cloned()))
            .collect();
        organism_ids.sort();
        organism_ids.dedup();
        let mut organisms: Vec<OrganismSeed> = Vec::new();
        for id in &organism_ids {
            organisms.extend(k.organism(id).await?);
        }
        let species: Option<SpeciesSeed> = match &snap.working_species {
            Some(s) => k.species(s).await?,
            None => None,
        };
        // The belief read and the farm-record search run beside the memory read and never fail:
        // Agent Memory off, down or slower than its timeout gives no beliefs, and a garden
        // without farm records, or a failed or slow search, gives no records ([connectors 7]);
        // the plan is written without them.
        let farm_query = farm_query(&snap.plant.name, diagnosis);
        let (memories, beliefs, farm_records) = tokio::join!(
            self.deps
                .memory
                .shared_for_plan(snap.garden.id, snap.plant.id),
            current_beliefs(
                self.deps.beliefs.as_ref(),
                snap.garden.id,
                Some(snap.plant.id),
                self.deps.config.beliefs_read_timeout,
            ),
            plan_farm_records(
                self.deps.store.as_ref(),
                self.deps.embedder.as_ref(),
                snap.garden.id,
                &farm_query,
                FARM_SEARCH_TIMEOUT,
            ),
        );
        let memories = memories?;
        let constraints = snap
            .plan
            .as_ref()
            .map(|p| p.constraints.clone())
            .unwrap_or_default();

        // Neighbours from the current map ([contracts 7]): healthy diagnoses get none.
        let neighbours =
            if diagnosis.status == DiagnosisStatus::Healthy || snap.garden.map_version == 0 {
                Vec::new()
            } else if let Some(map) = self.deps.store.current_map(snap.garden.id).await? {
                let plants = self.deps.store.garden_plants(snap.garden.id).await?;
                let mut names: Vec<(SpeciesId, String)> = Vec::new();
                for p in &plants {
                    if let Some(s) = &p.species
                        && !names.iter().any(|(id, _)| id == &s.id)
                    {
                        names.push((s.id.clone(), s.common_name.clone()));
                    }
                }
                let views: Vec<NeighbourPlant<'_>> = plants
                    .iter()
                    .map(|p| {
                        let species = p.species.as_ref().map(|s| &s.id);
                        NeighbourPlant {
                            plant: p,
                            species,
                            species_name: p.species.as_ref().map(|s| s.common_name.as_str()),
                        }
                    })
                    .collect();
                let refs: Vec<_> = problems.iter().collect();
                neighbours(snap.plant.id, &map.pins, &views, &refs)
            } else {
                Vec::new()
            };

        let mut known_sources: Vec<SourceId> = problems
            .iter()
            .flat_map(|p| p.sources.iter().cloned())
            .chain(treatments.iter().flat_map(|t| t.sources.iter().cloned()))
            .chain(organisms.iter().flat_map(|o| o.sources.iter().cloned()))
            .chain(species.iter().flat_map(|s| s.sources.iter().cloned()))
            .collect();
        known_sources.sort();
        known_sources.dedup();

        Ok(PlanContext {
            problems,
            treatments,
            organisms,
            species,
            memories,
            beliefs,
            constraints,
            neighbours,
            known_sources,
            farm_records,
        })
    }

    /// The pipeline's memory after a scan ([architecture 7], memory): a short summary,
    /// visible to every member. Best effort: a failure is logged, never fails the step.
    async fn remember_scan(
        &self,
        snap: &ScanSnapshot,
        diagnosis: &Diagnosis<Stored>,
        plan: &ActionPlan,
    ) {
        let what = match (&diagnosis.status, &diagnosis.primary) {
            (DiagnosisStatus::Healthy, _) => "looked healthy".to_owned(),
            (DiagnosisStatus::Confirmed, Some(p)) => format!("had {}", p.name),
            (_, Some(p)) => format!("possibly had {}", p.name),
            (_, None) => "was checked".to_owned(),
        };
        let targets = plan
            .steps
            .iter()
            .flat_map(|s| &s.targets)
            .filter(|t| matches!(t, PlanTarget::Disease(_)))
            .count();
        let text = format!(
            "Scan on {}: {} {}; plan of {} steps ({targets} targeting a problem).",
            local_today(snap.scan.created_at, &snap.garden.time_zone.0),
            snap.plant.name,
            what,
            plan.steps.len()
        );
        let model = self.deps.embedder.model();
        let embedding = match self
            .deps
            .embedder
            .embed(
                domain::ports::EmbedMode::Document,
                std::slice::from_ref(&text),
            )
            .await
        {
            Ok(mut v) if !v.is_empty() => v.swap_remove(0),
            Ok(_) => return,
            Err(e) => {
                tracing::warn!(error = %e, "pipeline memory not embedded");
                return;
            }
        };
        let memory = NewMemory {
            id: self.deps.ids.id(),
            plant: Some(snap.plant.id),
            kind: MemoryKind::Summary,
            topic: MemoryTopic::Health,
            text,
            embedding,
            embedding_model: model.name,
            source_msg: None,
            created_at: self.now(),
        };
        if let Err(e) = self
            .deps
            .memory
            .add_pipeline_memory(snap.garden.id, memory)
            .await
        {
            tracing::warn!(error = %e, "pipeline memory not stored");
        }
    }
}
