//! Plans, tasks and plan trigger rules ([contracts 7], [contracts 9] tasks; [database 4], plan
//! writes ✓ and alert dedupe ✓).

use chrono::{DateTime, Utc};
use domain::{
    authz::{Editor, Membership, Viewer},
    garden::tasks::{TaskQuery, UpdateTaskRequest},
    id::{GardenId, PlanId, ScanId, TaskId},
    ports::{
        Conflict, NewPlanTrigger, NewTask, PlanRepo, PlanTriggerRule, PlanWrite, StepClaim,
        StoreError,
    },
    records::{plan::ActionPlan, task::Task},
    wire::{LocalDate, Page, PageRequest, Timestamp},
};
use surrealdb::types::{RecordId, SurrealValue};

use crate::{
    SurrealStore,
    error::{DbFailure, RowError, take_failed, thrown},
    ids::{from_rid, opt_rid, rid},
    read::{page as page_of, take_all, take_one},
    rows::{
        DismissedReasonRow, PipelineStepRow, PlanReasonRow, TaskSourceRow, TriggerKindRow, dt,
        to_json,
    },
    tx::run_tx,
};

/// The statements.
pub mod sql {
    use crate::fragments::{claim_check, handover, plan_cas};

    /// The projection of a `task` into `Task` (`alert_key` stays server-side).
    macro_rules! task_fields {
        () => {
            "id, garden AS garden_id, plant AS plant_id, plan AS plan_id, plan_step_id, scan AS scan_id, title,
  detail, due, repeat, source, trigger, done_at, dismissed_at, dismissed_reason, created_at"
        };
    }

    /// The `plan` step's write under its claim: the re-check, the compare-and-set with the
    /// supersede and the new tasks and rules, `diagnosed → planned`, and the hand-over
    /// (release, or `score` when input arrived meanwhile). The last statement before `COMMIT`
    /// returns the next step.
    pub const COMMIT_PLAN: &str = concat!(
        "BEGIN;\n",
        claim_check!(),
        plan_cas!(),
        "IF $s.status = 'diagnosed' { UPDATE $scan SET status = 'planned'; };\n",
        handover!(),
        "COMMIT;"
    );

    /// A confirmed chat revision: the same compare-and-set with `$expected = base_plan_id`,
    /// with no claim and never retried on `stale_plan` ([contracts 7], re-planning).
    pub const WRITE_REVISION: &str = concat!("BEGIN;\n", plan_cas!(), "COMMIT;");

    /// The scan's current plan after a lost compare-and-set, for `StalePlan { current }`.
    pub const CURRENT_PLAN_ID: &str = "SELECT VALUE plan FROM ONLY $scan;";
    /// The task ids a plan version created (index `task_plan`).
    pub const PLAN_TASK_IDS: &str = "SELECT VALUE id FROM task WHERE plan = $plan;";
    /// The scan's garden, for the plan row.
    pub const SCAN_GARDEN: &str = "SELECT VALUE garden FROM ONLY $scan;";

    /// The scheduler's alert and plan-trigger tasks ([database 4], alert dedupe ✓): the
    /// unique `task_alert` index refuses a second task with the same key, and the plant must be
    /// live in the garden (contracts 7, tasks only on live plants).
    pub const CREATE_ALERT_TASK: &str = concat!(
        "BEGIN;
LET $p = $task.row.plant;
IF $p != NONE AND array::len(SELECT VALUE id FROM $p WHERE garden = $garden AND deleting_at = NONE) = 0 { THROW 'not_found' };
CREATE $task.id CONTENT $task.row;
SELECT ", task_fields!(), " FROM ONLY $task.id;
COMMIT;"
    );
    /// The index of the created task in [`CREATE_ALERT_TASK`].
    pub const CREATE_ALERT_TASK_RESULT: usize = 4;

