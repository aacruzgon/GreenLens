//! Deletion cascades ([contracts 9], deletion cascades; [database 4], deletion ✓; storage 4).
//!
//! Bottom-up in batches of 500, never one large transaction. `start_*` marks the target and
//! cancels its work in one transaction; `delete_batch` then deletes one batch of one
//! [`CascadeStep`]. A step covers several tables in a fixed order (each holds a
//! `REFERENCE ON DELETE REJECT` to something a later one deletes), so a batch runs the step's
//! statements in that order and stops at the first that deleted anything: the caller repeats
//! the step until it returns 0, then moves on.

use domain::{
    authz::{Editor, Membership, Owner},
    error::{DomainError, InvalidStateDetails},
    garden::account::GardenTransfer,
    id::{AuthId, ConversationId, GardenId, PlantId, ScanId, UserId},
    ports::{
        AccountDeletionMarker, AccountDeletionPlan, CascadeStep, DeletionRepo, DeletionStarted,
        DeletionTarget, ObjectKey, ObjectPrefix, StoreError,
    },
    wire::Timestamp,
};
use surrealdb::{
    engine::any::Any,
    method::Query,
    types::{RecordId, SurrealValue},
};

use crate::{
    SurrealStore,
    error::{DbFailure, RowError, take_failed, thrown},
    ids::{deleted_user_rid, from_rid, rid},
    rows::{RoleRow, dt},
    tx::run_tx,
};

/// The statements. The `start_*` transactions follow [database 4] steps 1–2; the batches
/// follow step 4, sub-step by sub-step. Every batch is
/// `DELETE (SELECT VALUE id FROM t WHERE … LIMIT 500) RETURN VALUE $before.id`: [database 4]
/// writes `RETURN NONE`, which can't tell an empty batch from a full one. Plans are selected by
/// equality (`plant = $plant`, `garden = $garden`), never `scan IN $scans` (#7593 ✓).
pub mod sql {
    /// `DELETE /gardens/{id}`: `deleting_at`, the jobs cancelled, every step claim and
    /// question step cleared, so a running step fails its commit check, and the garden queued
    /// for erasure in Agent Memory (`belief_forget`). Returns the name and the other members
    /// for the `garden_deleted` push.
    pub const START_GARDEN: &str = "BEGIN;
LET $g = (SELECT name, deleting_at FROM ONLY $garden);
IF $g = NONE { THROW 'not_found' };
UPDATE $garden SET deleting_at = $now WHERE deleting_at = NONE;
LET $scans = (SELECT VALUE id FROM scan WHERE garden = $garden);
UPDATE job SET status = 'cancelled', finished_at = $now WHERE scan IN $scans AND status IN ['queued', 'leased'];
UPDATE scan SET pipeline_step = NONE, pipeline_step_run = NONE, pipeline_step_at = NONE WHERE garden = $garden AND pipeline_step != NONE;
UPDATE question SET step = NONE, step_at = NONE WHERE scan IN $scans AND step != NONE;
IF array::len(SELECT VALUE id FROM belief_forget WHERE garden = $garden AND plant = NONE) = 0 { CREATE belief_forget SET garden = $garden, requested_at = $now };
RETURN { name: $g.name, notify: (SELECT VALUE in FROM member_of WHERE out = $garden AND in != $user) };
COMMIT;";

    /// `DELETE /plants/{id}`: as for a garden, for the plant's scans, its pin removed from the
    /// current map in place (no new version, contracts 9), and the plant queued for erasure in
    /// Agent Memory. Returns the scans and the
    /// conversations anchored to the plant or its scans, for their object prefixes.
    pub const START_PLANT: &str = "BEGIN;
IF array::len(SELECT VALUE id FROM $plant WHERE garden = $garden) = 0 { THROW 'not_found' };
UPDATE $plant SET deleting_at = $now WHERE deleting_at = NONE;
LET $scans = (SELECT VALUE id FROM scan WHERE plant = $plant);
UPDATE job SET status = 'cancelled', finished_at = $now WHERE scan IN $scans AND status IN ['queued', 'leased'];
UPDATE scan SET pipeline_step = NONE, pipeline_step_run = NONE, pipeline_step_at = NONE WHERE plant = $plant AND pipeline_step != NONE;
UPDATE question SET step = NONE, step_at = NONE WHERE scan IN $scans AND step != NONE;
UPDATE garden_map SET pins = array::filter(pins, |$p| $p.plant != $plant) WHERE garden = $garden;
IF array::len(SELECT VALUE id FROM belief_forget WHERE garden = $garden AND plant = $plant) = 0 { CREATE belief_forget SET garden = $garden, plant = $plant, requested_at = $now };
RETURN { scans: $scans, conversations: (SELECT VALUE id FROM conversation WHERE anchor = $plant OR anchor IN $scans) };
COMMIT;";

