# Store errors and the conflict retry

Part of [`rust-surrealdb`](SKILL.md). The rules are in [SKILL.md, how failures arrive](SKILL.md#how-failures-arrive) and [retrying conflicts](SKILL.md#retrying-conflicts); the retry follows [database 4](../../../docs/database/README.md#4-write-patterns) and [6](../../../docs/database/README.md#6-rust-sdk): the whole transaction, at most 5 attempts, 10–50 ms × 2^attempt with jitter. [`rust-errors`](../rust-errors/SKILL.md) owns the error design: the `StoreError` below is an exact copy of its canonical block in [rust-errors, mapping.md](../rust-errors/mapping.md), which `check_examples.py` compares line for line.

What the block contains:

- A copy of `StoreError`, with one variant per failure the store acts on, and `From<surrealdb::Error>`, which is the one place errors are classified: by `query_details()` for `TransactionConflict` (`TxConflict`, [`QueryError`](https://docs.rs/surrealdb-types/3.3.4/surrealdb_types/enum.QueryError.html)), by `already_exists_details()` for `AlreadyExistsError::Record` ([`Error`](https://docs.rs/surrealdb-types/3.3.4/surrealdb_types/struct.Error.html)), by `is_connection()` for a lost connection, and for a `THROW` by the text after `"An error occurred: "`, which is how the server reports one (✓ on 3.3.4; [errors](https://surrealdb.com/docs/reference/rest-api/errors)). The compare-and-set codes `stale_plan` and `claim_lost` become `Conflict { what }`.
- The canonical **retry kit** (`MAX_ATTEMPTS`, `first_failure`, `checked`, `retry_conflicts`), between `// canonical: rust-surrealdb/errors-and-retries.md retry_conflicts` and `// end: retry_conflicts`; [write-patterns.md](write-patterns.md) carries a copy.
- `first_failure` sorts `take_errors()` by statement index and skips the statements a failed transaction didn't run: those before the cause report `NotExecuted` and those after it `Cancelled` ([SKILL.md, queries](SKILL.md#queries)). So a failed transaction reports its cause.
- `checked`, the replacement for `.check()`.
- `retry_conflicts`, which re-runs a closure that rebuilds and re-sends the whole query. The jitter is `rand::random_range(10..=50)` from `rand` 0.10, drawn from the thread-local generator and dropped before the `.await` ([`random_range`](https://docs.rs/rand/0.10.3/rand/fn.random_range.html)); the sleep doubles with each attempt, so 4 sleeps total 150–750 ms.

The tests build the errors with `surrealdb::Error::query` and `surrealdb::Error::thrown`, the constructors `surrealdb-types` 3.3.4 makes public, and run on tokio's paused clock (`start_paused = true`), so the back-off takes no real time.

```rust
use std::time::Duration;

use surrealdb::types::QueryError;

// canonical: rust-surrealdb/errors-and-retries.md retry_conflicts
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    fn conflict() -> StoreError {
        surrealdb::Error::query(
            "Transaction conflict".to_owned(),
            QueryError::TransactionConflict,
        )
        .into()
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
        assert!(matches!(result, Err(StoreError::TxConflict(_))));
        assert_eq!(calls.load(Ordering::Relaxed), 5);
        let waited = started.elapsed();
        assert!(waited >= Duration::from_millis(150) && waited <= Duration::from_millis(750));
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

        let thrown = surrealdb::Error::thrown("An error occurred: stale_plan".to_owned());
        let calls = AtomicU32::new(0);
        let result: Result<(), _> = retry_conflicts(|| async {
            calls.fetch_add(1, Ordering::Relaxed);
            Err(thrown.clone().into())
        })
        .await;
        assert!(matches!(result, Err(StoreError::Conflict { what: "plan" })));
        assert_eq!(
            calls.load(Ordering::Relaxed),
            1,
            "a lost compare-and-set is final"
        );
    }

    #[test]
    fn the_retry_future_is_send() {
        fn assert_send<T: Send>(_: &T) {}
        let future = retry_conflicts(|| async { Ok::<_, StoreError>(()) });
        assert_send(&future);
    }

    #[test]
    fn first_failure_skips_not_executed_and_cancelled() {
        let errors = HashMap::from([
            (
                1,
                surrealdb::Error::query("not executed".to_owned(), QueryError::NotExecuted),
            ),
            (
                2,
                surrealdb::Error::thrown("An error occurred: claim_lost".to_owned()),
            ),
            (
                3,
                surrealdb::Error::query("cancelled".to_owned(), QueryError::Cancelled),
            ),
        ]);
        let cause = first_failure(errors).unwrap();
        assert!(matches!(
            StoreError::from(cause),
            StoreError::Conflict { what: "step claim" }
        ));
    }
}
```
