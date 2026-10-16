# Write patterns as store functions

Part of [`rust-surrealdb`](SKILL.md). One function per pattern in [database 4](../../../docs/database/README.md#4-write-patterns), plus connecting, the replay read, cursor paging and batched deletes. The database document wins. The constants for its patterns (`APPEND_SCAN_EVENT`, `EVENTS_AFTER`, `SET_PLAN_CAS`, `CLAIM_JOB`, `BEGIN_IDEMPOTENT`, `ADD_MEMBER`) are copied from it verbatim. The others apply its rules where it gives no statement: the read after an idempotency conflict, the alert task, the per-scan range delete of events (the replay range starting at `seq` 1), the plant pages, and the batched delete, which returns `$before.id` instead of `NONE` ([SKILL.md, deletes](SKILL.md#deletes)).

The block carries exact copies of three shared definitions, which `check_examples.py` compares line for line with their canonical blocks: `StoreError` ([rust-errors, mapping.md](../rust-errors/mapping.md)), the retry kit `first_failure`, `checked` and `retry_conflicts` ([errors-and-retries.md](errors-and-retries.md)), and the test harness `TestDb` ([rust-testing, database.md](../rust-testing/database.md#one-namespace-per-test)).

## What each function relies on

| Function | Doc | Statement read | Failure it maps |
|---|---|---|---|
| `apply_schema` | [database 1](../../../docs/database/README.md#1-version-and-setup) | all, as root | any failed statement fails start-up |
| `connect_api` | [database 1](../../../docs/database/README.md#1-version-and-setup), [6](../../../docs/database/README.md#6-rust-sdk) | none | wrong credentials: `NotAllowed(Auth(InvalidAuth))` |
| `append_scan_event` | [scan events](../../../docs/database/README.md#scan-events-a-gap-free-seq-) | 2, the `CREATE`; `seq` is the key's second element | conflict at statement 3, retried |
| `events_after` | [scan events](../../../docs/database/README.md#scan-events-a-gap-free-seq-), [contracts 4](../../../docs/architecture/contracts.md#transport) | 0 | none; `$after` must be a number |
| `set_plan_cas` | [plan writes](../../../docs/database/README.md#plan-writes-compare-and-set-) | none | `THROW 'stale_plan'` at statement 3 → `Conflict { what: "plan" }`; conflict retried |
| `claim_job` | [job claim](../../../docs/database/README.md#job-claim-with-slots-), [contracts 5](../../../docs/architecture/contracts.md#post-internaljobsclaim) | 2, the `IF`: the leased job, or `[]` | conflict retried (the loser gets the next job) |
| `begin_idempotent` | [idempotency keys](../../../docs/database/README.md#idempotency-keys-), [contracts 1](../../../docs/architecture/contracts.md#idempotency) | the stored key on a repeat | `AlreadyExists(Record)` → read the existing key; conflict retried |
| `add_member` | [membership](../../../docs/database/README.md#membership-) | none | `AlreadyExists(Record)`; conflict retried |
| `create_alert_task` | [alert dedupe](../../../docs/database/README.md#alert-dedupe-) | none | `Internal` "Database index \`task_alert\` already contains …" → `Ok(false)`; conflict retried |
| `delete_scan_events`, `delete_in_batches` | [deletion](../../../docs/database/README.md#deletion-) | 0 | a remaining `REFERENCE ON DELETE REJECT` fails loudly |
| `plants_page` | [database 5](../../../docs/database/README.md#5-reads-indexes-and-vector-search) | 0 | none |

The fallback worker's claim adds `AND (question = NONE OR !$perception_alive)` and `$eligible_before = time::now() - 15s` ([job claim](../../../docs/database/README.md#job-claim-with-slots-)); it is a second constant with the same reading code. The other patterns (limits, steps between jobs, garden map versions) follow the same shape, with each `THROW` code mapped in `StoreError`'s one `From` impl.

**Duplicates race too.** A single `CREATE` or `INSERT RELATION` is its own transaction, and when several connections write the same new key at once, the losers mostly get `AlreadyExists` but some get `TransactionConflict` instead. ✓ (*probe*) 40 rounds of 8 connections creating the same idempotency key: 40 created, 272 `AlreadyExists`, 8 `TransactionConflict`. Through clones of one client (one WebSocket) the same run gave no conflicts. So `begin_idempotent`, `add_member` and `create_alert_task` run inside `retry_conflicts` as well: the retry then sees `AlreadyExists` and gives the contracts 1 answer (replay, `409 request_in_progress` or `422`) instead of a `500`. ✓ `duplicate_writes_from_many_connections_never_fail` below proves it with 8 connections; with the two `retry_conflicts` wrappers removed, it failed in 3 runs out of 3 with `TxConflict` ("Write conflict, retry the transaction").

## The module

`#[cfg(test)] mod live` holds the tests that proved the ✓ claims in [SKILL.md](SKILL.md). They are `#[ignore = "needs SurrealDB"]`, so the example checker compiles them but runs them only with `--live`. Each test builds its own `TestDb` (a namespace with the real schema and an `api` user, signed in as `api`), so the tests run in parallel under nextest without seeing each other's rows.

```rust
use chrono::{DateTime, Utc};
use surrealdb::Surreal;
use surrealdb::engine::any::{Any, connect};
use surrealdb::opt::auth::{Database, Root};
use surrealdb::types::{Number, RecordId, RecordIdKey, SurrealValue, Value};

// A unique-index violation has no typed details on 3.3.4, only this message.
pub fn is_unique_violation(error: &surrealdb::Error, index: &str) -> bool {
    error.is_internal()
        && error
            .message()
            .strip_prefix("Database index `")
            .and_then(|rest| rest.strip_prefix(index))
            .is_some_and(|rest| rest.starts_with("` already contains"))
}

// ---- connecting (database 1 and 6) -----------------------------------------------------

/// The API's namespace and database (database 1); tests pass their own namespace.
pub const NAMESPACE: &str = "pdm";
pub const DATABASE: &str = "main";

// Production passes include_str!("../../../migrations/schema.surql") from the store crate.
pub async fn apply_schema(
    url: &str,
    root_user: &str,
    root_pass: &str,
    schema: &str,
) -> Result<(), StoreError> {
    let root = connect(url).await?;
    root.signin(Root {
        username: root_user.to_owned(),
        password: root_pass.to_owned(),
    })
    .await?;
    checked(root.query(schema).await?)?;
    Ok(())
}

pub async fn connect_api(
    url: &str,
    namespace: &str,
    password: &str,
) -> Result<Surreal<Any>, StoreError> {
    let db = connect(url).await?;
    db.signin(Database {
        namespace: namespace.to_owned(),
        database: DATABASE.to_owned(),
        username: "api".to_owned(),
        password: password.to_owned(),
    })
    .await?;
    db.use_ns(namespace).use_db(DATABASE).await?;
    Ok(db)
}

// ---- scan events (database 4) ----------------------------------------------------------

// Statements: 0 BEGIN, 1 LET, 2 CREATE (the new event), 3 COMMIT.
const APPEND_SCAN_EVENT: &str = "BEGIN;
LET $seq = (UPDATE ONLY $scan SET last_event_seq += 1 RETURN VALUE last_event_seq);
CREATE type::record('scan_event', [$scan, $seq]) SET run = $run, type = $type, payload = $payload;
COMMIT;";

// $after is a number: `$after + 1` with NONE fails ("Cannot perform addition with 'none' and 'int'").
const EVENTS_AFTER: &str = "SELECT * FROM scan_event:[$scan, $after + 1]..=[$scan, ..];";

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
    // The key is [scan, seq].
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
) -> Result<i64, StoreError> {
    retry_conflicts(|| async {
        let response = db
            .query(APPEND_SCAN_EVENT)
            .bind(("scan", scan.clone()))
            .bind(("run", run))
            .bind(("type", kind.to_owned()))
            .bind(("payload", payload.clone()))
            .await?;
        let event: Option<ScanEventRow> = checked(response)?.take(2)?;
        // None: no event came back, so the scan isn't there.
        event
            .as_ref()
            .and_then(ScanEventRow::seq)
            .ok_or(StoreError::NotFound)
    })
    .await
}

// after: the Last-Event-ID; 0 replays from the first event.
pub async fn events_after(
    db: &Surreal<Any>,
    scan: &RecordId,
    after: i64,
) -> Result<Vec<ScanEventRow>, StoreError> {
    let response = db
        .query(EVENTS_AFTER)
        .bind(("scan", scan.clone()))
        .bind(("after", after))
        .await?;
    Ok(checked(response)?.take(0)?)
}

// ---- plan compare-and-set (database 4) -------------------------------------------------

// Statements: 0 BEGIN, 1 CREATE, 2 LET, 3 IF (the THROW), 4 COMMIT.
const SET_PLAN_CAS: &str = "BEGIN;
CREATE $new_plan CONTENT $plan;
LET $won = (UPDATE $scan SET plan = $new_plan WHERE plan = $expected RETURN VALUE plan);
IF array::len($won) = 0 { THROW 'stale_plan' };
COMMIT;";

#[derive(Debug, Clone, SurrealValue)]
#[surreal(untagged, rename_all = "snake_case")]
pub enum PlanReason {
    Initial,
    LateAnswer,
    SpeciesChanged,
    GuidedCheck,
    ChatRevision,
}

// The action_plan fields schema.surql requires; constraints take their default.
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

// expected: the plan the caller read, None for the first plan (WHERE plan = NONE).
// Err(StoreError::Conflict { what: "plan" }): another plan won; nothing was written.
pub async fn set_plan_cas(
    db: &Surreal<Any>,
    new_plan: &RecordId,
    plan: &NewPlan,
    expected: Option<&RecordId>,
) -> Result<(), StoreError> {
    retry_conflicts(|| async {
        let response = db
            .query(SET_PLAN_CAS)
            .bind(("new_plan", new_plan.clone()))
            .bind(("plan", plan.clone()))
            .bind(("scan", plan.scan.clone()))
            .bind(("expected", expected.cloned()))
            .await?;
        checked(response)?;
        Ok(())
    })
    .await
}

// ---- job claim (database 4, contracts 5) -----------------------------------------------

// Statements: 0 BEGIN, 1 LET, 2 IF (the leased job, or []), 3 COMMIT.
const CLAIM_JOB: &str = "BEGIN;
LET $held = (SELECT count() AS n FROM job WHERE worker = $worker AND status = 'leased' GROUP ALL)[0].n ?? 0;
IF $held < $slots {
  UPDATE (SELECT VALUE id FROM job
          WHERE status = 'queued' AND kind IN $kinds AND queued_at <= $eligible_before
          ORDER BY queued_at LIMIT 1)
  SET status = 'leased', worker = $worker, lease_token = rand::ulid(),
      claimed_at = time::now(), lease_until = time::now() + 2m
  WHERE status = 'queued'
  RETURN AFTER;
} ELSE { RETURN []; };
COMMIT;";

#[derive(Debug, Clone, Copy, PartialEq, Eq, SurrealValue)]
#[surreal(untagged, rename_all = "snake_case")]
pub enum JobKind {
    Perception,
    Vlm,
    Verify,
    Guided,
}

#[derive(Debug, SurrealValue)]
pub struct LeasedJob {
    pub id: RecordId,
    pub kind: JobKind,
    pub scan: RecordId,
    pub payload: serde_json::Value,
    pub lease_token: String,
    pub lease_until: DateTime<Utc>,
    pub attempts: i64,
}

pub async fn claim_job(
    db: &Surreal<Any>,
    worker: &str,
    slots: i64,
    kinds: &[JobKind],
    eligible_before: DateTime<Utc>,
) -> Result<Option<LeasedJob>, StoreError> {
    retry_conflicts(|| async {
        let response = db
            .query(CLAIM_JOB)
            .bind(("worker", worker.to_owned()))
            .bind(("slots", slots))
            .bind(("kinds", kinds.to_vec()))
            .bind(("eligible_before", eligible_before))
            .await?;
        let jobs: Vec<LeasedJob> = checked(response)?.take(2)?;
        Ok(jobs.into_iter().next())
    })
    .await
}

// ---- idempotency keys (database 4, contracts 1) ----------------------------------------

const BEGIN_IDEMPOTENT: &str = "CREATE type::record('idempotency_key', [$user, $route, $key])
  SET fingerprint = $fp, expires_at = time::now() + 24h;";
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
    // contracts 1 decides: replay, 409 request_in_progress or 422 idempotency_conflict
    Existing(StoredKey),
}

// Retried: a duplicate CREATE from another connection can lose with TransactionConflict
// instead of AlreadyExists; the retry then reads the stored key.
pub async fn begin_idempotent(
    db: &Surreal<Any>,
    user: &RecordId,
    route: &str,
    key: &str,
    fingerprint: &str,
) -> Result<Begin, StoreError> {
    retry_conflicts(|| async {
        let created = db
            .query(BEGIN_IDEMPOTENT)
            .bind(("user", user.clone()))
            .bind(("route", route.to_owned()))
            .bind(("key", key.to_owned()))
            .bind(("fp", fingerprint.to_owned()))
            .await?;
        match checked(created) {
            Ok(_) => Ok(Begin::Started),
            Err(StoreError::AlreadyExists(_)) => {
                let response = db
                    .query(READ_IDEMPOTENT)
                    .bind(("user", user.clone()))
                    .bind(("route", route.to_owned()))
                    .bind(("key", key.to_owned()))
                    .await?;
                let stored: Option<StoredKey> = checked(response)?.take(0)?;
                // None: the cleanup deleted it in between; the caller retries the request.
                stored.map(Begin::Existing).ok_or(StoreError::NotFound)
            }
            Err(other) => Err(other),
        }
    })
    .await
}

// ---- membership and alert dedupe (database 4) ------------------------------------------

const ADD_MEMBER: &str = "INSERT RELATION INTO member_of { id: type::record('member_of', [$user, $garden]), in: $user, out: $garden, role: $role };";

#[derive(Debug, Clone, Copy, SurrealValue)]
#[surreal(untagged, rename_all = "snake_case")]
pub enum Role {
    Owner,
    Editor,
    Viewer,
    Reviewer,
}

// Err(StoreError::AlreadyExists(_)): the user is a member already; the role is unchanged.
pub async fn add_member(
    db: &Surreal<Any>,
    user: &RecordId,
    garden: &RecordId,
    role: Role,
) -> Result<(), StoreError> {
    retry_conflicts(|| async {
        let response = db
            .query(ADD_MEMBER)
            .bind(("user", user.clone()))
            .bind(("garden", garden.clone()))
            .bind(("role", role))
            .await?;
        checked(response)?;
        Ok(())
    })
    .await
}

const CREATE_ALERT_TASK: &str = "CREATE task SET garden = $garden, title = $title, source = 'forecast_alert', alert_key = $alert_key;";

// Ok(false): a task with this alert_key exists already (architecture 8.3).
pub async fn create_alert_task(
    db: &Surreal<Any>,
    garden: &RecordId,
    title: &str,
    alert_key: &str,
) -> Result<bool, StoreError> {
    retry_conflicts(|| async {
        let mut response = db
            .query(CREATE_ALERT_TASK)
            .bind(("garden", garden.clone()))
            .bind(("title", title.to_owned()))
            .bind(("alert_key", alert_key.to_owned()))
            .await?;
        match first_failure(response.take_errors()) {
            None => Ok(true),
            Some(error) if is_unique_violation(&error, "task_alert") => Ok(false),
            Some(error) => Err(error.into()),
        }
    })
    .await
}

// ---- reads and deletes (database 4 and 5) ----------------------------------------------

// One statement for every page: with $after NONE, `id > $after` holds for every record.
// The cursor is the ULID id, which is unique, so no row is skipped or repeated. EXPLAIN:
// IndexScan on plant_garden (garden, name) for the filter, then SortTopKByKey on id.
const PLANTS_PAGE: &str = "SELECT * FROM plant WHERE garden = $garden AND deleting_at = NONE AND id > $after ORDER BY id LIMIT 50;";

#[derive(Debug, SurrealValue)]
pub struct PlantRow {
    pub id: RecordId,
    pub name: String,
}

// after: the last id of the previous page, None for the first page.
pub async fn plants_page(
    db: &Surreal<Any>,
    garden: &RecordId,
    after: Option<&RecordId>,
) -> Result<Vec<PlantRow>, StoreError> {
    let response = db
        .query(PLANTS_PAGE)
        .bind(("garden", garden.clone()))
        .bind(("after", after.cloned()))
        .await?;
    Ok(checked(response)?.take(0)?)
}

const DELETE_SCAN_EVENTS: &str = "DELETE scan_event:[$scan, 1]..=[$scan, ..] RETURN NONE;";

pub async fn delete_scan_events(db: &Surreal<Any>, scan: &RecordId) -> Result<(), StoreError> {
    let response = db
        .query(DELETE_SCAN_EVENTS)
        .bind(("scan", scan.clone()))
        .await?;
    checked(response)?;
    Ok(())
}

// One batch: "DELETE (SELECT VALUE id FROM <t> WHERE <f> = $parent LIMIT 500) RETURN VALUE $before.id".
// RETURN NONE gives [] whether it deleted rows or not; RETURN VALUE id gives NONE per row.
pub const DELETE_OBSERVATIONS_OF_SCAN: &str = "DELETE (SELECT VALUE id FROM observation WHERE scan = $parent LIMIT 500) RETURN VALUE $before.id;";

pub async fn delete_in_batches(
    db: &Surreal<Any>,
    batch: &'static str,
    parent: &RecordId,
) -> Result<usize, StoreError> {
    let mut deleted = 0;
    loop {
        let response = db.query(batch).bind(("parent", parent.clone())).await?;
        let ids: Vec<RecordId> = checked(response)?.take(0)?;
        if ids.is_empty() {
            return Ok(deleted);
        }
        deleted += ids.len();
    }
}

// ---- shared definitions, copied from their canonical blocks ------------------------------

// copy of: rust-errors/mapping.md StoreError
/// The `store` crate's error. Messages: lowercase, no trailing punctuation, no source text.
/// `From<surrealdb::Error>` is the one place SDK errors are classified (rust-surrealdb).
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The record isn't there, or the caller may not see it.
    #[error("record not found")]
    NotFound,
    /// A compare-and-set lost (`THROW 'stale_plan'`, `THROW 'claim_lost'`, a lease check).
    /// The handler maps it: `lease_lost`, `invalid_state` or `request_in_progress`.
    #[error("{what} changed concurrently")]
    Conflict { what: &'static str },
    /// `TransactionConflict`: `retry_conflicts` re-runs the whole transaction.
    #[error("transaction conflict")]
    TxConflict(#[source] surrealdb::Error),
    /// A `CREATE` of an existing id, or an `INSERT RELATION` of an existing edge.
    #[error("record already exists")]
    AlreadyExists(#[source] surrealdb::Error),
    /// A `THROW` whose code has no variant of its own.
    #[error("query threw {0:?}")]
    Thrown(String),
    /// The connection to the server is gone.
    #[error("database unreachable")]
    Unavailable(#[source] surrealdb::Error),
    /// Anything else: a bug or a schema mismatch. The store function's span names the query.
    #[error("query failed")]
    Query(#[source] surrealdb::Error),
}

/// How the server reports a `THROW` (rust-surrealdb, how failures arrive).
pub const THROWN_PREFIX: &str = "An error occurred: ";

impl From<surrealdb::Error> for StoreError {
    fn from(error: surrealdb::Error) -> Self {
        use surrealdb::types::{AlreadyExistsError, QueryError};
        if matches!(error.query_details(), Some(QueryError::TransactionConflict)) {
            return Self::TxConflict(error);
        }
        if matches!(
            error.already_exists_details(),
            Some(AlreadyExistsError::Record { .. })
        ) {
            return Self::AlreadyExists(error);
        }
        if error.is_connection() {
            return Self::Unavailable(error);
        }
        if error.is_thrown() {
            match error.message().strip_prefix(THROWN_PREFIX) {
                Some("stale_plan") => return Self::Conflict { what: "plan" },
                Some("claim_lost") => return Self::Conflict { what: "step claim" },
                Some(code) => return Self::Thrown(code.to_owned()),
                None => {}
            }
        }
        Self::Query(error)
    }
}
// end: StoreError

// copy of: rust-surrealdb/errors-and-retries.md retry_conflicts
pub const MAX_ATTEMPTS: u32 = 5;

/// The statement that caused a failure: the lowest-index error that isn't one a failed
/// transaction skipped (`NotExecuted` before the cause, `Cancelled` after it).
pub fn first_failure(
    errors: impl IntoIterator<Item = (usize, surrealdb::Error)>,
) -> Option<surrealdb::Error> {
    use surrealdb::types::QueryError;
    let mut errors: Vec<_> = errors.into_iter().collect();
    errors.sort_unstable_by_key(|(index, _)| *index);
    let mut first = None;
    for (_, error) in errors {
        let skipped = matches!(
            error.query_details(),
            Some(QueryError::NotExecuted | QueryError::Cancelled)
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

/// Use instead of `.check()`, which returns the lowest-index error, a `NotExecuted`.
pub fn checked(
    mut response: surrealdb::IndexedResults,
) -> Result<surrealdb::IndexedResults, StoreError> {
    match first_failure(response.take_errors()) {
        Some(error) => Err(error.into()),
        None => Ok(response),
    }
}

/// Re-runs `run`, which must rebuild and re-send the whole query, on `TxConflict`.
pub async fn retry_conflicts<T, F, Fut>(mut run: F) -> Result<T, StoreError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, StoreError>>,
{
    let mut attempt = 0;
    loop {
        match run().await {
            Err(StoreError::TxConflict(_)) if attempt + 1 < MAX_ATTEMPTS => {
                let jitter_ms: u64 = rand::random_range(10..=50);
                tokio::time::sleep(std::time::Duration::from_millis(jitter_ms << attempt)).await;
                attempt += 1;
            }
            result => return result,
        }
    }
}
// end: retry_conflicts

// The test harness; in the backend it lives in the store crate's tests/common/mod.rs.
// copy of: rust-testing/database.md TestDb
/// The real schema, embedded as the store crate embeds it (backend/migrations/schema.surql).
const TEST_SCHEMA: &str = include_str!("../../../migrations/schema.surql");

/// The `api` password in every test namespace. The quote and backslash test `surql_string`.
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

/// One test's database: its own namespace holding the schema and an `api` user.
struct TestDb {
    url: String,
    ns: String,
    /// Signed in as `api` (database level, `ROLES EDITOR`), as the API runs.
    db: surrealdb::Surreal<surrealdb::engine::any::Any>,
}

impl TestDb {
    async fn new() -> Self {
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
        Self { url, ns, db }
    }

    /// Another `api` connection to the same namespace, as a second API process would have.
    async fn api(&self) -> surrealdb::Surreal<surrealdb::engine::any::Any> {
        Self::signed_in_api(&self.url, &self.ns).await
    }

    /// A root connection using the test's namespace, for what `api` may not do.
    async fn root(&self) -> surrealdb::Surreal<surrealdb::engine::any::Any> {
        let root = Self::signed_in_root(&self.url).await;
        root.use_ns(&self.ns)
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
mod live {
    use std::collections::BTreeMap;
    use std::sync::Arc;

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

    async fn fixture(db: &Surreal<Any>) -> Fixture {
        let f = Fixture {
            garden: new_id("garden"),
            plant: new_id("plant"),
            scan: new_id("scan"),
            diagnosis: new_id("diagnosis"),
        };
        let response = db
            .query(
                "CREATE $garden SET name = 'g', centroid = (-79.38, 43.65), \
                   location_precision = 'exact', location_label = 'Toronto, ON', time_zone = 'America/Toronto';
                 CREATE $plant SET garden = $garden, name = 'tomato';
                 CREATE $scan SET garden = $garden, plant = $plant;
                 CREATE $diagnosis SET scan = $scan, run = 1, status = 'healthy', candidates = [], \
                   organisms = [], soil = [], risks = [], evidence_basis = {}, citations = [], model_versions = {};",
            )
            .bind(("garden", f.garden.clone()))
            .bind(("plant", f.plant.clone()))
            .bind(("scan", f.scan.clone()))
            .bind(("diagnosis", f.diagnosis.clone()))
            .await
            .unwrap();
        checked(response).unwrap();
        f
    }

    async fn create_user(db: &Surreal<Any>) -> RecordId {
        let user = new_id("user");
        let response = db
            .query("CREATE $user SET auth_id = rand::ulid(), display_name = 'a'")
            .bind(("user", user.clone()))
            .await
            .unwrap();
        checked(response).unwrap();
        user
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB"]
    async fn query_is_ok_when_a_statement_fails() {
        let TestDb { db, .. } = TestDb::new().await;
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
    #[ignore = "needs SurrealDB"]
    async fn check_hides_the_cause_inside_a_transaction() {
        let TestDb { db, .. } = TestDb::new().await;
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
        let cause = first_failure(errors).unwrap();
        assert_eq!(cause.message(), "An error occurred: stale_plan");
        assert!(matches!(
            StoreError::from(cause),
            StoreError::Conflict { what: "plan" }
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
    #[ignore = "needs SurrealDB"]
    async fn misspelled_bind_is_none_and_none_is_not_null() {
        let TestDb { db, .. } = TestDb::new().await;
        let mut response = db.query("RETURN $nmae").bind(("name", "x")).await.unwrap();
        let value: Value = response.take(0).unwrap();
        assert_eq!(value, Value::None);

        let f = fixture(&db).await;
        let sql =
            "CREATE task SET garden = $garden, title = 't', source = 'user', detail = $detail";
        let none: Option<String> = None;
        let response = db
            .query(sql)
            .bind(("garden", f.garden.clone()))
            .bind(("detail", none))
            .await
            .unwrap();
        checked(response).unwrap();
        let response = db
            .query(sql)
            .bind(("garden", f.garden))
            .bind(("detail", Value::Null))
            .await
            .unwrap();
        let Err(StoreError::Query(error)) = checked(response) else {
            panic!("NULL into option<string> is a query error");
        };
        assert!(
            error.message().contains("found `NULL`"),
            "{}",
            error.message()
        );
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB"]
    async fn concurrent_appends_are_gap_free_and_replay_by_range() {
        let t = TestDb::new().await;
        let f = fixture(&t.db).await;
        let db = Arc::new(t.db);
        let mut tasks = Vec::new();
        for i in 0..39 {
            let db = Arc::clone(&db);
            let scan = f.scan.clone();
            tasks.push(tokio::spawn(async move {
                let payload = serde_json::json!({ "i": i });
                append_scan_event(&db, &scan, 1, "finding", &payload).await
            }));
        }
        let mut seqs = Vec::new();
        for task in tasks {
            seqs.push(task.await.unwrap().unwrap());
        }
        seqs.sort_unstable();
        assert_eq!(seqs, (1..=39).collect::<Vec<_>>());

        let replay = events_after(&db, &f.scan, 35).await.unwrap();
        let replayed: Vec<_> = replay.iter().filter_map(ScanEventRow::seq).collect();
        assert_eq!(replayed, vec![36, 37, 38, 39]);

        // The range needs a number: NONE + 1 fails.
        let error = db
            .query(EVENTS_AFTER)
            .bind(("scan", f.scan.clone()))
            .bind(("after", None::<i64>))
            .await
            .unwrap()
            .check()
            .unwrap_err();
        assert!(error.is_validation(), "{}", error.message());
        assert!(
            error
                .message()
                .contains("Cannot perform addition with 'none' and 'int'")
        );

        delete_scan_events(&db, &f.scan).await.unwrap();
        assert!(events_after(&db, &f.scan, 0).await.unwrap().is_empty());
    }

    fn plan(f: &Fixture, version: i64) -> NewPlan {
        NewPlan {
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
        }
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB"]
    async fn a_second_first_plan_is_stale_and_leaves_nothing() {
        let TestDb { db, .. } = TestDb::new().await;
        let f = fixture(&db).await;
        let first = new_id("action_plan");
        set_plan_cas(&db, &first, &plan(&f, 1), None).await.unwrap();
        let loser = new_id("action_plan");
        let result = set_plan_cas(&db, &loser, &plan(&f, 2), None).await;
        assert!(matches!(result, Err(StoreError::Conflict { what: "plan" })));
        let mut response = db
            .query("SELECT * FROM ONLY $p")
            .bind(("p", loser))
            .await
            .unwrap();
        let row: Option<Value> = response.take(0).unwrap();
        assert!(row.is_none());
        let second = new_id("action_plan");
        set_plan_cas(&db, &second, &plan(&f, 2), Some(&first))
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB"]
    async fn claim_stops_at_the_workers_slots() {
        let TestDb { db, .. } = TestDb::new().await;
        let f = fixture(&db).await;
        let response = db
            .query(
                "CREATE job SET kind = 'perception', scan = $scan, payload = {}, max_runtime = 5m;
                 CREATE job SET kind = 'perception', scan = $scan, payload = {}, max_runtime = 5m;",
            )
            .bind(("scan", f.scan.clone()))
            .await
            .unwrap();
        checked(response).unwrap();
        let worker = ulid::Ulid::generate().to_string();
        let kinds = [JobKind::Perception];
        let job = claim_job(&db, &worker, 1, &kinds, Utc::now())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(job.kind, JobKind::Perception);
        assert_eq!(
            job.scan, f.scan,
            "the namespace holds only this test's jobs"
        );
        assert_eq!(job.lease_token.len(), 26, "rand::ulid()");
        assert!(job.lease_until > Utc::now());
        let full = claim_job(&db, &worker, 1, &kinds, Utc::now())
            .await
            .unwrap();
        assert!(full.is_none(), "one slot, one lease");
        let other = claim_job(&db, "another-worker", 1, &kinds, Utc::now())
            .await
            .unwrap();
        assert!(other.is_some(), "the second job is still queued");
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB"]
    async fn duplicates_and_unique_indexes() {
        let TestDb { db, .. } = TestDb::new().await;
        let f = fixture(&db).await;
        let user = create_user(&db).await;

        let first = begin_idempotent(&db, &user, "POST /scans", "k1", "fp").await;
        assert!(matches!(first, Ok(Begin::Started)));
        let again = begin_idempotent(&db, &user, "POST /scans", "k1", "fp").await;
        let Ok(Begin::Existing(stored)) = again else {
            panic!("expected the stored key, got {again:?}");
        };
        assert_eq!(stored.state, KeyState::Running);
        assert_eq!(stored.fingerprint, "fp");

        add_member(&db, &user, &f.garden, Role::Owner)
            .await
            .unwrap();
        let twice = add_member(&db, &user, &f.garden, Role::Viewer).await;
        assert!(matches!(twice, Err(StoreError::AlreadyExists(_))));

        let mut response = db
            .query("UPSERT type::record('usage_daily', [$user, $utc_day]) SET scans += 1 RETURN VALUE scans;")
            .bind(("user", user.clone()))
            .bind(("utc_day", "2026-10-15"))
            .await
            .unwrap();
        let scans: Option<i64> = response.take(0).unwrap();
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

    #[tokio::test]
    #[ignore = "needs SurrealDB"]
    async fn duplicate_writes_from_many_connections_never_fail() {
        let t = TestDb::new().await;
        let f = fixture(&t.db).await;
        let user = create_user(&t.db).await;
        let mut connections = Vec::new();
        for _ in 0..8 {
            connections.push(Arc::new(t.api().await));
        }

        for round in 0..20 {
            let key = format!("k{round}");
            let mut tasks = Vec::new();
            for db in &connections {
                let (db, user, key) = (Arc::clone(db), user.clone(), key.clone());
                tasks.push(tokio::spawn(async move {
                    begin_idempotent(&db, &user, "POST /scans", &key, "fp").await
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
            let member = create_user(&t.db).await;
            let mut tasks = Vec::new();
            for db in &connections {
                let (db, member, garden) = (Arc::clone(db), member.clone(), f.garden.clone());
                tasks.push(tokio::spawn(async move {
                    add_member(&db, &member, &garden, Role::Editor).await
                }));
            }
            let mut added = 0;
            for task in tasks {
                match task.await.unwrap() {
                    Ok(()) => added += 1,
                    Err(StoreError::AlreadyExists(_)) => {}
                    Err(error) => panic!("a second invite accept must not fail: {error:?}"),
                }
            }
            assert_eq!(added, 1, "round {round}");
        }
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB"]
    async fn cursor_pages_and_batched_deletes() {
        let TestDb { db, .. } = TestDb::new().await;
        let f = fixture(&db).await;
        let response = db
            .query("FOR $i IN 0..60 { CREATE plant SET garden = $garden, name = 'p' };")
            .bind(("garden", f.garden.clone()))
            .await
            .unwrap();
        checked(response).unwrap();
        let first = plants_page(&db, &f.garden, None).await.unwrap();
        assert_eq!(first.len(), 50, "id > NONE is true for every row");
        let rest = plants_page(&db, &f.garden, first.last().map(|p| &p.id))
            .await
            .unwrap();
        assert_eq!(rest.len(), 11, "60 plus the fixture's plant");
        assert!(rest.iter().all(|p| first.iter().all(|q| q.id != p.id)));

        // What each RETURN clause gives back from a batch of 2 (plants with no scans).
        let garden = new_id("garden");
        let response = db
            .query(
                "CREATE $garden SET name = 'h', centroid = (-79.38, 43.65), \
                   location_precision = 'exact', location_label = 'Toronto, ON', time_zone = 'America/Toronto';
                 FOR $i IN 0..5 { CREATE plant SET garden = $garden, name = 'p' };",
            )
            .bind(("garden", garden.clone()))
            .await
            .unwrap();
        checked(response).unwrap();
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
            (0, Some(3)),
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
        let returned: Vec<RecordId> = response.take(0).unwrap();
        let left: Option<i64> = response.take(1).unwrap();
        assert_eq!((returned.len(), left), (1, Some(0)));
        let mut response = db
            .query(batch("RETURN NONE"))
            .bind(("garden", garden))
            .await
            .unwrap();
        let returned: Vec<Value> = response.take(0).unwrap();
        assert!(returned.is_empty(), "RETURN NONE: [] for 0 rows too");

        let response = db
            .query(
                "FOR $i IN 0..1200 { CREATE observation SET scan = $scan, run = 1, stage = 'soil', \
                   pipeline_stage = 'soil', label = 'l', confidence = 0.5, source = 'vlm', model_version = 'm' };",
            )
            .bind(("scan", f.scan.clone()))
            .await
            .unwrap();
        checked(response).unwrap();
        let deleted = delete_in_batches(&db, DELETE_OBSERVATIONS_OF_SCAN, &f.scan)
            .await
            .unwrap();
        assert_eq!(deleted, 1200);
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB"]
    async fn users_and_sessions() {
        let t = TestDb::new().await;
        let as_root = connect(&t.url).await.unwrap();
        let refused = as_root
            .signin(Root {
                username: "api".into(),
                password: TEST_API_PASS.into(),
            })
            .await
            .unwrap_err();
        assert!(refused.is_not_allowed());
        let api = connect_api(&t.url, &t.ns, TEST_API_PASS).await.unwrap();
        let mut response = api.query("RETURN session::ns()").await.unwrap();
        let ns: Option<String> = response.take(0).unwrap();
        assert_eq!(ns.as_deref(), Some(t.ns.as_str()));

        // DEFINE USER takes a literal: a bound, LET or PASSHASH parameter is a parse error.
        let root = t.root().await;
        for (sql, bind) in [
            ("DEFINE USER p1 ON DATABASE PASSWORD $p ROLES VIEWER", true),
            (
                "LET $p = 'secret'; DEFINE USER p2 ON DATABASE PASSWORD $p ROLES VIEWER",
                false,
            ),
            ("DEFINE USER p3 ON DATABASE PASSHASH $p ROLES VIEWER", true),
        ] {
            let query = root.query(sql);
            let query = if bind {
                query.bind(("p", "secret"))
            } else {
                query
            };
            let error = query.await.unwrap_err();
            assert!(error.is_validation(), "{sql}: {}", error.message());
            assert!(error.message().contains("expected a strand"), "{sql}");
        }

        // As api, the schema's DEFINE NAMESPACE and DEFINE DATABASE need more than EDITOR;
        // every other statement applies (the tables are this test's own).
        let mut response = t.db.query(schema_in(TEST_SCHEMA, &t.ns)).await.unwrap();
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
        let define_owner = format!(
            "DEFINE USER owner ON NAMESPACE PASSWORD {} ROLES OWNER;",
            surql_string("owner-pass")
        );
        checked(root.query(define_owner).await.unwrap()).unwrap();
        let owner = connect(&t.url).await.unwrap();
        owner
            .signin(Namespace {
                namespace: t.ns.clone(),
                username: "owner".into(),
                password: "owner-pass".into(),
            })
            .await
            .unwrap();
        owner.use_ns(&t.ns).await.unwrap();
        let mut response = owner
            .query(format!(
                "DEFINE NAMESPACE IF NOT EXISTS {}_x; DEFINE DATABASE IF NOT EXISTS second STRICT;",
                t.ns
            ))
            .await
            .unwrap();
        let errors: BTreeMap<usize, surrealdb::Error> =
            response.take_errors().into_iter().collect();
        assert_eq!(errors.keys().copied().collect::<Vec<_>>(), vec![0]);
        assert!(errors[&0].is_not_allowed());

        // As root, the whole schema applies into a fresh namespace twice in a row.
        let fresh = format!("{}_fresh", t.ns);
        let schema = schema_in(TEST_SCHEMA, &fresh);
        apply_schema(&t.url, "root", "root", &schema).await.unwrap();
        apply_schema(&t.url, "root", "root", &schema).await.unwrap();

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

`check_examples.py --live` runs the tests above: it starts the pinned image on a free port, sets `PDM_TEST_SURREALDB_URL`, runs `cargo nextest run --run-ignored only` (one process per test) and stops the container ([rust-testing, database.md](../rust-testing/database.md#running-them)). By hand, with the server from [SKILL.md, tests](SKILL.md#tests) on `127.0.0.1:18000`:

```sh
PDM_TEST_SURREALDB_URL=ws://127.0.0.1:18000 cargo nextest run --run-ignored only
```

Results on 2026-10-16 (crate 3.3.4, image `v3.3.4@sha256:649a60a3…`, `memory`, cargo-nextest 0.9.148): the 10 tests above and the 3 in [rust-testing, database.md](../rust-testing/database.md#one-namespace-per-test) passed together, 13 of 13, in 16 s, each in its own process and its own namespace. The earlier shared-namespace version of these tests failed 9 of 9 under the same runner.