    /// The scan's current plan id (the plan itself is read by `load_plan`).
    pub const CURRENT_PLAN: &str = "SELECT VALUE plan FROM ONLY $scan WHERE deleting_at = NONE;";
    /// A plan version's garden, for the membership scope.
    pub const PLAN_GARDEN: &str = "SELECT VALUE garden FROM ONLY $plan;";

    /// `GET /tasks`: by `due` (tasks without one last), then `created_at`, paged by a cursor on
    /// that key and the id ([database 5]: never `START`).
    pub const TASKS: &str = concat!(
        "SELECT ", task_fields!(), ", due ?? '9999-12-31' AS due_key FROM task
WHERE garden = $garden AND ($state = NONE OR state = $state) AND ($source = NONE OR source = $source)
  AND plant.deleting_at = NONE AND ($after = NONE OR [due ?? '9999-12-31', created_at, id] > $after)
ORDER BY due_key, created_at, id LIMIT $limit;"
    );
    /// A task of the garden.
    pub const TASK: &str = concat!(
        "SELECT ",
        task_fields!(),
        " FROM ONLY $task WHERE garden = $garden;"
    );
    /// The cursor key of a task, for the next page.
    pub const TASK_CURSOR: &str =
        "SELECT VALUE [due ?? '9999-12-31', created_at, id] FROM ONLY $task;";
    /// `POST /tasks` and a confirmed `create_task`: the plant and the scan checked live in the
    /// same transaction ([database 4], deletion: "`POST /tasks` with a `scan_id` checks the
    /// scan in the same transaction" ✓).
    pub const CREATE_TASK: &str = concat!(
        "BEGIN;
LET $p = $task.row.plant;
IF $p != NONE AND array::len(SELECT VALUE id FROM $p WHERE garden = $garden AND deleting_at = NONE) = 0 { THROW 'not_found' };
LET $sc = $task.row.scan;
IF $sc != NONE AND array::len(SELECT VALUE id FROM $sc WHERE garden = $garden AND deleting_at = NONE AND ($p = NONE OR plant = $p)) = 0 { THROW 'not_found' };
CREATE $task.id CONTENT $task.row;
SELECT ", task_fields!(), " FROM ONLY $task.id;
COMMIT;"
    );
    /// `PATCH /tasks/{id}`: only `user` and `chat` tasks change their text, date or repeat;
    /// completing a repeating task creates the next one in the same transaction.
    pub const UPDATE_TASK: &str = concat!(
        "BEGIN;
LET $t = (SELECT source FROM ONLY $task WHERE garden = $garden AND plant.deleting_at = NONE);
IF $t = NONE { THROW 'not_found' };
IF ($title != NONE OR $detail != NONE OR $due != NONE OR $repeat != NONE) AND !($t.source IN ['user', 'chat']) { THROW 'invalid_state' };
UPDATE $task SET done_at = IF $done = true { done_at ?? $now } ELSE IF $done = false { NONE } ELSE { done_at },
  dismissed_at = IF $dismissed = true { dismissed_at ?? $now } ELSE IF $dismissed = false { NONE } ELSE { dismissed_at },
  dismissed_reason = IF $dismissed = true { 'user' } ELSE IF $dismissed = false { NONE } ELSE { dismissed_reason },
  title = $title ?? title, detail = $detail ?? detail, due = $due ?? due, repeat = $repeat ?? repeat;
IF $next != NONE { CREATE $next.id CONTENT $next.row; };
SELECT ", task_fields!(), " FROM ONLY $task;
COMMIT;"
    );
    /// `DELETE /tasks/{id}`: `user` and `chat` tasks only.
    pub const DELETE_TASK: &str = "BEGIN;
LET $src = (SELECT VALUE source FROM ONLY $task WHERE garden = $garden);
IF $src = NONE { THROW 'not_found' };
IF !($src IN ['user', 'chat']) { THROW 'invalid_state' };
DELETE $task;
COMMIT;";
    /// Active rules of these gardens (index `plan_trigger_active`) whose plant is live, with
    /// their plan's steps for the task text.
    pub const ACTIVE_TRIGGERS: &str = "SELECT id, plan, garden, plant, step_id, trigger, until, plan.steps AS steps FROM plan_trigger
WHERE active = true AND garden IN $gardens AND plant.deleting_at = NONE;";
}