    /// `DELETE /scans/{id}` of a `pending` scan ([database 4], a pending scan ✓): any other
    /// status throws `invalid_state` and returns it; its guided jobs are cancelled.
    pub const START_SCAN: &str = "BEGIN;
LET $st = (SELECT VALUE status FROM ONLY $scan WHERE garden = $garden AND deleting_at = NONE);
IF $st = NONE { THROW 'not_found' };
IF $st != 'pending' { THROW 'invalid_state' };
UPDATE $scan SET deleting_at = $now;
UPDATE job SET status = 'cancelled', finished_at = $now WHERE scan = $scan AND status IN ['queued', 'leased'];
RETURN { conversations: (SELECT VALUE id FROM conversation WHERE anchor = $scan) };
COMMIT;";
    /// The status of a scan whose deletion was refused, for `invalid_state`'s `status`.
    pub const SCAN_STATUS: &str = "SELECT VALUE status FROM ONLY $scan;";

    /// The target's scans and conversations, read once per batch.
    pub const SCOPE_GARDEN: &str = "SELECT VALUE id FROM scan WHERE garden = $garden;
SELECT VALUE id FROM conversation WHERE garden = $garden;";
    /// See [`SCOPE_GARDEN`].
    pub const SCOPE_PLANT: &str = "LET $scans = (SELECT VALUE id FROM scan WHERE plant = $plant);
RETURN $scans;
SELECT VALUE id FROM conversation WHERE anchor = $plant OR anchor IN $scans;";
    /// See [`SCOPE_GARDEN`].
    pub const SCOPE_SCAN: &str = "RETURN [$scan];
SELECT VALUE id FROM conversation WHERE anchor = $scan;";
    /// See [`SCOPE_GARDEN`].
    pub const SCOPE_MEMBER: &str = "RETURN [];
SELECT VALUE id FROM conversation WHERE garden = $garden AND user = $user;";

    // ---- step 4.1: scan events (one range delete per scan) and the scans' jobs ----

    /// One scan's events, as one key-range delete ([database 4] ✓). The scan's counter is
    /// zeroed so the next batch picks the next scan; the scan id is returned with the events so
    /// a scan with a counter but no events still counts as progress.
    pub const EVENTS: &str = "LET $sc = (SELECT VALUE id FROM scan WHERE id IN $scans AND last_event_seq > 0 LIMIT 1)[0];
IF $sc = NONE { RETURN [] } ELSE {
  LET $gone = (DELETE scan_event:[$sc, 1]..=[$sc, ..] RETURN VALUE $before.id);
  UPDATE $sc SET last_event_seq = 0;
  RETURN array::concat([$sc], $gone);
};";
    /// The scans' jobs (index `job_scan`).
    pub const JOBS: &str =
        "DELETE (SELECT VALUE id FROM job WHERE scan IN $scans LIMIT 500) RETURN VALUE $before.id;";

    // ---- step 4.2: tasks and plan trigger rules ----

    /// A garden's tasks.
    pub const TASKS_OF_GARDEN: &str = "DELETE (SELECT VALUE id FROM task WHERE garden = $garden LIMIT 500) RETURN VALUE $before.id;";
    /// A garden's trigger rules.
    pub const TRIGGERS_OF_GARDEN: &str = "DELETE (SELECT VALUE id FROM plan_trigger WHERE garden = $garden LIMIT 500) RETURN VALUE $before.id;";
    /// A plant's own tasks, and those other plants' plans made for it (`task.plant`).
    pub const TASKS_OF_PLANT: &str = "DELETE (SELECT VALUE id FROM task WHERE plant = $plant LIMIT 500) RETURN VALUE $before.id;";
    /// The tasks its plans made for neighbours (`task.plan`, index `task_plan`).
    pub const TASKS_OF_PLANS: &str = "DELETE (SELECT VALUE id FROM task WHERE plan IN (SELECT VALUE id FROM action_plan WHERE plant = $plant) LIMIT 500) RETURN VALUE $before.id;";
    /// Rules for the plant (`plan_trigger.plant`, index `plan_trigger_plant`).
    pub const TRIGGERS_OF_PLANT: &str = "DELETE (SELECT VALUE id FROM plan_trigger WHERE plant = $plant LIMIT 500) RETURN VALUE $before.id;";
    /// Rules its plans made (`plan_trigger.plan`).
    pub const TRIGGERS_OF_PLANS: &str = "DELETE (SELECT VALUE id FROM plan_trigger WHERE plan IN (SELECT VALUE id FROM action_plan WHERE plant = $plant) LIMIT 500) RETURN VALUE $before.id;";

    // ---- step 4.3: plans and diagnoses, after unsetting the scans' links ----

    /// `scan.plan`, `scan.diagnosis` and `scan.open_question` unset first.
    pub const UNLINK_SCANS: &str = "UPDATE scan SET plan = NONE, diagnosis = NONE, open_question = NONE
WHERE id IN $scans AND (plan != NONE OR diagnosis != NONE OR open_question != NONE) RETURN VALUE id;";
    /// A garden's plans, by equality.
    pub const PLANS_OF_GARDEN: &str = "DELETE (SELECT VALUE id FROM action_plan WHERE garden = $garden LIMIT 500) RETURN VALUE $before.id;";
    /// A plant's plans, by equality.
    pub const PLANS_OF_PLANT: &str = "DELETE (SELECT VALUE id FROM action_plan WHERE plant = $plant LIMIT 500) RETURN VALUE $before.id;";
    /// The scans' diagnoses (index `diagnosis_scan`).
    pub const DIAGNOSES: &str = "DELETE (SELECT VALUE id FROM diagnosis WHERE scan IN $scans LIMIT 500) RETURN VALUE $before.id;";

