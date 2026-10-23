//! Plan validation ([contracts 7], plan validation): every rule of the table, applied to a
//! [`PlanDraft`] (the plan writer's output, or the plan a `revise_plan` preview would produce)
//! against the knowledge the plan may use, borrowed through a [`ValidationContext`].
//!
//! Validation never fails as a whole. It returns the steps that pass, with their category fixed
//! from the knowledge base and the safety note added, and one [`RuleFailure`] per broken rule.
//! The plan writer sends the failures back once and then stores the passing steps; a
//! `revise_plan` call returns them to the model as the tool result.

use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
};

use domain::{
    connectors::FarmCitation,
    id::{FarmRecordId, OrganismId, PlantId, ProblemId, SourceId, StepId, TreatmentId},
    knowledge::{OrganismSeed, ProblemSeed, SeedOrganismRole, TreatmentSeed},
    records::plan::{
        PlanNeighbour, PlanStep, PlanStepDraft, PlanTarget, PlanWhen, PlanWriterOutput,
        SAFETY_NOTE, StepCategory,
    },
};

use super::{ProductCatalogue, dose_check};

/// The summary stored when the plan writer's summary still gives a dose after the retry: the
/// steps carry the plan, so a neutral sentence replaces it rather than dropping the plan.
pub const FALLBACK_SUMMARY: &str = "Follow the steps below, starting with the first one.";

/// A plan to validate: `ActionPlan`'s model-written fields with steps as the model wrote them.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanDraft {
    /// The summary.
    pub summary: String,
    /// The steps, in the model's order.
    pub steps: Vec<PlanStepDraft>,
    /// What to watch for.
    pub watch_for: Vec<String>,
}

impl From<PlanWriterOutput> for PlanDraft {
    fn from(o: PlanWriterOutput) -> Self {
        Self {
            summary: o.summary,
            steps: o.steps,
            watch_for: o.watch_for,
        }
    }
}

/// A stored step back in draft form, for a `revise_plan` preview that keeps the base plan's
/// steps: the category and safety note are recomputed by validation.
pub fn step_to_draft(step: PlanStep) -> PlanStepDraft {
    PlanStepDraft {
        id: step.id,
        order: step.order,
        title: step.title,
        detail: step.detail,
        treatment_id: step.treatment_id,
        category: step.category,
        when: step.when,
        targets: step.targets,
        plant_id: step.plant_id,
        citations: step.citations,
        farm_citations: step
            .farm_citations
            .iter()
            .map(ToString::to_string)
            .collect(),
    }
}

/// What a plan may refer to, borrowed for one validation.
///
/// `'k` is the lifetime of the knowledge the caller loaded for this plan (the treatments of the
/// candidate problems, the profiles of the organisms seen, the neighbours from the current map).
/// Validation is synchronous and runs inside the caller's stack frame, so borrowing avoids
/// copying profiles that the plan writer's prompt already borrows too.
#[derive(Debug, Clone, Copy)]
pub struct ValidationContext<'k> {
    /// Every treatment a step may name.
    pub treatments: &'k [TreatmentSeed],
    /// The product list.
    pub products: &'k ProductCatalogue,
    /// Organism profiles, for the role of every organism a step may target.
    pub organisms: &'k [OrganismSeed],
    /// The problems the plan is about, for disease targets and the neighbour rule's `caused_by`.
    pub problems: &'k [ProblemSeed],
    /// The neighbours given to the plan writer (or computed from the current map for a preview).
    pub neighbours: &'k [PlanNeighbour],
    /// The scan found a beneficial organism.
    pub beneficials_seen: bool,
    /// The sources steps may cite.
    pub known_sources: &'k [SourceId],
    /// The farm records steps may cite ([connectors 7]): the plan writer's search hits, or for a
    /// preview the base plan's `farm_records`. Empty for a garden without farm records.
    ///
    /// [connectors 7]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#7-the-agent
    pub farm_records: &'k [FarmCitation],
}

/// Where a rule failed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PlanField {
    /// `summary`.
    Summary,
    /// `watch_for[i]`.
    WatchFor(usize),
    /// A step, by its id.
    Step(StepId),
}