/// The `action_plan` row ([database 3]); `steps` and `citations` are `FLEXIBLE` JSON validated
/// in Rust, `open_question` a link, `task_ids` computed when read.
#[derive(Debug, Clone, SurrealValue)]
pub struct PlanRow {
    /// Its scan.
    pub scan: RecordId,
    /// Its plant.
    pub plant: RecordId,
    /// Its garden.
    pub garden: RecordId,
    /// 1, 2, … per scan (unique with `scan`).
    pub version: i64,
    /// The version it replaces.
    pub supersedes: Option<RecordId>,
    /// Why it was written.
    pub reason: PlanReasonRow,
    /// True exactly when its diagnosis is `uncertain`.
    pub provisional: bool,
    /// The diagnosis it was written from.
    pub diagnosis: RecordId,
    /// The question open when it was written.
    pub open_question: Option<RecordId>,
    /// From confirmed chat revisions.
    pub constraints: Vec<ConstraintRow>,
    /// The summary.
    pub summary: String,
    /// `PlanStep[]`.
    pub steps: Vec<serde_json::Value>,
    /// What to watch for.
    pub watch_for: Vec<String>,
    /// `Citation[]`.
    pub citations: Vec<serde_json::Value>,
    /// `FarmCitation[]`, every farm record the steps cite ([connectors 7]). `DEFAULT []`
    /// applies only on write, so a plan stored before connectors has none: it reads as empty
    /// here and in `ActionPlan`.
    ///
    /// [connectors 7]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#7-the-agent
    #[surreal(default)]
    pub farm_records: Vec<serde_json::Value>,
    /// The plan writer's model.
    pub llm_model: String,
    /// The prompt version.
    pub prompt_version: String,
    /// When it was written.
    pub created_at: DateTime<Utc>,
}

/// `action_plan.constraints[*]`.
#[derive(Debug, Clone, SurrealValue)]
pub struct ConstraintRow {
    /// "No copper products".
    pub text: String,
    /// The card's id, a plain string: the card may be deleted with its conversation.
    pub from_pending_action: String,
    /// When it was confirmed.
    pub created_at: DateTime<Utc>,
}

/// `$plan` of [`plan_cas!`](crate::fragments).
#[derive(Debug, Clone, SurrealValue)]
pub struct PlanWriteRow {
    /// `action_plan:<ulid>`.
    pub id: RecordId,
    /// The row.
    pub row: PlanRow,
}

impl PlanWriteRow {
    /// The row of a plan version in `garden` written for `reason`.
    pub fn new(write: &PlanWrite, garden: RecordId) -> Result<Self, RowError> {
        let plan = &write.plan;
        Ok(Self {
            id: rid(&plan.id),
            row: PlanRow {
                scan: rid(&plan.scan_id),
                plant: rid(&plan.plant_id),
                garden,
                version: i64::from(plan.version),
                supersedes: opt_rid(plan.supersedes.as_ref()),
                reason: write.reason.into(),
                provisional: plan.provisional,
                diagnosis: rid(&plan.diagnosis_id),
                open_question: plan.open_question.as_ref().map(|q| rid(&q.id)),
                constraints: plan
                    .constraints
                    .iter()
                    .map(|c| ConstraintRow {
                        text: c.text.clone(),
                        from_pending_action: c.from_pending_action_id.to_string(),
                        created_at: dt(c.created_at),
                    })
                    .collect(),
                summary: plan.summary.clone(),
                steps: plan
                    .steps
                    .iter()
                    .map(|s| to_json("steps", s))
                    .collect::<Result<_, _>>()?,
                watch_for: plan.watch_for.clone(),
                citations: plan
                    .citations
                    .iter()
                    .map(|c| to_json("citations", c))
                    .collect::<Result<_, _>>()?,
                farm_records: plan
                    .farm_records
                    .iter()
                    .map(|c| to_json("farm_records", c))
                    .collect::<Result<_, _>>()?,
                llm_model: plan.llm_model.clone(),
                prompt_version: plan.prompt_version.clone(),
                created_at: dt(plan.created_at),
            },
        })
    }
}