    // ---- step 4.4: masks, guided checks and indicator answers, which point at scan images ----

    /// Masks.
    pub const MASKS: &str = "DELETE (SELECT VALUE id FROM mask WHERE scan IN $scans LIMIT 500) RETURN VALUE $before.id;";
    /// Guided checks.
    pub const GUIDED_CHECKS: &str = "DELETE (SELECT VALUE id FROM guided_check WHERE scan IN $scans LIMIT 500) RETURN VALUE $before.id;";
    /// Indicator answers (schema v1.4): they point at scan images and questions.
    pub const INDICATOR_ANSWERS: &str = "DELETE (SELECT VALUE id FROM indicator_answer WHERE scan IN $scans LIMIT 500) RETURN VALUE $before.id;";

    // ---- step 4.4 (cont.): observations, questions and scan feedback ----

    /// Observations.
    pub const OBSERVATIONS: &str = "DELETE (SELECT VALUE id FROM observation WHERE scan IN $scans LIMIT 500) RETURN VALUE $before.id;";
    /// Questions.
    pub const QUESTIONS: &str = "DELETE (SELECT VALUE id FROM question WHERE scan IN $scans LIMIT 500) RETURN VALUE $before.id;";
    /// Scan feedback.
    pub const FEEDBACK: &str = "DELETE (SELECT VALUE id FROM scan_feedback WHERE scan IN $scans LIMIT 500) RETURN VALUE $before.id;";

    // ---- step 4.5: scan images, then scans ----

    /// Scan images.
    pub const IMAGES: &str = "DELETE (SELECT VALUE id FROM scan_image WHERE scan IN $scans LIMIT 500) RETURN VALUE $before.id;";
    /// Scans (a pending scan's jar-test tasks go with it through `task.scan`'s `CASCADE` ✓).
    pub const SCANS: &str =
        "DELETE (SELECT VALUE id FROM scan WHERE id IN $scans LIMIT 500) RETURN VALUE $before.id;";

    // ---- step 4.6: conversations, children first ----

    /// Pending actions.
    pub const PENDING_ACTIONS: &str = "DELETE (SELECT VALUE id FROM pending_action WHERE conversation IN $conversations LIMIT 500) RETURN VALUE $before.id;";
    /// Message feedback (`message_feedback:[message]`, no reference field).
    pub const MESSAGE_FEEDBACK: &str = "DELETE (SELECT VALUE id FROM message_feedback WHERE record::id(id)[0].conversation IN $conversations LIMIT 500) RETURN VALUE $before.id;";
    /// Messages.
    pub const MESSAGES: &str = "DELETE (SELECT VALUE id FROM message WHERE conversation IN $conversations LIMIT 500) RETURN VALUE $before.id;";
    /// Chat images.
    pub const CHAT_IMAGES: &str = "DELETE (SELECT VALUE id FROM chat_image WHERE conversation IN $conversations LIMIT 500) RETURN VALUE $before.id;";
    /// The conversations.
    pub const CONVERSATIONS: &str = "DELETE (SELECT VALUE id FROM conversation WHERE id IN $conversations LIMIT 500) RETURN VALUE $before.id;";

    // ---- step 4.7: memories, then plants, then the garden map ----

    /// A garden's memories.
    pub const MEMORIES_OF_GARDEN: &str = "DELETE (SELECT VALUE id FROM memory WHERE garden = $garden LIMIT 500) RETURN VALUE $before.id;";
    /// A plant's memories.
    pub const MEMORIES_OF_PLANT: &str = "DELETE (SELECT VALUE id FROM memory WHERE plant = $plant LIMIT 500) RETURN VALUE $before.id;";
    /// A removed member's unshared chat memories in the garden (contracts 9).
    pub const MEMORIES_OF_MEMBER: &str = "DELETE (SELECT VALUE id FROM memory WHERE garden = $garden AND author = $user AND source = 'chat' AND shared = false LIMIT 500) RETURN VALUE $before.id;";
    /// A garden's plants.
    pub const PLANTS_OF_GARDEN: &str = "DELETE (SELECT VALUE id FROM plant WHERE garden = $garden LIMIT 500) RETURN VALUE $before.id;";
    /// The plant.
    pub const PLANT: &str = "DELETE $plant RETURN VALUE $before.id;";
    /// The garden map ([database 4] ✓: `DELETE garden` is rejected while a map exists).
    pub const MAPS: &str = "DELETE (SELECT VALUE id FROM garden_map WHERE garden = $garden LIMIT 500) RETURN VALUE $before.id;";

