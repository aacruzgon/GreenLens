//! `ActionPlan` and its steps ([contracts 7], action plan), plus the plan writer's output, the
//! fixed shape the agent asks the LLM for (JSON Schema through `schemars`, [contracts 1]).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    id::{
        DiagnosisId, OrganismId, PendingActionId, PlanId, PlantId, ProblemId, ScanId, SourceId,
        StepId, TaskId, TreatmentId,
    },
    records::diagnosis::Citation,
    scan::Question,
    wire::{LocalDate, Timestamp},
};

/// The fixed note the server adds to every `chemical` step; the plan writer never writes it
/// ([contracts 7], plan validation).
pub const SAFETY_NOTE: &str =
    "Use only a product registered for this use where you live, and follow its label exactly.";

/// `TriggerKind` ([contracts 7]): one vocabulary for forecast alerts, plan triggers and
/// `Task.trigger`.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    ToSchema,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    /// Frost forecast.
    Frost,
    /// Heat forecast.
    Heat,
    /// A humid stretch: fungal risk.
    HumidStretch,
    /// Heavy rain forecast.
    HeavyRain,
    /// After the next rain.
    AfterRain,
    /// Dry soil: a watering reminder from soil moisture and ET₀.
    DrySoil,
}

impl TriggerKind {
    /// The wire name, used in alert keys.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Frost => "frost",
            Self::Heat => "heat",
            Self::HumidStretch => "humid_stretch",
            Self::HeavyRain => "heavy_rain",
            Self::AfterRain => "after_rain",
            Self::DrySoil => "dry_soil",
        }
    }
}

/// `PlanWhen` ([contracts 7]), tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlanWhen {
    /// A task due today.
    Now,
    /// A task due on that date.
    ByDate {
        /// The due date.
        due: LocalDate,
    },
    /// A repeating task: "check for new spots every 3 days".
    Recurring {
        /// At least 1.
        every_days: u32,
        /// The last date a task is created for.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schema(nullable = false)]
        until: Option<LocalDate>,
    },
    /// No task yet: a plan trigger rule creates it when it fires.
    OnTrigger {
        /// The rule.
        trigger: TriggerKind,
        /// The last date the rule fires.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schema(nullable = false)]
        until: Option<LocalDate>,
    },
}

/// `PlanStep.category`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum StepCategory {
    /// Cultural.
    Cultural,
    /// Biological.
    Biological,
    /// Mechanical.
    Mechanical,
    /// Chemical: always carries [`SAFETY_NOTE`].
    Chemical,
    /// Monitoring; the only category a neighbour step may have.
    Monitoring,
    /// Humane wildlife deterrent.
    WildlifeDeterrent,
}

/// `PlanStep.targets[]`: what the step acts on, `{ table, id }`.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, schemars::JsonSchema,
)]
#[serde(tag = "table", content = "id", rename_all = "snake_case")]
pub enum PlanTarget {
    /// A problem.
    Disease(ProblemId),
    /// An organism; never a beneficial one.
    Organism(OrganismId),
}

/// `PlanStep` ([contracts 7]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PlanStep {
    /// Unique within the plan only.
    pub id: StepId,
    /// Position in the plan.
    pub order: u32,
    /// "Remove the lowest infected leaves".
    pub title: String,
    /// Detail.
    pub detail: String,
    /// The knowledge-base treatment the step applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub treatment_id: Option<TreatmentId>,
    /// From the treatment's `kind` when there is one; `chemical` when a product is named.
    pub category: StepCategory,
    /// When.
    pub when: PlanWhen,
    /// What it acts on; may be empty.
    pub targets: Vec<PlanTarget>,
    /// A neighbour this monitoring step is about; its task goes to that plant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plant_id: Option<PlantId>,
    /// Source ids; at least one.
    pub citations: Vec<SourceId>,
    /// Added by the server to every chemical step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub safety_note: Option<String>,
}

/// `ActionPlan.constraints[]`: from confirmed chat revisions; carried into every later version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PlanConstraint {
    /// "No copper products".
    pub text: String,
    /// The confirmed card; an id, not a link (the card may be deleted with its conversation).
    pub from_pending_action_id: PendingActionId,
    /// When it was confirmed.
    pub created_at: Timestamp,
}