/// Which rule of contracts 7's table (or a structural check) failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlanRule {
    /// No doses: the dose check failed.
    Dose,
    /// `treatment_id` names no known treatment.
    UnknownTreatment,
    /// A target is a beneficial organism.
    TargetsBeneficial,
    /// A target names no organism or problem the plan is about.
    UnknownTarget,
    /// A broad-spectrum product while beneficials were seen.
    BroadSpectrum,
    /// No citation.
    Uncited,
    /// A citation names no known source.
    UnknownSource,
    /// A farm citation names no farm record the plan was given.
    UnknownFarmRecord,
    /// A neighbour step that is not `monitoring`.
    NeighbourNotMonitoring,
    /// `plant_id` is not one of the neighbours.
    UnknownNeighbour,
    /// A neighbour step targets none of the neighbour's problems.
    NeighbourTarget,
    /// Two steps share an id.
    DuplicateStep,
    /// `recurring.every_days` is 0.
    InvalidWhen,
}

/// One broken rule, with the reason the model is given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleFailure {
    /// Where.
    pub at: PlanField,
    /// Which rule.
    pub rule: PlanRule,
    /// For the model and the logs. Never quotes more than the failing words.
    pub reason: String,
}

/// The validated plan.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanValidation {
    /// The summary, or [`FALLBACK_SUMMARY`] when it failed.
    pub summary: String,
    /// The steps that passed, renumbered `order` 1, 2, … in their original order, with
    /// `category` fixed and `safety_note` on every chemical step.
    pub steps: Vec<PlanStep>,
    /// The `watch_for` items that passed.
    pub watch_for: Vec<String>,
    /// Every source the passing steps cite, deduplicated, in first-cited order.
    pub citations: Vec<SourceId>,
    /// Every farm record the passing steps cite, resolved from the context's, deduplicated, in
    /// first-cited order: `ActionPlan.farm_records`.
    pub farm_records: Vec<FarmCitation>,
    /// Every broken rule.
    pub failures: Vec<RuleFailure>,
}

impl PlanValidation {
    /// No rule failed.
    pub fn is_clean(&self) -> bool {
        self.failures.is_empty()
    }

    /// The failures as one message for the plan writer's second round or the tool result.
    pub fn retry_message(&self) -> String {
        let mut out = String::from(
            "These parts of the plan break the rules. Rewrite them and keep everything else:\n",
        );
        for f in &self.failures {
            let at = match &f.at {
                PlanField::Summary => "summary".to_owned(),
                PlanField::WatchFor(i) => format!("watch_for[{i}]"),
                PlanField::Step(id) => format!("step {id}"),
            };
            let _ = writeln!(out, "- {at}: {}", f.reason);
        }
        out
    }
}

/// Validates a plan. Consumes the draft so passing steps move into the result without copying.
pub fn validate_plan(draft: PlanDraft, ctx: &ValidationContext<'_>) -> PlanValidation {
    let mut failures = Vec::new();

    let summary = match dose_check(&draft.summary, ctx.products) {
        Some(m) => {
            failures.push(RuleFailure {
                at: PlanField::Summary,
                rule: PlanRule::Dose,
                reason: m.reason(),
            });
            FALLBACK_SUMMARY.to_owned()
        }
        None => draft.summary,
    };

    let mut watch_for = Vec::with_capacity(draft.watch_for.len());
    for (i, item) in draft.watch_for.into_iter().enumerate() {
        if let Some(m) = dose_check(&item, ctx.products) {
            failures.push(RuleFailure {
                at: PlanField::WatchFor(i),
                rule: PlanRule::Dose,
                reason: m.reason(),
            });
        } else {
            watch_for.push(item);
        }
    }

    let index = Index::new(ctx);
    let mut seen_ids = HashSet::new();
    let mut steps = Vec::with_capacity(draft.steps.len());
    let mut drafts = draft.steps;
    drafts.sort_by_key(|s| s.order);
    for step in drafts {
        let first_use = seen_ids.insert(step.id.clone());
        match check_step(step, first_use, ctx, &index) {
            Ok(s) => steps.push(s),
            Err(mut f) => failures.append(&mut f),
        }
    }
    for (n, s) in steps.iter_mut().enumerate() {
        s.order = u32::try_from(n + 1).unwrap_or(u32::MAX);
    }

    let mut citations = Vec::new();
    let mut cited = HashSet::new();
    for c in steps.iter().flat_map(|s| &s.citations) {
        if cited.insert(c.clone()) {
            citations.push(c.clone());
        }
    }

    // Every farm id a passing step cites is one of the context's (checked per step), so each
    // resolves.
    let mut farm_records = Vec::new();
    let mut farm_cited = HashSet::new();
    for id in steps.iter().flat_map(|s| &s.farm_citations) {
        if farm_cited.insert(id)
            && let Some(c) = index.farm_records.get(id)
        {
            farm_records.push((*c).clone());
        }
    }

    PlanValidation {
        summary,
        steps,
        watch_for,
        citations,
        farm_records,
        failures,
    }
}

