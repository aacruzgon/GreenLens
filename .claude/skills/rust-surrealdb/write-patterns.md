# Write patterns as store functions

Part of [`rust-surrealdb`](SKILL.md). One function per pattern in [database 4](../../../docs/database/README.md#4-write-patterns), plus connecting, the replay read, cursor paging and batched deletes, written the way [backend store 5](../../../docs/backend/store.md#5-transactions-retries-and-the-error-mapping) writes every repository method: the statement in a `const`, `run_tx` with the binds, a typed `take` at a named index, and the failure mapped into the domain's `StoreError` with `into_store(what)` or, where the caller knows more, by hand first. The database document wins, and [backend store](../../../docs/backend/store.md) is how the store crate applies it.

**The statements.** Each constant is [backend store](../../../docs/backend/store.md)'s statement where its listing gives one, and database 4's otherwise. store.md keeps one deliberate difference from database 4: its statements bind `$now` where database 4 writes `time::now()`, because time comes from the `Clock` port ([backend README 7](../../../docs/backend/README.md#7-rust-conventions), "Time": every "now" comes from the `Clock` port), so tests control lease, stamp and expiry times ([backend store 1](../../../docs/backend/store.md#1-purpose-and-boundaries), the "Clock" row). Its section 22 names the exceptions still on the database clock or a field default: `finish_guided_check` reads `time::now()` in its transaction, and `create_plant` and `register_push_token` take the `created_at` default ([store 22](../../../docs/backend/store.md#22-open-questions), item 5). Each constant here follows store.md and binds `$now`, and names its source:

| Constant | Source | Differs from database 4 by |
|---|---|---|
| `APPEND_EVENT` | [store 10](../../../docs/backend/store.md#10-scans-and-events), verbatim | a `not_found` guard for a missing scan; `ts = $ts` from the clock |
| `EVENTS_AFTER` | store 10, verbatim | scoped to the proof's garden, skipping a scan, plant or garden being deleted, `not_found` otherwise |
| `SET_PLAN_CAS` | store 13's `plan_cas!`, verbatim, in its own `BEGIN … COMMIT` ([store 13](../../../docs/backend/store.md#13-plans-and-tasks)) | the supersede (the old plan's open tasks dismissed with `plan_superseded`, its trigger rules deactivated) and the new tasks and rules, each only on a live plant. This example binds no tasks or rules |
| `CLAIM_STEP` | store 7's `claim_entry!`, verbatim ([store 7](../../../docs/backend/store.md#7-shared-fragments)) | database 4's conditional claim (`IF $claimed = NONE`, database v1.4.2) plus an eligibility check: a scan being deleted or in `pending`, `queued` or `failed` is `not_needed`; the result is `{ outcome, run, stamp, status }` |
| `COMMIT_STEP` | store 11's `COMMIT_RELEASE` (`claim_check!` and `handover!`), verbatim ([store 11](../../../docs/backend/store.md#11-steps-between-jobs)) | the re-check also fails a missing scan and a claim of another run (`$s.run != $run`); no output of its own, where the real commits write theirs between the two fragments |
| `RECLAIM_STEP` | store 11, verbatim | the claim's run bound (`pipeline_step_run = $run`) |
| `CLAIM_JOB` | [store 14](../../../docs/backend/store.md#14-jobs-and-workers), verbatim | the fallback's hold (`$hold_question`, `$perception_alive`), the fallback's `max_runtime` doubled at claim (`$on_fallback`), and a run's first job moving the scan `queued → analysing`; the result is `{ job, started_analysing }` |
| `REQUEUE_STALE` | store 14, verbatim | database 4 gives only its `WHERE`; store.md adds the silent workers (question 20), the attempt rules and the guided check |
| `BEGIN_IDEMPOTENT`, `ADD_MEMBER` | database 4 | `$now` for `time::now()` in the first; store 15 binds the whole key and its `expires_at` instead ([store 15](../../../docs/backend/store.md#15-idempotency-usage-and-the-weather-cache)) |
| `ENQUEUE_JOB`, `CREATE_ALERT_TASK`, `READ_IDEMPOTENT`, `CURRENT_PLAN`, `PLANTS_PAGE`, `DELETE_*` | this skill's, from database 4 and 5's rules | store.md doesn't list them; its `CREATE_ALERT_TASK` also checks the plant is live ([store 13](../../../docs/backend/store.md#13-plans-and-tasks)). The batched delete returns `$before.id` instead of `NONE` ([SKILL.md, deletes](SKILL.md#deletes)) |

The block carries exact copies of the shared definitions, which `check_examples.py` compares line for line with their canonical blocks: `StoreError` with `BoxError` and `DomainError` ([rust-errors, mapping.md](../rust-errors/mapping.md)), the store kit `run_tx`, `classify`, `DbFailure` and the rest ([errors-and-retries.md](errors-and-retries.md)), and the test harness `TestDb` ([rust-testing, database.md](../rust-testing/database.md#3-the-harness)).

## What each function relies on

| Function | Doc | Statement read | Failure it maps |
|---|---|---|---|
| `apply_schema` | [database 1](../../../docs/database/README.md#1-version-and-setup), [store 4](../../../docs/backend/store.md#4-connecting-and-the-schema) | all, as root, through `run_tx` | any failed statement fails start-up |
| `connect_api` | [database 1](../../../docs/database/README.md#1-version-and-setup), [6](../../../docs/database/README.md#6-rust-sdk) | none | wrong credentials: `NotAllowed(Auth(InvalidAuth))`, `Query { what: "sign in" }` |
| `append_scan_event` | [scan events](../../../docs/database/README.md#scan-events-a-gap-free-seq-), [store 10](../../../docs/backend/store.md#10-scans-and-events) | 3, the `CREATE`; `seq` is the key's second element | `not_found` → `NotFound`; conflict at the `COMMIT`, retried |
| `events_after` | [scan events](../../../docs/database/README.md#scan-events-a-gap-free-seq-), [contracts 4](../../../docs/architecture/contracts.md#when-the-stream-closes), [store 10](../../../docs/backend/store.md#10-scans-and-events) | 1 | `not_found` → `NotFound` (another garden's scan, or one being deleted: contracts 4 gives a reconnect to a deleted scan `404`, and store 10's `STREAM_STATE` reads only scans without `deleting_at` the same way, which ends a live stream); `$after` must be a number |
| `set_plan_cas` | [plan writes](../../../docs/database/README.md#plan-writes-compare-and-set-), [store 13](../../../docs/backend/store.md#13-plans-and-tasks) | none | `stale_plan`, or the `action_plan_version` unique index, → the plan that won read into `Conflict::StalePlan { current }`; conflict retried |
| `claim_step` | [steps between jobs](../../../docs/database/README.md#steps-between-jobs-), [store 7, 11](../../../docs/backend/store.md#11-steps-between-jobs) | 5, `{ outcome, run, stamp, status }`: `claimed`, `busy` (flagged) or `not_needed` | conflict retried (the loser finds the claim taken) |
| `commit_step` | the same | 5, the next step | `claim_lost` → `Conflict::ClaimLost`; conflict retried |
| `reclaim_step` | the same | 0, the scan, or `NONE` | none |
| `enqueue_job` | [job claim](../../../docs/database/README.md#job-claim-with-slots-), [store 14, 18](../../../docs/backend/store.md#18-concurrency-and-lifetimes) | none | signals `jobs_queued` after the commit |
| `claim_job` | [job claim](../../../docs/database/README.md#job-claim-with-slots-), [contracts 5](../../../docs/architecture/contracts.md#post-internaljobsclaim) | 6, `{ job, started_analysing }` | conflict retried (the loser gets the next job) |
| `requeue_stale` | [job claim](../../../docs/database/README.md#job-claim-with-slots-), [contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts) | 5, each job touched with its new status | signals `jobs_queued` when any job went back to `queued` |
| `begin_idempotent` | [idempotency keys](../../../docs/database/README.md#idempotency-keys-), [contracts 1](../../../docs/architecture/contracts.md#idempotency), [store 15](../../../docs/backend/store.md#15-idempotency-usage-and-the-weather-cache) | the stored key on a repeat | `AlreadyExists(Record)` → read the existing key; conflict retried |
| `add_member` | [membership](../../../docs/database/README.md#membership-) | none | `AlreadyExists(Record)` → `Conflict::AlreadyExists`; conflict retried |
| `create_alert_task` | [alert dedupe](../../../docs/database/README.md#alert-dedupe-) | none | `UniqueIndex { index: "task_alert" }` → `Ok(false)`; conflict retried |
| `delete_scan_events`, `delete_in_batches` | [deletion](../../../docs/database/README.md#deletion-), [store 17](../../../docs/backend/store.md#17-deletion-and-cleanup) | 0, as `Vec` | a remaining `REFERENCE ON DELETE REJECT` fails loudly |
| `plants_page` | [database 5](../../../docs/database/README.md#5-reads-indexes-and-vector-search) | 0 | none |

The fallback worker uses the same `CLAIM_JOB` with `fallback: true`, which holds back a photo answer's segmentation job while a perception worker is alive and doubles its jobs' `max_runtime`, and an `eligible_before` 15 s earlier ([job claim](../../../docs/database/README.md#job-claim-with-slots-); [contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)). The other patterns (limits, garden map versions, indicator answers) follow the same shape, each `THROW` code in the `thrown` module and mapped by `into_store` or by its caller ([store 5](../../../docs/backend/store.md#classifying-failures)).

**The claim's wake-up.** The claim is a long poll: the job broker waits on the store's `jobs_queued` `Notify`, with a 1-second fallback poll ([database 4, job claim](../../../docs/database/README.md#job-claim-with-slots-)). That `Notify` is the only wake-up, so every write that makes a job claimable signals it after its commit: an enqueue, a step commit or run start that queues one, an upload's job, a retryable failure's requeue, and `requeue_stale` when any job went back to `queued`; no other component signals the broker ([backend store 18](../../../docs/backend/store.md#18-concurrency-and-lifetimes)). `notify_waiters` wakes only tasks already waiting ([`Notify::notify_waiters`](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Notify.html#method.notify_waiters)), so a waiter creates its `notified()` future and calls `enable()` before it claims, and the fallback poll covers the rest.

**Requeues ([contracts 12](../../../docs/architecture/contracts.md#12-open-questions), question 20, settled).** There is no lease hand-back call: a stopping worker's jobs are requeued when its heartbeat is 30 seconds old or its lease has expired ([contracts 5, leases](../../../docs/architecture/contracts.md#leases-runs-and-attempts); [backend store 21](../../../docs/backend/store.md#21-decisions-on-open-questions); [worker 16](../../../docs/backend/worker.md#16-decisions-on-the-contracts-open-questions)). `requeue_stale` selects exactly those and clears the token, so the old lease can never complete the job. It also applies contracts 5's attempt rules ([contracts 5, leases](../../../docs/architecture/contracts.md#leases-runs-and-attempts); `domain::rules::jobs`, [backend domain 7](../../../docs/backend/domain.md#jobs)): the first two requeues are free, every later one counts an attempt, a job at 3 attempts fails for good instead of going back, and a `guided` job that does marks its check `failed` in the same transaction. ✓ `requeues_count_attempts_fail_guided_checks_and_wake_the_broker` runs a job through five requeues to `failed` with 3 attempts, and a guided job's check to `failed`. It signals the broker only when a job went back to `queued`.

**Step stamps ([contracts 12](../../../docs/architecture/contracts.md#12-open-questions), question 12, settled).** Whole seconds are a wire matter only; the database keeps full precision, and a step's stamp is the bound `$now`, compared exactly by the commit (`pipeline_step_at != $stamp`), so the store never rounds an instant ([contracts 1, wire format](../../../docs/architecture/contracts.md#wire-format); [backend store 11, 21](../../../docs/backend/store.md#21-decisions-on-open-questions)). ✓ A stamp with nanoseconds reads back unchanged, and the same stamp rounded to the second loses the claim.

**`seq` ([contracts 12](../../../docs/architecture/contracts.md#12-open-questions), question 22, settled).** The `seq` comes from `append_scan_event`'s transaction: the scan row is the counter, and the increment and the `CREATE` commit together or not at all. So the append is gap-free from any number of writers in any number of processes, and the per-scan writer's lifecycle affects ordering latency, not correctness ([contracts 4, transport](../../../docs/architecture/contracts.md#transport); [backend store 21](../../../docs/backend/store.md#21-decisions-on-open-questions)). ✓ 40 concurrent appends on a 4-thread runtime commit exactly `seq` 1…40.

**Duplicates race too.** A single `CREATE` or `INSERT RELATION` is its own transaction, and when several writers create the same new key at once, the losers mostly get `AlreadyExists` but some get `TransactionConflict` ([database 4, idempotency keys](../../../docs/database/README.md#idempotency-keys-): 8 of 320 in one run on the server). `run_tx` retries the conflict, and the retry then sees `AlreadyExists` and gives the contracts 1 answer (replay, `409 request_in_progress` or `422`) instead of a `500` ([store 15](../../../docs/backend/store.md#15-idempotency-usage-and-the-weather-cache)). ✓ `duplicate_writes_from_many_tasks_never_fail` runs 8 writers per key on a 4-thread runtime.

**Parameter names** ([database 8, caveat 16](../../../docs/database/README.md#8-caveats)). ✓ `access`, `auth`, `token` and `session` are protected: binding one fails the whole request with "'auth' is a protected variable and cannot be set" (`PROTECTED_PARAM_NAMES` in `surrealdb-cnf-3.3.4/src/lib.rs`). So examples never bind `token`: the store binds `auth_id`, `lease_token` and `push_token`. Names the engine gives a meaning inside a statement can be bound, but the bind doesn't always win: a bound `$before` or `$after` is used in a `SELECT` and in an `UPDATE`'s `SET` and `WHERE`, while in a `RETURN` clause the record's own wins; a bound `$this` is ignored inside a record statement and a bound `$parent` inside a subquery. ✓ A bound `before` is overridden in `DELETE … RETURN VALUE $before.id`, which returns the deleted record's id. So binds avoid `before`, `this` and `parent`; `$after` is bound only where it is read in a `SELECT` (database 4's replay range, ✓), which caveat 16 calls safe. The page cursor is `$cursor` ([database 5](../../../docs/database/README.md#5-reads-indexes-and-vector-search)) and the batch's parent `$target`.

**Points.** ✓ A point literal doesn't take parameters: `centroid = ($lon, $lat)` is a parse error. Bind `type::point([$lon, $lat])`, longitude first ([database 8, caveat 17](../../../docs/database/README.md#8-caveats); [`type::point`](https://surrealdb.com/docs/reference/query-language/functions/database-functions/type#typepoint)).

## The module

The tests run on the embedded engine, one `TestDb::memory()` per test, in the normal run ([rust-testing, database.md](../rust-testing/database.md#1-the-embedded-engine)); those that race use a 4-thread runtime, because conflicts never happen on a current-thread one. `users_and_sessions` needs the server's users and the WebSocket, so it is `#[ignore = "needs SurrealDB"]` and runs with `--live`, on a `TestDb::server()` namespace of its own.

```rust
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use surrealdb::Surreal;
use surrealdb::engine::any::{Any, connect};
use surrealdb::opt::auth::{Database, Root};
use surrealdb::types::{Number, RecordId, RecordIdKey, SurrealValue, Value};
use tokio::sync::Notify;

/// Stand-ins for `domain::id::{JobId, PlanId}` (backend domain 4).
pub type JobId = String;
pub type PlanId = String;

/// The bare key of a ULID-keyed record id, as the domain's ids and the wire carry it.
pub fn key_of(id: &RecordId) -> Option<String> {
    match &id.key {
        RecordIdKey::String(key) => Some(key.clone()),
        _ => None,
    }
}

// ---- connecting (database 1 and 6, store 4) ---------------------------------------------

/// The API's namespace and database (database 1); server tests pass their own namespace.
pub const NAMESPACE: &str = "pdm";
pub const DATABASE: &str = "main";

/// Applies the schema over a short-lived root connection, before the API serves traffic.
/// Production passes the store crate's `SCHEMA`, the `include_str!` in the store crate's `src/client.rs`.
pub async fn apply_schema(
    url: &str,
    root_user: &str,
    root_pass: &str,
    schema: &str,
) -> Result<(), StoreError> {
    let root = connect(url)
        .await
        .map_err(|e| classify(e).into_store("connect as root"))?;
    root.signin(Root {
        username: root_user.to_owned(),
        password: root_pass.to_owned(),
    })
    .await
    .map_err(|e| classify(e).into_store("sign in as root"))?;
    run_tx(&root, schema, |q| q)
        .await
        .map(drop)
        .map_err(|f| f.into_store("apply schema"))
}

pub async fn connect_api(
    url: &str,
    namespace: &str,
    password: &str,
) -> Result<Surreal<Any>, StoreError> {
    let db = connect(url)
        .await
        .map_err(|e| classify(e).into_store("connect"))?;
    db.signin(Database {
        namespace: namespace.to_owned(),
        database: DATABASE.to_owned(),
        username: "api".to_owned(),
        password: password.to_owned(),
    })
    .await
    .map_err(|e| classify(e).into_store("sign in"))?;
    db.use_ns(namespace)
        .use_db(DATABASE)
        .await
        .map_err(|e| classify(e).into_store("select database"))?;
    Ok(db)
}

// ---- scan events (database 4, store 10) -------------------------------------------------

/// database 4's append with store 10's guard for a missing scan and `ts` bound from the
/// clock. Statements: 0 `BEGIN`, 1 `LET`, 2 `IF`, 3 `CREATE` (the event), 4 `COMMIT`.
const APPEND_EVENT: &str = "BEGIN;
LET $seq = (UPDATE ONLY $scan SET last_event_seq += 1 RETURN VALUE last_event_seq);
IF $seq = NONE { THROW 'not_found' };
CREATE type::record('scan_event', [$scan, $seq]) SET run = $run, type = $type, payload = $payload, ts = $ts;
COMMIT;";
const APPEND_EVENT_RESULT: usize = 3;

/// store 10's replay: database 4's key-range read, scoped to the proof's garden and skipping a
/// scan, plant or garden being deleted. `$after` is a number: `$after + 1` with NONE fails
/// ("Cannot perform addition with 'none' and 'int'"). Statements: 0 `IF`, 1 `SELECT`.
const EVENTS_AFTER: &str =
    "IF array::len(SELECT VALUE id FROM $scan WHERE garden = $garden AND deleting_at = NONE
      AND plant.deleting_at = NONE AND garden.deleting_at = NONE) = 0 { THROW 'not_found' };
SELECT * FROM scan_event:[$scan, $after + 1]..=[$scan, ..];";
const EVENTS_AFTER_RESULT: usize = 1;

#[derive(Debug, SurrealValue)]
pub struct ScanEventRow {
    pub id: RecordId,
    pub run: i64,
    #[surreal(rename = "type")]
    pub kind: String,
    pub payload: serde_json::Value,
    pub ts: DateTime<Utc>,
}

impl ScanEventRow {
    /// The key is `[scan, seq]`.
    pub fn seq(&self) -> Option<i64> {
        let RecordIdKey::Array(key) = &self.id.key else {
            return None;
        };
        match key.get(1) {
            Some(Value::Number(Number::Int(seq))) => Some(*seq),
            _ => None,
        }
    }
}

pub async fn append_scan_event(
    db: &Surreal<Any>,
    scan: &RecordId,
    run: i64,
    kind: &str,
    payload: &serde_json::Value,
    ts: DateTime<Utc>,
) -> Result<i64, StoreError> {
    const WHAT: &str = "append event";
    let mut results = run_tx(db, APPEND_EVENT, |q| {
        q.bind(("scan", scan.clone()))
            .bind(("run", run))
            .bind(("type", kind.to_owned()))
            .bind(("payload", payload.clone()))
            .bind(("ts", ts))
    })
    .await
    .map_err(|f| f.into_store(WHAT))?;
    let event: Option<ScanEventRow> = results
        .take(APPEND_EVENT_RESULT)
        .map_err(|e| take_failed(WHAT, e))?;
    event
        .as_ref()
        .and_then(ScanEventRow::seq)
        .ok_or(StoreError::NotFound)
}

/// `garden`: the membership proof's garden; `after`: the `Last-Event-ID`, 0 to replay from
/// the first event. A scan of another garden, or one being deleted, is `NotFound`.
pub async fn events_after(
    db: &Surreal<Any>,
    garden: &RecordId,
    scan: &RecordId,
    after: i64,
) -> Result<Vec<ScanEventRow>, StoreError> {
    const WHAT: &str = "replay events";
    let mut results = run_tx(db, EVENTS_AFTER, |q| {
        q.bind(("scan", scan.clone()))
            .bind(("garden", garden.clone()))
            .bind(("after", after))
    })
    .await
    .map_err(|f| f.into_store(WHAT))?;
    results
        .take(EVENTS_AFTER_RESULT)
        .map_err(|e| take_failed(WHAT, e))
}

// ---- plan compare-and-set (database 4, store 13) ----------------------------------------

/// store 13's `plan_cas!` in its own transaction: database 4's compare-and-set, then what a
/// supersede does (the old plan's open tasks dismissed with `plan_superseded`, its trigger
/// rules deactivated) and the new tasks and rules, each skipped when its plant isn't live in
/// the garden (contracts 7). Statements: 0 `BEGIN`, 1 `CREATE`, 2 `LET`, 3 `IF` (the `THROW`),
/// 4 `IF` (the supersede), 5 `FOR` tasks, 6 `FOR` rules, 7 `COMMIT`.
const SET_PLAN_CAS: &str = "BEGIN;
CREATE $plan.id CONTENT $plan.row;
LET $won = (UPDATE $scan SET plan = $plan.id WHERE plan = $expected RETURN VALUE plan);
IF array::len($won) = 0 { THROW 'stale_plan' };
IF $expected != NONE {
  UPDATE task SET dismissed_at = $now, dismissed_reason = 'plan_superseded'
    WHERE plan = $expected AND done_at = NONE AND dismissed_at = NONE;
  UPDATE plan_trigger SET active = false WHERE plan = $expected;
};
FOR $t IN $tasks {
  LET $p = $t.row.plant;
  IF $p = NONE OR array::len(SELECT VALUE id FROM $p WHERE garden = $plan.row.garden AND deleting_at = NONE) > 0 {
    CREATE $t.id CONTENT $t.row;
  };
};
FOR $r IN $triggers {
  LET $p = $r.row.plant;
  IF array::len(SELECT VALUE id FROM $p WHERE garden = $plan.row.garden AND deleting_at = NONE) > 0 {
    CREATE $r.id CONTENT $r.row;
  };
};
COMMIT;";

/// The plan that won, for `StalePlan { current }`.
const CURRENT_PLAN: &str = "SELECT VALUE plan FROM ONLY $scan;";

#[derive(Debug, Clone, SurrealValue)]
#[surreal(untagged, rename_all = "snake_case")]
pub enum PlanReason {
    Initial,
    LateAnswer,
    SpeciesChanged,
    GuidedCheck,
    ChatRevision,
}

/// The `action_plan` fields schema.surql requires; constraints take their default.
#[derive(Debug, Clone, SurrealValue)]
pub struct NewPlan {
    pub scan: RecordId,
    pub plant: RecordId,
    pub garden: RecordId,
    pub version: i64,
    pub supersedes: Option<RecordId>,
    pub reason: PlanReason,
    pub provisional: bool,
    pub diagnosis: RecordId,
    pub open_question: Option<RecordId>,
    pub summary: String,
    pub steps: Vec<serde_json::Value>,
    pub watch_for: Vec<String>,
    pub citations: Vec<serde_json::Value>,
    pub llm_model: String,
    pub prompt_version: String,
}

/// `$plan`: the record id and its row, as store 13's `PlanWriteRow` binds them.
#[derive(Debug, Clone, SurrealValue)]
pub struct PlanWrite {
    pub id: RecordId,
    pub row: NewPlan,
}

/// `expected`: the plan the caller read, `None` for the first plan (`WHERE plan = NONE`).
/// `Err(Conflict(StalePlan { current }))`: another plan won, and nothing was written. This
/// example writes no tasks or trigger rules (`$tasks` and `$triggers` are empty).
pub async fn set_plan_cas(
    db: &Surreal<Any>,
    plan: &PlanWrite,
    expected: Option<&RecordId>,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    const WHAT: &str = "write plan";
    let written = run_tx(db, SET_PLAN_CAS, |q| {
        q.bind(("plan", plan.clone()))
            .bind(("scan", plan.row.scan.clone()))
            .bind(("expected", expected.cloned()))
            .bind(("tasks", Vec::<Value>::new()))
            .bind(("triggers", Vec::<Value>::new()))
            .bind(("now", now))
    })
    .await;
    let Err(failure) = written else {
        return Ok(());
    };
    // The THROW, or the unique (scan, version) backstop when two writers create the same
    // version: the CREATE fails before the THROW runs (store 13).
    let lost = failure.thrown() == Some(thrown::STALE_PLAN)
        || matches!(&failure, DbFailure::UniqueIndex { index, .. } if index == "action_plan_version");
    if !lost {
        return Err(failure.into_store(WHAT));
    }
    let mut results = run_tx(db, CURRENT_PLAN, |q| {
        q.bind(("scan", plan.row.scan.clone()))
    })
    .await
    .map_err(|f| f.into_store(WHAT))?;
    let current: Option<RecordId> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
    Err(StoreError::Conflict(Conflict::StalePlan {
        current: current.as_ref().and_then(key_of),
    }))
}

// ---- steps between jobs (database 4, store 7 and 11) ------------------------------------

/// store 7's `claim_entry!` alone in its transaction: database 4's conditional claim with
/// `$now`, plus an eligibility check: a scan that is being deleted, or whose status claims
/// nothing (`pending`, `queued`, `failed`), is `not_needed`. `$only_statuses` left unbound is
/// NONE and allows every status. Statements: 0 `BEGIN`, 1 `LET $c`, 2 `LET $eligible`,
/// 3 `LET $claimed`, 4 `IF` (the flag), 5 `RETURN`, 6 `COMMIT`.
const CLAIM_STEP: &str = "BEGIN;
LET $c = (SELECT status, deleting_at, plant.deleting_at AS plant_deleting, garden.deleting_at AS garden_deleting FROM ONLY $scan);
LET $eligible = $c != NONE AND $c.deleting_at = NONE AND $c.plant_deleting = NONE AND $c.garden_deleting = NONE
  AND !($c.status IN ['pending', 'queued', 'failed']) AND ($only_statuses = NONE OR $c.status IN $only_statuses);
LET $claimed = IF $eligible {
  (UPDATE ONLY $scan SET pipeline_step = $step, pipeline_step_run = run, pipeline_step_at = $now,
                         rescore_pending = IF $step = 'score' { false } ELSE { rescore_pending }
     WHERE pipeline_step = NONE AND plant.deleting_at = NONE AND garden.deleting_at = NONE RETURN AFTER)
} ELSE { NONE };
IF $eligible AND $claimed = NONE { UPDATE ONLY $scan SET rescore_pending = true WHERE pipeline_step != NONE; };
RETURN { outcome: IF !$eligible { 'not_needed' } ELSE IF $claimed = NONE { 'busy' } ELSE { 'claimed' },
         run: $claimed.pipeline_step_run, stamp: $claimed.pipeline_step_at, status: $c.status };
COMMIT;";
const CLAIM_STEP_RESULT: usize = 5;

/// store 11's `COMMIT_RELEASE`: `claim_check!` (database 4's re-check, plus a missing scan
/// and the claim's own run) and `handover!`, with no output of its own. Statements:
/// 0 `BEGIN`, 1 `LET $s`, 2 `IF` (the `THROW`), 3 `LET $next`, 4 `UPDATE`, 5 `RETURN $next`,
/// 6 `COMMIT`.
const COMMIT_STEP: &str = "BEGIN;
LET $s = (SELECT pipeline_step, pipeline_step_run, pipeline_step_at, run, status, rescore_pending,
                 plant.deleting_at AS plant_deleting, garden.deleting_at AS garden_deleting
          FROM ONLY $scan);
IF $s = NONE OR $s.pipeline_step != $step OR $s.pipeline_step_at != $stamp OR $s.pipeline_step_run != $s.run
   OR $s.run != $run OR $s.plant_deleting != NONE OR $s.garden_deleting != NONE { THROW 'claim_lost'; };
LET $next = IF $s.rescore_pending { 'score' } ELSE { $handover };
UPDATE $scan SET pipeline_step = $next,
                 pipeline_step_run = IF $next = NONE { NONE } ELSE { run },
                 pipeline_step_at = IF $next = NONE { NONE } ELSE { $now },
                 rescore_pending = false;
RETURN $next;
COMMIT;";
const COMMIT_STEP_RESULT: usize = 5;

/// store 11's `RECLAIM_STEP`: database 4's sweep with `$now` and the claim's run; `NONE`
/// back means someone else has it.
const RECLAIM_STEP: &str = "UPDATE ONLY $scan SET pipeline_step_at = $now
  WHERE pipeline_step = $step AND pipeline_step_at = $seen AND pipeline_step_run = run AND pipeline_step_run = $run
    AND plant.deleting_at = NONE AND garden.deleting_at = NONE
  RETURN AFTER;";

/// What `CLAIM_STEP` returns.
#[derive(Debug, SurrealValue)]
pub struct ClaimAttempt {
    /// `claimed`, `busy` (another step holds it; the scan is flagged) or `not_needed`.
    pub outcome: String,
    pub run: Option<i64>,
    pub stamp: Option<DateTime<Utc>>,
    pub status: Option<String>,
}

/// The claim fields of a scan; a row may name fewer fields than the record has.
#[derive(Debug, SurrealValue)]
pub struct ClaimRow {
    pub pipeline_step: Option<String>,
    pub pipeline_step_run: Option<i64>,
    pub pipeline_step_at: Option<DateTime<Utc>>,
    pub rescore_pending: bool,
}

pub async fn claim_step(
    db: &Surreal<Any>,
    scan: &RecordId,
    step: &str,
    now: DateTime<Utc>,
) -> Result<ClaimAttempt, StoreError> {
    const WHAT: &str = "claim step";
    let mut results = run_tx(db, CLAIM_STEP, |q| {
        q.bind(("scan", scan.clone()))
            .bind(("step", step.to_owned()))
            .bind(("now", now))
    })
    .await
    .map_err(|f| f.into_store(WHAT))?;
    let attempt: Option<ClaimAttempt> = results
        .take(CLAIM_STEP_RESULT)
        .map_err(|e| take_failed(WHAT, e))?;
    attempt.ok_or(StoreError::NotFound)
}

/// Re-checks the claim (its step, its exact stamp, its run, nothing being deleted) and hands
/// over: returns the next step, `score` when input arrived meanwhile, or `None` (released).
pub async fn commit_step(
    db: &Surreal<Any>,
    scan: &RecordId,
    step: &str,
    run: i64,
    stamp: DateTime<Utc>,
    handover: Option<&str>,
    now: DateTime<Utc>,
) -> Result<Option<String>, StoreError> {
    const WHAT: &str = "commit step";
    let mut results = run_tx(db, COMMIT_STEP, |q| {
        q.bind(("scan", scan.clone()))
            .bind(("step", step.to_owned()))
            .bind(("run", run))
            .bind(("stamp", stamp))
            .bind(("handover", handover.map(str::to_owned)))
            .bind(("now", now))
    })
    .await
    .map_err(|f| f.into_store(WHAT))?;
    results
        .take(COMMIT_STEP_RESULT)
        .map_err(|e| take_failed(WHAT, e))
}

pub async fn reclaim_step(
    db: &Surreal<Any>,
    scan: &RecordId,
    step: &str,
    run: i64,
    seen: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<Option<ClaimRow>, StoreError> {
    const WHAT: &str = "reclaim step";
    let mut results = run_tx(db, RECLAIM_STEP, |q| {
        q.bind(("scan", scan.clone()))
            .bind(("step", step.to_owned()))
            .bind(("run", run))
            .bind(("seen", seen))
            .bind(("now", now))
    })
    .await
    .map_err(|f| f.into_store(WHAT))?;
    results.take(0).map_err(|e| take_failed(WHAT, e))
}

// ---- jobs (database 4, contracts 5, store 14) -------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, SurrealValue)]
#[surreal(untagged, rename_all = "snake_case")]
pub enum JobKind {
    Perception,
    Vlm,
    Verify,
    Guided,
}

/// This skill's: store 14's `ENQUEUE_JOB` isn't listed. Its `payload` is the job's own.
const ENQUEUE_JOB: &str = "CREATE $job SET kind = $kind, scan = $scan, run = $run, question = $question, payload = $payload, max_runtime = $max_runtime, queued_at = $now;";

/// A job to queue (contracts 5, payloads); `run` is `None` for a guided job.
#[derive(Debug, Clone)]
pub struct NewJob {
    pub id: RecordId,
    pub kind: JobKind,
    pub scan: RecordId,
    pub run: Option<i64>,
    pub question: Option<RecordId>,
    pub payload: serde_json::Value,
    pub max_runtime: Duration,
}

/// Creates a queued job, then wakes every waiting claim: `jobs_queued` is the broker's only
/// wake-up, signalled after the commit (store 18).
pub async fn enqueue_job(
    db: &Surreal<Any>,
    jobs_queued: &Notify,
    job: &NewJob,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    run_tx(db, ENQUEUE_JOB, |q| {
        q.bind(("job", job.id.clone()))
            .bind(("kind", job.kind))
            .bind(("scan", job.scan.clone()))
            .bind(("run", job.run))
            .bind(("question", job.question.clone()))
            .bind(("payload", job.payload.clone()))
            .bind(("max_runtime", job.max_runtime))
            .bind(("now", now))
    })
    .await
    .map_err(|f| f.into_store("enqueue job"))?;
    jobs_queued.notify_waiters();
    Ok(())
}

/// store 14's claim: database 4's statement with `$now` for `time::now()` and the fallback's
/// filter (`$hold_question`: hold a photo answer's job while a perception worker is alive),
/// plus two additions: the fallback's `max_runtime` doubled at claim (`$on_fallback`,
/// contracts 5) and a run's first job moving the scan `queued → analysing` in the same
/// transaction. Statements: 0 `BEGIN`, 1 `LET $held`, 2 `LET $perception_alive`,
/// 3 `LET $claimed`, 4 `LET $job`, 5 `LET $started`, 6 the result, 7 `COMMIT`.
const CLAIM_JOB: &str = "BEGIN;
LET $held = (SELECT count() AS n FROM job WHERE worker = $worker AND status = 'leased' GROUP ALL)[0].n ?? 0;
LET $perception_alive = ((SELECT count() AS n FROM worker WHERE caps CONTAINS 'perception' AND last_heartbeat > $now - 30s GROUP ALL)[0].n ?? 0) > 0;
LET $claimed = IF $held < $slots {
  (UPDATE (SELECT VALUE id FROM job
          WHERE status = 'queued' AND kind IN $kinds AND queued_at <= $eligible_before
            AND (!$hold_question OR question = NONE OR !$perception_alive)
          ORDER BY queued_at LIMIT 1)
  SET status = 'leased', worker = $worker, lease_token = rand::ulid(),
      claimed_at = $now, lease_until = $now + 2m,
      max_runtime = IF $on_fallback { max_runtime * 2 } ELSE { max_runtime }
  WHERE status = 'queued'
  RETURN AFTER)
} ELSE { [] };
LET $job = $claimed[0];
LET $started = IF $job != NONE AND $job.run != NONE {
  array::len(UPDATE $job.scan SET status = 'analysing' WHERE status = 'queued' AND run = $job.run RETURN VALUE id) > 0
} ELSE { false };
RETURN { job: $job, started_analysing: $started };
COMMIT;";
const CLAIM_JOB_RESULT: usize = 6;

#[derive(Debug, SurrealValue)]
pub struct LeasedJob {
    pub id: RecordId,
    pub kind: JobKind,
    pub scan: RecordId,
    pub run: Option<i64>,
    pub payload: serde_json::Value,
    pub lease_token: String,
    pub lease_until: DateTime<Utc>,
    pub max_runtime: Duration,
    pub attempts: i64,
}

#[derive(Debug, SurrealValue)]
pub struct Claimed {
    pub job: Option<LeasedJob>,
    /// The claim moved the scan from `queued` to `analysing`.
    pub started_analysing: bool,
}

/// Who claims: the worker's id, slots and kinds. The fallback worker sets `fallback`: it
/// holds back a photo answer's segmentation job while a perception worker is alive, and its
/// jobs get twice the runtime (contracts 5).
#[derive(Debug, Clone, Copy)]
pub struct Claimant<'a> {
    pub worker: &'a str,
    pub slots: i64,
    pub kinds: &'a [JobKind],
    pub fallback: bool,
}

/// `eligible_before`: `now` for the GPU workers, 15 s earlier for the fallback (contracts 5).
pub async fn claim_job(
    db: &Surreal<Any>,
    who: Claimant<'_>,
    eligible_before: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<Claimed, StoreError> {
    const WHAT: &str = "claim job";
    let mut results = run_tx(db, CLAIM_JOB, |q| {
        q.bind(("worker", who.worker.to_owned()))
            .bind(("slots", who.slots))
            .bind(("kinds", who.kinds.to_vec()))
            .bind(("eligible_before", eligible_before))
            .bind(("hold_question", who.fallback))
            .bind(("on_fallback", who.fallback))
            .bind(("now", now))
    })
    .await
    .map_err(|f| f.into_store(WHAT))?;
    let claimed: Option<Claimed> = results
        .take(CLAIM_JOB_RESULT)
        .map_err(|e| take_failed(WHAT, e))?;
    claimed.ok_or(StoreError::NotFound)
}

/// `domain::rules::jobs`: every requeue after the 2nd counts as an attempt, and a job is out
/// of attempts at 3 (backend domain 7, jobs; contracts 5).
pub const FREE_REQUEUES: i64 = 2;
pub const MAX_JOB_ATTEMPTS: i64 = 3;

/// store 14's `REQUEUE_STALE`, verbatim: expired leases and the leases of workers silent for
/// 30 s (question 20). Each is requeued with its token cleared and `queued_at` reset, counting
/// an attempt after the free requeues; a job out of attempts fails for good instead, and a
/// `guided` job that does marks its check `failed` in the same transaction (contracts 5).
/// Statements: 0 `BEGIN`, 1–3 `LET`, 4 `FOR`, 5 the requeued jobs, 6 `COMMIT`.
const REQUEUE_STALE: &str = "BEGIN;
LET $silent = (SELECT VALUE record::id(id)[0] FROM worker WHERE last_heartbeat < $now - 30s);
LET $expired = (SELECT VALUE id FROM job WHERE status = 'leased' AND lease_until < $now);
LET $orphaned = (SELECT VALUE id FROM job WHERE status = 'leased' AND worker IN $silent);
FOR $j IN array::union($expired, $orphaned) {
  LET $r = (SELECT requeues, attempts FROM ONLY $j);
  LET $att = IF $r.requeues >= $free_requeues { $r.attempts + 1 } ELSE { $r.attempts };
  UPDATE $j SET requeues = $r.requeues + 1, attempts = $att,
    status = IF $att >= $max_attempts { 'failed' } ELSE { 'queued' },
    finished_at = IF $att >= $max_attempts { $now } ELSE { NONE },
    worker = NONE, lease_token = NONE, lease_until = NONE, claimed_at = NONE, queued_at = $now;
  IF $att >= $max_attempts AND $j.kind = 'guided' {
    UPDATE guided_check SET status = 'failed'
      WHERE scan = $j.scan AND status = 'analysing' AND image = type::record('scan_image', $j.payload.image.image_id);
  };
};
SELECT id, kind, scan, run, question, status FROM job WHERE id IN array::union($expired, $orphaned);
COMMIT;";
const REQUEUE_STALE_RESULT: usize = 5;

#[derive(Debug, SurrealValue)]
pub struct RequeuedJob {
    pub id: RecordId,
    pub kind: JobKind,
    /// `queued`, or `failed` when it ran out of attempts.
    pub status: String,
}

/// Returns the jobs it touched, and wakes the broker when any went back to `queued`.
pub async fn requeue_stale(
    db: &Surreal<Any>,
    jobs_queued: &Notify,
    now: DateTime<Utc>,
) -> Result<Vec<RequeuedJob>, StoreError> {
    const WHAT: &str = "requeue stale jobs";
    let mut results = run_tx(db, REQUEUE_STALE, |q| {
        q.bind(("now", now))
            .bind(("free_requeues", FREE_REQUEUES))
            .bind(("max_attempts", MAX_JOB_ATTEMPTS))
    })
    .await
    .map_err(|f| f.into_store(WHAT))?;
    let jobs: Vec<RequeuedJob> = results
        .take(REQUEUE_STALE_RESULT)
        .map_err(|e| take_failed(WHAT, e))?;
    if jobs.iter().any(|job| job.status == "queued") {
        jobs_queued.notify_waiters();
    }
    Ok(jobs)
}

// ---- idempotency keys (database 4, contracts 1, store 15) -------------------------------

/// database 4's `CREATE` with `$now` for `time::now()`.
const BEGIN_IDEMPOTENT: &str = "CREATE type::record('idempotency_key', [$user, $route, $key])
  SET fingerprint = $fp, expires_at = $now + 24h;";
const READ_IDEMPOTENT: &str =
    "SELECT * FROM ONLY type::record('idempotency_key', [$user, $route, $key]);";

#[derive(Debug, Clone, Copy, PartialEq, Eq, SurrealValue)]
#[surreal(untagged, rename_all = "snake_case")]
pub enum KeyState {
    Running,
    Done,
}

#[derive(Debug, SurrealValue)]
pub struct StoredKey {
    pub fingerprint: String,
    pub state: KeyState,
    pub response_status: Option<i64>,
    pub response_body: Option<serde_json::Value>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug)]
pub enum Begin {
    Started,
    /// contracts 1 decides: replay, `409 request_in_progress` or `422 idempotency_conflict`.
    Existing(StoredKey),
}

/// A racing duplicate `CREATE` can lose with a conflict instead of "already exists";
/// `run_tx` retries it, and the retry gets "already exists". A key the cleanup deleted
/// between the two statements is created again; store 15 also replaces an expired one.
pub async fn begin_idempotent(
    db: &Surreal<Any>,
    user: &RecordId,
    route: &str,
    key: &str,
    fingerprint: &str,
    now: DateTime<Utc>,
) -> Result<Begin, StoreError> {
    const WHAT: &str = "begin idempotent request";
    for _ in 0..2 {
        let created = run_tx(db, BEGIN_IDEMPOTENT, |q| {
            bind_key(q, user, route, key)
                .bind(("fp", fingerprint.to_owned()))
                .bind(("now", now))
        })
        .await;
        match created {
            Ok(_) => return Ok(Begin::Started),
            Err(DbFailure::AlreadyExists(_)) => {}
            Err(failure) => return Err(failure.into_store(WHAT)),
        }
        let mut results = run_tx(db, READ_IDEMPOTENT, |q| bind_key(q, user, route, key))
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let stored: Option<StoredKey> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        if let Some(stored) = stored {
            return Ok(Begin::Existing(stored));
        }
    }
    Err(StoreError::Conflict(Conflict::Retries))
}

/// The key's three binds, shared by both statements: a function, so one named `'c` ties the
/// query it takes to the query it returns.
fn bind_key<'c>(
    query: surrealdb::method::Query<'c, Any>,
    user: &RecordId,
    route: &str,
    key: &str,
) -> surrealdb::method::Query<'c, Any> {
    query
        .bind(("user", user.clone()))
        .bind(("route", route.to_owned()))
        .bind(("key", key.to_owned()))
}

// ---- membership and alert dedupe (database 4) -------------------------------------------

/// database 4, verbatim.
const ADD_MEMBER: &str = "INSERT RELATION INTO member_of { id: type::record('member_of', [$user, $garden]), in: $user, out: $garden, role: $role };";

#[derive(Debug, Clone, Copy, SurrealValue)]
#[surreal(untagged, rename_all = "snake_case")]
pub enum Role {
    Owner,
    Editor,
    Viewer,
    Reviewer,
}

/// `Err(Conflict(AlreadyExists))`: the user is a member already; the role is unchanged.
pub async fn add_member(
    db: &Surreal<Any>,
    user: &RecordId,
    garden: &RecordId,
    role: Role,
) -> Result<(), StoreError> {
    run_tx(db, ADD_MEMBER, |q| {
        q.bind(("user", user.clone()))
            .bind(("garden", garden.clone()))
            .bind(("role", role))
    })
    .await
    .map(drop)
    .map_err(|f| f.into_store("add member"))
}

const CREATE_ALERT_TASK: &str = "CREATE task SET garden = $garden, title = $title, source = 'forecast_alert', alert_key = $alert_key;";

/// `Ok(false)`: a task with this alert key exists already (architecture 8.3).
pub async fn create_alert_task(
    db: &Surreal<Any>,
    garden: &RecordId,
    title: &str,
    alert_key: &str,
) -> Result<bool, StoreError> {
    let created = run_tx(db, CREATE_ALERT_TASK, |q| {
        q.bind(("garden", garden.clone()))
            .bind(("title", title.to_owned()))
            .bind(("alert_key", alert_key.to_owned()))
    })
    .await;
    match created {
        Ok(_) => Ok(true),
        Err(DbFailure::UniqueIndex { index, .. }) if index == "task_alert" => Ok(false),
        Err(failure) => Err(failure.into_store("create alert task")),
    }
}

// ---- reads and deletes (database 4 and 5, store 17) -------------------------------------

/// One statement for every page: with `$cursor` NONE, `id > $cursor` holds for every
/// record. The cursor is the ULID id, which is unique, so no row is skipped or repeated.
/// `EXPLAIN`: `IndexScan` on `plant_garden (garden, name)` for the filter, then
/// `SortTopKByKey` on `id`.
const PLANTS_PAGE: &str = "SELECT * FROM plant WHERE garden = $garden AND deleting_at = NONE AND id > $cursor ORDER BY id LIMIT 50;";

#[derive(Debug, SurrealValue)]
pub struct PlantRow {
    pub id: RecordId,
    pub name: String,
}

/// `cursor`: the last id of the previous page, `None` for the first page.
pub async fn plants_page(
    db: &Surreal<Any>,
    garden: &RecordId,
    cursor: Option<&RecordId>,
) -> Result<Vec<PlantRow>, StoreError> {
    const WHAT: &str = "plants page";
    let mut results = run_tx(db, PLANTS_PAGE, |q| {
        q.bind(("garden", garden.clone()))
            .bind(("cursor", cursor.cloned()))
    })
    .await
    .map_err(|f| f.into_store(WHAT))?;
    results.take(0).map_err(|e| take_failed(WHAT, e))
}

const DELETE_SCAN_EVENTS: &str = "DELETE scan_event:[$scan, 1]..=[$scan, ..] RETURN NONE;";

pub async fn delete_scan_events(db: &Surreal<Any>, scan: &RecordId) -> Result<(), StoreError> {
    run_tx(db, DELETE_SCAN_EVENTS, |q| q.bind(("scan", scan.clone())))
        .await
        .map(drop)
        .map_err(|f| f.into_store("delete scan events"))
}

/// One batch: `DELETE (SELECT VALUE id FROM <t> WHERE <f> = $target LIMIT 500) RETURN VALUE
/// $before.id`. `RETURN NONE` gives `[]` whether it deleted rows or not; `RETURN VALUE id`
/// gives NONE per row.
pub const DELETE_OBSERVATIONS_OF_SCAN: &str = "DELETE (SELECT VALUE id FROM observation WHERE scan = $target LIMIT 500) RETURN VALUE $before.id;";

/// Repeats `batch` until it deletes nothing; returns the rows deleted. The ids come back as
/// an array, so they are taken as `Vec`.
pub async fn delete_in_batches(
    db: &Surreal<Any>,
    batch: &'static str,
    target: &RecordId,
) -> Result<usize, StoreError> {
    const WHAT: &str = "delete batch";
    let mut deleted = 0;
    loop {
        let mut results = run_tx(db, batch, |q| q.bind(("target", target.clone())))
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let ids: Vec<RecordId> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        if ids.is_empty() {
            return Ok(deleted);
        }
        deleted += ids.len();
    }
}

// ---- shared definitions, copied from their canonical blocks ------------------------------

// copy of: rust-errors/mapping.md BoxError
/// A boxed error from a library, kept as the source of the log chain (`domain::ports`).
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;
// end: BoxError

// copy of: rust-errors/mapping.md DomainError
/// The variants of `domain::error::DomainError` (backend domain 6) that these examples use,
/// as the design writes them. The full enum also has `Forbidden`, `UnknownPlants`,
/// `InvalidState`, `Transition`, `TooManyImages`, `MissingRequiredShot`, `BudgetExceeded`,
/// `RequestInProgress` and `LeaseLost`, and maps each to its code in `kind()`.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DomainError {
    /// The record doesn't exist, is being deleted, or the caller may not see it. Always 404,
    /// never 403, for non-members ([contracts 1], roles and access).
    #[error("not found")]
    NotFound,
    /// A request field breaks a rule that serde can't express (exactly one of two fields, a
    /// length, a range).
    #[error("{field}: {problem}")]
    InvalidRequest {
        /// The field, as a dotted path.
        field: String,
        /// What is wrong.
        problem: String,
    },
    /// The same idempotency key was used with another body.
    #[error("idempotency key reused with a different body")]
    IdempotencyConflict,
}
// end: DomainError

// copy of: rust-errors/mapping.md StoreError
/// Which compare-and-set or uniqueness check lost ([database 4]). The handler maps it to the
/// right contract code before `?`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conflict {
    /// A plan write lost: `stale_plan`; carries the scan's current plan.
    StalePlan {
        /// The plan that won, if any.
        current: Option<PlanId>,
    },
    /// A step's claim, stamp or run no longer holds: `claim_lost`. The step stops.
    ClaimLost,
    /// A question step's claim no longer holds.
    QuestionStepLost,
    /// The lease token, worker or status no longer matches: `409 lease_lost`.
    LeaseLost {
        /// The job.
        job_id: JobId,
    },
    /// A map upload's `base_version` is stale: `409` with `current_map_version`.
    StaleMap {
        /// The garden's current version.
        current_version: u32,
    },
    /// A record with that key exists (idempotency key, membership, alert key).
    AlreadyExists,
    /// Transaction conflicts still failing after the store's 5 retries.
    Retries,
}

/// The store's error ([database 4], [database 6]).
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// No such record, or one being deleted, or one outside the proof's garden.
    #[error("record not found")]
    NotFound,
    /// A compare-and-set or uniqueness check lost.
    #[error("conflict: {0:?}")]
    Conflict(Conflict),
    /// A rule checked inside the transaction refused the write (the last owner, a dead plant,
    /// the daily scan limit, unknown map plants).
    #[error("rejected by a rule")]
    Rejected(#[source] DomainError),
    /// `SurrealDB` can't be reached.
    #[error("database unavailable")]
    Unavailable(#[source] BoxError),
    /// A query failed for another reason: a bug or a schema mismatch.
    #[error("query failed: {what}")]
    Query {
        /// The operation, such as `"append event"`.
        what: &'static str,
        /// The SDK error.
        #[source]
        source: BoxError,
    },
}

impl From<DomainError> for StoreError {
    fn from(e: DomainError) -> Self {
        match e {
            DomainError::NotFound => Self::NotFound,
            other => Self::Rejected(other),
        }
    }
}
// end: StoreError

// copy of: rust-surrealdb/errors-and-retries.md retry_conflicts
/// Attempts per transaction, the first included ([database 4], [database 6]).
pub const MAX_ATTEMPTS: u32 = 5;

/// The back-off before retry `attempt` (0-based): 10–50 ms of jitter, doubled per attempt, so
/// the four sleeps total 150–750 ms. The jitter is drawn from the thread-local generator and
/// dropped before the `.await`, so the caller's future stays `Send`.
pub fn backoff(attempt: u32) -> std::time::Duration {
    let jitter_ms: u64 = rand::random_range(10..=50);
    std::time::Duration::from_millis(jitter_ms << attempt)
}

/// How the server prefixes the message of a `THROW`.
pub const THROWN_PREFIX: &str = "An error occurred: ";

/// The codes the store's statements `THROW` (backend store 5). A thrown error carries no
/// typed details, so the code is the text after [`THROWN_PREFIX`].
pub mod thrown {
    /// The plan compare-and-set lost ([database 4], plan writes).
    pub const STALE_PLAN: &str = "stale_plan";
    /// A step's claim, stamp or run no longer holds ([database 4], steps between jobs).
    pub const CLAIM_LOST: &str = "claim_lost";
    /// A photo answer's step no longer holds.
    pub const QUESTION_STEP_LOST: &str = "question_step_lost";
    /// The lease token, worker or status no longer matches.
    pub const LEASE_LOST: &str = "lease_lost";
    /// A record the statement needs is missing, being deleted or outside the garden.
    pub const NOT_FOUND: &str = "not_found";
    /// A map upload's `base_version` is stale ([database 4], garden map versions).
    pub const STALE_MAP: &str = "stale_map";
    /// The daily scan limit ([database 4], limits).
    pub const BUDGET_EXCEEDED: &str = "budget_exceeded";
    /// A state check refused the write (a scan that isn't `pending`, the last owner).
    pub const INVALID_STATE: &str = "invalid_state";
    /// A map upload pins plants that aren't the garden's.
    pub const UNKNOWN_PLANTS: &str = "unknown_plants";
}

/// What one failed statement means, before the caller adds context. Crate-private in the
/// store: no `surrealdb` type leaves the crate.
#[derive(Debug)]
pub enum DbFailure {
    /// `QueryError::TransactionConflict`: retried; reaching a caller means the retries ran out.
    TxConflict(surrealdb::Error),
    /// `THROW '<code>'` (kind `Thrown`, message `"An error occurred: <code>"`).
    Thrown {
        /// The code after the prefix.
        code: String,
        /// The SDK error.
        source: surrealdb::Error,
    },
    /// `CREATE` of an existing id or `INSERT RELATION` of an existing edge
    /// (`AlreadyExistsError::Record`).
    AlreadyExists(surrealdb::Error),
    /// A `UNIQUE` index refused the write: kind `Internal` with no details on 3.3.4, only the
    /// message ``Database index `<name>` already contains …``.
    UniqueIndex {
        /// The index name.
        index: String,
        /// The SDK error.
        source: surrealdb::Error,
    },
    /// The connection is gone (kind `Connection`).
    Unavailable(surrealdb::Error),
    /// Anything else: a bug, a schema mismatch, a type the schema refused.
    Other(surrealdb::Error),
}

/// The statement that caused a failure: the lowest-index error that isn't one a failed
/// transaction merely skipped (`NotExecuted` before the cause, `Cancelled` after it). When
/// every error is a skip, the lowest-index one.
pub fn first_failure(
    errors: impl IntoIterator<Item = (usize, surrealdb::Error)>,
) -> Option<surrealdb::Error> {
    let mut errors: Vec<_> = errors.into_iter().collect();
    errors.sort_unstable_by_key(|(index, _)| *index);
    let mut first = None;
    for (_, error) in errors {
        let skipped = matches!(
            error.query_details(),
            Some(
                surrealdb::types::QueryError::NotExecuted | surrealdb::types::QueryError::Cancelled
            )
        );
        if !skipped {
            return Some(error);
        }
        if first.is_none() {
            first = Some(error);
        }
    }
    first
}

/// The one place SDK errors are classified.
pub fn classify(error: surrealdb::Error) -> DbFailure {
    if matches!(
        error.query_details(),
        Some(surrealdb::types::QueryError::TransactionConflict)
    ) {
        return DbFailure::TxConflict(error);
    }
    if matches!(
        error.already_exists_details(),
        Some(surrealdb::types::AlreadyExistsError::Record { .. })
    ) {
        return DbFailure::AlreadyExists(error);
    }
    if error.is_connection() {
        return DbFailure::Unavailable(error);
    }
    if error.is_thrown() {
        let code = error
            .message()
            .strip_prefix(THROWN_PREFIX)
            .unwrap_or(error.message())
            .trim_matches(['\'', '"'])
            .to_owned();
        return DbFailure::Thrown {
            code,
            source: error,
        };
    }
    if let Some(index) = unique_index(&error) {
        return DbFailure::UniqueIndex {
            index,
            source: error,
        };
    }
    DbFailure::Other(error)
}

/// The index named by ``Database index `<name>` already contains …``, the only sign of a
/// unique-index violation on 3.3.4.
fn unique_index(error: &surrealdb::Error) -> Option<String> {
    if !error.is_internal() {
        return None;
    }
    let rest = error.message().strip_prefix("Database index `")?;
    let (index, rest) = rest.split_once('`')?;
    rest.starts_with(" already contains")
        .then(|| index.to_owned())
}

impl DbFailure {
    /// The `THROW` code, if this is one.
    pub fn thrown(&self) -> Option<&str> {
        match self {
            Self::Thrown { code, .. } => Some(code),
            _ => None,
        }
    }

    /// The default mapping. `what` names the operation for the log (`"append event"`).
    pub fn into_store(self, what: &'static str) -> StoreError {
        match self {
            Self::TxConflict(_) => StoreError::Conflict(Conflict::Retries),
            Self::AlreadyExists(_) | Self::UniqueIndex { .. } => {
                StoreError::Conflict(Conflict::AlreadyExists)
            }
            Self::Unavailable(source) => StoreError::Unavailable(Box::new(source)),
            Self::Thrown { code, source } => match code.as_str() {
                thrown::STALE_PLAN => StoreError::Conflict(Conflict::StalePlan { current: None }),
                thrown::CLAIM_LOST => StoreError::Conflict(Conflict::ClaimLost),
                thrown::QUESTION_STEP_LOST => StoreError::Conflict(Conflict::QuestionStepLost),
                thrown::NOT_FOUND => StoreError::NotFound,
                _ => StoreError::Query {
                    what,
                    source: Box::new(source),
                },
            },
            Self::Other(source) => StoreError::Query {
                what,
                source: Box::new(source),
            },
        }
    }
}

/// A `take` that didn't decode: a bug or a schema drift, never a user error.
pub fn take_failed(what: &'static str, error: surrealdb::Error) -> StoreError {
    StoreError::Query {
        what,
        source: Box::new(error),
    }
}

/// Use instead of `.check()`, which returns the lowest-index error, a `NotExecuted`.
#[expect(
    clippy::result_large_err,
    reason = "the failure carries the SDK error by value and is mapped at once by the caller"
)]
pub fn checked(
    response: surrealdb::Result<surrealdb::IndexedResults>,
) -> Result<surrealdb::IndexedResults, DbFailure> {
    match response {
        Ok(mut results) => match first_failure(results.take_errors()) {
            None => Ok(results),
            Some(error) => Err(classify(error)),
        },
        // A parse error, a protected parameter or a lost connection fails the whole request.
        Err(error) => Err(classify(error)),
    }
}

/// Re-runs `attempt`, which must send the whole query again, on `TxConflict`, up to
/// [`MAX_ATTEMPTS`]. A conflicted transaction committed nothing, so a retry is always safe,
/// `+= 1` included. Any other failure returns at once: a lost compare-and-set is final.
#[expect(
    clippy::result_large_err,
    reason = "the failure carries the SDK error by value and is mapped at once by the caller"
)]
pub async fn retry_conflicts<T, Fut>(mut attempt: impl FnMut() -> Fut) -> Result<T, DbFailure>
where
    Fut: Future<Output = Result<T, DbFailure>>,
{
    let mut tries = 0;
    loop {
        match attempt().await {
            Err(DbFailure::TxConflict(_)) if tries + 1 < MAX_ATTEMPTS => {
                tokio::time::sleep(backoff(tries)).await;
                tries += 1;
            }
            Err(failure @ DbFailure::TxConflict(_)) => {
                tracing::warn!(attempts = MAX_ATTEMPTS, "transaction conflicts persisted");
                return Err(failure);
            }
            result => return result,
        }
    }
}

/// Sends `sql` (one statement, or a whole `BEGIN … COMMIT` as one query) and returns its
/// results once no statement failed. `bind` adds the parameters to a fresh query on each
/// attempt; `'c` is the borrow of the client and the statement, which the query holds.
#[expect(
    clippy::result_large_err,
    reason = "the failure carries the SDK error by value and is mapped at once by the caller"
)]
pub async fn run_tx<'c, F>(
    db: &'c surrealdb::Surreal<surrealdb::engine::any::Any>,
    sql: &'c str,
    bind: F,
) -> Result<surrealdb::IndexedResults, DbFailure>
where
    F: Fn(
            surrealdb::method::Query<'c, surrealdb::engine::any::Any>,
        ) -> surrealdb::method::Query<'c, surrealdb::engine::any::Any>
        + Send,
{
    retry_conflicts(move || {
        let query = bind(db.query(sql));
        async move { checked(query.await) }
    })
    .await
}
// end: retry_conflicts

// The test harness; in the backend it lives in the store crate's tests/embedded/support.rs,
// with the crate's SCHEMA for TEST_SCHEMA (rust-testing, database.md).
// copy of: rust-testing/database.md TestDb
/// The real schema, backend/migrations/schema.surql, from a top-level test file
/// (`crates/<crate>/tests/*.rs`); the store crate's own harness uses its `SCHEMA`.
const TEST_SCHEMA: &str = include_str!("../../../migrations/schema.surql");

/// The `api` password in every server test namespace. The quote and backslash test
/// `surql_string`.
const TEST_API_PASS: &str = r"api-test-'pass'\1";

/// The four statements in schema.surql that select `pdm`/`main`.
const SCHEMA_HEADER: &str = "DEFINE NAMESPACE IF NOT EXISTS pdm;\nUSE NS pdm;\n\
DEFINE DATABASE IF NOT EXISTS main STRICT;\nUSE DB main;\n";

/// `schema` with its `pdm` header pointed at the namespace `ns`.
fn schema_in(schema: &str, ns: &str) -> String {
    assert!(
        schema.contains(SCHEMA_HEADER),
        "schema.surql no longer selects pdm/main with the four statements the harness rewrites"
    );
    let header = format!(
        "DEFINE NAMESPACE IF NOT EXISTS {ns};\nUSE NS {ns};\n\
         DEFINE DATABASE IF NOT EXISTS main STRICT;\nUSE DB main;\n"
    );
    schema.replacen(SCHEMA_HEADER, &header, 1)
}

/// A quoted string literal, for the one statement that takes no parameter: `DEFINE USER`.
fn surql_string(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// Where a server test's database is: the pinned image's URL and the test's namespace.
struct ServerNs {
    url: String,
    ns: String,
}

/// One test's database, with the schema applied and `main` selected.
struct TestDb {
    /// The embedded engine, or on the server a client signed in as `api` (database level,
    /// `ROLES EDITOR`), as the API runs.
    db: surrealdb::Surreal<surrealdb::engine::any::Any>,
    /// `Some` for [`TestDb::server`].
    server: Option<ServerNs>,
}

impl TestDb {
    /// The default: a new datastore of the embedded engine (`kv-mem`), shared with no other
    /// test, with the schema applied and `pdm`/`main` selected.
    async fn memory() -> Self {
        let db = surrealdb::engine::any::connect("mem://")
            .await
            .expect("start the embedded engine");
        let mut response = db.query(TEST_SCHEMA).await.expect("send the schema");
        let errors = response.take_errors();
        assert!(errors.is_empty(), "the schema failed: {errors:?}");
        db.use_ns("pdm")
            .use_db("main")
            .await
            .expect("select pdm/main");
        Self { db, server: None }
    }

    /// The pinned image at `PDM_TEST_SURREALDB_URL`, for what only the server has: a
    /// namespace of the test's own, the schema applied as root, an `api` user, and a client
    /// signed in as `api`.
    async fn server() -> Self {
        let url = std::env::var("PDM_TEST_SURREALDB_URL")
            .expect("start the test container and set PDM_TEST_SURREALDB_URL (database.md)");
        let ns = format!("test_{}", ulid::Ulid::generate().to_string().to_lowercase());
        let root = Self::signed_in_root(&url).await;
        let setup = format!(
            "{}\nDEFINE USER api ON DATABASE PASSWORD {} ROLES EDITOR;",
            schema_in(TEST_SCHEMA, &ns),
            surql_string(TEST_API_PASS)
        );
        let mut response = root.query(setup).await.expect("send the schema");
        let errors = response.take_errors();
        assert!(errors.is_empty(), "the schema failed in {ns}: {errors:?}");
        let db = Self::signed_in_api(&url, &ns).await;
        Self {
            db,
            server: Some(ServerNs { url, ns }),
        }
    }

    /// The server and namespace of a [`TestDb::server`].
    fn on_server(&self) -> &ServerNs {
        self.server
            .as_ref()
            .expect("only TestDb::server() has a server")
    }

    /// Another `api` connection to the same namespace, as a second API process would have.
    async fn api(&self) -> surrealdb::Surreal<surrealdb::engine::any::Any> {
        let server = self.on_server();
        Self::signed_in_api(&server.url, &server.ns).await
    }

    /// A root connection using the test's namespace, for what `api` may not do.
    async fn root(&self) -> surrealdb::Surreal<surrealdb::engine::any::Any> {
        let server = self.on_server();
        let root = Self::signed_in_root(&server.url).await;
        root.use_ns(&server.ns)
            .use_db("main")
            .await
            .expect("select the test namespace");
        root
    }

    async fn signed_in_root(url: &str) -> surrealdb::Surreal<surrealdb::engine::any::Any> {
        let root = surrealdb::engine::any::connect(url)
            .await
            .expect("connect to SurrealDB");
        root.signin(surrealdb::opt::auth::Root {
            username: "root".to_owned(),
            password: "root".to_owned(),
        })
        .await
        .expect("sign in as the test container's root");
        root
    }

    async fn signed_in_api(url: &str, ns: &str) -> surrealdb::Surreal<surrealdb::engine::any::Any> {
        let db = surrealdb::engine::any::connect(url)
            .await
            .expect("connect to SurrealDB");
        db.signin(surrealdb::opt::auth::Database {
            namespace: ns.to_owned(),
            database: "main".to_owned(),
            username: "api".to_owned(),
            password: TEST_API_PASS.to_owned(),
        })
        .await
        .expect("sign in as api");
        db.use_ns(ns)
            .use_db("main")
            .await
            .expect("select the test namespace");
        db
    }
}
// end: TestDb

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use surrealdb::opt::auth::Namespace;
    use surrealdb::types::QueryError;

    use super::*;

    fn new_id(table: &str) -> RecordId {
        RecordId::new(table, ulid::Ulid::generate().to_string())
    }

    struct Fixture {
        garden: RecordId,
        plant: RecordId,
        scan: RecordId,
        diagnosis: RecordId,
    }

    const CREATE_GARDEN: &str =
        "CREATE $garden SET name = 'g', centroid = type::point([$lon, $lat]),
  location_precision = 'exact', location_label = 'Toronto, ON', time_zone = 'America/Toronto';";

    async fn fixture(db: &Surreal<Any>) -> Fixture {
        let f = Fixture {
            garden: new_id("garden"),
            plant: new_id("plant"),
            scan: new_id("scan"),
            diagnosis: new_id("diagnosis"),
        };
        let sql = format!(
            "{CREATE_GARDEN}
             CREATE $plant SET garden = $garden, name = 'tomato';
             CREATE $scan SET garden = $garden, plant = $plant;
             CREATE $diagnosis SET scan = $scan, run = 1, status = 'healthy', candidates = [], \
               organisms = [], soil = [], risks = [], evidence_basis = {{}}, citations = [], model_versions = {{}};"
        );
        run_tx(db, &sql, |q| {
            q.bind(("garden", f.garden.clone()))
                .bind(("lon", -79.38))
                .bind(("lat", 43.65))
                .bind(("plant", f.plant.clone()))
                .bind(("scan", f.scan.clone()))
                .bind(("diagnosis", f.diagnosis.clone()))
        })
        .await
        .unwrap();
        f
    }

    async fn create_user(db: &Surreal<Any>) -> RecordId {
        let user = new_id("user");
        run_tx(
            db,
            "CREATE $user SET auth_id = rand::ulid(), display_name = 'a'",
            |q| q.bind(("user", user.clone())),
        )
        .await
        .unwrap();
        user
    }

    /// An instant with nanoseconds, as the clock port gives it.
    fn instant(rfc3339: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(rfc3339).unwrap().to_utc()
    }

    #[tokio::test]
    async fn query_is_ok_when_a_statement_fails() {
        let TestDb { db, .. } = TestDb::memory().await;
        let mut response = db.query("CREATE nosuch:1; RETURN 1;").await.unwrap();
        let errors = response.take_errors();
        assert!(errors[&0].is_not_found());
        let one: Option<i64> = response.take(1).unwrap();
        assert_eq!(one, Some(1));
        // A statement index past the end is not an error.
        let past_end: Option<i64> = response.take(5).unwrap();
        assert_eq!(past_end, None);
        let past_end: Vec<i64> = response.take(5).unwrap();
        assert_eq!(past_end, Vec::<i64>::new());
    }

    #[tokio::test]
    async fn run_tx_reports_the_cause_of_a_failed_transaction() {
        let TestDb { db, .. } = TestDb::memory().await;
        let user = new_id("user");
        let sql = "BEGIN; CREATE $user SET auth_id = rand::ulid(), display_name = 'a'; \
                   IF true { THROW 'stale_plan' }; RETURN 5; COMMIT;";
        let response = db.query(sql).bind(("user", user.clone())).await.unwrap();
        let first = response.check().unwrap_err();
        assert_eq!(first.query_details(), Some(&QueryError::NotExecuted));

        let mut response = db.query(sql).bind(("user", user.clone())).await.unwrap();
        let errors: BTreeMap<usize, surrealdb::Error> =
            response.take_errors().into_iter().collect();
        let details: Vec<_> = errors
            .iter()
            .map(|(index, error)| (*index, error.query_details().cloned()))
            .collect();
        assert_eq!(
            details,
            vec![
                (1, Some(QueryError::NotExecuted)), // before the cause
                (2, None),                          // the THROW itself
                (3, Some(QueryError::Cancelled)),   // after the cause
                (4, Some(QueryError::NotExecuted)), // "Cannot COMMIT: … aborted due to a prior error"
            ]
        );
        assert_eq!(
            errors[&3].message(),
            "The query was not executed due to a cancelled transaction"
        );
        assert_eq!(
            first_failure(errors).unwrap().message(),
            "An error occurred: stale_plan"
        );

        let failure = run_tx(&db, sql, |q| q.bind(("user", user.clone())))
            .await
            .unwrap_err();
        assert_eq!(failure.thrown(), Some(thrown::STALE_PLAN));
        assert!(matches!(
            failure.into_store("write plan"),
            StoreError::Conflict(Conflict::StalePlan { current: None })
        ));
        let mut response = db
            .query("SELECT * FROM ONLY $user")
            .bind(("user", user))
            .await
            .unwrap();
        let row: Option<Value> = response.take(0).unwrap();
        assert!(row.is_none(), "the THROW rolled back the CREATE");
    }

    #[tokio::test]
    async fn misspelled_bind_is_none_and_none_is_not_null() {
        let TestDb { db, .. } = TestDb::memory().await;
        let mut results = run_tx(&db, "RETURN $nmae", |q| q.bind(("name", "x")))
            .await
            .unwrap();
        let value: Value = results.take(0).unwrap();
        assert_eq!(value, Value::None);

        let f = fixture(&db).await;
        let sql =
            "CREATE task SET garden = $garden, title = 't', source = 'user', detail = $detail";
        run_tx(&db, sql, |q| {
            q.bind(("garden", f.garden.clone()))
                .bind(("detail", None::<String>))
        })
        .await
        .unwrap();
        let null = run_tx(&db, sql, |q| {
            q.bind(("garden", f.garden.clone()))
                .bind(("detail", Value::Null))
        })
        .await
        .unwrap_err();
        let DbFailure::Other(error) = null else {
            panic!("NULL into option<string> is a plain query error, got {null:?}");
        };
        assert!(
            error.message().contains("found `NULL`"),
            "{}",
            error.message()
        );
    }

    #[tokio::test]
    async fn protected_and_shadowed_names_points_and_literals() {
        let TestDb { db, .. } = TestDb::memory().await;
        for name in ["access", "auth", "token", "session"] {
            let error = db.query("RETURN 1").bind((name, "x")).await.unwrap_err();
            assert!(
                error
                    .message()
                    .contains(&format!("'{name}' is a protected variable")),
                "{name}: {}",
                error.message()
            );
        }

        // A bound `before` is shadowed: inside the statement `$before` is the record.
        let f = fixture(&db).await;
        let garden = new_id("garden");
        let plant = new_id("plant");
        let sql = format!("{CREATE_GARDEN} CREATE $plant SET garden = $garden, name = 'p';");
        run_tx(&db, &sql, |q| {
            q.bind(("garden", garden.clone()))
                .bind(("plant", plant.clone()))
                .bind(("lon", -79.38))
                .bind(("lat", 43.65))
        })
        .await
        .unwrap();
        let shadowed =
            "DELETE (SELECT VALUE id FROM plant WHERE garden = $garden) RETURN VALUE $before.id;";
        let mut results = run_tx(&db, shadowed, |q| {
            q.bind(("garden", garden.clone()))
                .bind(("before", serde_json::json!({ "id": "bound" })))
        })
        .await
        .unwrap();
        let returned: Vec<RecordId> = results.take(0).unwrap();
        assert_eq!(returned, vec![plant], "the bound value can't be read there");

        // A point literal takes no parameters; type::point([lon, lat]) does.
        let literal = run_tx(
            &db,
            "CREATE $garden SET name = 'p', centroid = ($lon, $lat), location_precision = 'exact', \
               location_label = 'x', time_zone = 'UTC';",
            |q| {
                q.bind(("garden", new_id("garden")))
                    .bind(("lon", -79.38))
                    .bind(("lat", 43.65))
            },
        )
        .await
        .unwrap_err();
        let DbFailure::Other(error) = literal else {
            panic!("a parse error, got {literal:?}");
        };
        assert!(error.is_validation(), "{}", error.message());
        let mut results = run_tx(
            &db,
            "SELECT VALUE centroid.coordinates FROM ONLY $garden;",
            |q| q.bind(("garden", f.garden.clone())),
        )
        .await
        .unwrap();
        let coordinates: Vec<f64> = results.take(0).unwrap();
        assert_eq!(coordinates, vec![-79.38, 43.65], "longitude first");

        // DEFINE USER takes a literal: a bound, LET or PASSHASH parameter is a parse error.
        for (sql, bind) in [
            ("DEFINE USER p1 ON DATABASE PASSWORD $p ROLES VIEWER", true),
            (
                "LET $p = 'secret'; DEFINE USER p2 ON DATABASE PASSWORD $p ROLES VIEWER",
                false,
            ),
            ("DEFINE USER p3 ON DATABASE PASSHASH $p ROLES VIEWER", true),
        ] {
            let query = db.query(sql);
            let query = if bind {
                query.bind(("p", "secret"))
            } else {
                query
            };
            let error = query.await.unwrap_err();
            assert!(error.is_validation(), "{sql}: {}", error.message());
            assert!(error.message().contains("expected a strand"), "{sql}");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_appends_are_gap_free_and_replay_by_range() {
        let t = TestDb::memory().await;
        let f = fixture(&t.db).await;
        let db = Arc::new(t.db);
        let mut tasks = Vec::new();
        for i in 0..40 {
            let db = Arc::clone(&db);
            let scan = f.scan.clone();
            tasks.push(tokio::spawn(async move {
                let payload = serde_json::json!({ "i": i });
                append_scan_event(&db, &scan, 1, "finding", &payload, Utc::now()).await
            }));
        }
        let mut seqs = Vec::new();
        for task in tasks {
            seqs.push(task.await.unwrap().unwrap());
        }
        seqs.sort_unstable();
        assert_eq!(seqs, (1..=40).collect::<Vec<_>>());

        let replay = events_after(&db, &f.garden, &f.scan, 35).await.unwrap();
        let replayed: Vec<_> = replay.iter().filter_map(ScanEventRow::seq).collect();
        assert_eq!(replayed, vec![36, 37, 38, 39, 40]);

        // Another garden's scan is not found; the range needs a number: NONE + 1 fails.
        let elsewhere = events_after(&db, &new_id("garden"), &f.scan, 0).await;
        assert!(matches!(elsewhere, Err(StoreError::NotFound)));
        let failure = run_tx(&db, EVENTS_AFTER, |q| {
            q.bind(("scan", f.scan.clone()))
                .bind(("garden", f.garden.clone()))
                .bind(("after", None::<i64>))
        })
        .await
        .unwrap_err();
        let DbFailure::Other(error) = failure else {
            panic!("expected a plain query error, got {failure:?}");
        };
        assert!(
            error
                .message()
                .contains("Cannot perform addition with 'none' and 'int'"),
            "{}",
            error.message()
        );

        let missing = append_scan_event(
            &db,
            &new_id("scan"),
            1,
            "finding",
            &serde_json::json!({}),
            Utc::now(),
        )
        .await;
        assert!(matches!(missing, Err(StoreError::NotFound)));

        delete_scan_events(&db, &f.scan).await.unwrap();
        assert!(
            events_after(&db, &f.garden, &f.scan, 0)
                .await
                .unwrap()
                .is_empty()
        );

        // A scan being deleted reads as missing, so a reconnect gets 404 (contracts 4).
        run_tx(&db, "UPDATE $scan SET deleting_at = $now;", |q| {
            q.bind(("scan", f.scan.clone())).bind(("now", Utc::now()))
        })
        .await
        .unwrap();
        let deleting = events_after(&db, &f.garden, &f.scan, 0).await;
        assert!(matches!(deleting, Err(StoreError::NotFound)));
    }

    fn plan(f: &Fixture, version: i64) -> PlanWrite {
        PlanWrite {
            id: new_id("action_plan"),
            row: NewPlan {
                scan: f.scan.clone(),
                plant: f.plant.clone(),
                garden: f.garden.clone(),
                version,
                supersedes: None,
                reason: PlanReason::Initial,
                provisional: false,
                diagnosis: f.diagnosis.clone(),
                open_question: None,
                summary: "Water at the base.".into(),
                steps: Vec::new(),
                watch_for: Vec::new(),
                citations: Vec::new(),
                llm_model: "m".into(),
                prompt_version: "p1".into(),
            },
        }
    }

    #[tokio::test]
    async fn a_lost_plan_write_is_stale_and_leaves_nothing() {
        let TestDb { db, .. } = TestDb::memory().await;
        let f = fixture(&db).await;
        let now = Utc::now();
        let first = plan(&f, 1);
        set_plan_cas(&db, &first, None, now).await.unwrap();
        let task = new_id("task");
        run_tx(
            &db,
            "CREATE $task SET garden = $garden, plan = $plan, title = 'Water', source = 'plan';",
            |q| {
                q.bind(("task", task.clone()))
                    .bind(("garden", f.garden.clone()))
                    .bind(("plan", first.id.clone()))
            },
        )
        .await
        .unwrap();

        // A second "first plan": the THROW.
        let loser = plan(&f, 2);
        let result = set_plan_cas(&db, &loser, None, now).await;
        let Err(StoreError::Conflict(Conflict::StalePlan { current })) = result else {
            panic!("expected StalePlan, got {result:?}");
        };
        assert_eq!(current, key_of(&first.id), "the plan that won");
        let mut response = db
            .query("SELECT * FROM ONLY $p")
            .bind(("p", loser.id))
            .await
            .unwrap();
        let row: Option<Value> = response.take(0).unwrap();
        assert!(row.is_none(), "a losing write leaves nothing");

        // v2 over v1 dismisses v1's open task in the same transaction.
        let second = plan(&f, 2);
        set_plan_cas(&db, &second, Some(&first.id), now)
            .await
            .unwrap();
        let mut results = run_tx(&db, "SELECT VALUE dismissed_reason FROM ONLY $task;", |q| {
            q.bind(("task", task.clone()))
        })
        .await
        .unwrap();
        let reason: Option<String> = results.take(0).unwrap();
        assert_eq!(reason.as_deref(), Some("plan_superseded"));

        // Two writers of the same version: the unique index fails before the THROW runs.
        let again = set_plan_cas(&db, &plan(&f, 2), Some(&first.id), now).await;
        let Err(StoreError::Conflict(Conflict::StalePlan { current })) = again else {
            panic!("expected StalePlan, got {again:?}");
        };
        assert_eq!(current, key_of(&second.id));
    }

    #[tokio::test]
    async fn step_claims_flag_only_when_busy_and_stamps_compare_exactly() {
        let TestDb { db, .. } = TestDb::memory().await;
        let f = fixture(&db).await;
        let t1 = instant("2026-10-16T14:03:22.123456789Z");

        // A pending scan claims nothing.
        let pending = claim_step(&db, &f.scan, "score", t1).await.unwrap();
        assert_eq!(pending.outcome, "not_needed");
        run_tx(
            &db,
            "UPDATE $scan SET status = 'analysing', run = 1;",
            |q| q.bind(("scan", f.scan.clone())),
        )
        .await
        .unwrap();

        let claim = claim_step(&db, &f.scan, "score", t1).await.unwrap();
        assert_eq!(claim.outcome, "claimed");
        assert_eq!(claim.stamp, Some(t1), "stored with full precision");
        let flags = "SELECT VALUE rescore_pending FROM ONLY $scan;";
        let mut results = run_tx(&db, flags, |q| q.bind(("scan", f.scan.clone())))
            .await
            .unwrap();
        let flagged: Option<bool> = results.take(0).unwrap();
        assert_eq!(flagged, Some(false), "a successful claim flags nothing");

        // Busy: no claim, and the scan is flagged for score.
        let busy = claim_step(&db, &f.scan, "retrieve", t1).await.unwrap();
        assert_eq!(busy.outcome, "busy");

        // The stamp is compared exactly: the same instant in whole seconds loses.
        let rounded = instant("2026-10-16T14:03:22Z");
        let lost = commit_step(&db, &f.scan, "score", 1, rounded, None, t1).await;
        assert!(matches!(
            lost,
            Err(StoreError::Conflict(Conflict::ClaimLost))
        ));

        // The sweep re-claims once by the old stamp; the slow original then loses.
        let t2 = instant("2026-10-16T14:05:30.5Z");
        let swept = reclaim_step(&db, &f.scan, "score", 1, t1, t2)
            .await
            .unwrap();
        assert_eq!(swept.and_then(|c| c.pipeline_step_at), Some(t2));
        assert!(
            reclaim_step(&db, &f.scan, "score", 1, t1, t2)
                .await
                .unwrap()
                .is_none()
        );
        let slow = commit_step(&db, &f.scan, "score", 1, t1, None, t2).await;
        assert!(matches!(
            slow,
            Err(StoreError::Conflict(Conflict::ClaimLost))
        ));

        // A claim of another run never commits.
        let old_run = commit_step(&db, &f.scan, "score", 0, t2, None, t2).await;
        assert!(matches!(
            old_run,
            Err(StoreError::Conflict(Conflict::ClaimLost))
        ));

        // The new holder commits; the flag hands over to score instead of releasing.
        let t3 = instant("2026-10-16T14:05:31.25Z");
        let next = commit_step(&db, &f.scan, "score", 1, t2, None, t3)
            .await
            .unwrap();
        assert_eq!(next.as_deref(), Some("score"));
        let next = commit_step(&db, &f.scan, "score", 1, t3, None, t3)
            .await
            .unwrap();
        assert_eq!(next, None, "released");
    }

    fn job(f: &Fixture, kind: JobKind) -> NewJob {
        NewJob {
            id: new_id("job"),
            kind,
            scan: f.scan.clone(),
            run: Some(1),
            question: None,
            payload: serde_json::json!({}),
            max_runtime: Duration::from_secs(300),
        }
    }

    async fn heartbeat(db: &Surreal<Any>, worker: &str, caps: &[&str], at: DateTime<Utc>) {
        run_tx(
            db,
            "UPSERT type::record('worker', [$id]) SET caps = $caps, slots = 1, versions = {}, last_heartbeat = $at;",
            |q| {
                q.bind(("id", worker.to_owned()))
                    .bind((
                        "caps",
                        caps.iter().map(|c| (*c).to_owned()).collect::<Vec<_>>(),
                    ))
                    .bind(("at", at))
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn claims_respect_slots_the_fallback_and_the_scan_status() {
        let TestDb { db, .. } = TestDb::memory().await;
        let f = fixture(&db).await;
        let jobs_queued = Notify::new();
        let now = Utc::now();
        run_tx(&db, "UPDATE $scan SET status = 'queued', run = 1;", |q| {
            q.bind(("scan", f.scan.clone()))
        })
        .await
        .unwrap();
        for _ in 0..2 {
            enqueue_job(&db, &jobs_queued, &job(&f, JobKind::Perception), now)
                .await
                .unwrap();
        }
        let gpu = |worker| Claimant {
            worker,
            slots: 1,
            kinds: &[JobKind::Perception],
            fallback: false,
        };
        let first = claim_job(&db, gpu("w1"), now, now).await.unwrap();
        assert!(first.started_analysing, "the run's first job starts it");
        let leased = first.job.unwrap();
        assert_eq!((leased.kind, &leased.scan), (JobKind::Perception, &f.scan));
        assert_eq!(leased.lease_token.len(), 26, "rand::ulid()");
        assert_eq!(leased.lease_until, now + chrono::Duration::minutes(2));
        assert_eq!(leased.max_runtime, Duration::from_secs(300));
        let full = claim_job(&db, gpu("w1"), now, now).await.unwrap();
        assert!(full.job.is_none(), "one slot, one lease");
        let second = claim_job(&db, gpu("w2"), now, now).await.unwrap();
        assert!(!second.started_analysing, "the scan is analysing already");
        assert!(second.job.is_some(), "the second job was still queued");

        // A photo answer's job waits for a live perception worker, unless none is alive.
        let mut answer = job(&f, JobKind::Perception);
        answer.question = Some(new_id("question"));
        enqueue_job(&db, &jobs_queued, &answer, now).await.unwrap();
        heartbeat(&db, "w1", &["perception"], now).await;
        let fallback = Claimant {
            worker: "fb",
            slots: 1,
            kinds: &[JobKind::Perception, JobKind::Vlm],
            fallback: true,
        };
        let held = claim_job(&db, fallback, now, now).await.unwrap();
        assert!(held.job.is_none(), "held while perception is alive");
        let later = now + chrono::Duration::seconds(31);
        let taken = claim_job(&db, fallback, later, later)
            .await
            .unwrap()
            .job
            .unwrap();
        assert_eq!(taken.id, answer.id);
        assert_eq!(
            taken.max_runtime,
            Duration::from_secs(600),
            "doubled on the fallback"
        );
    }

    #[tokio::test]
    async fn requeues_count_attempts_fail_guided_checks_and_wake_the_broker() {
        let TestDb { db, .. } = TestDb::memory().await;
        let f = fixture(&db).await;
        let jobs_queued = Arc::new(Notify::new());
        let now = Utc::now();
        let gpu = Claimant {
            worker: "w1",
            slots: 1,
            kinds: &[JobKind::Perception],
            fallback: false,
        };
        let first = job(&f, JobKind::Perception);
        enqueue_job(&db, &jobs_queued, &first, now).await.unwrap();

        // w1 went silent 31 s ago: its job is requeued however fresh its lease (question 20).
        claim_job(&db, gpu, now, now).await.unwrap().job.unwrap();
        heartbeat(
            &db,
            "w1",
            &["perception"],
            now - chrono::Duration::seconds(31),
        )
        .await;
        let waiter = tokio::spawn(Arc::clone(&jobs_queued).notified_owned());
        tokio::task::yield_now().await;
        let requeued = requeue_stale(&db, &jobs_queued, now).await.unwrap();
        assert_eq!(requeued.len(), 1);
        assert_eq!(requeued[0].status, "queued");
        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("the requeue woke the waiting claim")
            .unwrap();
        heartbeat(&db, "w1", &["perception"], now).await;

        // Expired leases: the 2nd requeue is still free, the 3rd, 4th and 5th each count an
        // attempt, and at 3 attempts the job fails for good (contracts 5).
        let mut at = now;
        let mut statuses = Vec::new();
        for _ in 0..4 {
            claim_job(&db, gpu, at, at).await.unwrap().job.unwrap();
            at += chrono::Duration::minutes(3);
            heartbeat(&db, "w1", &["perception"], at).await;
            let requeued = requeue_stale(&db, &jobs_queued, at).await.unwrap();
            statuses.push(requeued[0].status.clone());
        }
        assert_eq!(statuses, ["queued", "queued", "queued", "failed"]);
        let mut results = run_tx(&db, "SELECT VALUE attempts FROM ONLY $job;", |q| {
            q.bind(("job", first.id.clone()))
        })
        .await
        .unwrap();
        let attempts: Option<i64> = results.take(0).unwrap();
        assert_eq!(attempts, Some(3));

        // A guided job out of attempts fails its check in the same transaction.
        let image = ulid::Ulid::generate().to_string();
        let check = new_id("guided_check");
        run_tx(
            &db,
            "CREATE type::record('scan_image', $image) SET scan = $scan, garden = $garden, kind = 'squeeze_test',
               width = 1, height = 1, storage_key = 'k', quality = {};
             CREATE $check SET scan = $scan, image = type::record('scan_image', $image), kind = 'squeeze_test';
             UPDATE $scan SET status = 'diagnosed';",
            |q| {
                q.bind(("image", image.clone()))
                    .bind(("scan", f.scan.clone()))
                    .bind(("garden", f.garden.clone()))
                    .bind(("check", check.clone()))
            },
        )
        .await
        .unwrap();
        let mut guided = job(&f, JobKind::Guided);
        guided.run = None;
        guided.payload = serde_json::json!({ "image": { "image_id": image } });
        enqueue_job(&db, &jobs_queued, &guided, at).await.unwrap();
        run_tx(&db, "UPDATE $job SET attempts = 2, requeues = 2;", |q| {
            q.bind(("job", guided.id.clone()))
        })
        .await
        .unwrap();
        let worker = Claimant {
            kinds: &[JobKind::Guided],
            ..gpu
        };
        claim_job(&db, worker, at, at).await.unwrap().job.unwrap();
        at += chrono::Duration::minutes(3);
        let requeued = requeue_stale(&db, &jobs_queued, at).await.unwrap();
        assert_eq!(requeued[0].status, "failed");
        let mut results = run_tx(&db, "SELECT VALUE status FROM ONLY $check;", |q| {
            q.bind(("check", check.clone()))
        })
        .await
        .unwrap();
        let status: Option<String> = results.take(0).unwrap();
        assert_eq!(status.as_deref(), Some("failed"));
    }

    #[tokio::test]
    async fn duplicates_and_unique_indexes() {
        let TestDb { db, .. } = TestDb::memory().await;
        let f = fixture(&db).await;
        let user = create_user(&db).await;
        let now = Utc::now();

        let first = begin_idempotent(&db, &user, "POST /scans", "k1", "fp", now).await;
        assert!(matches!(first, Ok(Begin::Started)));
        let again = begin_idempotent(&db, &user, "POST /scans", "k1", "fp", now).await;
        let Ok(Begin::Existing(stored)) = again else {
            panic!("expected the stored key, got {again:?}");
        };
        assert_eq!(stored.state, KeyState::Running);
        assert_eq!(stored.fingerprint, "fp");
        assert_eq!(stored.expires_at, now + chrono::Duration::hours(24));

        add_member(&db, &user, &f.garden, Role::Owner)
            .await
            .unwrap();
        let twice = add_member(&db, &user, &f.garden, Role::Viewer).await;
        assert!(matches!(
            twice,
            Err(StoreError::Conflict(Conflict::AlreadyExists))
        ));

        let mut results = run_tx(
            &db,
            "UPSERT type::record('usage_daily', [$user, $utc_day]) SET scans += 1 RETURN VALUE scans;",
            |q| q.bind(("user", user.clone())).bind(("utc_day", "2026-10-16")),
        )
        .await
        .unwrap();
        let scans: Option<i64> = results.take(0).unwrap();
        assert_eq!(scans, Some(1), "RETURN VALUE gives [1], taken as Option");

        let key = format!("{}:frost:2026-10-20", ulid::Ulid::generate());
        assert!(
            create_alert_task(&db, &f.garden, "Cover", &key)
                .await
                .unwrap()
        );
        assert!(
            !create_alert_task(&db, &f.garden, "Cover", &key)
                .await
                .unwrap()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn duplicate_writes_from_many_tasks_never_fail() {
        let t = TestDb::memory().await;
        let f = fixture(&t.db).await;
        let user = create_user(&t.db).await;
        let db = Arc::new(t.db);

        for round in 0..20 {
            let key = format!("k{round}");
            let mut tasks = Vec::new();
            for _ in 0..8 {
                let (db, user, key) = (Arc::clone(&db), user.clone(), key.clone());
                tasks.push(tokio::spawn(async move {
                    begin_idempotent(&db, &user, "POST /scans", &key, "fp", Utc::now()).await
                }));
            }
            let mut started = 0;
            for task in tasks {
                match task.await.unwrap() {
                    Ok(Begin::Started) => started += 1,
                    Ok(Begin::Existing(stored)) => assert_eq!(stored.fingerprint, "fp"),
                    Err(error) => panic!("a duplicate key must not fail: {error:?}"),
                }
            }
            assert_eq!(
                started, 1,
                "round {round}: one request runs, the rest wait or replay"
            );
        }

        for round in 0..20 {
            let member = create_user(&db).await;
            let mut tasks = Vec::new();
            for _ in 0..8 {
                let (db, member, garden) = (Arc::clone(&db), member.clone(), f.garden.clone());
                tasks.push(tokio::spawn(async move {
                    add_member(&db, &member, &garden, Role::Editor).await
                }));
            }
            let mut added = 0;
            for task in tasks {
                match task.await.unwrap() {
                    Ok(()) => added += 1,
                    Err(StoreError::Conflict(Conflict::AlreadyExists)) => {}
                    Err(error) => panic!("a second invite accept must not fail: {error:?}"),
                }
            }
            assert_eq!(added, 1, "round {round}");
        }
    }

    #[tokio::test]
    async fn cursor_pages_and_batched_deletes() {
        let TestDb { db, .. } = TestDb::memory().await;
        let f = fixture(&db).await;
        run_tx(
            &db,
            "FOR $i IN 0..60 { CREATE plant SET garden = $garden, name = 'p' };",
            |q| q.bind(("garden", f.garden.clone())),
        )
        .await
        .unwrap();
        let first = plants_page(&db, &f.garden, None).await.unwrap();
        assert_eq!(first.len(), 50, "id > NONE is true for every row");
        let rest = plants_page(&db, &f.garden, first.last().map(|p| &p.id))
            .await
            .unwrap();
        assert_eq!(rest.len(), 11, "60 plus the fixture's plant");
        assert!(rest.iter().all(|p| first.iter().all(|q| q.id != p.id)));

        // What each RETURN clause gives back from a batch of 2 (plants with no scans).
        let garden = new_id("garden");
        let sql = format!(
            "{CREATE_GARDEN} FOR $i IN 0..6 {{ CREATE plant SET garden = $garden, name = 'p' }};"
        );
        run_tx(&db, &sql, |q| {
            q.bind(("garden", garden.clone()))
                .bind(("lon", -79.38))
                .bind(("lat", 43.65))
        })
        .await
        .unwrap();
        let batch = |returning: &str| {
            format!(
                "DELETE (SELECT VALUE id FROM plant WHERE garden = $garden LIMIT 2) {returning}; \
                 RETURN count(SELECT VALUE id FROM plant WHERE garden = $garden);"
            )
        };
        let mut response = db
            .query(batch("RETURN NONE"))
            .bind(("garden", garden.clone()))
            .await
            .unwrap();
        let returned: Vec<Value> = response.take(0).unwrap();
        let left: Option<i64> = response.take(1).unwrap();
        assert_eq!(
            (returned.len(), left),
            (0, Some(4)),
            "RETURN NONE: [] for 2 rows"
        );
        let mut response = db
            .query(batch("RETURN VALUE id"))
            .bind(("garden", garden.clone()))
            .await
            .unwrap();
        let returned: Vec<Value> = response.take(0).unwrap();
        assert_eq!(
            returned,
            vec![Value::None, Value::None],
            "the record is gone"
        );
        let mut response = db
            .query(batch("RETURN VALUE $before.id"))
            .bind(("garden", garden.clone()))
            .await
            .unwrap();
        // Two ids are an array: as Option the take fails.
        let as_option = response.take::<Option<RecordId>>(0).unwrap_err();
        assert!(
            as_option
                .message()
                .contains("Tried to take only a single result"),
            "{}",
            as_option.message()
        );
        let left: Option<i64> = response.take(1).unwrap();
        assert_eq!(left, Some(0));
        let mut response = db
            .query(batch("RETURN NONE"))
            .bind(("garden", garden))
            .await
            .unwrap();
        let returned: Vec<Value> = response.take(0).unwrap();
        assert!(returned.is_empty(), "RETURN NONE: [] for 0 rows too");

        run_tx(
            &db,
            "FOR $i IN 0..1200 { CREATE observation SET scan = $scan, run = 1, stage = 'soil', \
               pipeline_stage = 'soil', label = 'l', confidence = 0.5, source = 'vlm', model_version = 'm' };",
            |q| q.bind(("scan", f.scan.clone())),
        )
        .await
        .unwrap();
        let deleted = delete_in_batches(&db, DELETE_OBSERVATIONS_OF_SCAN, &f.scan)
            .await
            .unwrap();
        assert_eq!(deleted, 1200);
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB"]
    async fn users_and_sessions() {
        let t = TestDb::server().await;
        let (url, ns) = (t.on_server().url.clone(), t.on_server().ns.clone());
        let as_root = connect(&url).await.unwrap();
        let refused = as_root
            .signin(Root {
                username: "api".into(),
                password: TEST_API_PASS.into(),
            })
            .await
            .unwrap_err();
        assert!(refused.is_not_allowed());
        let api = connect_api(&url, &ns, TEST_API_PASS).await.unwrap();
        let mut response = api.query("RETURN session::ns()").await.unwrap();
        let session_ns: Option<String> = response.take(0).unwrap();
        assert_eq!(session_ns.as_deref(), Some(ns.as_str()));

        // As api, the schema's DEFINE NAMESPACE and DEFINE DATABASE need more than EDITOR;
        // every other statement applies (the tables are this test's own).
        let mut response = t.db.query(schema_in(TEST_SCHEMA, &ns)).await.unwrap();
        let errors: BTreeMap<usize, surrealdb::Error> =
            response.take_errors().into_iter().collect();
        assert_eq!(errors.keys().copied().collect::<Vec<_>>(), vec![0, 2]);
        assert!(errors.values().all(surrealdb::Error::is_not_allowed));
        assert!(
            errors[&0]
                .message()
                .contains("Not enough permissions to perform this action")
        );

        // A namespace OWNER may define a database but not a namespace.
        let root = t.root().await;
        let define_owner = format!(
            "DEFINE USER owner ON NAMESPACE PASSWORD {} ROLES OWNER;",
            surql_string("owner-pass")
        );
        checked(root.query(define_owner).await).unwrap();
        let owner = connect(&url).await.unwrap();
        owner
            .signin(Namespace {
                namespace: ns.clone(),
                username: "owner".into(),
                password: "owner-pass".into(),
            })
            .await
            .unwrap();
        owner.use_ns(&ns).await.unwrap();
        let mut response = owner
            .query(format!(
                "DEFINE NAMESPACE IF NOT EXISTS {ns}_x; DEFINE DATABASE IF NOT EXISTS second STRICT;"
            ))
            .await
            .unwrap();
        let errors: BTreeMap<usize, surrealdb::Error> =
            response.take_errors().into_iter().collect();
        assert_eq!(errors.keys().copied().collect::<Vec<_>>(), vec![0]);
        assert!(errors[&0].is_not_allowed());

        // As root, the whole schema applies into a fresh namespace twice in a row (store 20,
        // tests/server.rs).
        let schema = schema_in(TEST_SCHEMA, &format!("{ns}_fresh"));
        apply_schema(&url, "root", "root", &schema).await.unwrap();
        apply_schema(&url, "root", "root", &schema).await.unwrap();

        // A clone is a new session on the same connection.
        let clone = api.clone();
        clone.set("probe", 7).await.unwrap();
        let mut response = api.query("RETURN [$session.id, $probe]").await.unwrap();
        let original: Value = response.take(0).unwrap();
        let mut response = clone.query("RETURN [$session.id, $probe]").await.unwrap();
        let cloned: Value = response.take(0).unwrap();
        let (Value::Array(original), Value::Array(cloned)) = (original, cloned) else {
            panic!("RETURN [..] is an array");
        };
        assert_ne!(original[0], cloned[0], "a clone is a new session");
        assert_eq!(original[1], Value::None);
        assert_eq!(cloned[1], Value::Number(Number::Int(7)));
    }
}
```

## Live tests

`check_examples.py` runs every test above except `users_and_sessions` in its normal run, on the embedded engine. `--live` runs that one too: it starts the pinned image on a free port, sets `PDM_TEST_SURREALDB_URL`, runs `cargo nextest run --run-ignored only` (one process per test) and stops the container ([rust-testing, database.md](../rust-testing/database.md#4-running-them)). By hand, with the server from [rust-testing, database.md](../rust-testing/database.md#2-the-pinned-image) on `127.0.0.1:18000`:

```sh
PDM_TEST_SURREALDB_URL=ws://127.0.0.1:18000 cargo nextest run --run-ignored only
```

Results on 2026-10-16 (crate 3.3.4 with `kv-mem`; image `v3.3.4@sha256:649a60a3…`, `memory`; cargo-nextest 0.9.148): the 12 tests above on the embedded engine passed in the normal run, and `users_and_sessions` and the harness test in [rust-testing, database.md](../rust-testing/database.md#3-the-harness) passed against the image, 2 of 2, each in its own process and namespace.
