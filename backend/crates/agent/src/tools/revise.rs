//! `revise_plan` ([contracts 7], re-planning; [contracts 8], pending actions): the model proposes
//! changed steps, removed steps and constraints against the scan's **current** plan; the full
//! plan the change would produce passes plan validation before the card exists, so the card
//! shows exactly what confirming stores. A failing preview goes back to the model with the
//! reasons, never trimmed silently.
//!
//! [`apply_preview`] is the pure half `api` reuses on confirm: the base plan plus the preview.

use std::collections::{BTreeSet, HashSet};

use domain::{
    authz::{Membership, Viewer},
    chat::{Card, Preview, ToolName, WriteTool},
    garden::map::{distance_m, neighbours},
    id::{
        OrganismId, PendingActionId, PlanId, PlantId, ProblemId, ScanId, SourceId, StepId,
        TreatmentId,
    },
    knowledge::{OrganismSeed, ProblemSeed},
    records::{
        diagnosis::{Diagnosis, DiagnosisStatus},
        observation::OrganismRole,
        plan::{ActionPlan, PlanConstraint, PlanNeighbour, PlanStep, PlanStepDraft, PlanTarget},
    },
    rules::limits::MAX_PLAN_NEIGHBOURS,
    url::Stored,
    wire::Timestamp,
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{Tool, ToolCtx, ToolKind, ToolOutput, write::propose};
use crate::{
    deps::AgentDeps,
    error::ToolError,
    safety::{PlanDraft, ValidationContext, step_to_draft, validate_plan},
};

/// `revise_plan(constraints)`.
#[derive(Debug)]
pub struct RevisePlan;

/// Arguments of `revise_plan`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RevisePlanArgs {
    /// The scan whose current plan changes.
    pub scan_id: ScanId,
    /// The revised plan's summary. Dose-checked.
    pub summary: String,
    /// Steps that replace the step with the same id, or are added. Dose-checked.
    #[serde(default)]
    pub changed_steps: Vec<PlanStepDraft>,
    /// Steps to drop.
    #[serde(default)]
    pub removed_step_ids: Vec<StepId>,
    /// Constraints that every later version keeps ("No copper products"). Dose-checked.
    #[serde(default)]
    pub constraints: Vec<String>,
}

impl Tool for RevisePlan {
    type Args = RevisePlanArgs;
    const NAME: ToolName = ToolName::RevisePlan;
    const DESCRIPTION: &'static str = "Propose a new version of a scan's current plan. The user \
        sees a card; the plan is shared with everyone in the garden.";
    const LABEL: &'static str = "Preparing a plan change";
    const KIND: ToolKind = ToolKind::Write;

    fn model_strings(args: &RevisePlanArgs) -> Vec<&str> {
        let mut v = vec![args.summary.as_str()];
        v.extend(args.constraints.iter().map(String::as_str));
        for s in &args.changed_steps {
            v.push(&s.title);
            v.push(&s.detail);
        }
        v
    }

    async fn run(&self, ctx: &ToolCtx<'_>, args: RevisePlanArgs) -> Result<ToolOutput, ToolError> {
        let member = ctx.owner.membership();
        let detail = ctx
            .deps
            .stores
            .scans
            .scan_detail(member, args.scan_id)
            .await?;
        let base = detail
            .plan
            .ok_or_else(|| ToolError::Rejected("This scan has no plan yet.".to_owned()))?;
        let diagnosis = detail
            .diagnosis
            .ok_or_else(|| ToolError::Rejected("This scan has no diagnosis yet.".to_owned()))?;

        let kb =
            PlanKnowledge::load(ctx.deps, member, &diagnosis, &base, &args.changed_steps).await?;
        let ctx_v = kb.context(ctx.deps, &diagnosis);

        let draft = merge(
            &base,
            args.summary.clone(),
            args.changed_steps,
            &args.removed_step_ids,
        )?;
        let validation = validate_plan(draft, &ctx_v);
        if !validation.is_clean() {
            return Err(ToolError::Rejected(validation.retry_message()));
        }

        let changed_steps: Vec<PlanStep> = validation
            .steps
            .into_iter()
            .filter(|s| base.steps.iter().find(|b| b.id == s.id) != Some(s))
            .collect();
        let card = Card {
            title: "Change the plan?".to_owned(),
            body: format!(
                "{} This changes the plan for everyone in the garden.",
                validation.summary
            ),
            confirm_label: "Change plan".to_owned(),
            reject_label: "Not now".to_owned(),
        };
        let preview = Preview::Plan {
            base_plan_id: base.id,
            plan_summary: validation.summary,
            changed_steps,
            removed_step_ids: args.removed_step_ids,
            constraints_added: args.constraints,
        };
        propose(ctx, WriteTool::RevisePlan, card, preview).await
    }
}