    // ---- farm records, then farm connectors ([connectors 8], deletion) ----

    /// A garden's farm records (each holds a `REFERENCE ON DELETE REJECT` to its connector).
    pub const FARM_RECORDS_OF_GARDEN: &str = "DELETE (SELECT VALUE id FROM farm_record WHERE garden = $garden LIMIT 500) RETURN VALUE $before.id;";
    /// A garden's farm connectors. Revoking their tokens is the runner's, before this step.
    pub const CONNECTORS_OF_GARDEN: &str = "DELETE (SELECT VALUE id FROM connector WHERE garden = $garden LIMIT 500) RETURN VALUE $before.id;";

    // ---- step 4.7 (cont.): invites, memberships and the garden ----

    /// Invites (they also `CASCADE`).
    pub const INVITES: &str = "DELETE (SELECT VALUE id FROM invite WHERE garden = $garden LIMIT 500) RETURN VALUE $before.id;";
    /// Memberships.
    pub const MEMBERSHIPS: &str = "DELETE (SELECT VALUE id FROM member_of WHERE out = $garden LIMIT 500) RETURN VALUE $before.id;";
    /// The garden.
    pub const GARDEN: &str = "DELETE $garden RETURN VALUE $before.id;";

    /// Deletions started but not finished, to resume after a restart. A member's removal leaves
    /// no marker; its conversations are found again by `member_conversations`.
    pub const UNFINISHED: &str = "SELECT VALUE id FROM garden WHERE deleting_at != NONE;
SELECT VALUE id FROM plant WHERE deleting_at != NONE AND garden.deleting_at = NONE;
SELECT VALUE id FROM scan WHERE deleting_at != NONE AND plant.deleting_at = NONE AND garden.deleting_at = NONE;";

    /// The orphan sweep's database side ([storage 4]): which of these keys no record points
    /// to. Every key field counts: `scan_image.storage_key`, `depth_key` and
    /// `depth_confidence_key`, `mask.storage_key` and `chat_image.storage_key`.
    pub const UNREFERENCED_KEYS: &str =
        "LET $used = array::union(array::union(array::union(array::union(
  (SELECT VALUE storage_key FROM scan_image WHERE storage_key IN $keys),
  (SELECT VALUE depth_key FROM scan_image WHERE depth_key IN $keys)),
  (SELECT VALUE depth_confidence_key FROM scan_image WHERE depth_confidence_key IN $keys)),
  (SELECT VALUE storage_key FROM mask WHERE storage_key IN $keys)),
  (SELECT VALUE storage_key FROM chat_image WHERE storage_key IN $keys));
RETURN array::complement($keys, $used);";

    /// Account deletion, step 1–3: each garden's members with roles and `since`.
    pub const ACCOUNT_GARDENS: &str = "SELECT out AS garden, role,
       (SELECT in AS user, role, since FROM member_of WHERE out = $parent.out AND in != $user ORDER BY since) AS others
FROM member_of WHERE in = $user AND out.deleting_at = NONE;";
    /// Step 2: the heir becomes an owner in the leaver's place (the leaver drops to editor);
    /// the leaver's membership is then removed by the `Member` cascade.
    pub const TRANSFER_OWNERSHIP: &str = "BEGIN;
IF array::len(SELECT VALUE id FROM $heir) = 0 { THROW 'not_found' };
UPDATE $heir SET role = 'owner';
UPDATE $leaver SET role = 'editor' WHERE role = 'owner';
COMMIT;";
    /// A member's conversations in a garden.
    pub const MEMBER_CONVERSATIONS: &str =
        "SELECT VALUE id FROM conversation WHERE garden = $garden AND user = $user;";
    /// Step 4: the user's scan feedback (it also `CASCADE`s with the user).
    pub const USER_FEEDBACK: &str = "DELETE (SELECT VALUE id FROM scan_feedback WHERE user = $user LIMIT 500) RETURN VALUE $before.id;";
    /// The account tombstone, created once: a second `CREATE` fails with "already exists", so
    /// only one cascade starts (schema v1.4 `deleted_user`).
    pub const START_ACCOUNT_DELETION: &str = "CREATE $tomb SET user = $user, started_at = $now;";
    /// Tombstones whose auth-provider deletion hasn't finished (index `deleted_user_pending`),
    /// with whether the `user` record is gone.
    pub const UNFINISHED_ACCOUNT_DELETIONS: &str =
        "SELECT record::id(id)[0] AS auth_id, user, started_at,
       array::len(SELECT VALUE id FROM type::record('user', $parent.user)) = 0 AS user_deleted
FROM deleted_user WHERE clerk_deleted_at = NONE ORDER BY started_at;";
    /// The auth provider confirmed the deletion.
    pub const FINISH_ACCOUNT_DELETION: &str = "UPDATE $tomb SET clerk_deleted_at = $now;";
    /// Cleanup: tombstones finished before the cutoff, one batch of 500.
    pub const DELETE_FINISHED_TOMBSTONES: &str = "DELETE (SELECT VALUE id FROM deleted_user WHERE clerk_deleted_at != NONE AND clerk_deleted_at < $cutoff LIMIT 500) RETURN VALUE $before.id;";
    /// Step 5: the user (push tokens `CASCADE`; author links `UNSET`).
    pub const DELETE_USER: &str = "DELETE $user;";
}

