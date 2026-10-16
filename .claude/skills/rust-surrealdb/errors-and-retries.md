# Store errors and the conflict retry

Part of [`rust-surrealdb`](SKILL.md). The rules are in [SKILL.md, how failures arrive](SKILL.md#how-failures-arrive) and [retrying conflicts](SKILL.md#retrying-conflicts); the retry follows [database 4](../../../docs/database/README.md#4-write-patterns) and [6](../../../docs/database/README.md#6-rust-sdk): the whole transaction, at most 5 attempts, 10–50 ms × 2^attempt with jitter. The design is [backend store 5](../../../docs/backend/store.md#5-transactions-retries-and-the-error-mapping): the store classifies each SDK error once, into a crate-private `DbFailure`, and maps that into the domain's `StoreError` ([backend domain 6, port errors](../../../docs/backend/domain.md#port-errors)), keeping the SDK error as the `source`. No `surrealdb` type leaves the crate. [`rust-errors`](../rust-errors/SKILL.md) owns the port errors: the `StoreError`, `BoxError` and `DomainError` below are exact copies of their canonical blocks in [rust-errors, mapping.md](../rust-errors/mapping.md), which `check_examples.py` compares line for line.

What the block contains, between `// canonical: rust-surrealdb/errors-and-retries.md retry_conflicts` and `// end: retry_conflicts` (the **store kit**, which [write-patterns.md](write-patterns.md) copies):

- `MAX_ATTEMPTS` and `backoff`: 5 attempts, and before retry `attempt` (0-based) a sleep of `rand::random_range(10..=50)` ms shifted left by `attempt`, so the four sleeps total 150–750 ms. The jitter is drawn from the thread-local generator and dropped before the `.await`, so the future stays `Send` ([`random_range`](https://docs.rs/rand/0.10.3/rand/fn.random_range.html)).
- `THROWN_PREFIX` and the `thrown` codes: every code the store's statements `THROW` ([backend store 5, classifying failures](../../../docs/backend/store.md#classifying-failures)).
- `DbFailure`: `TxConflict`, `Thrown { code, source }`, `AlreadyExists`, `UniqueIndex { index, source }`, `Unavailable`, `Other`, each holding the SDK error.
- `first_failure`: sorts `take_errors()` by statement index and skips the statements a failed transaction didn't run, `NotExecuted` before the cause and `Cancelled` after it ([SKILL.md, queries](SKILL.md#queries)). So a failed transaction reports its cause.
- `classify`, the one place SDK errors are read: `query_details()` for `TransactionConflict` ([`QueryError`](https://docs.rs/surrealdb-types/3.3.4/surrealdb_types/enum.QueryError.html)), `already_exists_details()` for `AlreadyExistsError::Record`, `is_connection()`, `is_thrown()` with the text after `"An error occurred: "`, and a unique index by its name in an `Internal` message, which is all 3.3.4 gives ([`Error`](https://docs.rs/surrealdb-types/3.3.4/surrealdb_types/struct.Error.html); the table in [SKILL.md](SKILL.md#how-failures-arrive)).
- `DbFailure::thrown`, the code a caller checks before the default mapping, and `DbFailure::into_store(what)`, that default mapping: `TxConflict` (retries spent) is `Conflict(Retries)`, `AlreadyExists` and `UniqueIndex` are `Conflict(AlreadyExists)`, `stale_plan`, `claim_lost` and `question_step_lost` are their `Conflict`s, `not_found` is `NotFound`, a lost connection is `Unavailable`, and anything else is `Query { what, source }` ([backend store 5](../../../docs/backend/store.md#classifying-failures)). A caller that knows more maps first: the plan write reads the plan that won into `StalePlan { current }`, the map upload reads the version into `StaleMap { current_version }`, and the alert insert treats its unique index as success (the table in [write-patterns.md](write-patterns.md#what-each-function-relies-on)).
- `take_failed`, for a `take` that doesn't decode: `Query { what, source }`, a bug or a schema drift.
- `checked`, the replacement for `.check()`: an `Err` from `.query().await` (a parse error, a protected parameter, a lost connection fails the whole request) or the first statement failure, classified.
- `retry_conflicts`, which re-runs an attempt on `TxConflict` only: any other failure returns at once, because a lost compare-and-set is final. When the retries run out it logs one warning.
- `run_tx(db, sql, bind)`, every statement's entry point: it sends `sql` (one statement, or a whole `BEGIN … COMMIT` as **one** query, [database 4](../../../docs/database/README.md#4-write-patterns)) with the binds `bind` adds to a fresh query on each attempt. A single statement is its own transaction and can conflict the same way, so plain reads go through it too ([backend store 5, `run_tx`](../../../docs/backend/store.md#run_tx)).

`run_tx` is store.md's, with one change, this skill's choice: the loop is the separate `retry_conflicts`, so the back-off is tested on tokio's paused clock without a database. Its bounds are store.md's, each for a reason ([backend store 5, the closure's bounds](../../../docs/backend/store.md#run_tx)):

| Bound | Why |
|---|---|
| `'c` | The borrow of the client and the statement text. `Surreal::query` returns a `Query<'c, Any>` that borrows both (`src/method/query.rs`); one named lifetime for the argument and the return ties the closure's output to the query it was given |
| `Fn`, not `FnOnce` | It runs once per attempt and changes nothing: it borrows the caller's arguments and clones each value into a bind (`.bind(("scan", scan.clone()))`) |
| `+ Send` | The `move` closure that calls it lives across the `.await`s of `run_tx`'s future, which `#[async_trait]` boxes as `dyn Future + Send` ([rust-async, traits](../rust-async/traits.md#3-what-the-project-does-with-its-boundary-traits)). `the_store_futures_are_send` proves it |

A closure written on its own, `|q: Query<'_, Any>| q.bind(…)`, doesn't get one lifetime: its argument's `'_` and its inferred return type are two unrelated lifetimes, and it fails with "lifetime may not live long enough" (the `compile_fail` block at the end). Passed straight to `run_tx`, the bound `Fn(Query<'c, Any>) -> Query<'c, Any>` gives the closure its signature, so `|q| q.bind(…)` compiles. A binder reused by several statements is a function with a named `'c`, as `bind_scan` below and store.md's `bind_claim` are ([backend store 18, named lifetimes](../../../docs/backend/store.md#18-concurrency-and-lifetimes)).

The tests build the errors with the constructors `surrealdb-types` 3.3.4 makes public (`Error::query`, `thrown`, `already_exists`, `internal`, `connection`) and run the retry on tokio's paused clock (`start_paused = true`), so the back-off takes no real time ([`tokio::test`](https://docs.rs/tokio/1.53.3/tokio/attr.test.html#configure-the-runtime-to-start-with-time-paused)). What the server really sends for each failure is proved on the embedded engine in [write-patterns.md](write-patterns.md#the-module). `JobId` and `PlanId` are `String` stand-ins for `domain`'s ids, as in mapping.md.

```rust
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb::method::Query;
use surrealdb::types::RecordId;

/// Stand-ins for `domain::id::{JobId, PlanId}` (backend domain 4).
pub type JobId = String;
pub type PlanId = String;

// canonical: rust-surrealdb/errors-and-retries.md retry_conflicts
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

/// A binder several statements share: a function, so one named `'c` ties its argument to
/// its result.
pub fn bind_scan<'c>(query: Query<'c, Any>, scan: &RecordId) -> Query<'c, Any> {
    query.bind(("scan", scan.clone()))
}

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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::error::Error as _;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use surrealdb::types::{AlreadyExistsError, ConnectionError, QueryError};

    use super::*;

    fn conflict() -> DbFailure {
        classify(surrealdb::Error::query(
            "Transaction conflict: Write conflict, retry the transaction".to_owned(),
            QueryError::TransactionConflict,
        ))
    }

    fn threw(code: &str) -> surrealdb::Error {
        surrealdb::Error::thrown(format!("{THROWN_PREFIX}{code}"))
    }

    #[tokio::test(start_paused = true)]
    async fn gives_up_after_five_attempts() {
        let calls = AtomicU32::new(0);
        let started = tokio::time::Instant::now();
        let result: Result<(), _> = retry_conflicts(|| async {
            calls.fetch_add(1, Ordering::Relaxed);
            Err(conflict())
        })
        .await;
        assert_eq!(calls.load(Ordering::Relaxed), 5);
        let waited = started.elapsed();
        assert!(waited >= Duration::from_millis(150) && waited <= Duration::from_millis(750));
        let error = result.unwrap_err().into_store("append event");
        assert!(matches!(error, StoreError::Conflict(Conflict::Retries)));
    }

    #[tokio::test(start_paused = true)]
    async fn returns_the_first_success_and_never_retries_other_errors() {
        let calls = AtomicU32::new(0);
        let result = retry_conflicts(|| async {
            match calls.fetch_add(1, Ordering::Relaxed) {
                0 | 1 => Err(conflict()),
                n => Ok(n),
            }
        })
        .await;
        assert_eq!(result.unwrap(), 2);

        let calls = AtomicU32::new(0);
        let result: Result<(), _> = retry_conflicts(|| async {
            calls.fetch_add(1, Ordering::Relaxed);
            Err(classify(threw(thrown::STALE_PLAN)))
        })
        .await;
        assert_eq!(
            calls.load(Ordering::Relaxed),
            1,
            "a lost compare-and-set is final"
        );
        assert_eq!(result.unwrap_err().thrown(), Some(thrown::STALE_PLAN));
    }

    #[test]
    fn the_store_futures_are_send() {
        fn assert_send<T: Send>(_: &T) {}
        // `init` builds a client without connecting; the future is never polled.
        let db = Surreal::<Any>::init();
        let scan = RecordId::new("scan", "01JAX3K9T6R2B8V0QZ4M7D1E5F");
        let future = run_tx(&db, "SELECT * FROM ONLY $scan;", |q| bind_scan(q, &scan));
        assert_send(&future);
        let retry = retry_conflicts(|| async { Ok::<_, DbFailure>(()) });
        assert_send(&retry);
    }

    #[test]
    fn first_failure_skips_not_executed_and_cancelled() {
        let errors = HashMap::from([
            (
                1,
                surrealdb::Error::query("not executed".to_owned(), QueryError::NotExecuted),
            ),
            (2, threw(thrown::CLAIM_LOST)),
            (
                3,
                surrealdb::Error::query("cancelled".to_owned(), QueryError::Cancelled),
            ),
        ]);
        let cause = first_failure(errors).unwrap();
        assert!(matches!(
            classify(cause).into_store("commit step"),
            StoreError::Conflict(Conflict::ClaimLost)
        ));
    }

    const TASK_ALERT: &str =
        "Database index `task_alert` already contains 'k1', with record `task:1`";

    fn already_exists() -> surrealdb::Error {
        surrealdb::Error::already_exists(
            "Database record `member_of:[user:u, garden:g]` already exists".to_owned(),
            AlreadyExistsError::Record {
                id: "member_of:[user:u, garden:g]".to_owned(),
            },
        )
    }

    fn connection_lost() -> surrealdb::Error {
        surrealdb::Error::connection(
            "connection failed".to_owned(),
            ConnectionError::ConnectionFailed,
        )
    }

    fn coercion() -> surrealdb::Error {
        surrealdb::Error::internal("Couldn't coerce value for field `status`".to_owned())
    }

    #[test]
    fn each_failure_is_classified_once() {
        assert!(matches!(conflict(), DbFailure::TxConflict(_)));
        assert_eq!(
            classify(threw(thrown::STALE_PLAN)).thrown(),
            Some("stale_plan")
        );
        assert!(matches!(
            classify(already_exists()),
            DbFailure::AlreadyExists(_)
        ));
        let DbFailure::UniqueIndex { index, .. } =
            classify(surrealdb::Error::internal(TASK_ALERT.to_owned()))
        else {
            panic!("a unique index is recognised by its message");
        };
        assert_eq!(index, "task_alert");
        assert!(matches!(
            classify(connection_lost()),
            DbFailure::Unavailable(_)
        ));
        assert!(matches!(classify(coercion()), DbFailure::Other(_)));
    }

    #[test]
    fn into_store_is_the_default_mapping() {
        let map = |error| classify(error).into_store("op");
        assert!(matches!(
            map(threw(thrown::STALE_PLAN)),
            StoreError::Conflict(Conflict::StalePlan { current: None })
        ));
        assert!(matches!(
            map(threw(thrown::CLAIM_LOST)),
            StoreError::Conflict(Conflict::ClaimLost)
        ));
        assert!(matches!(
            map(threw(thrown::QUESTION_STEP_LOST)),
            StoreError::Conflict(Conflict::QuestionStepLost)
        ));
        assert!(matches!(
            map(threw(thrown::NOT_FOUND)),
            StoreError::NotFound
        ));
        for error in [
            already_exists(),
            surrealdb::Error::internal(TASK_ALERT.to_owned()),
        ] {
            assert!(matches!(
                map(error),
                StoreError::Conflict(Conflict::AlreadyExists)
            ));
        }
        assert!(matches!(map(connection_lost()), StoreError::Unavailable(_)));

        // A code the default mapping leaves to the caller, and anything else: `Query` with the
        // operation's name, the SDK error kept as the source and out of `Display`.
        for error in [threw(thrown::BUDGET_EXCEEDED), coercion()] {
            let message = error.message().to_owned();
            let mapped = classify(error).into_store("create scan");
            assert!(matches!(
                mapped,
                StoreError::Query {
                    what: "create scan",
                    ..
                }
            ));
            assert_eq!(mapped.to_string(), "query failed: create scan");
            assert_eq!(mapped.source().map(ToString::to_string), Some(message));
        }
    }
}
```

The closure rule, as the compiler sees it: a closure written on its own gets two unrelated lifetimes for its argument and its result, so it fails with "lifetime may not live long enough". `bind_scan` above, a function with a named `'c`, is the fix.

```rust,compile_fail
use surrealdb::engine::any::Any;
use surrealdb::method::Query;

fn main() {
    let bind_scan = |query: Query<'_, Any>| query.bind(("scan", 1));
    drop(bind_scan);
}
```
