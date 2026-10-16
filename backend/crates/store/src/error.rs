//! Classifying `SurrealDB` errors ([database 4], [database 6]; research notes `surrealdb.md` §5–6).
//!
//! `.query().await` is `Ok` even when a statement failed: each statement's error is read from
//! `take_errors()`. In a failed transaction every other statement reports `NotExecuted` (or
//! `Cancelled`), so [`first_failure`] skips those and returns the cause; `check()` would return a
//! `NotExecuted` instead. [`classify`] turns the cause into a [`DbFailure`], and
//! [`DbFailure::into_store`] maps it to the domain's [`StoreError`]. A caller that knows more
//! (which plan won, which job lost its lease) matches the failure first.

use domain::ports::{BoxError, Conflict, StoreError};
use surrealdb::types::{AlreadyExistsError, QueryError};

/// How the server prefixes the message of a `THROW` ✓.
pub const THROWN_PREFIX: &str = "An error occurred: ";

/// The codes the store's statements `THROW`. Matched with `e.is_thrown()` and the text after
/// [`THROWN_PREFIX`], since a thrown error carries no typed details.
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

/// What one failed statement means, before the caller adds context.
#[derive(Debug)]
pub enum DbFailure {
    /// `QueryError::TransactionConflict` ✓: [`crate::tx::run_tx`] retries the whole
    /// transaction; reaching a caller means the retries ran out.
    TxConflict(surrealdb::Error),
    /// `THROW '<code>'` ✓ (kind `Thrown`, message `"An error occurred: <code>"`).
    Thrown {
        /// The code after the prefix.
        code: String,
        /// The SDK error.
        source: surrealdb::Error,
    },
    /// `CREATE` of an existing id or `INSERT RELATION` of an existing edge ✓
    /// (`AlreadyExistsError::Record`).
    AlreadyExists(surrealdb::Error),
    /// A `UNIQUE` index refused the write ✓. On 3.3.4 this is kind `Internal` with no details,
    /// only the message ``Database index `<name>` already contains …``.
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
/// every error is a skip (which shouldn't happen), the lowest-index one.
pub fn first_failure(
    errors: impl IntoIterator<Item = (usize, surrealdb::Error)>,
) -> Option<surrealdb::Error> {
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

/// The one place SDK errors are classified.
pub fn classify(error: surrealdb::Error) -> DbFailure {
    if matches!(error.query_details(), Some(QueryError::TransactionConflict)) {
        return DbFailure::TxConflict(error);
    }
    if matches!(
        error.already_exists_details(),
        Some(AlreadyExistsError::Record { .. })
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

/// The index named by a unique-index violation's message, if it is one.
fn unique_index(error: &surrealdb::Error) -> Option<String> {
    if !error.is_internal() {
        return None;
    }
    let rest = error.message().strip_prefix("Database index `")?;
    let (name, tail) = rest.split_once('`')?;
    tail.starts_with(" already contains")
        .then(|| name.to_owned())
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

/// A stored row that doesn't convert into its domain type: a bug or a schema drift, never a
/// user error. Reported as [`StoreError::Query`].
#[derive(Debug, thiserror::Error)]
pub enum RowError {
    /// A record id of the wrong table or key shape.
    #[error("record id {id} is not a {expected} id")]
    BadId {
        /// The id as stored.
        id: String,
        /// The table expected.
        expected: &'static str,
    },
    /// A field whose value the domain type refuses.
    #[error("field {field}: {problem}")]
    BadField {
        /// The field.
        field: &'static str,
        /// Why.
        problem: String,
    },
    /// A FLEXIBLE object that doesn't parse as its contract shape.
    #[error("field {field} does not match its contract shape")]
    Json {
        /// The field.
        field: &'static str,
        /// The serde error.
        #[source]
        source: serde_json::Error,
    },
}

impl RowError {
    /// As the store's error.
    pub fn into_store(self, what: &'static str) -> StoreError {
        StoreError::Query {
            what,
            source: Box::new(self),
        }
    }
}

/// A typed `take(n)` that failed (the statement returned a shape the row type refuses).
pub fn take_failed(what: &'static str, error: surrealdb::Error) -> StoreError {
    StoreError::Query {
        what,
        source: Box::new(error) as BoxError,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

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
        let cause = classify(first_failure(errors).unwrap());
        assert_eq!(cause.thrown(), Some("claim_lost"));
        assert!(matches!(
            cause.into_store("test"),
            StoreError::Conflict(Conflict::ClaimLost)
        ));
    }

    #[test]
    fn unique_index_is_read_from_the_message() {
        let e = surrealdb::Error::internal(
            "Database index `task_alert` already contains 'k', with record `task:1`".to_owned(),
        );
        assert!(
            matches!(classify(e), DbFailure::UniqueIndex { index, .. } if index == "task_alert")
        );
    }
}
