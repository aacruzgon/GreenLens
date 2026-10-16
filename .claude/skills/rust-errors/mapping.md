# Lower-layer errors and how they become `ApiError`, compiled

Part of [rust-errors](SKILL.md#the-mapping-table). The table in SKILL.md is the summary. The `ApiError` enum itself, with every code of [contracts 2](../../../docs/architecture/contracts.md#2-errors) and the test that each is produced with its status, `retryable`, `retry_after_s` and `details`, is the canonical block in [rust-axum-api, error responses](../rust-axum-api/error-responses.md#2-the-compiled-example); this file copies it. What this file owns:

- the **canonical `StoreError`**, the `store` crate's error, with the `From<surrealdb::Error>` that classifies the SDK's errors ([rust-surrealdb, how failures arrive](../rust-surrealdb/SKILL.md#how-failures-arrive));
- the **canonical `BlobError`**, the `blob` crate's error;
- the conversions from both to `ApiError`, and a call-site mapping for the errors whose meaning depends on the call.

Design points:

- **`StoreError::Conflict` is a lost compare-and-set**: a `THROW 'stale_plan'` or `THROW 'claim_lost'` from the database 4 patterns, or a lease check that matched no row. Its meaning depends on the call (`lease_lost`, `invalid_state` with `current_plan_id` or `current_map_version`, `request_in_progress`), so the handler maps it before `?`.
- **`StoreError::TxConflict` is SurrealDB's `TransactionConflict`**, which `retry_conflicts` retries ([rust-surrealdb, retrying conflicts](../rust-surrealdb/SKILL.md#retrying-conflicts)). One that survives five attempts is `internal`.
- **`Unavailable`** is a connection error (`Error::is_connection`; the SDK builds its WebSocket errors with `Error::connection`, `src/engine/remote/ws/native.rs`). It becomes `503 upstream_unavailable` with `upstream: "database"`.
- **`Query` carries no `what`**: `From<surrealdb::Error>` is the one place errors are classified, and it can't know the query. Every store function has an `#[instrument]` span that names it ([rust-tracing](../rust-tracing/SKILL.md)), so the log line still says which query failed.
- **Messages** are lowercase, without trailing punctuation or the source's text ([SKILL.md, display and source](SKILL.md#display-and-source)).

```rust
use std::error::Error as StdError;

use serde_json::json;

// canonical: rust-errors/mapping.md StoreError
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

// canonical: rust-errors/mapping.md BlobError
/// The `blob` crate's error.
#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    #[error("object store unreachable")]
    Unavailable(#[source] aws_sdk_s3::Error),
    #[error("object store rejected {operation}")]
    Rejected {
        operation: &'static str,
        #[source]
        source: aws_sdk_s3::Error,
    },
}
// end: BlobError

/// Context-free mappings only. A `Conflict` that reaches `?` was not handled by the
/// handler, which is a bug, so it is `internal` and logged.
impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::NotFound => Self::NotFound,
            StoreError::Unavailable(_) => Self::UpstreamUnavailable {
                upstream: "database",
                retry_after_s: UNAVAILABLE_RETRY_AFTER_S,
                source: Box::new(error),
            },
            StoreError::Conflict { .. }
            | StoreError::TxConflict(_)
            | StoreError::AlreadyExists(_)
            | StoreError::Thrown(_)
            | StoreError::Query(_) => Self::Internal(Box::new(error)),
        }
    }
}

impl From<BlobError> for ApiError {
    fn from(error: BlobError) -> Self {
        match error {
            BlobError::Unavailable(_) => Self::UpstreamUnavailable {
                upstream: "storage",
                retry_after_s: UNAVAILABLE_RETRY_AFTER_S,
                source: Box::new(error),
            },
            BlobError::Rejected { .. } => Self::Internal(Box::new(error)),
        }
    }
}

/// Stand-in for the store's lease-checked write: `Conflict` when the lease is gone.
fn complete_job(lease_ok: bool) -> Result<(), StoreError> {
    if lease_ok {
        Ok(())
    } else {
        Err(StoreError::Conflict { what: "job lease" })
    }
}

/// The handler maps the call-dependent case before `?`; the rest goes through `From`.
pub fn complete(job_id: &str, lease_ok: bool) -> Result<(), ApiError> {
    complete_job(lease_ok).map_err(|error| match error {
        StoreError::Conflict { .. } => ApiError::LeaseLost {
            job_id: job_id.to_owned(),
        },
        other => ApiError::from(other),
    })
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
/// `retry_after_s` on a `503` when the cause gives no better number. The value is this
/// skill's choice; contracts 2 only says the field is sent with `503`.
pub const UNAVAILABLE_RETRY_AFTER_S: u64 = 5;

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
/// RFC 3339 in UTC, whole seconds, `Z` suffix: `"2026-10-18T14:03:22Z"` (contracts 1).
/// Keeps full precision in memory; only the wire form is truncated. Whole seconds are
/// this skill's choice, not the contracts' (contracts 12, question 12).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, utoipa::ToSchema, schemars::JsonSchema,
)]
#[schema(value_type = String, format = DateTime, example = "2026-10-18T14:03:22Z")]
#[schemars(transparent)]
pub struct Timestamp(chrono::DateTime<chrono::Utc>);

impl Timestamp {
    pub fn now() -> Self {
        Self(chrono::Utc::now())
    }

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
    use surrealdb::types::{AlreadyExistsError, ConnectionError, QueryError};

    #[test]
    fn the_sdk_errors_are_classified_once() {
        let conflict = surrealdb::Error::query(
            "Transaction conflict".to_owned(),
            QueryError::TransactionConflict,
        );
        assert!(matches!(
            StoreError::from(conflict),
            StoreError::TxConflict(_)
        ));
        let exists = surrealdb::Error::already_exists(
            "Database record already exists".to_owned(),
            AlreadyExistsError::Record {
                id: "member_of:x".to_owned(),
            },
        );
        assert!(matches!(
            StoreError::from(exists),
            StoreError::AlreadyExists(_)
        ));
        let down = surrealdb::Error::connection(
            "WebSocket error".to_owned(),
            ConnectionError::ConnectionFailed,
        );
        assert!(matches!(StoreError::from(down), StoreError::Unavailable(_)));
        let stale = surrealdb::Error::thrown("An error occurred: stale_plan".to_owned());
        assert!(matches!(
            StoreError::from(stale),
            StoreError::Conflict { what: "plan" }
        ));
        let other = surrealdb::Error::thrown("An error occurred: map_full".to_owned());
        assert!(matches!(StoreError::from(other), StoreError::Thrown(code) if code == "map_full"));
        let bug = surrealdb::Error::internal("Couldn't coerce value for field".to_owned());
        assert!(matches!(StoreError::from(bug), StoreError::Query(_)));
    }

    #[test]
    fn store_errors_map_without_context() {
        let down = StoreError::Unavailable(surrealdb::Error::connection(
            "socket closed".to_owned(),
            ConnectionError::ConnectionFailed,
        ));
        let api = ApiError::from(down);
        assert_eq!(api.code(), ErrorCode::UpstreamUnavailable);
        assert_eq!(api.details(), Some(json!({ "upstream": "database" })));
        assert_eq!(api.retry_after_s(), Some(UNAVAILABLE_RETRY_AFTER_S));
        assert_eq!(
            ApiError::from(StoreError::NotFound).code(),
            ErrorCode::NotFound
        );
        let lost = StoreError::Conflict { what: "job lease" };
        assert_eq!(ApiError::from(lost).code(), ErrorCode::Internal);
    }

    #[test]
    fn a_lost_lease_is_lease_lost_at_the_call_site() {
        let Err(error) = complete("01JAX", false) else {
            panic!("the lease was lost");
        };
        assert_eq!(error.code(), ErrorCode::LeaseLost);
        assert_eq!(error.details(), Some(json!({ "job_id": "01JAX" })));
        assert!(complete("01JAX", true).is_ok());
    }

    #[test]
    fn the_log_carries_the_store_chain_and_the_body_does_not() {
        let error: Box<dyn StdError + Send + Sync> = Box::new(StoreError::Query(
            surrealdb::Error::internal("Couldn't coerce value for field `name`".to_owned()),
        ));
        let api = ApiError::Internal(error);
        assert_eq!(api.to_string(), "Something went wrong on our side.");
        assert_eq!(
            api.parts().cause.as_deref(),
            Some("query failed: Couldn't coerce value for field `name`")
        );
    }
}
```