/// The base plan's steps as drafts, with the model's changes applied.
fn merge(
    base: &ActionPlan,
    summary: String,
    changed: Vec<PlanStepDraft>,
    removed: &[StepId],
) -> Result<PlanDraft, ToolError> {
    for id in removed {
        if !base.steps.iter().any(|s| &s.id == id) {
            return Err(ToolError::Rejected(format!("The plan has no step {id}.")));
        }
    }
    let mut steps: Vec<PlanStepDraft> = base
        .steps
        .iter()
        .filter(|s| !removed.contains(&s.id))
        .cloned()
        .map(step_to_draft)
        .collect();
    for c in changed {
        match steps.iter_mut().find(|s| s.id == c.id) {
            Some(slot) => *slot = c,
            None => steps.push(c),
        }
    }
    Ok(PlanDraft {
        summary,
        steps,
        watch_for: base.watch_for.clone(),
    })
}

/// The knowledge a preview is validated against, owned; [`PlanKnowledge::context`] lends it.
struct PlanKnowledge {
    problems: Vec<ProblemSeed>,
    treatments: Vec<domain::knowledge::TreatmentSeed>,
    organisms: Vec<OrganismSeed>,
    neighbours: Vec<PlanNeighbour>,
    sources: Vec<SourceId>,
    /// The base plan's farm records: a revised step may keep citing them, and cite no other.
    farm_records: Vec<domain::connectors::FarmCitation>,
}

