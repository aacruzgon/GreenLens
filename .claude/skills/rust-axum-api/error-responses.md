# Error responses: `ApiError`, rejections, panics and body limits

Part of [rust-axum-api](SKILL.md). Every non-2xx response carries the `ErrorBody` of [contracts 2](../../../docs/architecture/contracts.md#2-errors), whatever produced it: a handler, an extractor rejection, a panic, a body limit, a timeout or axum's own 404 and 405. The error *design* (when to use which variant, the lower layers' errors and how they convert, `source` and `Display`) is in [rust-errors](../rust-errors/SKILL.md); this file holds the `ApiError` enum itself and how its body reaches the wire with its `request_id`.

## Contents

1. [The design: one renderer](#1-the-design-one-renderer)
2. [The compiled example](#2-the-compiled-example)
3. [Extractor rejections](#3-extractor-rejections)
4. [Body limits](#4-body-limits)
5. [404, not 403](#5-404-not-403)

## 1. The design: one renderer

`IntoResponse` has no access to the request, so `ApiError::into_response` can't know the `request_id`. The API therefore renders error bodies in one place:

| Step | Who | What |
|---|---|---|
| 1 | `SetRequestIdLayer` (outermost, after removing any client-sent `x-request-id`) | Puts `RequestId` (`req_<ULID>`) in the request's extensions. Owned by [rust-tracing](../rust-tracing/request-id.md) |
| 2 | `ApiError::into_response` | Returns the status with an empty body and an `ErrorParts` value in the **response** extensions. For a `5xx`, `ErrorParts.cause` holds the source chain |
| 3 | `render_errors` middleware | Reads `RequestId` from the request, runs the inner service, and if the response holds `ErrorParts`, writes the `ErrorBody` JSON with that `request_id`. It logs every `5xx` once, with `request_id` as a field of the event and the cause chain |
| 4 | `render_errors`, fallback | A non-2xx response with no `ErrorParts` and no JSON body (axum's 405 and rejections, tower-http's 413 and timeout responses) gets a body chosen from its status by `from_status` |

Why not a task-local or a body rewrite in each layer: one function owns the JSON shape and the log line, and responses that no `ApiError` made (from tower-http and axum itself) get the same body. A stored idempotent replay of a `4xx` is already JSON, so the fallback leaves it alone.

Contracts 2 says `internal` is "logged with `request_id`". The request span carries `request_id` too ([rust-tracing](../rust-tracing/request-id.md#2-the-code-and-its-test)), but a filter that drops the span's level drops the span's fields from the line, so the renderer writes `request_id` into the event itself.

Sources: [`IntoResponse`](https://docs.rs/axum/0.8.9/axum/response/trait.IntoResponse.html), [`middleware::from_fn`](https://docs.rs/axum/0.8.9/axum/middleware/fn.from_fn.html), [`Response::extensions_mut`](https://docs.rs/http/1.4.0/http/response/struct.Response.html#method.extensions_mut), [`RequestId`](https://docs.rs/tower-http/0.7.1/tower_http/request_id/struct.RequestId.html).

## 2. The compiled example

The block holds the **canonical `ApiError`**: every contracts 2 code, with its status, `retryable`, `retry_after_s` and `details`, between `// canonical: rust-axum-api/error-responses.md ApiError` and `// end: ApiError`. It also holds the canonical renderer (`render_errors`, `from_status`, `panic_response`) and the JSON extractor (`ApiJson`). Other blocks that need them carry exact copies, which `check_examples.py` compares line for line ([rust, checking the examples](../rust/SKILL.md#checking-the-examples)). The block also copies `Timestamp` ([rust-contract-types](../rust-contract-types/primitives.md#1-the-code)), the error-chain adapter `Chain` ([rust-errors](../rust-errors/SKILL.md#display-and-source)) and `MakeReqId` ([rust-tracing](../rust-tracing/request-id.md#2-the-code-and-its-test)). The tests check the exact JSON through `tower::ServiceExt::oneshot`, and that every code is produced with the properties contracts 2 gives it.

```rust
use std::error::Error as StdError;

use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Request},
    http::{HeaderValue, StatusCode},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tower::ServiceBuilder;
use tower_http::{
    catch_panic::CatchPanicLayer,
    limit::RequestBodyLimitLayer,
    request_id::{PropagateRequestIdLayer, SetRequestIdLayer},
};

// canonical: rust-axum-api/error-responses.md ErrorCode
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

// canonical: rust-axum-api/error-responses.md ApiError
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

// canonical: rust-axum-api/error-responses.md render_errors
#[derive(serde::Serialize)]
struct ErrorBody<'a> {
    error: ErrorObject<'a>,
}

#[derive(serde::Serialize)]
struct ErrorObject<'a> {
    code: ErrorCode,
    message: &'a str,
    retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    retry_after_s: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<&'a serde_json::Value>,
    request_id: &'a str,
}

/// The one place that writes `ErrorBody` and logs a `5xx`. Layered inside
/// `SetRequestIdLayer` and `TraceLayer`, outside `CatchPanicLayer`.
pub async fn render_errors(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::{HeaderValue, header};
    let request_id = request
        .extensions()
        .get::<tower_http::request_id::RequestId>()
        .and_then(|id| id.header_value().to_str().ok())
        .unwrap_or("req_unknown")
        .to_owned();
    let mut response = next.run(request).await;
    let parts = match response.extensions_mut().remove::<ErrorParts>() {
        Some(parts) => parts,
        None if response.status().as_u16() < 400 || is_json(&response) => return response,
        None => {
            // Not made by ApiError: axum's 405 and rejections, tower-http's 413, a timeout.
            let error = from_status(response.status());
            *response.status_mut() = error.status();
            error.parts()
        }
    };
    if let Some(cause) = &parts.cause {
        // contracts 2: logged with request_id, as an event field so no filter drops it.
        if response.status().is_server_error() && parts.code == ErrorCode::Internal {
            tracing::error!(request_id, error = %cause, "internal error");
        } else {
            tracing::warn!(request_id, error = %cause, "dependency unavailable");
        }
    }
    let body = ErrorBody {
        error: ErrorObject {
            code: parts.code,
            message: &parts.message,
            retryable: parts.retryable,
            retry_after_s: parts.retry_after_s,
            details: parts.details.as_ref(),
            request_id: &request_id,
        },
    };
    let bytes = serde_json::to_vec(&body).expect("ErrorBody has only string keys");
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.remove(header::CONTENT_LENGTH);
    *response.body_mut() = axum::body::Body::from(bytes);
    response
}

fn is_json(response: &axum::response::Response) -> bool {
    response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .is_some_and(|value| value.as_bytes().starts_with(b"application/json"))
}

/// The error for a response that no `ApiError` made, from its status.
fn from_status(status: axum::http::StatusCode) -> ApiError {
    use axum::http::StatusCode;
    match status {
        // Only the global RequestBodyLimitLayer answers 413 itself (section 4).
        StatusCode::PAYLOAD_TOO_LARGE => ApiError::UploadTooLarge {
            max_bytes: IMAGE_MAX_BYTES,
        },
        // Not in the contracts yet: a method a route lacks is a 404 (contracts 12, question 16).
        StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED => ApiError::NotFound,
        // Not in the contracts yet: a timeout is `internal` (contracts 12, questions 16 and 17).
        StatusCode::REQUEST_TIMEOUT | StatusCode::GATEWAY_TIMEOUT => {
            ApiError::Internal("the request timed out".into())
        }
        s if s.is_client_error() => ApiError::invalid_request(
            "request",
            format!(
                "The request couldn't be processed ({}).",
                s.canonical_reason().unwrap_or("client error")
            ),
        ),
        s => ApiError::Internal(format!("unrendered {s} response").into()),
    }
}

/// axum's method router appends `Allow` to a wrong-method response *after* the route's
/// layers ran (`MethodRouter::call_with_state`, `routing/method_routing.rs`), so
/// `render_errors` can't remove it. This outer router wraps the finished one, routes
/// everything to it, and strips `Allow` from the 404 a 405 became.
pub fn without_allow(app: axum::Router) -> axum::Router {
    axum::Router::new()
        .fallback_service(app)
        .layer(axum::middleware::map_response(
            |mut response: axum::response::Response| async move {
                if response.status() == axum::http::StatusCode::NOT_FOUND {
                    response.headers_mut().remove(axum::http::header::ALLOW);
                }
                response
            },
        ))
}

/// The panic payload, as an error so it can travel in `ApiError::Internal`.
#[derive(Debug, thiserror::Error)]
#[error("handler panicked: {0}")]
pub struct Panicked(String);

/// `CatchPanicLayer::custom` handler: the body is still an `ErrorBody` with `internal`.
#[expect(
    clippy::needless_pass_by_value,
    reason = "CatchPanicLayer hands over the payload by value"
)]
pub fn panic_response(panic: Box<dyn std::any::Any + Send + 'static>) -> axum::response::Response {
    let message = if let Some(s) = panic.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_owned()
    };
    axum::response::IntoResponse::into_response(ApiError::Internal(Box::new(Panicked(message))))
}
// end: render_errors

// canonical: rust-axum-api/error-responses.md ApiJson
/// The limit for JSON bodies, set explicitly so the rejection can report it.
pub const JSON_MAX_BYTES: usize = 1024 * 1024;

/// `axum::Json` with contracts 2 rejections.
#[derive(axum::extract::FromRequest)]
#[from_request(via(axum::Json), rejection(ApiError))]
pub struct ApiJson<T>(pub T);

impl From<axum::extract::rejection::JsonRejection> for ApiError {
    fn from(rejection: axum::extract::rejection::JsonRejection) -> Self {
        use axum::extract::rejection::JsonRejection;
        match rejection {
            JsonRejection::JsonDataError(e) => json_field_error(&e),
            JsonRejection::JsonSyntaxError(e) => json_field_error(&e),
            JsonRejection::MissingJsonContentType(_) => {
                Self::invalid_request("Content-Type", "Send the body as application/json.")
            }
            // Not in the contracts yet: an oversized JSON body (contracts 12, question 15).
            JsonRejection::BytesRejection(e)
                if e.status() == axum::http::StatusCode::PAYLOAD_TOO_LARGE =>
            {
                Self::invalid_request(
                    "body",
                    format!("The request body is larger than {JSON_MAX_BYTES} bytes."),
                )
            }
            other => Self::invalid_request("body", other.body_text()),
        }
    }
}

/// axum documents reaching the `serde_path_to_error` error through `source()`.
fn json_field_error(rejection: &(dyn std::error::Error + 'static)) -> ApiError {
    let found = std::iter::successors(Some(rejection), |e| (*e).source())
        .find_map(|e| e.downcast_ref::<serde_path_to_error::Error<serde_json::Error>>());
    let Some(error) = found else {
        return ApiError::invalid_request("body", rejection.to_string());
    };
    let problem = error.inner().to_string();
    let path = error.path().to_string();
    // serde reports a missing field at its parent's path, in the message.
    let missing = problem
        .strip_prefix("missing field `")
        .and_then(|rest| rest.split('`').next());
    let field = match (path.as_str(), missing) {
        (".", Some(name)) => name.to_owned(),
        (parent, Some(name)) => format!("{parent}.{name}"),
        (".", None) => "body".to_owned(),
        (path, None) => path.to_owned(),
    };
    ApiError::invalid_request(field, problem)
}
// end: ApiJson

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

// copy of: rust-tracing/request-id.md MakeReqId
/// `req_<ULID>`: 30 ASCII characters, always a valid header value.
#[derive(Clone, Copy, Debug, Default)]
pub struct MakeReqId;

impl tower_http::request_id::MakeRequestId for MakeReqId {
    fn make_request_id<B>(
        &mut self,
        _: &axum::http::Request<B>,
    ) -> Option<tower_http::request_id::RequestId> {
        let id = format!("req_{}", ulid::Ulid::generate());
        axum::http::HeaderValue::try_from(id)
            .ok()
            .map(tower_http::request_id::RequestId::new)
    }
}
// end: MakeReqId

/// The global backstop (section 4): above every route's `DefaultBodyLimit`, so a JSON
/// route's own limit always answers first. Photo, depth, confidence and form fields.
pub const UPLOAD_REQUEST_MAX: usize = IMAGE_MAX_BYTES + 2 * 4 * 1024 * 1024 + 64 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateGarden {
    pub name: String,
    pub location: Location,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Location {
    pub lat: f64,
    pub lon: f64,
}

async fn create_garden(ApiJson(body): ApiJson<CreateGarden>) -> (StatusCode, Json<CreateGarden>) {
    (StatusCode::CREATED, Json(body))
}

async fn get_scan() -> Result<Json<Value>, ApiError> {
    Err(ApiError::NotFound)
}

async fn boom() -> &'static str {
    panic!("invariant broken")
}

/// A dependency that failed: the body hides the cause, the log has it.
async fn weather() -> Result<Json<Value>, ApiError> {
    let cause: Box<dyn StdError + Send + Sync> = "connection refused".into();
    Err(ApiError::UpstreamUnavailable {
        upstream: "weather",
        retry_after_s: UNAVAILABLE_RETRY_AFTER_S,
        source: cause,
    })
}

pub fn app() -> Router {
    let routes = Router::new()
        .route("/gardens", post(create_garden))
        .route("/scans/{id}", get(get_scan))
        .route("/boom", get(boom))
        .route("/gardens/{id}/weather", get(weather))
        .route(
            "/invites/{code}/accept",
            post(|| async { ApiError::RateLimited { retry_after_s: 6 } }),
        )
        .route("/raw", post(|| async { StatusCode::NO_CONTENT }))
        .fallback(|| async { ApiError::NotFound })
        .layer(DefaultBodyLimit::max(JSON_MAX_BYTES))
        .layer(
            ServiceBuilder::new()
                // rust-tracing: drop a client-sent ID so every ID is ours.
                .map_request(|mut request: Request| {
                    request.headers_mut().remove("x-request-id");
                    request
                })
                .layer(SetRequestIdLayer::x_request_id(MakeReqId))
                .layer(PropagateRequestIdLayer::x_request_id())
                .layer(axum::middleware::from_fn(render_errors))
                .layer(CatchPanicLayer::custom(panic_response))
                .layer(RequestBodyLimitLayer::new(UPLOAD_REQUEST_MAX)),
        );
    without_allow(routes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{Method, header};
    use http_body_util::BodyExt;
    use serde_json::json;
    use tower::ServiceExt;

    /// Calls the app and checks that the body's `request_id` is the response's
    /// `x-request-id`, then replaces it with `[request_id]` so bodies compare exactly.
    async fn call(method: Method, uri: &str, body: &str) -> (StatusCode, HeaderValue, Value) {
        let request = axum::http::Request::builder()
            .method(method)
            .uri(uri)
            .header("x-request-id", "client-chosen")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_owned()))
            .unwrap();
        let response = app().oneshot(request).await.unwrap();
        let status = response.status();
        assert!(!response.headers().contains_key(header::ALLOW));
        let content_type = response.headers()[header::CONTENT_TYPE].clone();
        let id = response.headers()["x-request-id"]
            .to_str()
            .unwrap()
            .to_owned();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let mut body: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(id.starts_with("req_") && id.len() == 30, "{id}");
        assert_eq!(body["error"]["request_id"], id.as_str());
        body["error"]["request_id"] = Value::from("[request_id]");
        (status, content_type, body)
    }

    #[tokio::test]
    async fn api_error_renders_the_contract_body() {
        let (status, content_type, body) = call(Method::GET, "/scans/01JAX3K9T6", "").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(content_type, "application/json");
        assert_eq!(
            body,
            json!({ "error": {
                "code": "not_found",
                "message": "We couldn't find that.",
                "retryable": false,
                "request_id": "[request_id]"
            }})
        );
    }

    #[tokio::test]
    async fn wrong_type_names_the_nested_field() {
        let sent = r#"{"name":"Back yard","location":{"lat":"north","lon":1.0}}"#;
        let (status, _, body) = call(Method::POST, "/gardens", sent).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "invalid_request");
        assert_eq!(body["error"]["details"]["field"], "location.lat");
        let problem = body["error"]["details"]["problem"].as_str().unwrap();
        assert!(problem.starts_with("invalid type: string \"north\", expected f64"));
        assert_eq!(body["error"]["message"], problem);
    }

    #[tokio::test]
    async fn missing_and_unknown_fields_are_named() {
        let (status, _, body) = call(Method::POST, "/gardens", r#"{"location":{"lat":1}}"#).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["details"]["field"], "location.lon");
        let sent = r#"{"name":"g","location":{"lat":1,"lon":2},"colour":"red"}"#;
        let (status, _, body) = call(Method::POST, "/gardens", sent).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let problem = body["error"]["details"]["problem"].as_str().unwrap();
        assert!(problem.starts_with("unknown field `colour`"), "{problem}");
    }

    #[tokio::test]
    async fn malformed_json_names_where_parsing_stopped() {
        let (status, _, body) = call(Method::POST, "/gardens", "{\"name\":").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "invalid_request");
        assert_eq!(body["error"]["details"]["field"], "name");
        let problem = body["error"]["details"]["problem"].as_str().unwrap();
        assert!(problem.starts_with("EOF while parsing a value"));
    }

    #[tokio::test]
    async fn too_large_json_is_invalid_request_with_the_limit() {
        let big = format!(r#"{{"name":"{}"}}"#, "a".repeat(JSON_MAX_BYTES));
        let (status, _, body) = call(Method::POST, "/gardens", &big).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["details"]["field"], "body");
        assert_eq!(
            body["error"]["message"],
            "The request body is larger than 1048576 bytes."
        );
    }

    #[tokio::test]
    async fn rate_limited_carries_retry_after_s() {
        let (status, _, body) = call(Method::POST, "/invites/ABC/accept", "").await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            body,
            json!({ "error": {
                "code": "rate_limited",
                "message": "Too many requests. Try again in 6 seconds.",
                "retryable": true,
                "retry_after_s": 6,
                "request_id": "[request_id]"
            }})
        );
    }

    #[tokio::test]
    async fn an_unavailable_dependency_is_503_with_retry_after_and_no_cause() {
        let (status, _, body) = call(Method::GET, "/gardens/g1/weather", "").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            body,
            json!({ "error": {
                "code": "upstream_unavailable",
                "message": "A service we depend on is unavailable. Try again shortly.",
                "retryable": true,
                "retry_after_s": 5,
                "details": { "upstream": "weather" },
                "request_id": "[request_id]"
            }})
        );
    }

    #[tokio::test]
    async fn the_backstop_413_is_rendered_with_the_image_limit() {
        // A Content-Length over the limit is refused before the body is read.
        let request = axum::http::Request::post("/raw")
            .header(header::CONTENT_LENGTH, (UPLOAD_REQUEST_MAX + 1).to_string())
            .body(Body::from("x"))
            .unwrap();
        let response = app().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"]["code"], "upload_too_large");
        assert_eq!(body["error"]["details"]["max_bytes"], IMAGE_MAX_BYTES);
    }

    #[tokio::test]
    async fn panic_becomes_internal_error_body() {
        let (status, _, body) = call(Method::GET, "/boom", "").await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            body,
            json!({ "error": {
                "code": "internal",
                "message": "Something went wrong on our side.",
                "retryable": false,
                "request_id": "[request_id]"
            }})
        );
    }

    #[tokio::test]
    async fn wrong_method_is_a_404_without_allow() {
        let (status, _, body) = call(Method::DELETE, "/gardens", "").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "not_found");
    }

    #[test]
    fn other_unrendered_statuses_get_a_sentence() {
        let error = from_status(StatusCode::LENGTH_REQUIRED);
        assert_eq!(error.code(), ErrorCode::InvalidRequest);
        assert_eq!(
            error.to_string(),
            "The request couldn't be processed (Length Required)."
        );
        assert_eq!(
            from_status(StatusCode::GATEWAY_TIMEOUT).code(),
            ErrorCode::Internal
        );
    }

    /// Every contracts 2 code, in the table's order.
    fn every_code() -> Vec<ApiError> {
        let source = || -> Box<dyn StdError + Send + Sync> { "upstream said no".into() };
        let resets_at = chrono::DateTime::from_timestamp(1_792_281_600, 0).unwrap();
        vec![
            ApiError::invalid_request("name", "missing field `name`"),
            ApiError::Unauthenticated,
            ApiError::Forbidden {
                required_role: Role::Editor,
            },
            ApiError::InvalidSignature,
            ApiError::NotFound,
            ApiError::InvalidState(InvalidStateDetails {
                status: Some("failed".into()),
                hint: Some(StateHint::NewScan),
                ..InvalidStateDetails::default()
            }),
            ApiError::RequestInProgress { retry_after_s: 2 },
            ApiError::LeaseLost {
                job_id: "01JAX".into(),
            },
            ApiError::UploadTooLarge {
                max_bytes: IMAGE_MAX_BYTES,
            },
            ApiError::UnsupportedMediaType,
            ApiError::ImageQuality {
                problem: QualityProblem::TooBlurry,
                blur_score: 41.2,
                exposure: 0.52,
            },
            ApiError::MissingRequiredShot {
                missing: vec!["whole_plant".into()],
            },
            ApiError::TooManyImages {
                kind: "leaf_closeup".into(),
                max: 2,
            },
            ApiError::IdempotencyConflict,
            ApiError::RateLimited { retry_after_s: 6 },
            ApiError::BudgetExceeded {
                budget: Budget::Scans,
                resets_at: Timestamp::from(resets_at),
            },
            ApiError::Internal(source()),
            ApiError::ProcessingUnavailable {
                retry_after_s: 5,
                source: source(),
            },
            ApiError::UpstreamUnavailable {
                upstream: "weather",
                retry_after_s: 5,
                source: source(),
            },
        ]
    }

    #[test]
    fn every_contract_code_is_produced_once() {
        let codes: Vec<Value> = every_code().iter().map(|e| json!(e.code())).collect();
        let contract = [
            "invalid_request",
            "unauthenticated",
            "forbidden",
            "invalid_signature",
            "not_found",
            "invalid_state",
            "request_in_progress",
            "lease_lost",
            "upload_too_large",
            "unsupported_media_type",
            "image_quality",
            "missing_required_shot",
            "too_many_images",
            "idempotency_conflict",
            "rate_limited",
            "budget_exceeded",
            "internal",
            "processing_unavailable",
            "upstream_unavailable",
        ];
        assert_eq!(codes, contract.map(Value::from));
    }

    #[test]
    fn statuses_retryable_and_retry_after_match_contracts_2() {
        let rows: Vec<(u16, bool, Option<u64>)> = every_code()
            .iter()
            .map(|e| (e.status().as_u16(), e.retryable(), e.retry_after_s()))
            .collect();
        assert_eq!(
            rows,
            [
                (400, false, None),
                (401, false, None),
                (403, false, None),
                (403, false, None),
                (404, false, None),
                (409, false, None),
                (409, true, Some(2)),
                (409, false, None),
                (413, false, None),
                (415, false, None),
                (422, true, None),
                (422, false, None),
                (422, false, None),
                (422, false, None),
                (429, true, Some(6)),
                (429, false, None),
                (500, false, None),
                (503, true, Some(5)),
                (503, true, Some(5)),
            ]
        );
    }

    #[test]
    fn details_have_the_contract_keys() {
        let details: Vec<Option<Value>> = every_code().iter().map(ApiError::details).collect();
        assert_eq!(details[2], Some(json!({ "required_role": "editor" })));
        assert_eq!(
            details[5],
            Some(json!({ "status": "failed", "hint": "new_scan" }))
        );
        assert_eq!(details[7], Some(json!({ "job_id": "01JAX" })));
        assert_eq!(details[8], Some(json!({ "max_bytes": 12_582_912 })));
        assert_eq!(
            details[10],
            Some(json!({
                "problem": "too_blurry", "blur_score": 41.2,
                "exposure": 0.52, "hint": "tap_to_focus"
            }))
        );
        assert_eq!(
            details[15],
            Some(json!({ "budget": "scans", "resets_at": "2026-10-18T00:00:00Z" }))
        );
        assert_eq!(details[18], Some(json!({ "upstream": "weather" })));
        let none = [1, 3, 4, 6, 13, 14, 16, 17];
        for (i, d) in details.iter().enumerate() {
            assert_eq!(d.is_none(), none.contains(&i), "details of code #{i}");
        }
    }

    #[test]
    fn the_message_is_safe_and_the_cause_is_only_for_the_log() {
        let codes = every_code();
        assert_eq!(
            codes[10].to_string(),
            "This photo is too blurry. Hold the phone still and tap the leaf to focus."
        );
        assert_eq!(codes[16].to_string(), "Something went wrong on our side.");
        assert_eq!(codes[16].parts().cause.as_deref(), Some("upstream said no"));
        assert_eq!(codes[4].parts().cause, None);
    }
}
```

What the example shows, rule by rule:

- **`ErrorBody` exactly.** `retry_after_s` and `details` are omitted when absent, never `null` ([contracts 1, notation](../../../docs/architecture/contracts.md#notation)); `request_id` is always present. The tests compare the whole JSON value.
- **`message` is `Display`.** For `Internal`, `ProcessingUnavailable` and `UpstreamUnavailable` it is a fixed sentence; the source chain goes to the log, not the body ([rust-errors](../rust-errors/SKILL.md#display-and-source)). A response that no `ApiError` made gets a fixed sentence from `from_status`, never a bare status number.
- **`retry_after_s` on `503`.** Both `503` codes always send it, as contracts 2 lists it "with … 503"; `UNAVAILABLE_RETRY_AFTER_S` (5 s) is the value when the cause gives none.
- **Logging.** `render_errors` logs each `5xx` once: `internal` at `error`, the `503` codes at `warn`, with `request_id` and the cause chain as fields ([contracts 2](../../../docs/architecture/contracts.md#2-errors): `internal` is "logged with `request_id`"; [rust-tracing](../rust-tracing/SKILL.md)). Handlers don't log the error they return.
- **Panics.** [`CatchPanicLayer::custom`](https://docs.rs/tower-http/0.7.1/tower_http/catch_panic/struct.CatchPanicLayer.html#method.custom) takes a handler that implements `ResponseForPanic`, which any `FnMut(Box<dyn Any + Send>) -> Response + Clone` does (`catch_panic.rs`, `impl ResponseForPanic for F`). Returning `ApiError::Internal(...).into_response()` puts `ErrorParts` on the response, so the renderer writes `internal`. The default handler answers `text/plain` "Service panicked" (`DefaultResponseForPanic`). A panic is still a bug ([rust-errors](../rust-errors/SKILL.md#panics-and-expect)). It catches panics while the response future runs, not while a streamed body (SSE) is polled.
- **Unknown routes and methods.** `Router::fallback` returns `not_found`. axum's 405 has an empty body and an `Allow` header; the renderer turns it into `404 not_found`, and `without_allow`, wrapped around the finished router, removes `Allow`, so the response says nothing about the route. It has to sit outside the router because axum's method router adds `Allow` after the route's own layers have run (`MethodRouter::call_with_state`). **Not in the contracts yet:** contracts 2 has no code for `405`, so this skill answers `404` ([contracts 12, question 16](../../../docs/architecture/contracts.md#12-open-questions)). [`Router::method_not_allowed_fallback`](https://docs.rs/axum/0.8.9/axum/struct.Router.html#method.method_not_allowed_fallback) is the alternative if a 405 status is wanted.
- **Timeouts.** tower-http's `TimeoutLayer` answers with the status it was given and an empty body ([layers 4](layers.md#4-timeouts)); the renderer turns `408` and `504` into `internal`, which is logged. **Not in the contracts yet:** contracts 2 has no timeout code, and `internal` is `retryable: false`, so the app doesn't retry a timed-out request ([frontend 9](../../../docs/frontend/README.md#error-handling); [contracts 12, questions 16 and 17](../../../docs/architecture/contracts.md#12-open-questions)).
- **The request ID** comes from the request's `RequestId` extension, the same value [`PropagateRequestIdLayer`](https://docs.rs/tower-http/0.7.1/tower_http/request_id/struct.PropagateRequestIdLayer.html) copies to the response's `x-request-id`. `SetRequestIdLayer` would keep an `x-request-id` the client sent (`request_id.rs`, `SetRequestId::call`), so the stack first removes it, as [rust-tracing](../rust-tracing/request-id.md#1-the-layer-stack) decides; the tests send one and check it was replaced.

## 3. Extractor rejections

axum's built-in rejections answer in plain text with their own statuses: a JSON body of the wrong shape is `422` ("Failed to deserialize the JSON body into the target type"), malformed JSON `400`, a missing `Content-Type` `415`, a `multipart/form-data` request without a boundary `400` (`axum/src/extract/rejection.rs`, `axum/src/extract/multipart.rs`). Contracts 2 wants `400 invalid_request` with `{ field, problem }`. So handlers never take `axum::Json`, `Query`, `Path` or `Multipart` directly; they take wrappers whose rejection is `ApiError`, the "own extractor" option of [axum's extractor docs](https://docs.rs/axum/0.8.9/axum/extract/index.html#customizing-extractor-responses) and the [customize-extractor-error example](https://github.com/tokio-rs/axum/tree/axum-v0.8.9/examples/customize-extractor-error).

| Extractor | Use | Rejection becomes |
|---|---|---|
| `ApiJson<T>` (section 2) | Every JSON body | `invalid_request`, `field` = the JSON path (`location.lat`) from `serde_path_to_error`, which axum documents as reachable through `source()` ([accessing inner errors](https://docs.rs/axum/0.8.9/axum/extract/index.html#accessing-inner-errors)) |
| `ApiQuery<T>` | `?cursor=&limit=` and other query strings | `invalid_request`, `field` = the parameter name |
| `ApiPath<T>` | Path captures, typed (`ApiPath<ScanId>`) | `invalid_request` for a value that doesn't parse, `field` = the capture's name; `internal` for `MissingPathParams` or a wrong parameter count, which are bugs |
| `ApiMultipart` | Uploads | `invalid_request` for a missing or bad boundary. Errors while reading arrive per field as `MultipartError`; map them by its `status()`: `413` → `upload_too_large`, else `invalid_request` |
| `HeaderMap` | `Idempotency-Key`, `Last-Event-ID`, `X-Lease-Token` | Parse by hand; a bad value is `invalid_request` with the header's name as `field` |

Rules:

- **The body extractor goes last**, and only one may consume the body. Everything before it must implement `FromRequestParts` (`State`, `ApiPath`, `ApiQuery`, `HeaderMap`, the auth extractor). ([the order of extractors](https://docs.rs/axum/0.8.9/axum/extract/index.html#the-order-of-extractors))
- **IDs in paths are typed** (`ApiPath<ScanId>`, with the ID newtypes of [rust-contract-types](../rust-contract-types/primitives.md#1-the-code)), so a value that isn't a ULID fails extraction with `400 invalid_request` naming the capture. A well-formed ID that names no record, or one the caller may not see, is `404` ([contracts 2](../../../docs/architecture/contracts.md#2-errors)). **Not in the contracts yet:** contracts 2 doesn't say whether a malformed ID is `400` or `404`; this skill answers `400`, which reveals nothing because nothing can exist under it ([contracts 12, question 14](../../../docs/architecture/contracts.md#12-open-questions)).
- **Path syntax is `/{id}`.** axum 0.8 changed captures from `/:id` to `/{id}` and wildcards to `/{*rest}`; the old syntax panics when the route is added ([axum CHANGELOG 0.8.0](https://docs.rs/crate/axum/0.8.9/source/CHANGELOG.md), `routing/path_router.rs`).
- **Unknown JSON fields in app requests are rejected.** Request types use `#[serde(deny_unknown_fields)]`, so a misspelt field is `400 invalid_request` naming it instead of a silent `None` ([rust-contract-types 5](../rust-contract-types/SKILL.md#5-unknown-fields-and-unknown-values-by-direction); the `missing_and_unknown_fields_are_named` test). **Not in the contracts yet:** contracts 2 lists malformed JSON and missing or wrong-typed fields as `invalid_request`, not unknown ones ([contracts 12, question 13](../../../docs/architecture/contracts.md#12-open-questions)). Worker requests keep ignoring unknown fields, because older and newer workers run side by side ([contracts 1, versioning](../../../docs/architecture/contracts.md#versioning)).

```rust
use std::error::Error as StdError;
use std::fmt;

use axum::{
    extract::{FromRequest, FromRequestParts, Multipart, Request},
    http::{HeaderMap, StatusCode},
};
use serde::Deserialize;

// canonical: rust-axum-api/error-responses.md ApiQuery
/// `axum::extract::Query` with contracts 2 rejections.
#[derive(axum::extract::FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(ApiError))]
pub struct ApiQuery<T>(pub T);

impl From<axum::extract::rejection::QueryRejection> for ApiError {
    fn from(rejection: axum::extract::rejection::QueryRejection) -> Self {
        // Query parses with serde_urlencoded, whose error type is serde's value::Error.
        type QueryError = serde_path_to_error::Error<serde::de::value::Error>;
        let found = std::iter::successors(
            Some(&rejection as &(dyn std::error::Error + 'static)),
            |e| (*e).source(),
        )
        .find_map(|e| e.downcast_ref::<QueryError>());
        match found {
            Some(e) => Self::invalid_request(e.path().to_string(), e.inner().to_string()),
            None => Self::invalid_request("query", rejection.body_text()),
        }
    }
}
// end: ApiQuery

// canonical: rust-axum-api/error-responses.md ApiPath
/// `axum::extract::Path` with contracts 2 rejections.
#[derive(axum::extract::FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(ApiError))]
pub struct ApiPath<T>(pub T);

impl From<axum::extract::rejection::PathRejection> for ApiError {
    fn from(rejection: axum::extract::rejection::PathRejection) -> Self {
        use axum::extract::path::ErrorKind;
        use axum::extract::rejection::PathRejection;
        match rejection {
            PathRejection::FailedToDeserializePathParams(e) => match e.into_kind() {
                ErrorKind::ParseErrorAtKey {
                    key, expected_type, ..
                } => Self::invalid_request(key, format!("expected {expected_type}")),
                ErrorKind::DeserializeError { key, message, .. } => {
                    Self::invalid_request(key, message)
                }
                ErrorKind::InvalidUtf8InPathParam { key } => {
                    Self::invalid_request(key, "invalid UTF-8")
                }
                // The route and the type disagree: a bug, not the client's input.
                kind @ (ErrorKind::WrongNumberOfParameters { .. }
                | ErrorKind::UnsupportedType { .. }) => Self::Internal(kind.to_string().into()),
                other => Self::invalid_request("path", other.to_string()),
            },
            other => Self::Internal(other.body_text().into()),
        }
    }
}
// end: ApiPath

/// `axum::extract::Multipart` with a contracts 2 rejection for a missing boundary.
pub struct ApiMultipart(pub Multipart);

impl<S: Send + Sync> FromRequest<S> for ApiMultipart {
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, ApiError> {
        Multipart::from_request(request, state)
            .await
            .map(Self)
            .map_err(|_| {
                ApiError::invalid_request(
                    "Content-Type",
                    "Send the photo as multipart/form-data with a boundary.",
                )
            })
    }
}

/// A path ID, typed: the real newtypes are in rust-contract-types primitives.md.
/// It parses inside a `visit_str`, so axum's path deserializer can name the capture in
/// the error (`DeserializeError { key, .. }`, `extract/path/de.rs`). `ulid`'s own
/// `Deserialize` parses after the visitor returns, and the key is lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanId(ulid::Ulid);

impl<'de> Deserialize<'de> for ScanId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl serde::de::Visitor<'_> for Visitor {
            type Value = ScanId;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a 26-character ULID")
            }
            fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<ScanId, E> {
                ulid::Ulid::from_string(s).map(ScanId).map_err(E::custom)
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}

impl fmt::Display for ScanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Upload limits: contracts 2 (`upload_too_large`, 12 MB) and contracts 3 (depth, 4 MB).
#[derive(Debug, Clone, Copy)]
pub struct UploadLimits {
    pub image_bytes: usize,
    pub depth_bytes: usize,
}

impl UploadLimits {
    pub const CONTRACT: Self = Self {
        image_bytes: IMAGE_MAX_BYTES,
        depth_bytes: 4 * 1024 * 1024,
    };

    /// For `DefaultBodyLimit::max` on the upload route: photo, depth, confidence, form fields.
    pub const fn request_bytes(self) -> usize {
        self.image_bytes + 2 * self.depth_bytes + 64 * 1024
    }
}

#[derive(Debug, Default)]
pub struct Upload {
    pub kind: Option<String>,
    pub file: Option<Vec<u8>>,
    pub depth: Option<Vec<u8>>,
    /// contracts 3: depth problems never fail the upload; the response says why.
    pub depth_problem: Option<&'static str>,
}

fn multipart_error(e: &axum::extract::multipart::MultipartError, limits: UploadLimits) -> ApiError {
    if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
        ApiError::UploadTooLarge {
            max_bytes: limits.image_bytes,
        }
    } else {
        ApiError::invalid_request("body", e.body_text())
    }
}

/// Reads a field in chunks and stops at `max` bytes: `Ok(None)` means it was too large.
async fn read_capped(
    field: &mut axum::extract::multipart::Field<'_>,
    max: usize,
    limits: UploadLimits,
) -> Result<Option<Vec<u8>>, ApiError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|e| multipart_error(&e, limits))?
    {
        if bytes.len() + chunk.len() > max {
            return Ok(None);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(Some(bytes))
}

pub async fn read_upload(
    ApiMultipart(mut form): ApiMultipart,
    limits: UploadLimits,
) -> Result<Upload, ApiError> {
    let mut upload = Upload::default();
    while let Some(mut field) = form
        .next_field()
        .await
        .map_err(|e| multipart_error(&e, limits))?
    {
        match field.name() {
            Some("kind") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| multipart_error(&e, limits))?;
                upload.kind = Some(text);
            }
            Some("file") => {
                let Some(bytes) = read_capped(&mut field, limits.image_bytes, limits).await? else {
                    return Err(ApiError::UploadTooLarge {
                        max_bytes: limits.image_bytes,
                    });
                };
                upload.file = Some(bytes);
            }
            Some("depth") => match read_capped(&mut field, limits.depth_bytes, limits).await? {
                Some(bytes) => upload.depth = Some(bytes),
                None => upload.depth_problem = Some("too_large"),
            },
            // Unknown parts are skipped: dropping a field skips the rest of it.
            _ => {}
        }
    }
    Ok(upload)
}

/// `Last-Event-ID` (contracts 4): the last `seq` the app applied.
pub fn last_event_id(headers: &HeaderMap) -> Result<Option<u64>, ApiError> {
    headers
        .get("last-event-id")
        .map(|value| {
            value
                .to_str()
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok())
                .ok_or_else(|| ApiError::invalid_request("Last-Event-ID", "expected a seq number"))
        })
        .transpose()
}

/// `Idempotency-Key` (contracts 1): a UUID per user action.
pub fn idempotency_key(headers: &HeaderMap) -> Result<Option<uuid::Uuid>, ApiError> {
    headers
        .get("idempotency-key")
        .map(|value| {
            value
                .to_str()
                .ok()
                .and_then(|s| uuid::Uuid::parse_str(s).ok())
                .ok_or_else(|| ApiError::invalid_request("Idempotency-Key", "expected a UUID"))
        })
        .transpose()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListParams {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
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
    use axum::body::Body;
    use axum::http::Request as HttpRequest;

    async fn query(uri: &str) -> Result<ApiQuery<ListParams>, ApiError> {
        let (mut parts, ()) = HttpRequest::get(uri).body(()).unwrap().into_parts();
        ApiQuery::<ListParams>::from_request_parts(&mut parts, &()).await
    }

    #[tokio::test]
    async fn bad_query_value_names_the_parameter() {
        let Err(ApiError::InvalidRequest { field, problem }) = query("/plants?limit=ten").await
        else {
            panic!("expected invalid_request");
        };
        assert_eq!(field, "limit");
        assert_eq!(problem, "invalid digit found in string");
        let ApiQuery(ok) = query("/plants?cursor=abc&limit=20").await.unwrap();
        assert_eq!((ok.cursor.as_deref(), ok.limit), (Some("abc"), Some(20)));
    }

    #[tokio::test]
    async fn a_malformed_path_id_is_invalid_request_naming_the_capture() {
        use axum::{Router, routing::get};
        use http_body_util::BodyExt;
        use tower::ServiceExt;
        async fn scan(ApiPath(id): ApiPath<ScanId>) -> String {
            id.to_string()
        }
        let app = Router::new().route("/scans/{id}", get(scan));
        let ok = HttpRequest::get("/scans/01JAX3K9T6R2B8V0QZ4M7D1E5F")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(ok).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"01JAX3K9T6R2B8V0QZ4M7D1E5F");

        let bad = HttpRequest::get("/scans/not-a-ulid")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(bad).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let parts = response.extensions().get::<ErrorParts>().unwrap();
        assert_eq!(parts.details.as_ref().unwrap()["field"], "id");
    }

    fn form(parts: &[(&str, &[u8])]) -> Request {
        let mut body = Vec::new();
        for (name, bytes) in parts {
            body.extend_from_slice(
                format!("--X\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n")
                    .as_bytes(),
            );
            body.extend_from_slice(bytes);
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(b"--X--\r\n");
        HttpRequest::post("/scans/1/images")
            .header("content-type", "multipart/form-data; boundary=X")
            .body(Body::from(body))
            .unwrap()
    }

    const SMALL: UploadLimits = UploadLimits {
        image_bytes: 8,
        depth_bytes: 4,
    };

    #[tokio::test]
    async fn oversized_photo_is_upload_too_large() {
        let request = form(&[("kind", b"leaf_closeup"), ("file", b"123456789")]);
        let multipart = ApiMultipart::from_request(request, &()).await.unwrap();
        let result = read_upload(multipart, SMALL).await;
        assert!(matches!(
            result,
            Err(ApiError::UploadTooLarge { max_bytes: 8 })
        ));
    }

    #[tokio::test]
    async fn oversized_depth_is_dropped_and_the_photo_kept() {
        let request = form(&[("depth", b"12345"), ("file", b"12345678")]);
        let multipart = ApiMultipart::from_request(request, &()).await.unwrap();
        let upload = read_upload(multipart, SMALL).await.unwrap();
        assert_eq!(upload.file.as_deref(), Some(&b"12345678"[..]));
        assert_eq!(
            (upload.depth, upload.depth_problem),
            (None, Some("too_large"))
        );
    }

    #[tokio::test]
    async fn a_text_body_on_the_upload_route_is_a_readable_invalid_request() {
        let request = HttpRequest::post("/scans/1/images")
            .header("content-type", "text/plain")
            .body(Body::from("hello"))
            .unwrap();
        let Err(error) = ApiMultipart::from_request(request, &()).await else {
            panic!("a text body is not multipart");
        };
        assert_eq!(
            error.to_string(),
            "Send the photo as multipart/form-data with a boundary."
        );
        assert_eq!(error.details().unwrap()["field"], "Content-Type");
    }

    #[test]
    fn headers_parse_or_name_the_header() {
        let mut headers = HeaderMap::new();
        assert_eq!(last_event_id(&headers).unwrap(), None);
        headers.insert("last-event-id", "17".parse().unwrap());
        assert_eq!(last_event_id(&headers).unwrap(), Some(17));
        headers.insert("idempotency-key", "not-a-uuid".parse().unwrap());
        let Err(ApiError::InvalidRequest { field, .. }) = idempotency_key(&headers) else {
            panic!("expected invalid_request");
        };
        assert_eq!(field, "Idempotency-Key");
    }

    #[test]
    fn the_copied_error_type_is_the_canonical_one() {
        let error: Box<dyn StdError + Send + Sync> = "boom".into();
        assert_eq!(
            ApiError::Internal(error).status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
```

## 4. Body limits

Two mechanisms exist, and the API uses both:

| | [`DefaultBodyLimit`](https://docs.rs/axum/0.8.9/axum/extract/struct.DefaultBodyLimit.html) (axum) | [`RequestBodyLimitLayer`](https://docs.rs/tower-http/0.7.1/tower_http/limit/struct.RequestBodyLimitLayer.html) (tower-http) |
|---|---|---|
| Applies to | Extractors that use it: `Bytes`, `String`, `Json`, `Form`, `Multipart`. Not to a handler that reads `Request`'s body itself | Every request body |
| Default | 2 MB (2,097,152 bytes, `axum-core/src/ext_traits/request.rs`) | None; you set it |
| Over the limit | The extractor's rejection: `JsonRejection::BytesRejection` or `MultipartError` with status 413, so it reaches `ApiError` through the extractor mapping | A `Content-Length` over the limit gets `413` at once, plain text, without calling the handler; a body without one errors while it is read, which an extractor reports as its own 413 rejection |
| Turned into an `ErrorBody` by | The `From<…Rejection> for ApiError` impls (sections 2 and 3) | The renderer's status fallback (section 2, `from_status`) |

Settings:

- **One `RequestBodyLimitLayer`**: the global backstop at `UPLOAD_REQUEST_MAX`, the upload request size, around the whole router, so no handler can read more, whatever extractor it uses. It is larger than every route's `DefaultBodyLimit`, so on any route the route's own limit answers first and reports its own number. Never add a smaller `RequestBodyLimitLayer` on a route: a chunked body over it errors inside the extractor, which then reports the extractor's limit, not the layer's. The backstop's own `413` (a `Content-Length` over 20 MiB) renders as `upload_too_large` with `max_bytes` = 12,582,912, the photo limit.
- **Upload routes** (`POST /scans/{id}/images`, `POST /conversations/{id}/images`): `.layer(DefaultBodyLimit::max(UploadLimits::CONTRACT.request_bytes()))` on that route only ([different limits for different routes](https://docs.rs/axum/0.8.9/axum/extract/struct.DefaultBodyLimit.html#different-limits-for-different-routes)), and the handler caps each part as in section 3: the photo at 12 MiB (`upload_too_large`, `max_bytes` = 12,582,912), the depth map at 4 MiB (dropped with `too_large`, the photo kept, [contracts 3](../../../docs/architecture/contracts.md#post-scansidimages)).
- **JSON routes**: `DefaultBodyLimit::max(JSON_MAX_BYTES)` set explicitly, so the limit the rejection reports is the one in force. 1 MiB is this skill's choice; the contracts give no JSON limit. A body over it is `400 invalid_request` with `field: "body"` and the limit in the message. **Not in the contracts yet:** contracts 2 defines `upload_too_large` only for an image over 12 MB, and the app answers it by re-encoding the photo ([frontend 9](../../../docs/frontend/README.md#error-handling)), so an oversized JSON body must not use it ([contracts 12, question 15](../../../docs/architecture/contracts.md#12-open-questions)).
- "12 MB" in the contracts is read as 12 MiB here; `max_bytes` tells the app the exact number either way.

## 5. 404, not 403

A user who isn't a member of the garden (or doesn't own the conversation) gets `404 not_found`, exactly what an unknown ID gets. `403 forbidden`, with `details.required_role`, is only for a member whose role is too low ([contracts 1, roles and access](../../../docs/architecture/contracts.md#roles-and-access); [architecture 4](../../../docs/architecture/README.md#authorization-model)).

- Resolve membership and the record **in one step** that returns "not found" for both cases, so no code path can tell them apart. The garden-scoped repository function takes the user ([architecture 4](../../../docs/architecture/README.md#authorization-model); the query is in [rust-surrealdb](../rust-surrealdb/SKILL.md)).
- Check the role **after** membership: a non-member never learns which role a route needs.
- The roles form a chain (each column of the role matrix has every right of the one before), so `Role` derives `Ord` in matrix order.
- The SSE stream checks membership before it sends anything, the same way ([architecture 4](../../../docs/architecture/README.md#authorization-model)).

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Viewer,
    Reviewer,
    Editor,
    Owner,
}

/// The two outcomes of the access check; the handler turns them into
/// `ApiError::NotFound` and `ApiError::Forbidden { required_role }` (section 2).
#[derive(Debug, PartialEq, Eq)]
pub enum AccessError {
    /// 404 `not_found`: unknown ID, or the caller is not a member.
    NotFound,
    /// 403 `forbidden` with `details.required_role`.
    Forbidden { required_role: Role },
}

/// `membership` is the caller's role in the record's garden: `None` when the record
/// doesn't exist *or* the caller isn't a member. The store returns both as `None`.
pub fn require(membership: Option<Role>, required: Role) -> Result<Role, AccessError> {
    let role = membership.ok_or(AccessError::NotFound)?;
    if role < required {
        return Err(AccessError::Forbidden {
            required_role: required,
        });
    }
    Ok(role)
}

#[test]
fn non_members_get_not_found_and_low_roles_get_forbidden() {
    assert_eq!(require(None, Role::Viewer), Err(AccessError::NotFound));
    assert_eq!(
        require(Some(Role::Viewer), Role::Editor),
        Err(AccessError::Forbidden {
            required_role: Role::Editor
        })
    );
    assert_eq!(require(Some(Role::Owner), Role::Editor), Ok(Role::Owner));
}
```
