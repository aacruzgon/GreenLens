# Lower-layer errors and how they become `ApiError`, compiled

Part of [rust-errors](SKILL.md#the-mapping-table). The table in SKILL.md is the summary. The `ApiError` enum itself, with every code of [contracts 2](../../../docs/architecture/contracts.md#2-errors) and the test that each is produced with its status, `retryable`, `retry_after_s` and `details`, is the canonical block in [rust-axum-api, error responses](../rust-axum-api/error-responses.md#2-the-compiled-example); this file copies it. What this file owns:

- the **canonical `StoreError`** with its `Conflict` and the **canonical `BlobError`**: the port errors of `domain::ports`, copied from [backend domain 6, port errors](../../../docs/backend/domain.md#port-errors), and `BoxError`, the boxed source they carry;
- the **canonical `DomainError`**: the variants of `domain::error::DomainError` these examples need ([backend domain 6, `DomainError`](../../../docs/backend/domain.md#domainerror));
- the conversions from all three to `ApiError` ([backend api 8](../../../docs/backend/api.md#8-errors)), and a call-site mapping for the case whose meaning depends on the call.

Design points:

- **The port's error lives in `domain`, not in `store`.** Every implementation of a port and every caller share one enum per kind of boundary, so `Arc<dyn Store>` needs no associated error type ([backend domain 6, port errors](../../../docs/backend/domain.md#port-errors); [backend store 1](../../../docs/backend/store.md#1-purpose-and-boundaries)). The `store` crate classifies each SDK error once, into its own crate-private `DbFailure`, and maps that into `StoreError`, keeping the SDK error as the `source` ([backend store 5, classifying failures](../../../docs/backend/store.md#classifying-failures); [rust-surrealdb, errors and retries](../rust-surrealdb/errors-and-retries.md)). So no `surrealdb` or `aws-sdk-s3` type appears in a port error, and the sources are `BoxError`.
- **`StoreError::Conflict(Conflict)` says which check lost**: `StalePlan { current }`, `ClaimLost`, `QuestionStepLost`, `LeaseLost { job_id }`, `StaleMap { current_version }`, `AlreadyExists`, or `Retries` (transaction conflicts that outlived the store's 5 attempts). Because each carries what the response needs, most map without context; `AlreadyExists` and an automatic re-plan's `StalePlan` are handled by the caller first ([backend domain 6](../../../docs/backend/domain.md#port-errors), the "Becomes" table).
- **`Rejected(DomainError)`** is a rule checked inside the store's transaction (the last owner, the daily scan limit, unknown map plants), mapped by the rule's own code. `From<DomainError>` sends `NotFound` to `StoreError::NotFound` and everything else to `Rejected`.
- **`Unavailable`** is a lost connection; with `Conflict(Retries)` it becomes `503 upstream_unavailable` with `upstream: "database"` and `retry_after_s: 5` ([contracts 2](../../../docs/architecture/contracts.md#2-errors): 5 for `upstream_unavailable` unless the provider named a longer wait).
- **`Query { what, source }`** is a bug or a schema drift, `500 internal`; `what` names the operation for the log (`"append event"`), passed by the store at the call ([backend store 5](../../../docs/backend/store.md#classifying-failures), `into_store`).
- **Messages** are lowercase, without trailing punctuation or the source's text ([SKILL.md, display and source](SKILL.md#display-and-source)). `Conflict`'s message prints the variant with `{0:?}`, as the design writes it: a `Conflict` holds no source and no secret.

The block defines `JobId` and `PlanId` as `String` stand-ins for `domain`'s `Id<Job>` and `Id<Plan>` ([backend domain 4](../../../docs/backend/domain.md#4-ids-and-primitives)), because `ApiError` here carries ids as strings.

```rust
use std::error::Error as StdError;

use serde_json::json;

/// Stand-ins for `domain::id::{JobId, PlanId}` (backend domain 4).
pub type JobId = String;
pub type PlanId = String;

// canonical: rust-errors/mapping.md BoxError
/// A boxed error from a library, kept as the source of the log chain (`domain::ports`).
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;
// end: BoxError

// canonical: rust-errors/mapping.md DomainError
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

// canonical: rust-errors/mapping.md StoreError
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

// canonical: rust-errors/mapping.md BlobError
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
// end: BlobError

/// A rule's refusal, by its code (backend domain 6, `DomainError::kind`).
impl From<DomainError> for ApiError {
    fn from(error: DomainError) -> Self {
        match error {
            DomainError::NotFound => Self::NotFound,
            DomainError::InvalidRequest { field, problem } => {
                Self::InvalidRequest { field, problem }
            }
            DomainError::IdempotencyConflict => Self::IdempotencyConflict,
        }
    }
}

/// The context-free mappings of backend api 8. Only `AlreadyExists` and an automatic
/// re-plan's `StalePlan` need the caller first; if one reaches `?`, it is `invalid_state`.
impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::NotFound => Self::NotFound,
            StoreError::Rejected(rule) => Self::from(rule),
            StoreError::Conflict(Conflict::LeaseLost { job_id }) => Self::LeaseLost { job_id },
            StoreError::Conflict(Conflict::StalePlan { current }) => {
                Self::InvalidState(InvalidStateDetails {
                    current_plan_id: current,
                    ..InvalidStateDetails::default()
                })
            }
            StoreError::Conflict(Conflict::StaleMap { current_version }) => {
                Self::InvalidState(InvalidStateDetails {
                    current_map_version: Some(u64::from(current_version)),
                    ..InvalidStateDetails::default()
                })
            }
            // The pipeline's own conflicts reach HTTP only through a bug (backend api 8).
            StoreError::Conflict(
                Conflict::ClaimLost | Conflict::QuestionStepLost | Conflict::AlreadyExists,
            ) => Self::InvalidState(InvalidStateDetails::default()),
            StoreError::Conflict(Conflict::Retries) | StoreError::Unavailable(_) => {
                Self::UpstreamUnavailable {
                    upstream: "database",
                    retry_after_s: UNAVAILABLE_RETRY_AFTER_S,
                    source: Box::new(error),
                }
            }
            StoreError::Query { .. } => Self::Internal(Box::new(error)),
        }
    }
}

impl From<BlobError> for ApiError {
    fn from(error: BlobError) -> Self {
        match error {
            BlobError::NotFound => Self::NotFound,
            BlobError::Unavailable(_) => Self::UpstreamUnavailable {
                upstream: "storage",
                retry_after_s: UNAVAILABLE_RETRY_AFTER_S,
                source: Box::new(error),
            },
            // Keys are never overwritten, so an existing key is a bug.
            BlobError::AlreadyExists | BlobError::Rejected { .. } => {
                Self::Internal(Box::new(error))
            }
        }
    }
}

/// Stand-in for the store's alert insert: its unique index refuses a second task for the
/// same alert key (database 4, alert dedupe).
fn insert_alert_task(key_taken: bool) -> Result<(), StoreError> {
    if key_taken {
        Err(StoreError::Conflict(Conflict::AlreadyExists))
    } else {
        Ok(())
    }
}

/// The caller gives `AlreadyExists` its meaning before `?`: the alert already has its task,
/// which is success. The rest goes through `From`.
pub fn create_alert_task(key_taken: bool) -> Result<bool, ApiError> {
    match insert_alert_task(key_taken) {
        Ok(()) => Ok(true),
        Err(StoreError::Conflict(Conflict::AlreadyExists)) => Ok(false),
        Err(other) => Err(other.into()),
    }
}

// copy of: rust-axum-api/error-responses.md ErrorCode
/// contracts 2 `ErrorCode`. In the domain crate; `ToSchema` adds it to the generated spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    Unauthenticated,
    Forbidden,
    InvalidSignature,
    NotFound,
    InvalidState,
    RequestInProgress,
    LeaseLost,
    UploadTooLarge,
    UnsupportedMediaType,
    ImageQuality,
    MissingRequiredShot,
    TooManyImages,
    IdempotencyConflict,
    RateLimited,
    BudgetExceeded,
    Internal,
    ProcessingUnavailable,
    UpstreamUnavailable,
}
// end: ErrorCode

// copy of: rust-axum-api/error-responses.md ApiError
/// contracts 1, roles: each role has every right of the one before.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Viewer,
    Reviewer,
    Editor,
    Owner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityProblem {
    TooBlurry,
    TooDark,
    TooBright,
}

impl QualityProblem {
    /// contracts 2 `QualityHint`: one hint per problem.
    pub const fn hint(self) -> &'static str {
        match self {
            Self::TooBlurry => "tap_to_focus",
            Self::TooDark => "more_light",
            Self::TooBright => "avoid_glare",
        }
    }

    const fn message(self) -> &'static str {
        match self {
            Self::TooBlurry => {
                "This photo is too blurry. Hold the phone still and tap the leaf to focus."
            }
            Self::TooDark => "This photo is too dark. Move into better light.",
            Self::TooBright => {
                "This photo is too bright. Shade the plant or step out of direct sun."
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StateHint {
    NewScan,
    NeighbourRemoved,
    PlantRemoved,
}

/// contracts 2 `invalid_state` details; every key optional.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct InvalidStateDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_plan_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_map_version: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<StateHint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Budget {
    ChatTokens,
    Scans,
}

/// contracts 2: the four upload formats.
pub const ACCEPTED_MEDIA_TYPES: [&str; 4] = ["image/jpeg", "image/png", "image/webp", "image/heic"];
/// contracts 2: `max_bytes` of `upload_too_large`, an image over 12 MB (read as MiB).
pub const IMAGE_MAX_BYTES: usize = 12 * 1024 * 1024;
/// contracts 2: `retry_after_s` of `upstream_unavailable` (also a request past the API's
/// timeout) unless the provider named a longer wait.
pub const UNAVAILABLE_RETRY_AFTER_S: u64 = 5;
/// contracts 2: `retry_after_s` of `request_in_progress` and `processing_unavailable`.
pub const SHORT_RETRY_AFTER_S: u64 = 2;

/// One variant per contracts 2 code. `Display` is the `message`: a sentence safe to show.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{problem}")]
    InvalidRequest { field: String, problem: String },
    #[error("Please sign in again.")]
    Unauthenticated,
    #[error("Ask an owner to change your role.")]
    Forbidden { required_role: Role },
    #[error("This link has expired.")]
    InvalidSignature,
    #[error("We couldn't find that.")]
    NotFound,
    #[error("This can't be done right now.")]
    InvalidState(InvalidStateDetails),
    #[error("This request is still being processed.")]
    RequestInProgress { retry_after_s: u64 },
    #[error("This job's lease is no longer valid.")]
    LeaseLost { job_id: String },
    #[error("This photo is too large to upload.")]
    UploadTooLarge { max_bytes: usize },
    #[error("This file type isn't supported. Use JPEG, PNG, WebP or HEIC.")]
    UnsupportedMediaType,
    #[error("{}", .problem.message())]
    ImageQuality {
        problem: QualityProblem,
        blur_score: f64,
        exposure: f64,
    },
    #[error("A photo of the whole plant is needed.")]
    MissingRequiredShot { missing: Vec<String> },
    #[error("There are already enough photos of this kind.")]
    TooManyImages { kind: String, max: u32 },
    #[error("This request doesn't match the earlier one with the same key.")]
    IdempotencyConflict,
    #[error("Too many requests. Try again in {retry_after_s} seconds.")]
    RateLimited { retry_after_s: u64 },
    #[error("You've reached today's limit.")]
    BudgetExceeded {
        budget: Budget,
        resets_at: Timestamp,
    },
    #[error("Something went wrong on our side.")]
    Internal(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("We couldn't process this photo right now. It wasn't saved.")]
    ProcessingUnavailable {
        retry_after_s: u64,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("A service we depend on is unavailable. Try again shortly.")]
    UpstreamUnavailable {
        upstream: &'static str,
        retry_after_s: u64,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl ApiError {
    pub fn invalid_request(field: impl Into<String>, problem: impl Into<String>) -> Self {
        Self::InvalidRequest {
            field: field.into(),
            problem: problem.into(),
        }
    }

    pub fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::InvalidRequest { .. } => StatusCode::BAD_REQUEST,
            Self::Unauthenticated => StatusCode::UNAUTHORIZED,
            Self::Forbidden { .. } | Self::InvalidSignature => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::InvalidState(_) | Self::RequestInProgress { .. } | Self::LeaseLost { .. } => {
                StatusCode::CONFLICT
            }
            Self::UploadTooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            Self::UnsupportedMediaType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::ImageQuality { .. }
            | Self::MissingRequiredShot { .. }
            | Self::TooManyImages { .. }
            | Self::IdempotencyConflict => StatusCode::UNPROCESSABLE_ENTITY,
            Self::RateLimited { .. } | Self::BudgetExceeded { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ProcessingUnavailable { .. } | Self::UpstreamUnavailable { .. } => {
                StatusCode::SERVICE_UNAVAILABLE
            }
        }
    }

    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::InvalidRequest { .. } => ErrorCode::InvalidRequest,
            Self::Unauthenticated => ErrorCode::Unauthenticated,
            Self::Forbidden { .. } => ErrorCode::Forbidden,
            Self::InvalidSignature => ErrorCode::InvalidSignature,
            Self::NotFound => ErrorCode::NotFound,
            Self::InvalidState(_) => ErrorCode::InvalidState,
            Self::RequestInProgress { .. } => ErrorCode::RequestInProgress,
            Self::LeaseLost { .. } => ErrorCode::LeaseLost,
            Self::UploadTooLarge { .. } => ErrorCode::UploadTooLarge,
            Self::UnsupportedMediaType => ErrorCode::UnsupportedMediaType,
            Self::ImageQuality { .. } => ErrorCode::ImageQuality,
            Self::MissingRequiredShot { .. } => ErrorCode::MissingRequiredShot,
            Self::TooManyImages { .. } => ErrorCode::TooManyImages,
            Self::IdempotencyConflict => ErrorCode::IdempotencyConflict,
            Self::RateLimited { .. } => ErrorCode::RateLimited,
            Self::BudgetExceeded { .. } => ErrorCode::BudgetExceeded,
            Self::Internal(_) => ErrorCode::Internal,
            Self::ProcessingUnavailable { .. } => ErrorCode::ProcessingUnavailable,
            Self::UpstreamUnavailable { .. } => ErrorCode::UpstreamUnavailable,
        }
    }

    /// contracts 2: `true` for the codes it marks retryable, and for `image_quality`,
    /// whose example sends `true` (a retake can pass).
    pub const fn retryable(&self) -> bool {
        matches!(
            self,
            Self::RequestInProgress { .. }
                | Self::ImageQuality { .. }
                | Self::RateLimited { .. }
                | Self::ProcessingUnavailable { .. }
                | Self::UpstreamUnavailable { .. }
        )
    }

    /// contracts 2: sent with `request_in_progress`, `rate_limited` and both `503` codes.
    pub const fn retry_after_s(&self) -> Option<u64> {
        match self {
            Self::RequestInProgress { retry_after_s }
            | Self::RateLimited { retry_after_s }
            | Self::ProcessingUnavailable { retry_after_s, .. }
            | Self::UpstreamUnavailable { retry_after_s, .. } => Some(*retry_after_s),
            _ => None,
        }
    }

    pub fn details(&self) -> Option<serde_json::Value> {
        use serde_json::json;
        let details = match self {
            Self::InvalidRequest { field, problem } => {
                json!({ "field": field, "problem": problem })
            }
            Self::Forbidden { required_role } => json!({ "required_role": required_role }),
            Self::InvalidState(details) => json!(details),
            Self::LeaseLost { job_id } => json!({ "job_id": job_id }),
            Self::UploadTooLarge { max_bytes } => json!({ "max_bytes": max_bytes }),
            Self::UnsupportedMediaType => json!({ "accepted": ACCEPTED_MEDIA_TYPES }),
            Self::ImageQuality {
                problem,
                blur_score,
                exposure,
            } => json!({
                "problem": problem, "blur_score": blur_score,
                "exposure": exposure, "hint": problem.hint(),
            }),
            Self::MissingRequiredShot { missing } => json!({ "missing": missing }),
            Self::TooManyImages { kind, max } => json!({ "kind": kind, "max": max }),
            Self::BudgetExceeded { budget, resets_at } => {
                json!({ "budget": budget, "resets_at": resets_at })
            }
            Self::UpstreamUnavailable { upstream, .. } => json!({ "upstream": upstream }),
            Self::Unauthenticated
            | Self::InvalidSignature
            | Self::NotFound
            | Self::RequestInProgress { .. }
            | Self::IdempotencyConflict
            | Self::RateLimited { .. }
            | Self::Internal(_)
            | Self::ProcessingUnavailable { .. } => return None,
        };
        Some(details)
    }

    /// The source chain of a `5xx`, for the log line; never part of the body.
    fn cause(&self) -> Option<String> {
        match self {
            Self::Internal(source)
            | Self::ProcessingUnavailable { source, .. }
            | Self::UpstreamUnavailable { source, .. } => Some(Chain(&**source).to_string()),
            _ => None,
        }
    }

    pub fn parts(&self) -> ErrorParts {
        ErrorParts {
            code: self.code(),
            message: self.to_string(),
            retryable: self.retryable(),
            retry_after_s: self.retry_after_s(),
            details: self.details(),
            cause: self.cause(),
        }
    }
}

/// Everything of the body except `request_id`, left on the response for `render_errors`.
#[derive(Debug, Clone)]
pub struct ErrorParts {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    pub retry_after_s: Option<u64>,
    pub details: Option<serde_json::Value>,
    /// The source chain, logged by `render_errors`; set for `5xx` only.
    pub cause: Option<String>,
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let mut response = axum::response::IntoResponse::into_response(self.status());
        response.extensions_mut().insert(self.parts());
        response
    }
}
// end: ApiError

// copy of: rust-contract-types/primitives.md Timestamp
/// RFC 3339 in UTC, whole seconds, `Z` suffix: `"2026-10-18T14:03:22Z"` (contracts 1,
/// wire format; contracts 12, question 12). Keeps full precision in memory and in the
/// database; only the wire form is truncated. No `now()`: every reading comes from the
/// `Clock` port, so tests control time (backend domain 4).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, utoipa::ToSchema, schemars::JsonSchema,
)]
#[schema(value_type = String, format = DateTime, example = "2026-10-18T14:03:22Z")]
#[schemars(transparent)]
pub struct Timestamp(chrono::DateTime<chrono::Utc>);

impl Timestamp {
    /// For the store, which binds `chrono::DateTime<Utc>` (database 2).
    pub const fn into_inner(self) -> chrono::DateTime<chrono::Utc> {
        self.0
    }
}

impl From<chrono::DateTime<chrono::Utc>> for Timestamp {
    fn from(t: chrono::DateTime<chrono::Utc>) -> Self {
        Self(t)
    }
}

impl serde::Serialize for Timestamp {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let text = self.0.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        serializer.serialize_str(&text)
    }
}

impl<'de> serde::Deserialize<'de> for Timestamp {
    /// Accepts what chrono's RFC 3339 parser accepts and converts it to UTC.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <chrono::DateTime<chrono::Utc> as serde::Deserialize>::deserialize(deserializer).map(Self)
    }
}
// end: Timestamp

// copy of: rust-errors/SKILL.md Chain
/// The log form of an error: its message, then each source's, joined by ": ".
/// Log it with `error = %Chain(&error)`; store it with `Chain(&error).to_string()`.
pub struct Chain<'a>(pub &'a (dyn std::error::Error + 'static));

impl std::fmt::Display for Chain<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)?;
        let mut source = self.0.source();
        while let Some(error) = source {
            write!(f, ": {error}")?;
            source = error.source();
        }
        Ok(())
    }
}
// end: Chain

#[cfg(test)]
mod tests {
    use super::*;

    fn sdk_error(message: &str) -> BoxError {
        message.into()
    }

    #[test]
    fn store_errors_map_without_context() {
        let down = StoreError::Unavailable(sdk_error("WebSocket error"));
        let api = ApiError::from(down);
        assert_eq!(api.code(), ErrorCode::UpstreamUnavailable);
        assert_eq!(api.details(), Some(json!({ "upstream": "database" })));
        assert_eq!(api.retry_after_s(), Some(UNAVAILABLE_RETRY_AFTER_S));
        let retries = ApiError::from(StoreError::Conflict(Conflict::Retries));
        assert_eq!(retries.code(), ErrorCode::UpstreamUnavailable);
        assert_eq!(
            ApiError::from(StoreError::NotFound).code(),
            ErrorCode::NotFound
        );
        let query = StoreError::Query {
            what: "append event",
            source: sdk_error("Couldn't coerce value for field `seq`"),
        };
        assert_eq!(ApiError::from(query).code(), ErrorCode::Internal);
    }

    #[test]
    fn each_conflict_carries_what_its_response_needs() {
        let lost = ApiError::from(StoreError::Conflict(Conflict::LeaseLost {
            job_id: "01JAX".to_owned(),
        }));
        assert_eq!(lost.code(), ErrorCode::LeaseLost);
        assert_eq!(lost.details(), Some(json!({ "job_id": "01JAX" })));
        let stale = ApiError::from(StoreError::Conflict(Conflict::StalePlan {
            current: Some("01JPLAN".to_owned()),
        }));
        assert_eq!(stale.code(), ErrorCode::InvalidState);
        assert_eq!(
            stale.details(),
            Some(json!({ "current_plan_id": "01JPLAN" }))
        );
        let map = ApiError::from(StoreError::Conflict(Conflict::StaleMap {
            current_version: 4,
        }));
        assert_eq!(map.details(), Some(json!({ "current_map_version": 4 })));
        let bug = ApiError::from(StoreError::Conflict(Conflict::ClaimLost));
        assert_eq!(bug.code(), ErrorCode::InvalidState);
    }

    #[test]
    fn a_rule_inside_the_transaction_keeps_its_code() {
        let refused = StoreError::from(DomainError::InvalidRequest {
            field: "pins".to_owned(),
            problem: "unknown_plant".to_owned(),
        });
        assert!(matches!(refused, StoreError::Rejected(_)));
        assert_eq!(ApiError::from(refused).code(), ErrorCode::InvalidRequest);
        assert!(matches!(
            StoreError::from(DomainError::NotFound),
            StoreError::NotFound
        ));
    }

    #[test]
    fn the_caller_handles_already_exists_first() {
        assert!(matches!(create_alert_task(false), Ok(true)));
        assert!(matches!(create_alert_task(true), Ok(false)));
    }

    #[test]
    fn blob_errors_map_without_context() {
        let down = ApiError::from(BlobError::Unavailable(sdk_error("dispatch failure")));
        assert_eq!(down.details(), Some(json!({ "upstream": "storage" })));
        assert_eq!(
            ApiError::from(BlobError::NotFound).code(),
            ErrorCode::NotFound
        );
        assert_eq!(
            ApiError::from(BlobError::AlreadyExists).code(),
            ErrorCode::Internal
        );
    }

    #[test]
    fn the_log_carries_the_store_chain_and_the_body_does_not() {
        let error: Box<dyn StdError + Send + Sync> = Box::new(StoreError::Query {
            what: "load scan",
            source: sdk_error("Couldn't coerce value for field `name`"),
        });
        let api = ApiError::Internal(error);
        assert_eq!(api.to_string(), "Something went wrong on our side.");
        assert_eq!(
            api.parts().cause.as_deref(),
            Some("query failed: load scan: Couldn't coerce value for field `name`")
        );
    }
}
```