/// Lookups built once per validation; they borrow the context's slices (`'k`).
struct Index<'k> {
    treatments: HashMap<&'k TreatmentId, &'k TreatmentSeed>,
    organisms: HashMap<&'k OrganismId, &'k OrganismSeed>,
    problems: HashMap<&'k ProblemId, &'k ProblemSeed>,
    neighbours: HashMap<PlantId, &'k PlanNeighbour>,
    sources: HashSet<&'k SourceId>,
    farm_records: HashMap<&'k FarmRecordId, &'k FarmCitation>,
}

impl<'k> Index<'k> {
    fn new(ctx: &ValidationContext<'k>) -> Self {
        Self {
            treatments: ctx.treatments.iter().map(|t| (&t.id, t)).collect(),
            organisms: ctx.organisms.iter().map(|o| (&o.id, o)).collect(),
            problems: ctx.problems.iter().map(|p| (&p.id, p)).collect(),
            neighbours: ctx.neighbours.iter().map(|n| (n.plant_id, n)).collect(),
            sources: ctx.known_sources.iter().collect(),
            farm_records: ctx.farm_records.iter().map(|c| (&c.record_id, c)).collect(),
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "one block per rule, in the order of contracts 7's table"
)]
fn check_step(
    draft: PlanStepDraft,
    first_use: bool,
    ctx: &ValidationContext<'_>,
    index: &Index<'_>,
) -> Result<PlanStep, Vec<RuleFailure>> {
    let at = PlanField::Step(draft.id.clone());
    let mut failures = Vec::new();
    let mut fail = |rule: PlanRule, reason: String| {
        failures.push(RuleFailure {
            at: at.clone(),
            rule,
            reason,
        });
    };

    if !first_use {
        fail(
            PlanRule::DuplicateStep,
            "another step has the same id".to_owned(),
        );
    }
    if matches!(draft.when, PlanWhen::Recurring { every_days: 0, .. }) {
        fail(
            PlanRule::InvalidWhen,
            "every_days must be at least 1".to_owned(),
        );
    }

    // No doses: title and detail of every step, whatever its category.
    for text in [&draft.title, &draft.detail] {
        if let Some(m) = dose_check(text, ctx.products) {
            fail(PlanRule::Dose, m.reason());
        }
    }

    // The category comes from the knowledge base.
    let mut category = draft.category;
    if let Some(t) = &draft.treatment_id {
        match index.treatments.get(t) {
            Some(seed) => category = seed.kind.into(),
            None => fail(
                PlanRule::UnknownTreatment,
                format!("treatment {t} is not one of the treatments you were given"),
            ),
        }
    }

    // Products make a step chemical.
    let mentions: Vec<_> = ctx
        .products
        .mentions(&draft.title)
        .chain(ctx.products.mentions(&draft.detail))
        .collect();
    if !mentions.is_empty() {
        category = StepCategory::Chemical;
    }

    // Beneficials are never targeted; every target is one the plan is about.
    for target in &draft.targets {
        match target {
            PlanTarget::Organism(id) => match index.organisms.get(id).map(|o| o.role) {
                Some(SeedOrganismRole::Beneficial) => fail(
                    PlanRule::TargetsBeneficial,
                    format!("{id} is a beneficial organism and must never be targeted"),
                ),
                Some(_) => {}
                None => fail(
                    PlanRule::UnknownTarget,
                    format!("organism {id} is not one you were given"),
                ),
            },
            PlanTarget::Disease(id) => {
                if !index.problems.contains_key(id) {
                    fail(
                        PlanRule::UnknownTarget,
                        format!("problem {id} is not one you were given"),
                    );
                }
            }
        }
    }

    // Beneficials are not harmed by the way.
    if ctx.beneficials_seen
        && category == StepCategory::Chemical
        && let Some(p) = mentions.iter().find(|m| m.broad_spectrum)
    {
        fail(
            PlanRule::BroadSpectrum,
            format!(
                "\"{}\" is broad-spectrum and beneficial insects were seen; suggest a targeted option",
                p.name
            ),
        );
    }

    // Every step is cited, by a knowledge source: farm records are the farm's history, never
    // the advice ([connectors 7]).
    if draft.citations.is_empty() {
        let reason = if draft.farm_citations.is_empty() {
            "cite at least one source"
        } else {
            "farm records are not sources: cite at least one of the sources you were given too"
        };
        fail(PlanRule::Uncited, reason.to_owned());
    }
    for c in &draft.citations {
        if !index.sources.contains(c) {
            fail(
                PlanRule::UnknownSource,
                format!("source {c} is not one you were given"),
            );
        }
    }
    // Each farm record once, in the model's order; an id that doesn't parse is as unknown as
    // one the plan wasn't given.
    let mut farm_citations: Vec<FarmRecordId> = Vec::with_capacity(draft.farm_citations.len());
    for raw in &draft.farm_citations {
        match raw.parse::<FarmRecordId>() {
            Ok(id) if index.farm_records.contains_key(&id) => {
                if !farm_citations.contains(&id) {
                    farm_citations.push(id);
                }
            }
            _ => fail(
                PlanRule::UnknownFarmRecord,
                format!("farm record {raw} is not one you were given"),
            ),
        }
    }

    // Neighbour steps.
    if let Some(plant) = draft.plant_id {
        if category != StepCategory::Monitoring {
            fail(
                PlanRule::NeighbourNotMonitoring,
                "a step about a neighbour must be a monitoring step".to_owned(),
            );
        }
        match index.neighbours.get(&plant) {
            None => fail(
                PlanRule::UnknownNeighbour,
                format!("plant {plant} is not one of the neighbours you were given"),
            ),
            Some(n) => {
                if !targets_neighbour_problem(&draft.targets, n, index) {
                    fail(
                        PlanRule::NeighbourTarget,
                        format!(
                            "the step must target one of {}'s problems: {}",
                            n.name,
                            join(&n.problems)
                        ),
                    );
                }
            }
        }
    }

    if !failures.is_empty() {
        return Err(failures);
    }
    let safety_note = (category == StepCategory::Chemical).then(|| SAFETY_NOTE.to_owned());
    Ok(PlanStep {
        id: draft.id,
        order: draft.order,
        title: draft.title,
        detail: draft.detail,
        treatment_id: draft.treatment_id,
        category,
        when: draft.when,
        targets: draft.targets,
        plant_id: draft.plant_id,
        citations: draft.citations,
        farm_citations,
        safety_note,
    })
}

/// A neighbour step's targets include one of the neighbour's problems, or an organism one of
/// them is `caused_by`.
fn targets_neighbour_problem(
    targets: &[PlanTarget],
    neighbour: &PlanNeighbour,
    index: &Index<'_>,
) -> bool {
    targets.iter().any(|t| match t {
        PlanTarget::Disease(p) => neighbour.problems.contains(p),
        PlanTarget::Organism(o) => neighbour.problems.iter().any(|p| {
            index
                .problems
                .get(p)
                .is_some_and(|seed| seed.caused_by.contains(o))
        }),
    })
}

fn join(ids: &[ProblemId]) -> String {
    ids.iter()
        .map(ProblemId::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests;