/// `task.repeat`.
#[derive(Debug, Clone, SurrealValue)]
pub struct RepeatRow {
    /// Days between occurrences.
    pub every_days: i64,
    /// The last date, `YYYY-MM-DD`.
    pub until: Option<String>,
}

/// `task.trigger`.
#[derive(Debug, Clone, SurrealValue)]
pub struct TaskTriggerRow {
    /// The rule that fired.
    pub kind: TriggerKindRow,
    /// The local date it fired for.
    pub date: String,
}

/// A `task` row ([database 3]); `state` is computed by the schema.
#[derive(Debug, Clone, SurrealValue)]
pub struct TaskRow {
    /// Its garden.
    pub garden: RecordId,
    /// Its plant.
    pub plant: Option<RecordId>,
    /// Its plan version.
    pub plan: Option<RecordId>,
    /// The plan step.
    pub plan_step_id: Option<String>,
    /// A guided-check task's scan.
    pub scan: Option<RecordId>,
    /// Title.
    pub title: String,
    /// Detail.
    pub detail: Option<String>,
    /// Due date, `YYYY-MM-DD`.
    pub due: Option<String>,
    /// Repeat.
    pub repeat: Option<RepeatRow>,
    /// Who made it.
    pub source: TaskSourceRow,
    /// The rule that fired.
    pub trigger: Option<TaskTriggerRow>,
    /// The dedupe key, server-side only.
    pub alert_key: Option<String>,
    /// The user who made it; `NONE` once deleted.
    pub created_by: Option<RecordId>,
    /// When done.
    pub done_at: Option<DateTime<Utc>>,
    /// When dismissed.
    pub dismissed_at: Option<DateTime<Utc>>,
    /// Why dismissed.
    pub dismissed_reason: Option<DismissedReasonRow>,
    /// When created.
    pub created_at: DateTime<Utc>,
}

/// A task with its record id, as written.
#[derive(Debug, Clone, SurrealValue)]
pub struct TaskWriteRow {
    /// `task:<ulid>`.
    pub id: RecordId,
    /// The row.
    pub row: TaskRow,
}

impl TaskWriteRow {
    /// The row of a new task in `garden`.
    pub fn new(task: &NewTask, garden: GardenId) -> Self {
        Self {
            id: rid(&task.id),
            row: TaskRow {
                garden: rid(&garden),
                plant: opt_rid(task.plant.as_ref()),
                plan: opt_rid(task.plan.as_ref()),
                plan_step_id: task.plan_step.as_ref().map(ToString::to_string),
                scan: opt_rid(task.scan.as_ref()),
                title: task.title.clone(),
                detail: task.detail.clone(),
                due: task.due.map(|d| d.to_string()),
                repeat: task.repeat.as_ref().map(|r| RepeatRow {
                    every_days: i64::from(r.every_days),
                    until: r.until.map(|d| d.to_string()),
                }),
                source: task.source.into(),
                trigger: task.trigger.as_ref().map(|t| TaskTriggerRow {
                    kind: t.kind.into(),
                    date: t.date.to_string(),
                }),
                alert_key: task.alert_key.as_ref().map(ToString::to_string),
                created_by: opt_rid(task.created_by.as_ref()),
                done_at: None,
                dismissed_at: None,
                dismissed_reason: None,
                created_at: dt(task.created_at),
            },
        }
    }
}

fn local_date(field: &'static str, s: &str) -> Result<LocalDate, RowError> {
    s.parse().map_err(|_| RowError::BadField {
        field,
        problem: format!("{s:?} is not YYYY-MM-DD"),
    })
}

