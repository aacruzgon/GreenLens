//! Re-planning ([contracts 7], re-planning, neighbours for the plan writer, steps to tasks):
//! when a plan is (re)written, for whom, and which tasks and trigger rules it creates. Pure.

use domain::{
    garden::map::{Pin, distance_m},
    id::{PlanId, PlanTriggerId, PlantId, ProblemId, SpeciesId, TaskId},
    knowledge::ProblemSeed,
    ports::{NewPlanTrigger, NewTask},
    records::plan::PlanNeighbour,
    records::{
        diagnosis::{Diagnosis, DiagnosisStatus},
        plan::{ActionPlan, PlanReason, PlanStep, PlanWhen},
        task::{Repeat, TaskSource},
    },
    rules::limits::{MAX_PLAN_NEIGHBOURS, NEIGHBOUR_RADIUS_M},
    scan::{Plant, Question},
    url::Stored,
    wire::{LocalDate, Timestamp},
};

use crate::findings::texture;

/// What the `plan` step must do, read from the snapshot each time it starts (so the sweep's
/// re-run decides the same way).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanNeed {
    /// Write a plan for this reason.
    Write(PlanReason),
    /// The current plan already follows the current diagnosis: nothing to write.
    UpToDate,
}

/// Decides whether a plan must be written ([contracts 7], re-planning: these reasons only):
///
/// | Case | Need |
/// |---|---|
/// | no plan yet | `initial` |
/// | the plan was written from this diagnosis | up to date |
/// | the plan's diagnosis is of an earlier run | `species_changed` |
/// | the question open when the plan was written is no longer open, or the scores or status changed | `late_answer` |
/// | the texture changed, or the plan's diagnosis had none and this one has | `guided_check` |
/// | otherwise | up to date |
pub fn plan_need(
    plan: Option<&ActionPlan>,
    plan_diagnosis: Option<&Diagnosis<Stored>>,
    diagnosis: &Diagnosis<Stored>,
    open_question: Option<&Question>,
) -> PlanNeed {
    let Some(plan) = plan else {
        return PlanNeed::Write(PlanReason::Initial);
    };
    if plan.diagnosis_id == diagnosis.id {
        return PlanNeed::UpToDate;
    }
    let Some(before) = plan_diagnosis else {
        return PlanNeed::Write(PlanReason::LateAnswer);
    };
    if before.run != diagnosis.run {
        return PlanNeed::Write(PlanReason::SpeciesChanged);
    }
    let question_resolved = plan
        .open_question
        .as_ref()
        .is_some_and(|q| open_question.is_none_or(|now| now.id != q.id));
    if question_resolved
        || before.status != diagnosis.status
        || before.candidates != diagnosis.candidates
    {
        return PlanNeed::Write(PlanReason::LateAnswer);
    }
    if texture(&before.soil) != texture(&diagnosis.soil) {
        return PlanNeed::Write(PlanReason::GuidedCheck);
    }
    PlanNeed::UpToDate
}

/// The problems a plan is written for: the primary, or for an `uncertain` diagnosis every
/// candidate that scored zero or more; none when healthy.
pub fn plan_problems(diagnosis: &Diagnosis<Stored>) -> Vec<ProblemId> {
    match diagnosis.status {
        DiagnosisStatus::Healthy => Vec::new(),
        DiagnosisStatus::Confirmed => diagnosis
            .primary
            .iter()
            .map(|p| p.problem_id.clone())
            .collect(),
        DiagnosisStatus::Uncertain => diagnosis
            .candidates
            .iter()
            .filter(|c| c.score >= 0)
            .map(|c| c.problem_id.clone())
            .collect(),
    }
}

/// A plant on the map with what the neighbour rule needs.
#[derive(Debug, Clone, Copy)]
pub struct NeighbourPlant<'a> {
    /// The plant.
    pub plant: &'a Plant<Stored>,
    /// Its species, confirmed or suggested.
    pub species: Option<&'a SpeciesId>,
    /// The common name shown.
    pub species_name: Option<&'a str>,
}

