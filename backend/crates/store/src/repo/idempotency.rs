//! Idempotency keys and usage counters ([contracts 1], idempotency; [database 4], idempotency
//! keys ✓ and limits ✓).

use chrono::{DateTime, NaiveDate, Utc};
use domain::{
    chat::Usage,
    id::UserId,
    ports::{
        IdempotencyBegin, IdempotencyRepo, IdempotencyScope, StoreError, StoredResponse, UsageRepo,
        UsageTotals,
    },
    rules::limits::IDEMPOTENCY_TTL,
    wire::Timestamp,
};
use surrealdb::types::{RecordId, SurrealValue};

use crate::{
    SurrealStore,
    error::{DbFailure, RowError, take_failed},
    ids::{idempotency_rid, usage_rid},
    rows::{KeyStateRow, dt, u32_of},
    tx::run_tx,
};

/// The statements.
pub mod sql {
    /// The race-free "first request wins" ([database 4] ✓): a second `CREATE` of the same
    /// `idempotency_key:[user, route, key]` fails with "already exists".
    pub const BEGIN_KEY: &str = "CREATE $key SET fingerprint = $fp, expires_at = $expires_at;";
    /// The stored key after "already exists".
    pub const READ_KEY: &str = "SELECT * FROM ONLY $key;";
    /// An expired key the cleanup hasn't reached yet: deleted, then `BEGIN_KEY` again.
    pub const DELETE_EXPIRED_KEY: &str = "DELETE $key WHERE expires_at <= $now;";
    /// The finished response ([contracts 1]: `2xx` and the deterministic `4xx` only).
    pub const FINISH_KEY: &str =
        "UPDATE $key SET state = 'done', response_status = $status, response_body = $body;";
    /// A `409`, `429` or `5xx`, or an interrupted chat message: the key is released.
    pub const RELEASE_KEY: &str = "DELETE $key;";
    /// Cleanup, one batch of 500 ([database 4], cleanup). `RETURN VALUE $before.id`, not
    /// `RETURN NONE`, so the caller sees when a batch came back empty.
    pub const DELETE_EXPIRED: &str = "DELETE (SELECT VALUE id FROM idempotency_key WHERE expires_at < $now LIMIT 500) RETURN VALUE $before.id;";

    /// A user's day.
    pub const USAGE: &str = "SELECT * FROM ONLY $usage;";
    /// Chat tokens after a turn ([database 4], limits ✓: `+=` on a missing record starts from
    /// the default).
    pub const ADD_CHAT_USAGE: &str =
        "UPSERT ONLY $usage SET tokens_in += $tokens_in, tokens_out += $tokens_out RETURN AFTER;";
}

/// An `idempotency_key` row.
#[derive(Debug, Clone, SurrealValue)]
pub struct KeyRow {
    /// The request's fingerprint.
    pub fingerprint: String,
    /// `running` or `done`.
    pub state: KeyStateRow,
    /// The stored status.
    pub response_status: Option<i64>,
    /// The stored body.
    pub response_body: Option<serde_json::Value>,
    /// 24 hours after the first request.
    pub expires_at: DateTime<Utc>,
}

/// A `usage_daily` row.
#[derive(Debug, Clone, Copy, SurrealValue)]
pub struct UsageRow {
    /// Scans created.
    #[surreal(default)]
    pub scans: i64,
    /// Requests.
    #[surreal(default)]
    pub requests: i64,
    /// Chat tokens in.
    #[surreal(default)]
    pub tokens_in: i64,
    /// Chat tokens out.
    #[surreal(default)]
    pub tokens_out: i64,
}

impl TryFrom<UsageRow> for UsageTotals {
    type Error = RowError;

    fn try_from(row: UsageRow) -> Result<Self, RowError> {
        let u64_of = |field, n: i64| {
            u64::try_from(n).map_err(|_| RowError::BadField {
                field,
                problem: "negative".to_owned(),
            })
        };
        Ok(Self {
            scans: u32_of("scans", row.scans)?,
            requests: u32_of("requests", row.requests)?,
            tokens_in: u64_of("tokens_in", row.tokens_in)?,
            tokens_out: u64_of("tokens_out", row.tokens_out)?,
        })
    }
}

impl SurrealStore {
    #[expect(clippy::result_large_err, reason = "mapped at once by `begin`")]
    async fn create_key(
        &self,
        key: &RecordId,
        fingerprint: &str,
        expires_at: Timestamp,
    ) -> Result<(), DbFailure> {
        run_tx(&self.db, sql::BEGIN_KEY, |q| {
            q.bind(("key", key.clone()))
                .bind(("fp", fingerprint))
                .bind(("expires_at", dt(expires_at)))
        })
        .await
        .map(drop)
    }

    async fn read_key(&self, key: &RecordId) -> Result<Option<KeyRow>, StoreError> {
        const WHAT: &str = "read idempotency key";
        let mut results = run_tx(&self.db, sql::READ_KEY, |q| q.bind(("key", key.clone())))
            .await
            .map_err(|f| f.into_store(WHAT))?;
        results.take(0).map_err(|e| take_failed(WHAT, e))
    }
}