/// A rule as [`sql::ACTIVE_TRIGGERS`] reads it.
#[derive(Debug, Clone, SurrealValue)]
pub struct TriggerReadRow {
    /// `plan_trigger:<ulid>`.
    pub id: RecordId,
    /// Its plan version.
    pub plan: RecordId,
    /// Its garden.
    pub garden: RecordId,
    /// The plant its tasks go to.
    pub plant: RecordId,
    /// The step.
    pub step_id: String,
    /// The rule.
    pub trigger: TriggerKindRow,
    /// The last date.
    pub until: Option<String>,
    /// The plan's steps, for the task text.
    pub steps: Vec<serde_json::Value>,
}

/// `invalid_state` as `Rejected(InvalidState)`.
fn rejected_state(f: DbFailure, what: &'static str) -> StoreError {
    if f.thrown() == Some(thrown::INVALID_STATE) {
        StoreError::Rejected(domain::error::DomainError::InvalidState(
            domain::error::InvalidStateDetails::default(),
        ))
    } else {
        f.into_store(what)
    }
}

/// A `plan_trigger` row.
#[derive(Debug, Clone, SurrealValue)]
pub struct TriggerRow {
    /// The plan version.
    pub plan: RecordId,
    /// Its garden.
    pub garden: RecordId,
    /// The plant its tasks go to.
    pub plant: RecordId,
    /// The step.
    pub step_id: String,
    /// The rule.
    pub trigger: TriggerKindRow,
    /// The last date it fires.
    pub until: Option<String>,
}

/// A rule with its record id, as written.
#[derive(Debug, Clone, SurrealValue)]
pub struct TriggerWriteRow {
    /// `plan_trigger:<ulid>`.
    pub id: RecordId,
    /// The row.
    pub row: TriggerRow,
}

impl TriggerWriteRow {
    /// The row of a rule of `plan` in `garden`.
    pub fn new(rule: &NewPlanTrigger, plan: PlanId, garden: RecordId) -> Self {
        Self {
            id: rid(&rule.id),
            row: TriggerRow {
                plan: rid(&plan),
                garden,
                plant: rid(&rule.plant),
                step_id: rule.step.to_string(),
                trigger: rule.trigger.into(),
                until: rule.until.map(|d| d.to_string()),
            },
        }
    }
}

/// The binds every plan compare-and-set shares.
pub(crate) struct PlanBinds {
    pub(crate) plan: PlanWriteRow,
    pub(crate) expected: Option<RecordId>,
    pub(crate) tasks: Vec<TaskWriteRow>,
    pub(crate) triggers: Vec<TriggerWriteRow>,
}

impl SurrealStore {
    /// The scan's garden (it never changes once the scan exists).
    pub(crate) async fn scan_garden(&self, scan: ScanId) -> Result<GardenId, StoreError> {
        const WHAT: &str = "scan's garden";
        let mut results = run_tx(&self.db, sql::SCAN_GARDEN, |q| q.bind(("scan", rid(&scan))))
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let garden: Option<RecordId> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        from_rid(&garden.ok_or(StoreError::NotFound)?).map_err(|e| e.into_store(WHAT))
    }

    /// The rows of a plan write.
    pub(crate) fn plan_binds(write: &PlanWrite, garden: GardenId) -> Result<PlanBinds, RowError> {
        let garden_rid = rid(&garden);
        Ok(PlanBinds {
            plan: PlanWriteRow::new(write, garden_rid.clone())?,
            expected: opt_rid(write.expected.as_ref()),
            tasks: write
                .tasks
                .iter()
                .map(|t| TaskWriteRow::new(t, garden))
                .collect(),
            triggers: write
                .triggers
                .iter()
                .map(|r| TriggerWriteRow::new(r, write.plan.id, garden_rid.clone()))
                .collect(),
        })
    }

