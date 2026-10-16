//! The shared error enums of the ports. One enum per kind of boundary rather than an
//! associated `Error` type per trait: `Arc<dyn Trait>` would otherwise have to name the error
//! (`dyn Trait<Error = …>`), and every caller of a port of the same kind handles the same cases.
//! Each implementation maps its library's errors into these, keeping the library error as the
//! `source` for the log.

use crate::{
    error::DomainError,
    id::{JobId, PlanId},
};

/// A boxed error from a library, kept as the source of the log chain.
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

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

/// The object store's error ([storage 4]).
#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    /// No object under that key.
    #[error("object not found")]
    NotFound,
    /// `If-None-Match: *` refused: the key exists (`412`). Keys are never overwritten.
    #[error("object already exists")]
    AlreadyExists,
    /// `RustFS` can't be reached or timed out.
    #[error("storage unavailable")]
    Unavailable(#[source] BoxError),
    /// `RustFS` refused the call (a policy, a bad key).
    #[error("storage refused {operation}")]
    Rejected {
        /// The S3 operation.
        operation: &'static str,
        /// The SDK error.
        #[source]
        source: BoxError,
    },
}

/// An external HTTP provider's error: weather, soil, geocoding, push, the auth provider's
/// Backend API.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// The provider knows nothing for this input (an unknown postal code).
    #[error("{provider}: not found")]
    NotFound {
        /// The provider, such as `"geocoder"`.
        provider: &'static str,
    },
    /// Rate limited; retry after the given seconds.
    #[error("{provider}: rate limited")]
    RateLimited {
        /// The provider.
        provider: &'static str,
        /// When to retry.
        retry_after_s: Option<u32>,
    },
    /// Down, timed out or answering 5xx: `503 upstream_unavailable` with `upstream`.
    #[error("{provider}: unavailable")]
    Unavailable {
        /// The provider.
        provider: &'static str,
        /// The client error.
        #[source]
        source: BoxError,
    },
    /// The response didn't parse.
    #[error("{provider}: bad response")]
    BadResponse {
        /// The provider.
        provider: &'static str,
        /// The parse error.
        #[source]
        source: BoxError,
    },
}

impl ProviderError {
    /// The `upstream` of `503 upstream_unavailable`.
    pub const fn provider(&self) -> &'static str {
        match self {
            Self::NotFound { provider }
            | Self::RateLimited { provider, .. }
            | Self::Unavailable { provider, .. }
            | Self::BadResponse { provider, .. } => provider,
        }
    }
}

/// A model's error: the LLM API, the VLM sidecar, `ort` sessions, the text embedder.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    /// The adapter doesn't do this (a perception adapter asked for a `vlm` job).
    #[error("{model} does not support {what}")]
    Unsupported {
        /// The model.
        model: &'static str,
        /// The operation.
        what: &'static str,
    },
    /// The model's output failed validation after its one retry ([contracts 6], after
    /// decoding): the job fails with `retryable: true`.
    #[error("{model} returned invalid output")]
    InvalidOutput {
        /// The model.
        model: &'static str,
        /// Why.
        #[source]
        source: BoxError,
    },
    /// Out of memory, a crashed session, a 5xx from the sidecar: worth another attempt.
    #[error("{model} failed")]
    Failed {
        /// The model.
        model: &'static str,
        /// The cause.
        #[source]
        source: BoxError,
    },
    /// The hosted API is down or rate limited: `503 upstream_unavailable` for chat.
    #[error("{model} unavailable")]
    Unavailable {
        /// The model.
        model: &'static str,
        /// The cause.
        #[source]
        source: BoxError,
    },
}

impl ModelError {
    /// Whether a job that failed with this error may be retried ([contracts 5], `retryable`).
    pub const fn retryable(&self) -> bool {
        !matches!(self, Self::Unsupported { .. })
    }
}

/// The auth provider's error.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// Missing, malformed, expired or wrongly signed token: `401 unauthenticated`.
    #[error("invalid token")]
    InvalidToken(#[source] Option<BoxError>),
    /// A webhook whose Svix signature or timestamp failed: `401`.
    #[error("invalid webhook signature")]
    InvalidWebhook,
    /// The JWKS couldn't be fetched: `503 upstream_unavailable`.
    #[error("auth provider unavailable")]
    Unavailable(#[source] BoxError),
    /// A valid token for an account that was deleted (its tombstone exists): `401
    /// unauthenticated`, never a new user.
    #[error("account deleted")]
    AccountDeleted,
}
