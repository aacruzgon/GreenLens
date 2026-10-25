//! Plans, tasks and plan trigger rules ([contracts 7], action plan and task; [contracts 9],
//! tasks; [database 4], plan writes and alert dedupe).

use crate::{
    authz::{Editor, Membership, Viewer},
    garden::tasks::{TaskQuery, UpdateTaskRequest},
    id::{GardenId, PlanId, PlanTriggerId, PlantId, ScanId, StepId, TaskId, UserId},
    ports::{StepClaim, StoreError},
    records::{
        plan::{ActionPlan, PlanReason, TriggerKind},
        task::{AlertKey, Repeat, Task, TaskSource, TaskTrigger},
    },
    wire::{LocalDate, Page, PageRequest, Timestamp},
};

/// A task to create. Every write that creates a task with a plant checks, in its transaction,
/// that the plant is live in the garden ([contracts 7], tasks only on live plants).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTask {
    /// Its id.
    pub id: TaskId,
    /// The plant; checked live.
    pub plant: Option<PlantId>,
    /// The plan version.
    pub plan: Option<PlanId>,
    /// The step.
    pub plan_step: Option<StepId>,
    /// The scan (a jar-test task); checked live, of the garden and of the plant.
    pub scan: Option<ScanId>,
    /// Title.
    pub title: String,
    /// Detail.
    pub detail: Option<String>,
    /// Due date.
    pub due: Option<LocalDate>,
    /// Repeat.
    pub repeat: Option<Repeat>,
    /// Who made it.
    pub source: TaskSource,
    /// The rule that fired.
    pub trigger: Option<TaskTrigger>,
    /// The dedupe key, for alerts and plan triggers.
    pub alert_key: Option<AlertKey>,
    /// The user who made it.
    pub created_by: Option<UserId>,
    /// Now.
    pub created_at: Timestamp,
}

/// A plan trigger rule an `on_trigger` step registers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPlanTrigger {
    /// Its id.
    pub id: PlanTriggerId,
    /// The plant its tasks go to: the step's neighbour or the plan's plant; checked live.
    pub plant: PlantId,
    /// The step.
    pub step: StepId,
    /// The rule.
    pub trigger: TriggerKind,
    /// The last date it fires.
    pub until: Option<LocalDate>,
}

/// A plan version to store with its compare-and-set ([database 4], plan writes): the write
/// goes through only if `scan.plan` still equals `expected` (`None` for a first plan); a loser
/// gets `Conflict::StalePlan` and nothing is stored. The same transaction dismisses the
/// superseded plan's open tasks (`plan_superseded`), deactivates its trigger rules, and creates
/// the new tasks and rules, skipping any whose plant is gone.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanWrite {
    /// The plan as it will be read.
    pub plan: ActionPlan,
    /// Why it was written.
    pub reason: PlanReason,
    /// The plan it replaces.
    pub expected: Option<PlanId>,
    /// Tasks of `now` and `by_date` and `recurring` steps.
    pub tasks: Vec<NewTask>,
    /// Rules of `on_trigger` steps.
    pub triggers: Vec<NewPlanTrigger>,
}

/// An active plan trigger rule, for the scheduler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanTriggerRule {
    /// The rule.
    pub id: PlanTriggerId,
    /// Its plan version.
    pub plan: PlanId,
    /// Its garden.
    pub garden: GardenId,
    /// The plant its tasks go to.
    pub plant: PlantId,
    /// The step.
    pub step: StepId,
    /// The rule.
    pub trigger: TriggerKind,
    /// The last date it fires.
    pub until: Option<LocalDate>,
    /// The step's title, read from its plan: the task's title when the rule fires.
    pub title: String,
    /// The step's detail: the task's detail.
    pub detail: String,
}

/// The `action_plan`, `task` and `plan_trigger` tables.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait PlanRepo: Send + Sync {
    /// The scan's current plan.
    async fn current_plan(&self, scan: ScanId) -> Result<Option<ActionPlan>, StoreError>;

    /// A plan version of the garden.
    async fn plan(
        &self,
        member: Membership<Viewer>,
        plan: PlanId,
    ) -> Result<ActionPlan, StoreError>;

    /// The `plan` step's write, under its claim.
    async fn write_plan(
        &self,
        claim: &StepClaim,
        write: PlanWrite,
    ) -> Result<ActionPlan, StoreError>;

    /// A confirmed chat revision, stored exactly as previewed: the same compare-and-set with
    /// `expected = base_plan_id`, never retried ([contracts 7], re-planning). Runs on the
    /// scan's serialised writer.
    async fn write_revision(
        &self,
        editor: Membership<Editor>,
        write: PlanWrite,
    ) -> Result<ActionPlan, StoreError>;

    /// `GET /tasks`: by `due` (none last), then `created_at` (index `task_garden`).
    async fn tasks(
        &self,
        member: Membership<Viewer>,
        query: TaskQuery,
        page: PageRequest,
    ) -> Result<Page<Task>, StoreError>;

    /// A task of the garden.
    async fn task(&self, member: Membership<Viewer>, task: TaskId) -> Result<Task, StoreError>;

    /// `POST /tasks` and a confirmed `create_task`: a dead plant or scan is `NotFound`.
    async fn create_task(
        &self,
        editor: Membership<Editor>,
        task: NewTask,
    ) -> Result<Task, StoreError>;

    /// `PATCH /tasks/{id}`: completing a repeating task creates the next occurrence in the same
    /// transaction (`next` is its new id and due date, or `None` past `until`).
    async fn update_task(
        &self,
        editor: Membership<Editor>,
        task: TaskId,
        patch: UpdateTaskRequest,
        next: Option<NewTask>,
        now: Timestamp,
    ) -> Result<Task, StoreError>;

    /// `DELETE /tasks/{id}`: `user` and `chat` tasks only; others are
    /// `Rejected(InvalidState)`.
    async fn delete_task(&self, editor: Membership<Editor>, task: TaskId)
    -> Result<(), StoreError>;

    /// The scheduler's alert and plan-trigger tasks, one per spell ([database 4], alert
    /// dedupe): `task.alert_key` (required) names the spell (`AlertKey::spell`) and its first
    /// day, and `until` its last fired day. When a task of the same spell was seen up to the
    /// day before the first day or later, or has this very key, the spell continues: its
    /// `spell_until` moves on to `until` and nothing is created (`Ok(None)`), whatever the
    /// task's state. Otherwise the task is created with `spell_until = until`. A duplicate
    /// key from a racing run, a dead plant or a garden being deleted also yield `None`.
    async fn create_alert_task(
        &self,
        garden: GardenId,
        task: NewTask,
        until: LocalDate,
    ) -> Result<Option<Task>, StoreError>;

    /// Active plan trigger rules of these gardens (index `plan_trigger_active`).
    async fn active_triggers(
        &self,
        gardens: &[GardenId],
    ) -> Result<Vec<PlanTriggerRule>, StoreError>;
}