/// `ActionPlan` ([contracts 7]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ActionPlan {
    /// The plan version.
    pub id: PlanId,
    /// Its scan.
    pub scan_id: ScanId,
    /// Its plant.
    pub plant_id: PlantId,
    /// 1, 2, …; unique per scan.
    pub version: u32,
    /// The version it replaced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub supersedes: Option<PlanId>,
    /// True exactly when the diagnosis it was written from is `uncertain`.
    pub provisional: bool,
    /// The diagnosis this version was written from.
    pub diagnosis_id: DiagnosisId,
    /// The question still open when this version was written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub open_question: Option<Question>,
    /// From confirmed chat revisions.
    pub constraints: Vec<PlanConstraint>,
    /// Summary.
    pub summary: String,
    /// Steps.
    pub steps: Vec<PlanStep>,
    /// "New spots on upper leaves within 5 days".
    pub watch_for: Vec<String>,
    /// Every source the steps cite, resolved.
    pub citations: Vec<Citation>,
    /// The tasks the steps created.
    pub task_ids: Vec<TaskId>,
    /// The LLM that wrote it.
    pub llm_model: String,
    /// The prompt version.
    pub prompt_version: String,
    /// When it was stored.
    pub created_at: Timestamp,
}

/// Why a plan version was written: the stored `action_plan.reason` ([database 3]), which adds
/// `initial` to the re-planning reasons of contracts 7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanReason {
    /// A run's first plan.
    Initial,
    /// A late answer or photo.
    LateAnswer,
    /// A species-change run.
    SpeciesChanged,
    /// A guided check changed the texture.
    GuidedCheck,
    /// A confirmed `revise_plan` card.
    ChatRevision,
}

impl PlanReason {
    /// The `plan_superseded.reason`, for every reason but `initial`.
    pub const fn supersede_reason(self) -> Option<crate::events::SupersedeReason> {
        use crate::events::SupersedeReason as S;
        match self {
            Self::Initial => None,
            Self::LateAnswer => Some(S::LateAnswer),
            Self::SpeciesChanged => Some(S::SpeciesChanged),
            Self::GuidedCheck => Some(S::GuidedCheck),
            Self::ChatRevision => Some(S::ChatRevision),
        }
    }
}

/// A neighbour given to the plan writer ([contracts 7], neighbours for the plan writer): up to
/// 5, nearest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PlanNeighbour {
    /// The neighbour.
    pub plant_id: PlantId,
    /// Its name.
    pub name: String,
    /// Its species' common name.
    pub species_name: String,
    /// Centre to centre, metres.
    pub distance_m: f64,
    /// The spreading problems its species is `susceptible_to`.
    pub problems: Vec<ProblemId>,
}

/// One step as the plan writer writes it: a `PlanStep` without `safety_note`, which the server
/// adds. The validator then fixes `category` from the treatment and the product list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(extend("additionalProperties" = false))]
pub struct PlanStepDraft {
    /// Unique within the plan.
    pub id: StepId,
    /// Position.
    pub order: u32,
    /// Title; dose-checked.
    pub title: String,
    /// Detail; dose-checked.
    pub detail: String,
    /// The treatment applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub treatment_id: Option<TreatmentId>,
    /// Proposed category.
    pub category: StepCategory,
    /// When.
    pub when: PlanWhen,
    /// Targets.
    pub targets: Vec<PlanTarget>,
    /// A neighbour, for monitoring steps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plant_id: Option<PlantId>,
    /// At least one source.
    pub citations: Vec<SourceId>,
}

/// The plan writer's structured output: the fixed shape whose JSON Schema the agent sends to
/// the LLM API ([contracts 1], source of truth). Not in contracts 7 as a type: it is
/// `ActionPlan`'s model-written fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(extend("additionalProperties" = false))]
pub struct PlanWriterOutput {
    /// Summary; dose-checked.
    pub summary: String,
    /// Steps.
    pub steps: Vec<PlanStepDraft>,
    /// What to watch for; dose-checked.
    pub watch_for: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_writer_schema_is_strict_but_serde_is_tolerant() {
        let schema = serde_json::to_value(schemars::schema_for!(PlanWriterOutput)).unwrap();
        assert_eq!(schema["additionalProperties"], serde_json::json!(false));
        let out: PlanWriterOutput = serde_json::from_value(serde_json::json!({
            "summary": "Remove spotted leaves.",
            "steps": [{
                "id": "s1", "order": 1, "title": "Remove the lowest infected leaves",
                "detail": "Bag them; don't compost.", "category": "cultural",
                "when": {"kind": "now"}, "targets": [{"table": "disease", "id": "tomato_early_blight"}],
                "citations": ["example_extension_tomato_diseases"], "extra": true
            }],
            "watch_for": ["New spots on upper leaves within 5 days"]
        }))
        .unwrap();
        assert_eq!(out.steps[0].when, PlanWhen::Now);
    }
}