/// The statements of one cascade step for one target, in their fixed order.
pub fn step_statements(target: DeletionTarget, step: CascadeStep) -> &'static [&'static str] {
    use CascadeStep as S;
    use DeletionTarget as T;
    match (target, step) {
        (_, S::Conversations) => &[
            sql::PENDING_ACTIONS,
            sql::MESSAGE_FEEDBACK,
            sql::MESSAGES,
            sql::CHAT_IMAGES,
            sql::CONVERSATIONS,
        ],
        (T::Member { .. }, S::MemoriesPlantsMap) => &[sql::MEMORIES_OF_MEMBER],
        (T::Member { .. }, _) => &[],
        (_, S::EventsAndJobs) => &[sql::EVENTS, sql::JOBS],
        (T::Garden(_), S::TasksAndTriggers) => &[sql::TASKS_OF_GARDEN, sql::TRIGGERS_OF_GARDEN],
        (T::Plant(_), S::TasksAndTriggers) => &[
            sql::TASKS_OF_PLANT,
            sql::TASKS_OF_PLANS,
            sql::TRIGGERS_OF_PLANT,
            sql::TRIGGERS_OF_PLANS,
        ],
        (T::Garden(_), S::PlansAndDiagnoses) => {
            &[sql::UNLINK_SCANS, sql::PLANS_OF_GARDEN, sql::DIAGNOSES]
        }
        (T::Plant(_), S::PlansAndDiagnoses) => {
            &[sql::UNLINK_SCANS, sql::PLANS_OF_PLANT, sql::DIAGNOSES]
        }
        (_, S::MasksAndGuidedChecks) => &[sql::MASKS, sql::GUIDED_CHECKS, sql::INDICATOR_ANSWERS],
        (_, S::ObservationsQuestionsFeedback) => {
            &[sql::OBSERVATIONS, sql::QUESTIONS, sql::FEEDBACK]
        }
        (_, S::ImagesAndScans) => &[sql::IMAGES, sql::SCANS],
        (T::Garden(_), S::MemoriesPlantsMap) => {
            &[sql::MEMORIES_OF_GARDEN, sql::PLANTS_OF_GARDEN, sql::MAPS]
        }
        (T::Plant(_), S::MemoriesPlantsMap) => &[sql::MEMORIES_OF_PLANT, sql::PLANT],
        (T::Garden(_), S::FarmRecordsAndConnectors) => {
            &[sql::FARM_RECORDS_OF_GARDEN, sql::CONNECTORS_OF_GARDEN]
        }
        (T::Garden(_), S::Garden) => &[sql::INVITES, sql::MEMBERSHIPS, sql::GARDEN],
        // A pending scan has no tasks, plans, diagnoses, memories or garden step, and a plant no
        // connectors (CascadeStep::steps_for never asks for them).
        (T::PendingScan(_), _) | (T::Plant(_), S::FarmRecordsAndConnectors | S::Garden) => &[],
    }
}

/// The target's binds for the scope query and every batch (`NONE` for what the target lacks).
#[derive(Debug, Clone, Default)]
pub struct TargetBinds {
    garden: Option<RecordId>,
    plant: Option<RecordId>,
    scan: Option<RecordId>,
    user: Option<RecordId>,
}

impl TargetBinds {
    /// The scope statement and binds of a target.
    pub fn of(target: DeletionTarget) -> (&'static str, Self) {
        match target {
            DeletionTarget::Garden(g) => (
                sql::SCOPE_GARDEN,
                Self {
                    garden: Some(rid(&g)),
                    ..Self::default()
                },
            ),
            DeletionTarget::Plant(p) => (
                sql::SCOPE_PLANT,
                Self {
                    plant: Some(rid(&p)),
                    ..Self::default()
                },
            ),
            DeletionTarget::PendingScan(s) => (
                sql::SCOPE_SCAN,
                Self {
                    scan: Some(rid(&s)),
                    ..Self::default()
                },
            ),
            DeletionTarget::Member { garden, user } => (
                sql::SCOPE_MEMBER,
                Self {
                    garden: Some(rid(&garden)),
                    user: Some(rid(&user)),
                    ..Self::default()
                },
            ),
        }
    }

    /// Adds the binds. A function, not a closure: a closure's signature gets no lifetime
    /// elision tying its returned `Query<'c, _>` to its argument's `'c` (the compiler infers
    /// two unrelated lifetimes), while a function's `'c` names the one borrow both share.
    pub fn bind<'c>(&self, q: Query<'c, Any>) -> Query<'c, Any> {
        q.bind(("garden", self.garden.clone()))
            .bind(("plant", self.plant.clone()))
            .bind(("scan", self.scan.clone()))
            .bind(("user", self.user.clone()))
    }
}