/// Neighbours for the plan writer ([contracts 7]): when the plant is pinned and a plan
/// problem spreads, up to 5 pinned plants within 1.5 m, nearest first, each with the spreading
/// problems its species is `susceptible_to`; a neighbour with no species, or susceptible to
/// none of them, is left out.
pub fn neighbours(
    plant: PlantId,
    pins: &[Pin],
    plants: &[NeighbourPlant<'_>],
    problems: &[&ProblemSeed],
) -> Vec<PlanNeighbour> {
    let spreading: Vec<&ProblemSeed> = problems
        .iter()
        .copied()
        .filter(|p| p.kind.spreads())
        .collect();
    let Some(me) = pins.iter().find(|p| p.plant_id == plant) else {
        return Vec::new();
    };
    if spreading.is_empty() {
        return Vec::new();
    }
    let mut near: Vec<(f64, PlanNeighbour)> = pins
        .iter()
        .filter(|p| p.plant_id != plant)
        .filter_map(|pin| {
            let d = distance_m(me, pin);
            if d > NEIGHBOUR_RADIUS_M {
                return None;
            }
            let np = plants.iter().find(|n| n.plant.id == pin.plant_id)?;
            let species = np.species?;
            let problems: Vec<ProblemId> = spreading
                .iter()
                .filter(|p| p.affects.contains(species))
                .map(|p| p.id.clone())
                .collect();
            (!problems.is_empty()).then(|| {
                (
                    d,
                    PlanNeighbour {
                        plant_id: pin.plant_id,
                        name: np.plant.name.clone(),
                        species_name: np.species_name.unwrap_or_default().to_owned(),
                        distance_m: d,
                        problems,
                    },
                )
            })
        })
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    near.into_iter()
        .take(MAX_PLAN_NEIGHBOURS)
        .map(|(_, n)| n)
        .collect()
}

/// The tasks and trigger rules a plan version creates ([contracts 7], steps to tasks): `now`
/// is due today, `by_date` on its date, `recurring` repeats, `on_trigger` registers a rule.
/// A step's task goes to its `plant_id` when it has one, otherwise to the plan's plant. The
/// store skips any whose plant is gone (tasks only on live plants).
pub fn tasks_for(
    plan: PlanId,
    plan_plant: PlantId,
    steps: &[PlanStep],
    today: LocalDate,
    now: Timestamp,
    mut task_id: impl FnMut() -> TaskId,
    mut trigger_id: impl FnMut() -> PlanTriggerId,
) -> (Vec<NewTask>, Vec<NewPlanTrigger>) {
    let mut tasks = Vec::new();
    let mut triggers = Vec::new();
    for step in steps {
        let plant = step.plant_id.unwrap_or(plan_plant);
        let task = |due: Option<LocalDate>, repeat: Option<Repeat>, id: TaskId| NewTask {
            id,
            plant: Some(plant),
            plan: Some(plan),
            plan_step: Some(step.id.clone()),
            scan: None,
            title: step.title.clone(),
            detail: Some(step.detail.clone()),
            due,
            repeat,
            source: TaskSource::Plan,
            trigger: None,
            alert_key: None,
            created_by: None,
            created_at: now,
        };
        match &step.when {
            PlanWhen::Now => tasks.push(task(Some(today), None, task_id())),
            PlanWhen::ByDate { due } => tasks.push(task(Some(*due), None, task_id())),
            PlanWhen::Recurring { every_days, until } => tasks.push(task(
                Some(today),
                Some(Repeat {
                    every_days: *every_days,
                    until: *until,
                }),
                task_id(),
            )),
            PlanWhen::OnTrigger { trigger, until } => triggers.push(NewPlanTrigger {
                id: trigger_id(),
                plant,
                step: step.id.clone(),
                trigger: *trigger,
                until: *until,
            }),
        }
    }
    (tasks, triggers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit;
    use domain::ports::IdGenExt;

    #[test]
    fn plan_need_reasons() {
        let d1 = testkit::diagnosis(1, DiagnosisStatus::Uncertain);
        assert_eq!(
            plan_need(None, None, &d1, None),
            PlanNeed::Write(PlanReason::Initial)
        );
        let mut plan = testkit::plan(&d1);
        assert_eq!(
            plan_need(Some(&plan), Some(&d1), &d1, None),
            PlanNeed::UpToDate
        );

        let mut d2 = testkit::diagnosis(2, DiagnosisStatus::Uncertain);
        d2.id = "01JAX3K9T6R2B8V0QZ4M7D1E97".parse().unwrap();
        d2.supersedes = Some(d1.id);
        assert_eq!(
            plan_need(Some(&plan), Some(&d1), &d2, None),
            PlanNeed::Write(PlanReason::SpeciesChanged)
        );

        let mut late = testkit::diagnosis(1, DiagnosisStatus::Confirmed);
        late.id = "01JAX3K9T6R2B8V0QZ4M7D1E99".parse().unwrap();
        assert_eq!(
            plan_need(Some(&plan), Some(&d1), &late, None),
            PlanNeed::Write(PlanReason::LateAnswer)
        );

        let mut same = d1.clone();
        same.id = "01JAX3K9T6R2B8V0QZ4M7D1E98".parse().unwrap();
        assert_eq!(
            plan_need(Some(&plan), Some(&d1), &same, None),
            PlanNeed::UpToDate
        );
        same.soil = vec![testkit::texture_obs("loamy")];
        assert_eq!(
            plan_need(Some(&plan), Some(&d1), &same, None),
            PlanNeed::Write(PlanReason::GuidedCheck)
        );

        let q = testkit::question();
        plan.open_question = Some(q.clone());
        let mut unchanged = d1.clone();
        unchanged.id = same.id;
        assert_eq!(
            plan_need(Some(&plan), Some(&d1), &unchanged, Some(&q)),
            PlanNeed::UpToDate
        );
        assert_eq!(
            plan_need(Some(&plan), Some(&d1), &unchanged, None),
            PlanNeed::Write(PlanReason::LateAnswer)
        );
    }

    #[test]
    fn steps_to_tasks() {
        let steps: Vec<PlanStep> = serde_json::from_value(serde_json::json!([
            {"id": "s1", "order": 1, "title": "Remove leaves", "detail": "d", "category": "cultural",
             "when": {"kind": "now"}, "targets": [], "citations": ["src"]},
            {"id": "s2", "order": 2, "title": "Check", "detail": "d", "category": "monitoring",
             "when": {"kind": "recurring", "every_days": 3}, "targets": [], "citations": ["src"],
             "plant_id": "01JAX3K9T6R2B8V0QZ4M7D1E5B"},
            {"id": "s3", "order": 3, "title": "After rain", "detail": "d", "category": "cultural",
             "when": {"kind": "on_trigger", "trigger": "after_rain"}, "targets": [], "citations": ["src"]}
        ]))
        .unwrap();
        let plant: PlantId = "01JAX3K9T6R2B8V0QZ4M7D1E5A".parse().unwrap();
        let ids = domain::testing::SequentialIds::default();
        let (tasks, triggers) = tasks_for(
            "01JAX3K9T6R2B8V0QZ4M7D1E5C".parse().unwrap(),
            plant,
            &steps,
            "2026-10-20".parse().unwrap(),
            "2026-10-20T12:00:00Z".parse().unwrap(),
            || ids.id(),
            || ids.id(),
        );
        assert_eq!(tasks.len(), 2);
        assert_eq!(
            tasks[1].plant,
            Some("01JAX3K9T6R2B8V0QZ4M7D1E5B".parse().unwrap()),
            "the neighbour's task"
        );
        assert_eq!(tasks[1].repeat.unwrap().every_days, 3);
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].plant, plant);
    }
}