impl PlanKnowledge {
    async fn load(
        deps: &AgentDeps,
        member: Membership<Viewer>,
        diagnosis: &Diagnosis<Stored>,
        base: &ActionPlan,
        changed: &[PlanStepDraft],
    ) -> Result<Self, ToolError> {
        let k = &deps.knowledge;
        let problem_ids = plan_problems(diagnosis);
        let problems = k.problems(&problem_ids).await?;

        let treatment_ids: Vec<TreatmentId> = problems
            .iter()
            .flat_map(|p| p.treatments.iter().cloned())
            .chain(base.steps.iter().filter_map(|s| s.treatment_id.clone()))
            .chain(changed.iter().filter_map(|s| s.treatment_id.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let treatments = k.treatments(&treatment_ids).await?;

        let organism_ids: BTreeSet<OrganismId> = diagnosis
            .organisms
            .iter()
            .filter_map(|o| o.organism_id.clone())
            .chain(problems.iter().flat_map(|p| p.caused_by.iter().cloned()))
            .chain(
                base.steps
                    .iter()
                    .flat_map(|s| s.targets.iter())
                    .chain(changed.iter().flat_map(|s| s.targets.iter()))
                    .filter_map(|t| match t {
                        PlanTarget::Organism(o) => Some(o.clone()),
                        PlanTarget::Disease(_) => None,
                    }),
            )
            .collect();
        let mut organisms = Vec::with_capacity(organism_ids.len());
        for id in &organism_ids {
            if let Some(o) = k.organism(id).await? {
                organisms.push(o);
            }
        }

        let neighbours =
            preview_neighbours(deps, member, base.plant_id, diagnosis, &problems).await?;

        let sources: Vec<SourceId> = problems
            .iter()
            .flat_map(|p| p.sources.iter())
            .chain(treatments.iter().flat_map(|t| t.sources.iter()))
            .chain(organisms.iter().flat_map(|o| o.sources.iter()))
            .cloned()
            .chain(base.citations.iter().map(|c| c.source_id.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();

        Ok(Self {
            problems,
            treatments,
            organisms,
            neighbours,
            sources,
            farm_records: base.farm_records.clone(),
        })
    }

    /// Lends the loaded knowledge: the context's `'k` is the borrow of `self`.
    fn context<'k>(
        &'k self,
        deps: &'k AgentDeps,
        diagnosis: &Diagnosis<Stored>,
    ) -> ValidationContext<'k> {
        ValidationContext {
            treatments: &self.treatments,
            products: &deps.products,
            organisms: &self.organisms,
            problems: &self.problems,
            neighbours: &self.neighbours,
            beneficials_seen: diagnosis
                .organisms
                .iter()
                .any(|o| o.role == OrganismRole::Beneficial),
            known_sources: &self.sources,
            farm_records: &self.farm_records,
        }
    }
}

/// The problems a plan is about: the primary one, or for an uncertain diagnosis every
/// candidate scored 0 or more; none when healthy.
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

/// The neighbours a preview is checked against, from the **current** map ([contracts 7],
/// neighbour steps): up to 5 pinned plants within the neighbour radius, nearest first, each
/// with the spreading problems of this plan its species is susceptible to; a neighbour with no
/// species or no such problem is left out, and a healthy diagnosis has none.
pub async fn preview_neighbours(
    deps: &AgentDeps,
    member: Membership<Viewer>,
    plant: PlantId,
    diagnosis: &Diagnosis<Stored>,
    problems: &[ProblemSeed],
) -> Result<Vec<PlanNeighbour>, ToolError> {
    let spreading: Vec<&ProblemSeed> = problems.iter().filter(|p| p.kind.spreads()).collect();
    if diagnosis.status == DiagnosisStatus::Healthy || spreading.is_empty() {
        return Ok(Vec::new());
    }
    let Some(map) = deps.stores.maps.current_map(member.garden()).await? else {
        return Ok(Vec::new());
    };
    let Some(me) = map.pins.iter().find(|p| p.plant_id == plant) else {
        return Ok(Vec::new());
    };
    let near = neighbours(&map.pins).remove(&plant).unwrap_or_default();
    let mut pins: Vec<_> = map
        .pins
        .iter()
        .filter(|p| near.contains(&p.plant_id))
        .collect();
    pins.sort_by(|a, b| distance_m(me, a).total_cmp(&distance_m(me, b)));

    let mut out = Vec::new();
    for pin in pins {
        if out.len() == MAX_PLAN_NEIGHBOURS {
            break;
        }
        let neighbour = match deps.stores.plants.plant(member, pin.plant_id).await {
            Ok(p) => p,
            Err(domain::ports::StoreError::NotFound) => continue,
            Err(e) => return Err(e.into()),
        };
        let Some(species) = neighbour.species else {
            continue;
        };
        let susceptible: HashSet<ProblemId> = deps
            .knowledge
            .problems_of_species(&species.id)
            .await?
            .into_iter()
            .map(|p| p.id)
            .collect();
        let shared: Vec<ProblemId> = spreading
            .iter()
            .filter(|p| susceptible.contains(&p.id))
            .map(|p| p.id.clone())
            .collect();
        if !shared.is_empty() {
            out.push(PlanNeighbour {
                plant_id: pin.plant_id,
                name: neighbour.name,
                species_name: species.common_name,
                distance_m: distance_m(me, pin),
                problems: shared,
            });
        }
    }
    Ok(out)
}

/// What a confirmed `revise_plan` card stores, before `api` adds ids, the version, tasks and
/// resolved citations.
#[derive(Debug, Clone, PartialEq)]
pub struct RevisedPlan {
    /// The preview's summary.
    pub summary: String,
    /// The base plan's steps with the preview applied, in order.
    pub steps: Vec<PlanStep>,
    /// The base plan's `watch_for`.
    pub watch_for: Vec<String>,
    /// The base plan's constraints plus the ones added.
    pub constraints: Vec<PlanConstraint>,
    /// Every source the steps cite.
    pub citation_ids: Vec<SourceId>,
}

/// Why a preview can't be applied.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PreviewError {
    /// A task card, not a plan revision.
    #[error("not a plan preview")]
    NotAPlan,
    /// The preview was made against another plan version: `409 invalid_state` with
    /// `current_plan_id`.
    #[error("the preview is based on plan {base}, not the current plan {current}")]
    BaseMismatch {
        /// The preview's base.
        base: PlanId,
        /// The plan passed in.
        current: PlanId,
    },
}

/// The plan a confirmed card stores: `base` with `changed_steps` replacing the steps with the
/// same id (or added), `removed_step_ids` dropped and `constraints_added` appended
/// ([contracts 7], re-planning). Pure; it doesn't validate again (the preview was validated
/// when the card was made) and the "named neighbour still exists" check is the store's.
pub fn apply_preview(
    base: &ActionPlan,
    preview: &Preview,
    action: PendingActionId,
    now: Timestamp,
) -> Result<RevisedPlan, PreviewError> {
    let Preview::Plan {
        base_plan_id,
        plan_summary,
        changed_steps,
        removed_step_ids,
        constraints_added,
    } = preview
    else {
        return Err(PreviewError::NotAPlan);
    };
    if *base_plan_id != base.id {
        return Err(PreviewError::BaseMismatch {
            base: *base_plan_id,
            current: base.id,
        });
    }
    let mut steps: Vec<PlanStep> = base
        .steps
        .iter()
        .filter(|s| !removed_step_ids.contains(&s.id))
        .cloned()
        .collect();
    for c in changed_steps {
        match steps.iter_mut().find(|s| s.id == c.id) {
            Some(slot) => slot.clone_from(c),
            None => steps.push(c.clone()),
        }
    }
    steps.sort_by_key(|s| s.order);
    let mut seen = BTreeSet::new();
    let citation_ids = steps
        .iter()
        .flat_map(|s| s.citations.iter())
        .filter(|c| seen.insert((*c).clone()))
        .cloned()
        .collect();
    let constraints = base
        .constraints
        .iter()
        .cloned()
        .chain(constraints_added.iter().map(|text| PlanConstraint {
            text: text.clone(),
            from_pending_action_id: action,
            created_at: now,
        }))
        .collect();
    Ok(RevisedPlan {
        summary: plan_summary.clone(),
        steps,
        watch_for: base.watch_for.clone(),
        constraints,
        citation_ids,
    })
}
