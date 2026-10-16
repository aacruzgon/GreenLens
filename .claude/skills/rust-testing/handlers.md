# Handler tests

Part of [`rust-testing`](SKILL.md). Handler tests call the axum `Router` as a tower `Service`: no listener, no client, no port ([axum testing example at 0.8.9](https://github.com/tokio-rs/axum/blob/axum-v0.8.9/examples/testing/src/main.rs)). The expected bodies are the shapes in [contracts §2](../../../docs/architecture/contracts.md#2-errors) and [§4](../../../docs/architecture/contracts.md#4-scan-events-sse); the request ID is `x-request-id: req_<ULID>`, copied into `error.request_id`.

## The app under test

The real router, `AppState` and `ApiError` belong to [`rust-axum-api`](../rust-axum-api/SKILL.md) and the request-ID layers to [`rust-tracing`](../rust-tracing/SKILL.md). The block below builds a small router with the real error path, so the test checks the body the API actually writes: `ApiError`, the `render_errors` middleware that turns it into the contracts §2 body, and `MakeReqId`, the `req_<ULID>` generator, are exact copies of their canonical blocks ([error-responses.md](../rust-axum-api/error-responses.md), [request-id.md](../rust-tracing/request-id.md#2-the-code-and-its-test)), which `check_examples.py` compares line for line. The layer order is the one in [`rust-axum-api`, layers](../rust-axum-api/layers.md): the incoming `x-request-id` removed, then `SetRequestIdLayer`, `PropagateRequestIdLayer` and `render_errors`. Only the two handlers are stand-ins; the real `get_scan` takes `ApiPath<ScanId>`.

In `bins/api/tests/` call the real `api::app` the same way. An integration test is linked with the package's library, not its binary ([Cargo book, integration tests](https://doc.rust-lang.org/cargo/reference/cargo-targets.html#integration-tests): "Integration tests can use the public API of the package's library"), so `bins/api` has a `src/lib.rs` that exports `app` and `AppState`, and its `src/main.rs` calls them ([binaries](https://doc.rust-lang.org/cargo/reference/cargo-targets.html#binaries): "Binaries can use the public API of the package's library").

- `Router` implements `Service<Request<Body>>`, so `oneshot` sends one request and returns the response ([`ServiceExt::oneshot`](https://docs.rs/tower/0.5.3/tower/trait.ServiceExt.html#method.oneshot)).
- For several requests against one state, turn the router into a service once and call `ready().await?.call(request)` each time; `ServiceExt::<Request<Body>>::ready` names the request type because `Router` is a service for any body ([`ServiceExt::ready`](https://docs.rs/tower/0.5.3/tower/trait.ServiceExt.html#method.ready); [`Service::call`](https://docs.rs/tower/0.5.3/tower/trait.Service.html#tymethod.call)).
- `response.into_body().collect().await?.to_bytes()` reads the whole body ([`BodyExt::collect`](https://docs.rs/http-body-util/0.1.5/http_body_util/trait.BodyExt.html#method.collect)).

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{Request, StatusCode, header};
use axum::routing::{get, post};
use axum::{Router, middleware};
use http_body_util::BodyExt;
use tower::{Service, ServiceBuilder, ServiceExt};
use tower_http::request_id::{PropagateRequestIdLayer, SetRequestIdLayer};

#[derive(Default)]
struct AppState {
    plants_created: AtomicU32,
}

// Stand-in: the real handler reads the store and answers 404 for a scan the user can't see.
async fn get_scan(Path(_id): Path<String>) -> Result<String, ApiError> {
    Err(ApiError::NotFound)
}

async fn create_plant(State(state): State<Arc<AppState>>) -> (StatusCode, String) {
    let n = state.plants_created.fetch_add(1, Ordering::Relaxed) + 1;
    (StatusCode::CREATED, n.to_string())
}

fn app(state: &Arc<AppState>) -> Router {
    Router::new()
        .route("/scans/{id}", get(get_scan))
        .route("/plants", post(create_plant))
        .with_state(Arc::clone(state))
        .layer(
            ServiceBuilder::new()
                .map_request(|mut request: Request<Body>| {
                    request.headers_mut().remove("x-request-id");
                    request
                })
                .layer(SetRequestIdLayer::x_request_id(MakeReqId))
                .layer(PropagateRequestIdLayer::x_request_id())
                .layer(middleware::from_fn(render_errors)),
        )
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

// copy of: rust-axum-api/error-responses.md render_errors
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

/// The `req_<ULID>` check behind the redaction.
fn is_request_id(id: &str) -> bool {
    id.strip_prefix("req_")
        .is_some_and(|ulid| ulid.parse::<ulid::Ulid>().is_ok())
}

#[tokio::test]
async fn unknown_scan_is_404_with_the_error_body() {
    let request = Request::get("/scans/01JAX3K9T6R2B8V0QZ4M7D1E5F")
        .header("x-request-id", "req_from_the_client")
        .body(Body::empty())
        .unwrap();
    let response = app(&Arc::default()).oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
    let header_id = response.headers()["x-request-id"]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(
        is_request_id(&header_id),
        "a client's id is replaced: {header_id}"
    );

    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"]["request_id"], header_id.as_str());
    insta::assert_json_snapshot!(body, {
        ".error.request_id" => insta::dynamic_redaction(|value, _| {
            let id = value.as_str().expect("request_id is a string");
            assert!(is_request_id(id), "not req_<ULID>: {id}");
            "[request_id]"
        }),
    }, @r#"
    {
      "error": {
        "code": "not_found",
        "message": "We couldn't find that.",
        "request_id": "[request_id]",
        "retryable": false
      }
    }
    "#);
}

#[tokio::test]
async fn state_carries_across_calls() {
    let mut svc = app(&Arc::default()).into_service();
    for expected in ["1", "2"] {
        let request = Request::post("/plants").body(Body::empty()).unwrap();
        let response = ServiceExt::<Request<Body>>::ready(&mut svc)
            .await
            .unwrap()
            .call(request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(bytes, expected);
    }
}
```

What the snapshot shows: `retry_after_s` and `details` are absent, not `null`, because contracts §1 says an optional field is omitted ([contracts 1, notation](../../../docs/architecture/contracts.md#notation)). The keys are sorted because the body was parsed into a `serde_json::Value` ([SKILL.md, snapshots](SKILL.md#4-snapshots-insta)).

A client's `x-request-id` is replaced. `SetRequestIdLayer` alone would keep it: it only generates an ID when the header is missing (`SetRequestId::call` in `tower-http-0.7.1/src/request_id.rs`). The API removes the incoming header first, which is [`rust-tracing`](../rust-tracing/SKILL.md)'s rule, and the test above sends one to prove it.

## Reading an SSE stream

The scan event stream ([contracts §4, transport](../../../docs/architecture/contracts.md#transport)) never ends while a scan is active, so `collect()` would hang. Read it a frame at a time with `BodyExt::frame()` ([`frame`](https://docs.rs/http-body-util/0.1.5/http_body_util/trait.BodyExt.html#method.frame)) and drive the 15-second keepalive with the paused clock. axum's `Sse` sets `content-type: text/event-stream` and `cache-control: no-cache` (the header the tunnel needs, [architecture 12](../../../docs/architecture/README.md#hosting-the-control-plane)) and writes a comment as `: <text>` ([`KeepAlive`](https://docs.rs/axum/0.8.9/axum/response/sse/struct.KeepAlive.html); ✓ `axum-0.8.9/src/response/sse.rs`). The handler here is a stand-in; the real one is in [`rust-axum-api`](../rust-axum-api/SKILL.md).

```rust
use std::convert::Infallible;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use futures::stream::{self, Stream, StreamExt};
use http_body_util::BodyExt;
use tokio::time::Instant;
use tower::ServiceExt;

async fn events() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let first = Event::default()
        .id("1")
        .event("run_started")
        .data(r#"{"v":1,"seq":1}"#);
    let live = stream::once(async move { Ok(first) }).chain(stream::pending());
    let keepalive = KeepAlive::new()
        .interval(Duration::from_secs(15))
        .text("keepalive");
    Sse::new(live).keep_alive(keepalive)
}

/// The next data frame of the body, as text.
async fn next_chunk(body: &mut Body) -> String {
    loop {
        let frame = body
            .frame()
            .await
            .expect("stream ended")
            .expect("body error");
        if let Ok(data) = frame.into_data() {
            return String::from_utf8(data.to_vec()).expect("utf-8");
        }
    }
}

#[tokio::test(start_paused = true)]
async fn sends_the_event_then_a_keepalive_every_15_s() {
    let app = Router::new().route("/scans/{id}/events", get(events));
    let request = Request::get("/scans/s1/events")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/event-stream"
    );
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-cache");

    let mut body = response.into_body();
    let start = Instant::now();
    assert_eq!(
        next_chunk(&mut body).await,
        "id: 1\nevent: run_started\ndata: {\"v\":1,\"seq\":1}\n\n"
    );
    assert_eq!(next_chunk(&mut body).await, ": keepalive\n\n");
    assert_eq!(start.elapsed(), Duration::from_secs(15));
    assert_eq!(next_chunk(&mut body).await, ": keepalive\n\n");
    assert_eq!(start.elapsed(), Duration::from_secs(30));
}
```

For resume, send `Last-Event-ID` on the request and assert the first `id:` is the next `seq`; the dedupe rule itself is a property test in [properties.md](properties.md#sse-seq-dedupe).
