//! Idempotency keys and usage counters ([contracts 1], idempotency; [database 4],
//! idempotency keys and limits).

use crate::{
    chat::Usage,
    id::{IdempotencyKey, UserId},
    ports::StoreError,
    wire::Timestamp,
};

/// A key's scope: unique per (user, method, path), kept 24 hours.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IdempotencyScope {
    /// The caller.
    pub user: UserId,
    /// `"POST /scans/01JAX…/images"`.
    pub route: String,
    /// The header's UUID.
    pub key: IdempotencyKey,
}

/// A stored response: `2xx` and the deterministic `4xx` only.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredResponse {
    /// The HTTP status.
    pub status: u16,
    /// The JSON body.
    pub body: serde_json::Value,
}

/// What `begin` found ([contracts 1], the rules table).
#[derive(Debug, Clone, PartialEq)]
pub enum IdempotencyBegin {
    /// First use: the caller runs the request, then `finish` or `release`.
    Started,
    /// Same fingerprint, finished: replay with the original status.
    Replay(StoredResponse),
    /// Same fingerprint, still running: `409 request_in_progress`.
    InProgress,
    /// Different fingerprint: `422 idempotency_conflict`.
    Conflict,
}

/// The `idempotency_key` table.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait IdempotencyRepo: Send + Sync {
    /// `CREATE idempotency_key:[user, route, key]`; on "already exists", reads the record and
    /// decides.
    async fn begin(
        &self,
        scope: &IdempotencyScope,
        fingerprint: &str,
        expires_at: Timestamp,
    ) -> Result<IdempotencyBegin, StoreError>;

    /// Stores the response and sets `state = 'done'`.
    async fn finish(
        &self,
        scope: &IdempotencyScope,
        response: StoredResponse,
    ) -> Result<(), StoreError>;

    /// Deletes the key: a `409`, `429` or `5xx`, or an interrupted chat message, so a retry runs
    /// again.
    async fn release(&self, scope: &IdempotencyScope) -> Result<(), StoreError>;

    /// Cleanup: expired keys, in batches of 500.
    async fn delete_expired(&self, now: Timestamp) -> Result<u32, StoreError>;
}

/// A user's usage on one UTC day (`usage_daily:[user, day]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UsageTotals {
    /// Scans created.
    pub scans: u32,
    /// Requests.
    pub requests: u32,
    /// Chat tokens in.
    pub tokens_in: u64,
    /// Chat tokens out.
    pub tokens_out: u64,
}

/// The `usage_daily` table. The scan count is incremented inside `ScanRepo::create_scan`'s
/// transaction; chat tokens here.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait UsageRepo: Send + Sync {
    /// The day's totals, for the budget check before a chat turn.
    async fn usage(&self, user: UserId, day: chrono::NaiveDate) -> Result<UsageTotals, StoreError>;

    /// `UPSERT … SET tokens_in += …, tokens_out += …` after a chat turn.
    async fn add_chat_usage(
        &self,
        user: UserId,
        day: chrono::NaiveDate,
        usage: Usage,
    ) -> Result<UsageTotals, StoreError>;
}