/// One row of [`sql::UNFINISHED_ACCOUNT_DELETIONS`].
#[derive(Debug, Clone, SurrealValue)]
pub struct TombstoneRow {
    /// The auth provider's id (the key).
    pub auth_id: String,
    /// The user's ULID.
    pub user: String,
    /// When the cascade started.
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// The `user` record is gone.
    pub user_deleted: bool,
}

/// One garden of [`sql::ACCOUNT_GARDENS`].
#[derive(Debug, Clone, SurrealValue)]
pub struct AccountGardenRow {
    /// The garden.
    pub garden: RecordId,
    /// The user's role.
    pub role: RoleRow,
    /// The other members, longest-standing first.
    pub others: Vec<OtherMemberRow>,
}

/// Another member of the garden.
#[derive(Debug, Clone, SurrealValue)]
pub struct OtherMemberRow {
    /// The member.
    pub user: RecordId,
    /// Their role.
    pub role: RoleRow,
    /// Since when.
    pub since: chrono::DateTime<chrono::Utc>,
}

/// The result of [`sql::START_GARDEN`].
#[derive(Debug, Clone, SurrealValue)]
pub struct GardenStartRow {
    /// The garden's name.
    pub name: String,
    /// The other members.
    pub notify: Vec<RecordId>,
}

/// The result of [`sql::START_PLANT`] and [`sql::START_SCAN`].
#[derive(Debug, Clone, SurrealValue)]
pub struct ScopeStartRow {
    /// The scans whose prefixes go.
    #[surreal(default)]
    pub scans: Vec<RecordId>,
    /// The conversations whose chat prefixes go.
    pub conversations: Vec<RecordId>,
}

fn ids<T: crate::ids::FromRecordId>(rows: &[RecordId]) -> Result<Vec<T>, RowError> {
    rows.iter().map(from_rid).collect()
}

fn count(ids: &[RecordId]) -> u32 {
    u32::try_from(ids.len()).unwrap_or(u32::MAX)
}