/// What a stored key means for a repeat with `fingerprint` ([contracts 1], the rules table).
pub fn decide(row: KeyRow, fingerprint: &str) -> Result<IdempotencyBegin, RowError> {
    if row.fingerprint != fingerprint {
        return Ok(IdempotencyBegin::Conflict);
    }
    match row.state {
        KeyStateRow::Running => Ok(IdempotencyBegin::InProgress),
        KeyStateRow::Done => {
            let status = row.response_status.ok_or(RowError::BadField {
                field: "response_status",
                problem: "missing on a done key".to_owned(),
            })?;
            Ok(IdempotencyBegin::Replay(StoredResponse {
                status: u16::try_from(status).map_err(|_| RowError::BadField {
                    field: "response_status",
                    problem: format!("{status} is not an HTTP status"),
                })?,
                body: row.response_body.unwrap_or(serde_json::Value::Null),
            }))
        }
    }
}

#[async_trait::async_trait]
impl IdempotencyRepo for SurrealStore {
    async fn begin(
        &self,
        scope: &IdempotencyScope,
        fingerprint: &str,
        expires_at: Timestamp,
    ) -> Result<IdempotencyBegin, StoreError> {
        const WHAT: &str = "begin idempotent request";
        let key = idempotency_rid(scope.user, &scope.route, scope.key);
        // The port passes `expires_at`; the request's own time is 24 hours before it.
        let now = expires_at.minus(IDEMPOTENCY_TTL);
        for _ in 0..2 {
            match self.create_key(&key, fingerprint, expires_at).await {
                Ok(()) => return Ok(IdempotencyBegin::Started),
                Err(DbFailure::AlreadyExists(_)) => {}
                Err(failure) => return Err(failure.into_store(WHAT)),
            }
            match self.read_key(&key).await? {
                Some(row) if row.expires_at > dt(now) => {
                    return decide(row, fingerprint).map_err(|e| e.into_store(WHAT));
                }
                // Expired but not yet cleaned up (reads hide expired keys, database 4).
                Some(_) => {
                    run_tx(&self.db, sql::DELETE_EXPIRED_KEY, |q| {
                        q.bind(("key", key.clone())).bind(("now", dt(now)))
                    })
                    .await
                    .map_err(|f| f.into_store(WHAT))?;
                }
                // The cleanup deleted it in between: create again.
                None => {}
            }
        }
        Err(StoreError::Conflict(domain::ports::Conflict::Retries))
    }

    async fn finish(
        &self,
        scope: &IdempotencyScope,
        response: StoredResponse,
    ) -> Result<(), StoreError> {
        const WHAT: &str = "finish idempotent request";
        let body = match response.body {
            serde_json::Value::Null => None,
            body @ serde_json::Value::Object(_) => Some(body),
            other => Some(serde_json::json!({ "value": other })),
        };
        let key = idempotency_rid(scope.user, &scope.route, scope.key);
        run_tx(&self.db, sql::FINISH_KEY, |q| {
            q.bind(("key", key.clone()))
                .bind(("status", i64::from(response.status)))
                .bind(("body", body.clone()))
        })
        .await
        .map(drop)
        .map_err(|f| f.into_store(WHAT))
    }

    async fn release(&self, scope: &IdempotencyScope) -> Result<(), StoreError> {
        const WHAT: &str = "release idempotency key";
        let key = idempotency_rid(scope.user, &scope.route, scope.key);
        run_tx(&self.db, sql::RELEASE_KEY, |q| q.bind(("key", key.clone())))
            .await
            .map(drop)
            .map_err(|f| f.into_store(WHAT))
    }

    async fn delete_expired(&self, now: Timestamp) -> Result<u32, StoreError> {
        const WHAT: &str = "delete expired idempotency keys";
        let mut results = run_tx(&self.db, sql::DELETE_EXPIRED, |q| q.bind(("now", dt(now))))
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let ids: Vec<RecordId> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        Ok(u32::try_from(ids.len()).unwrap_or(u32::MAX))
    }
}

#[async_trait::async_trait]
impl UsageRepo for SurrealStore {
    async fn usage(&self, user: UserId, day: NaiveDate) -> Result<UsageTotals, StoreError> {
        const WHAT: &str = "read usage";
        let mut results = run_tx(&self.db, sql::USAGE, |q| {
            q.bind(("usage", usage_rid(user, day)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let row: Option<UsageRow> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        row.map_or(Ok(UsageTotals::default()), |r| {
            UsageTotals::try_from(r).map_err(|e| e.into_store(WHAT))
        })
    }

    async fn add_chat_usage(
        &self,
        user: UserId,
        day: NaiveDate,
        usage: Usage,
    ) -> Result<UsageTotals, StoreError> {
        const WHAT: &str = "add chat usage";
        let mut results = run_tx(&self.db, sql::ADD_CHAT_USAGE, |q| {
            q.bind(("usage", usage_rid(user, day)))
                .bind(("tokens_in", i64::from(usage.tokens_in)))
                .bind(("tokens_out", i64::from(usage.tokens_out)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let row: Option<UsageRow> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        UsageTotals::try_from(row.ok_or(StoreError::NotFound)?).map_err(|e| e.into_store(WHAT))
    }
}
