//! Sending a statement or a transaction, with the conflict retry ([database 4]: "The API
//! retries the whole transaction, up to 5 times with jittered back-off").

use std::time::Duration;

use surrealdb::{IndexedResults, Surreal, engine::any::Any, method::Query};

use crate::error::{DbFailure, classify, first_failure};

/// Attempts per transaction, the first included.
pub const MAX_ATTEMPTS: u32 = 5;

/// The back-off before retry `attempt` (0-based): 10–50 ms of jitter, doubled per attempt, so
/// the four sleeps total 150–750 ms. The jitter is drawn from the thread-local generator and
/// dropped before the `.await`, so the caller's future stays `Send`.
pub fn backoff(attempt: u32) -> Duration {
    let jitter_ms: u64 = rand::random_range(10..=50);
    Duration::from_millis(jitter_ms << attempt)
}

/// Sends `sql` (one statement, or a whole `BEGIN … COMMIT` as **one** query) and returns its
/// results once no statement failed. `bind` adds the parameters to a fresh query on each
/// attempt.
///
/// - **Retry.** A `TransactionConflict` re-sends the whole query, up to [`MAX_ATTEMPTS`]. A
///   conflicted transaction committed nothing, so a retry is always safe, even for `+= 1`.
///   Any other failure returns at once: a lost compare-and-set is final.
/// - **`'c`** is the borrow of the client and of the statement text; the returned
///   [`Query<'c, Any>`] borrows both, and nothing outlives the call.
/// - **`F: Fn(Query<'c, Any>) -> Query<'c, Any> + Send`.** `Fn`, not `FnOnce`, because it runs
///   once per attempt; it borrows the caller's arguments and clones each value into a bind
///   (`.bind(("scan", rid(scan)))`), so the arguments are never moved out. `Send` because the
///   closure lives across the `.await`s of this future, which `#[async_trait]` boxes as
///   `dyn Future + Send`.
#[expect(
    clippy::result_large_err,
    reason = "the failure carries the SDK error by value and is mapped at once by the caller"
)]
pub async fn run_tx<'c, F>(
    db: &'c Surreal<Any>,
    sql: &'c str,
    bind: F,
) -> Result<IndexedResults, DbFailure>
where
    F: Fn(Query<'c, Any>) -> Query<'c, Any> + Send,
{
    let mut attempt = 0;
    loop {
        let failure = match bind(db.query(sql)).await {
            Ok(mut results) => match first_failure(results.take_errors()) {
                None => return Ok(results),
                Some(error) => classify(error),
            },
            // A parse error or a lost connection fails the whole request.
            Err(error) => classify(error),
        };
        match failure {
            DbFailure::TxConflict(_) if attempt + 1 < MAX_ATTEMPTS => {
                tokio::time::sleep(backoff(attempt)).await;
                attempt += 1;
            }
            failure => {
                if matches!(failure, DbFailure::TxConflict(_)) {
                    tracing::warn!(attempts = MAX_ATTEMPTS, "transaction conflicts persisted");
                }
                return Err(failure);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_within_the_jitter() {
        for attempt in 0..4 {
            let d = backoff(attempt).as_millis();
            assert!((10u128 << attempt..=50u128 << attempt).contains(&d));
        }
    }
}