#[async_trait::async_trait]
impl DeletionRepo for SurrealStore {
    async fn start_garden_deletion(
        &self,
        owner: Membership<Owner>,
        now: Timestamp,
    ) -> Result<DeletionStarted, StoreError> {
        const WHAT: &str = "start garden deletion";
        let garden = owner.garden();
        let mut results = run_tx(&self.db, sql::START_GARDEN, |q| {
            q.bind(("garden", rid(&garden)))
                .bind(("user", rid(&owner.user())))
                .bind(("now", dt(now)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let last = results.num_statements().saturating_sub(2);
        let row: Option<GardenStartRow> = results.take(last).map_err(|e| take_failed(WHAT, e))?;
        let row = row.ok_or(StoreError::NotFound)?;
        Ok(DeletionStarted {
            target: DeletionTarget::Garden(garden),
            prefixes: vec![ObjectPrefix::garden(garden)],
            keys: Vec::new(),
            notify: ids(&row.notify).map_err(|e| e.into_store(WHAT))?,
            garden_name: Some(row.name),
        })
    }

    async fn start_plant_deletion(
        &self,
        owner: Membership<Owner>,
        plant: PlantId,
        now: Timestamp,
    ) -> Result<DeletionStarted, StoreError> {
        const WHAT: &str = "start plant deletion";
        let garden = owner.garden();
        let mut results = run_tx(&self.db, sql::START_PLANT, |q| {
            q.bind(("garden", rid(&garden)))
                .bind(("plant", rid(&plant)))
                .bind(("now", dt(now)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let last = results.num_statements().saturating_sub(2);
        let row: Option<ScopeStartRow> = results.take(last).map_err(|e| take_failed(WHAT, e))?;
        let row = row.ok_or(StoreError::NotFound)?;
        let scans: Vec<ScanId> = ids(&row.scans).map_err(|e| e.into_store(WHAT))?;
        let conversations: Vec<ConversationId> =
            ids(&row.conversations).map_err(|e| e.into_store(WHAT))?;
        Ok(DeletionStarted {
            target: DeletionTarget::Plant(plant),
            prefixes: scans
                .iter()
                .map(|s| ObjectPrefix::scan(garden, *s))
                .chain(conversations.iter().map(|c| ObjectPrefix::chat(garden, *c)))
                .collect(),
            keys: Vec::new(),
            notify: Vec::new(),
            garden_name: None,
        })
    }

    async fn start_scan_deletion(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
        now: Timestamp,
    ) -> Result<DeletionStarted, StoreError> {
        const WHAT: &str = "start scan deletion";
        let garden = editor.garden();
        let result = run_tx(&self.db, sql::START_SCAN, |q| {
            q.bind(("garden", rid(&garden)))
                .bind(("scan", rid(&scan)))
                .bind(("now", dt(now)))
        })
        .await;
        let mut results = match result {
            Ok(results) => results,
            Err(failure) if failure.thrown() == Some(thrown::INVALID_STATE) => {
                let mut status =
                    run_tx(&self.db, sql::SCAN_STATUS, |q| q.bind(("scan", rid(&scan))))
                        .await
                        .map_err(|f| f.into_store(WHAT))?;
                let status: Option<String> = status.take(0).map_err(|e| take_failed(WHAT, e))?;
                return Err(StoreError::Rejected(DomainError::InvalidState(
                    InvalidStateDetails {
                        status,
                        ..InvalidStateDetails::default()
                    },
                )));
            }
            Err(failure) => return Err(failure.into_store(WHAT)),
        };
        let last = results.num_statements().saturating_sub(2);
        let row: Option<ScopeStartRow> = results.take(last).map_err(|e| take_failed(WHAT, e))?;
        let row = row.ok_or(StoreError::NotFound)?;
        let conversations: Vec<ConversationId> =
            ids(&row.conversations).map_err(|e| e.into_store(WHAT))?;
        Ok(DeletionStarted {
            target: DeletionTarget::PendingScan(scan),
            prefixes: std::iter::once(ObjectPrefix::scan(garden, scan))
                .chain(conversations.iter().map(|c| ObjectPrefix::chat(garden, *c)))
                .collect(),
            keys: Vec::new(),
            notify: Vec::new(),
            garden_name: None,
        })
    }

    async fn delete_batch(
        &self,
        target: DeletionTarget,
        step: CascadeStep,
    ) -> Result<u32, StoreError> {
        const WHAT: &str = "delete batch";
        let statements = step_statements(target, step);
        if statements.is_empty() {
            return Ok(0);
        }
        let (scope, binds) = TargetBinds::of(target);
        let mut results = run_tx(&self.db, scope, |q| binds.bind(q))
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let n = results.num_statements();
        let scans: Vec<RecordId> = results.take(n - 2).map_err(|e| take_failed(WHAT, e))?;
        let conversations: Vec<RecordId> = results.take(n - 1).map_err(|e| take_failed(WHAT, e))?;
        for statement in statements {
            let mut results = run_tx(&self.db, statement, |q| {
                binds
                    .bind(q)
                    .bind(("scans", scans.clone()))
                    .bind(("conversations", conversations.clone()))
            })
            .await
            .map_err(|f| f.into_store(WHAT))?;
            let last = results.num_statements() - 1;
            let deleted: Vec<RecordId> = results.take(last).map_err(|e| take_failed(WHAT, e))?;
            if !deleted.is_empty() {
                return Ok(count(&deleted));
            }
        }
        Ok(0)
    }

    async fn unfinished_deletions(&self) -> Result<Vec<DeletionTarget>, StoreError> {
        const WHAT: &str = "unfinished deletions";
        let mut results = run_tx(&self.db, sql::UNFINISHED, |q| q)
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let gardens: Vec<RecordId> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        let plants: Vec<RecordId> = results.take(1).map_err(|e| take_failed(WHAT, e))?;
        let scans: Vec<RecordId> = results.take(2).map_err(|e| take_failed(WHAT, e))?;
        let row_err = |e: RowError| e.into_store(WHAT);
        let mut targets = Vec::new();
        for g in ids::<GardenId>(&gardens).map_err(row_err)? {
            targets.push(DeletionTarget::Garden(g));
        }
        for p in ids::<PlantId>(&plants).map_err(row_err)? {
            targets.push(DeletionTarget::Plant(p));
        }
        for s in ids::<ScanId>(&scans).map_err(row_err)? {
            targets.push(DeletionTarget::PendingScan(s));
        }
        Ok(targets)
    }

    /// Runs [`sql::ACCOUNT_GARDENS`] and decides each garden with architecture 5's rules: the
    /// only member deletes it; the only owner hands it to the longest-standing editor (and then
    /// leaves it), or deletes it when no editor can inherit; anything else is left.
    async fn plan_account_deletion(&self, user: UserId) -> Result<AccountDeletionPlan, StoreError> {
        const WHAT: &str = "plan account deletion";
        let mut r = run_tx(&self.db, sql::ACCOUNT_GARDENS, |q| {
            q.bind(("user", rid(&user)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<AccountGardenRow> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        let mut plan = AccountDeletionPlan::default();
        for row in rows {
            let garden: GardenId = from_rid(&row.garden).map_err(|e| e.into_store(WHAT))?;
            let other_owner = row.others.iter().any(|o| o.role == RoleRow::Owner);
            if row.others.is_empty() {
                plan.delete.push(garden);
            } else if row.role == RoleRow::Owner && !other_owner {
                // `others` is ordered by `since`: the first editor has been there longest.
                match row.others.iter().find(|o| o.role == RoleRow::Editor) {
                    Some(heir) => {
                        plan.transfer.push(GardenTransfer {
                            garden_id: garden,
                            new_owner_user_id: from_rid(&heir.user)
                                .map_err(|e| e.into_store(WHAT))?,
                        });
                        plan.leave.push(garden);
                    }
                    None => plan.delete.push(garden),
                }
            } else {
                plan.leave.push(garden);
            }
        }
        Ok(plan)
    }

    async fn transfer_ownership(
        &self,
        transfer: GardenTransfer,
        from: UserId,
    ) -> Result<(), StoreError> {
        const WHAT: &str = "transfer ownership";
        run_tx(&self.db, sql::TRANSFER_OWNERSHIP, |q| {
            q.bind((
                "heir",
                crate::ids::member_rid(transfer.new_owner_user_id, transfer.garden_id),
            ))
            .bind(("leaver", crate::ids::member_rid(from, transfer.garden_id)))
        })
        .await
        .map(drop)
        .map_err(|f| f.into_store(WHAT))
    }

    async fn member_conversations(
        &self,
        garden: GardenId,
        user: UserId,
    ) -> Result<Vec<(ConversationId, ObjectPrefix)>, StoreError> {
        const WHAT: &str = "member conversations";
        let mut r = run_tx(&self.db, sql::MEMBER_CONVERSATIONS, |q| {
            q.bind(("garden", rid(&garden))).bind(("user", rid(&user)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<RecordId> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        ids::<ConversationId>(&rows)
            .map(|c| {
                c.into_iter()
                    .map(|c| (c, ObjectPrefix::chat(garden, c)))
                    .collect()
            })
            .map_err(|e| e.into_store(WHAT))
    }

    async fn delete_user_feedback(&self, user: UserId) -> Result<u32, StoreError> {
        const WHAT: &str = "delete user feedback";
        let mut r = run_tx(&self.db, sql::USER_FEEDBACK, |q| {
            q.bind(("user", rid(&user)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<RecordId> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        Ok(count(&rows))
    }

    async fn delete_user(&self, user: UserId) -> Result<(), StoreError> {
        const WHAT: &str = "delete user";
        run_tx(&self.db, sql::DELETE_USER, |q| q.bind(("user", rid(&user))))
            .await
            .map(drop)
            .map_err(|f| f.into_store(WHAT))
    }

    async fn start_account_deletion(
        &self,
        user: UserId,
        auth_id: &AuthId,
        now: Timestamp,
    ) -> Result<bool, StoreError> {
        const WHAT: &str = "start account deletion";
        match run_tx(&self.db, sql::START_ACCOUNT_DELETION, |q| {
            q.bind(("tomb", deleted_user_rid(auth_id)))
                .bind(("user", user.to_string()))
                .bind(("now", dt(now)))
        })
        .await
        {
            Ok(_) => Ok(true),
            // Another request or the webhook started it first.
            Err(DbFailure::AlreadyExists(_)) => Ok(false),
            Err(f) => Err(f.into_store(WHAT)),
        }
    }

    async fn unfinished_account_deletions(&self) -> Result<Vec<AccountDeletionMarker>, StoreError> {
        const WHAT: &str = "unfinished account deletions";
        let mut r = run_tx(&self.db, sql::UNFINISHED_ACCOUNT_DELETIONS, |q| q)
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<TombstoneRow> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        rows.into_iter()
            .map(|row| {
                Ok(AccountDeletionMarker {
                    user: row.user.parse().map_err(|_| RowError::BadField {
                        field: "user",
                        problem: row.user.clone(),
                    })?,
                    auth_id: row.auth_id.parse().map_err(|_| RowError::BadField {
                        field: "auth_id",
                        problem: row.auth_id.clone(),
                    })?,
                    started_at: Timestamp::from(row.started_at),
                    user_deleted: row.user_deleted,
                })
            })
            .collect::<Result<_, RowError>>()
            .map_err(|e| e.into_store(WHAT))
    }

    async fn finish_account_deletion(
        &self,
        auth_id: &AuthId,
        now: Timestamp,
    ) -> Result<(), StoreError> {
        const WHAT: &str = "finish account deletion";
        run_tx(&self.db, sql::FINISH_ACCOUNT_DELETION, |q| {
            q.bind(("tomb", deleted_user_rid(auth_id)))
                .bind(("now", dt(now)))
        })
        .await
        .map(drop)
        .map_err(|f| f.into_store(WHAT))
    }

    async fn delete_finished_tombstones(&self, before: Timestamp) -> Result<u32, StoreError> {
        const WHAT: &str = "delete finished tombstones";
        let mut r = run_tx(&self.db, sql::DELETE_FINISHED_TOMBSTONES, |q| {
            q.bind(("cutoff", dt(before)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<RecordId> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        Ok(count(&rows))
    }

    async fn unreferenced_keys(&self, keys: &[ObjectKey]) -> Result<Vec<ObjectKey>, StoreError> {
        const WHAT: &str = "unreferenced keys";
        let keys: Vec<String> = keys.iter().map(|k| k.as_str().to_owned()).collect();
        let mut results = run_tx(&self.db, sql::UNREFERENCED_KEYS, |q| {
            q.bind(("keys", keys.clone()))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let unused: Vec<String> = results.take(1).map_err(|e| take_failed(WHAT, e))?;
        Ok(unused
            .iter()
            .filter_map(|k| ObjectKey::from_stored(k))
            .collect())
    }
}