    /// Maps a failed plan write: `stale_plan` becomes `StalePlan` with the plan that won.
    pub(crate) async fn plan_failure(
        &self,
        scan: ScanId,
        failure: DbFailure,
        what: &'static str,
    ) -> StoreError {
        // The THROW, or the unique `action_plan (scan, version)` backstop when two writers both
        // create the same version ([database 4] ✓): the CREATE fails before the THROW runs.
        let lost = failure.thrown() == Some(thrown::STALE_PLAN)
            || matches!(&failure, DbFailure::UniqueIndex { index, .. } if index == "action_plan_version");
        if !lost {
            return failure.into_store(what);
        }
        let current = async {
            let mut results = run_tx(&self.db, sql::CURRENT_PLAN_ID, |q| {
                q.bind(("scan", rid(&scan)))
            })
            .await
            .ok()?;
            let plan: Option<RecordId> = results.take(0).ok()?;
            plan.and_then(|p| from_rid::<PlanId>(&p).ok())
        }
        .await;
        StoreError::Conflict(Conflict::StalePlan { current })
    }

    /// The plan as written, with the task ids it created.
    pub(crate) async fn written_plan(
        &self,
        mut plan: ActionPlan,
    ) -> Result<ActionPlan, StoreError> {
        const WHAT: &str = "plan task ids";
        let mut results = run_tx(&self.db, sql::PLAN_TASK_IDS, |q| {
            q.bind(("plan", rid(&plan.id)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let ids: Vec<RecordId> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        plan.task_ids = ids
            .iter()
            .map(from_rid::<TaskId>)
            .collect::<Result<_, _>>()
            .map_err(|e| e.into_store(WHAT))?;
        Ok(plan)
    }

    /// [`sql::COMMIT_PLAN`]: returns the next step's name (`NONE` when released).
    pub(crate) async fn commit_plan(
        &self,
        claim: &StepClaim,
        write: &PlanWrite,
        now: Timestamp,
    ) -> Result<Option<PipelineStepRow>, StoreError> {
        const WHAT: &str = "write plan";
        let garden = self.scan_garden(claim.scan).await?;
        let binds = Self::plan_binds(write, garden).map_err(|e| e.into_store(WHAT))?;
        let result = run_tx(&self.db, sql::COMMIT_PLAN, |q| {
            crate::repo::steps::bind_claim(q, claim)
                .bind(("plan", binds.plan.clone()))
                .bind(("expected", binds.expected.clone()))
                .bind(("tasks", binds.tasks.clone()))
                .bind(("triggers", binds.triggers.clone()))
                .bind(("handover", None::<PipelineStepRow>))
                .bind(("now", dt(now)))
        })
        .await;
        let mut results = match result {
            Ok(results) => results,
            Err(failure) => return Err(self.plan_failure(claim.scan, failure, WHAT).await),
        };
        let last = results.num_statements().saturating_sub(2);
        results.take(last).map_err(|e| take_failed(WHAT, e))
    }
}

#[async_trait::async_trait]
impl PlanRepo for SurrealStore {
    async fn current_plan(&self, scan: ScanId) -> Result<Option<ActionPlan>, StoreError> {
        const WHAT: &str = "current plan";
        let mut r = run_tx(&self.db, sql::CURRENT_PLAN, |q| {
            q.bind(("scan", rid(&scan)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let plan: Option<RecordId> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        match plan {
            Some(p) => {
                self.load_plan(from_rid(&p).map_err(|e| e.into_store(WHAT))?)
                    .await
            }
            None => Ok(None),
        }
    }

    async fn plan(
        &self,
        member: Membership<Viewer>,
        plan: PlanId,
    ) -> Result<ActionPlan, StoreError> {
        const WHAT: &str = "plan";
        let mut r = run_tx(&self.db, sql::PLAN_GARDEN, |q| q.bind(("plan", rid(&plan))))
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let garden: Option<RecordId> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        if garden != Some(rid(&member.garden())) {
            return Err(StoreError::NotFound);
        }
        self.load_plan(plan).await?.ok_or(StoreError::NotFound)
    }

    async fn write_plan(
        &self,
        claim: &StepClaim,
        write: PlanWrite,
    ) -> Result<ActionPlan, StoreError> {
        self.commit_plan(claim, &write, write.plan.created_at)
            .await?;
        self.written_plan(write.plan).await
    }

    async fn write_revision(
        &self,
        editor: Membership<Editor>,
        write: PlanWrite,
    ) -> Result<ActionPlan, StoreError> {
        const WHAT: &str = "write revision";
        let scan = write.plan.scan_id;
        if self.scan_garden(scan).await? != editor.garden() {
            return Err(StoreError::NotFound);
        }
        let binds = Self::plan_binds(&write, editor.garden()).map_err(|e| e.into_store(WHAT))?;
        let now = write.plan.created_at;
        let result = run_tx(&self.db, sql::WRITE_REVISION, |q| {
            q.bind(("scan", rid(&scan)))
                .bind(("plan", binds.plan.clone()))
                .bind(("expected", binds.expected.clone()))
                .bind(("tasks", binds.tasks.clone()))
                .bind(("triggers", binds.triggers.clone()))
                .bind(("now", dt(now)))
        })
        .await;
        if let Err(failure) = result {
            return Err(self.plan_failure(scan, failure, WHAT).await);
        }
        self.written_plan(write.plan).await
    }

    async fn tasks(
        &self,
        member: Membership<Viewer>,
        query: TaskQuery,
        page: PageRequest,
    ) -> Result<Page<Task>, StoreError> {
        const WHAT: &str = "tasks";
        let after = match &page.cursor {
            Some(c) => {
                let task: TaskId = c.as_str().parse().map_err(|_| StoreError::NotFound)?;
                let mut r = run_tx(&self.db, sql::TASK_CURSOR, |q| q.bind(("task", rid(&task))))
                    .await
                    .map_err(|f| f.into_store(WHAT))?;
                let key: Option<surrealdb::types::Value> =
                    r.take(0).map_err(|e| take_failed(WHAT, e))?;
                key
            }
            None => None,
        };
        let state = query
            .state
            .map(|s| crate::repo::analysis::enum_str("state", &s))
            .transpose()
            .map_err(|e| e.into_store(WHAT))?;
        let source = query.source.map(TaskSourceRow::from);
        let limit = page.limit();
        let mut r = run_tx(&self.db, sql::TASKS, |q| {
            q.bind(("garden", rid(&member.garden())))
                .bind(("state", state.clone()))
                .bind(("source", source))
                .bind(("after", after.clone()))
                .bind(("limit", i64::from(limit) + 1))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<Task> = take_all(&mut r, 0, WHAT)?;
        Ok(page_of(rows, limit, |t| t.id.to_string()))
    }

    async fn task(&self, member: Membership<Viewer>, task: TaskId) -> Result<Task, StoreError> {
        const WHAT: &str = "task";
        let mut r = run_tx(&self.db, sql::TASK, |q| {
            q.bind(("task", rid(&task)))
                .bind(("garden", rid(&member.garden())))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        take_one(&mut r, 0, WHAT)?.ok_or(StoreError::NotFound)
    }

    async fn create_task(
        &self,
        editor: Membership<Editor>,
        task: NewTask,
    ) -> Result<Task, StoreError> {
        const WHAT: &str = "create task";
        let row = TaskWriteRow::new(&task, editor.garden());
        let mut r = run_tx(&self.db, sql::CREATE_TASK, |q| {
            q.bind(("task", row.clone()))
                .bind(("garden", rid(&editor.garden())))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let last = r.num_statements().saturating_sub(2);
        take_one(&mut r, last, WHAT)?.ok_or(StoreError::NotFound)
    }

    async fn update_task(
        &self,
        editor: Membership<Editor>,
        task: TaskId,
        patch: UpdateTaskRequest,
        next: Option<NewTask>,
        now: Timestamp,
    ) -> Result<Task, StoreError> {
        const WHAT: &str = "update task";
        let next = next.map(|n| TaskWriteRow::new(&n, editor.garden()));
        let repeat = patch.repeat.as_ref().map(|r| RepeatRow {
            every_days: i64::from(r.every_days),
            until: r.until.map(|d| d.to_string()),
        });
        let mut r = run_tx(&self.db, sql::UPDATE_TASK, |q| {
            q.bind(("task", rid(&task)))
                .bind(("garden", rid(&editor.garden())))
                .bind(("done", patch.done))
                .bind(("dismissed", patch.dismissed))
                .bind(("title", patch.title.clone()))
                .bind(("detail", patch.detail.clone()))
                .bind(("due", patch.due.map(|d| d.to_string())))
                .bind(("repeat", repeat.clone()))
                .bind(("next", next.clone()))
                .bind(("now", dt(now)))
        })
        .await
        .map_err(|f| rejected_state(f, WHAT))?;
        let last = r.num_statements().saturating_sub(2);
        take_one(&mut r, last, WHAT)?.ok_or(StoreError::NotFound)
    }

    async fn delete_task(
        &self,
        editor: Membership<Editor>,
        task: TaskId,
    ) -> Result<(), StoreError> {
        const WHAT: &str = "delete task";
        run_tx(&self.db, sql::DELETE_TASK, |q| {
            q.bind(("task", rid(&task)))
                .bind(("garden", rid(&editor.garden())))
        })
        .await
        .map(drop)
        .map_err(|f| rejected_state(f, WHAT))
    }

    async fn create_alert_task(
        &self,
        garden: GardenId,
        task: NewTask,
    ) -> Result<Option<Task>, StoreError> {
        const WHAT: &str = "create alert task";
        let row = TaskWriteRow::new(&task, garden);
        let result = run_tx(&self.db, sql::CREATE_ALERT_TASK, |q| {
            q.bind(("task", row.clone())).bind(("garden", rid(&garden)))
        })
        .await;
        let mut results = match result {
            Ok(results) => results,
            // The dedupe: another run of the scheduler created it first.
            Err(DbFailure::UniqueIndex { index, .. }) if index == "task_alert" => return Ok(None),
            // The plant is gone: the rule is skipped (contracts 7).
            Err(failure) if failure.thrown() == Some(thrown::NOT_FOUND) => return Ok(None),
            Err(failure) => return Err(failure.into_store(WHAT)),
        };
        take_one(&mut results, sql::CREATE_ALERT_TASK_RESULT, WHAT)
    }

    async fn active_triggers(
        &self,
        gardens: &[GardenId],
    ) -> Result<Vec<PlanTriggerRule>, StoreError> {
        const WHAT: &str = "active triggers";
        let gardens: Vec<RecordId> = gardens.iter().map(rid).collect();
        let mut r = run_tx(&self.db, sql::ACTIVE_TRIGGERS, |q| {
            q.bind(("gardens", gardens.clone()))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<TriggerReadRow> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        rows.into_iter()
            .map(|row| {
                let step = row.steps.iter().find(|s| {
                    s.get("id").and_then(serde_json::Value::as_str) == Some(row.step_id.as_str())
                });
                let text = |k: &str| {
                    step.and_then(|s| s.get(k))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_owned()
                };
                Ok(PlanTriggerRule {
                    id: from_rid(&row.id)?,
                    plan: from_rid(&row.plan)?,
                    garden: from_rid(&row.garden)?,
                    plant: from_rid(&row.plant)?,
                    step: row.step_id.parse().map_err(|_| RowError::BadField {
                        field: "step_id",
                        problem: row.step_id.clone(),
                    })?,
                    trigger: row.trigger.into(),
                    until: row
                        .until
                        .as_deref()
                        .map(|d| local_date("until", d))
                        .transpose()?,
                    title: text("title"),
                    detail: text("detail"),
                })
            })
            .collect::<Result<_, RowError>>()
            .map_err(|e| e.into_store(WHAT))
    }
}
