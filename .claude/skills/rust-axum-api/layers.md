# Middleware: order, CORS, timeouts, rate limits, shutdown

Part of [rust-axum-api](SKILL.md). Versions: axum 0.8.9, tower 0.5.3, tower-http 0.7.1, governor 0.10.4 ([versions](../rust/versions.md)).

## Contents

1. [How order works](#1-how-order-works)
2. [The stack in order](#2-the-stack-in-order)
3. [CORS](#3-cors)
4. [Timeouts](#4-timeouts)
5. [Rate limits](#5-rate-limits)
6. [Graceful shutdown](#6-graceful-shutdown)

## 1. How order works

- `Router::layer` wraps everything added **before** it, so successive `.layer` calls run bottom to top. `ServiceBuilder` runs top to bottom: the first layer listed sees the request first. Put the global stack in **one** `ServiceBuilder` so it reads in request order ([axum middleware, ordering](https://docs.rs/axum/0.8.9/axum/middleware/index.html#ordering)).
- `Router::layer` also wraps the fallback; `Router::route_layer` wraps matched routes only, so a layer that rejects (auth) belongs in `route_layer` and unknown paths still get `404` ([`Router::route_layer`](https://docs.rs/axum/0.8.9/axum/struct.Router.html#method.route_layer)).
- A layer on a sub-router applies only to the routes added to that sub-router before it ([`Router::layer`](https://docs.rs/axum/0.8.9/axum/struct.Router.html#method.layer)), and `merge` combines the groups, each keeping its own layers ([`Router::merge`](https://docs.rs/axum/0.8.9/axum/struct.Router.html#method.merge)). Per-group layers are how routes get different auth, timeouts and body limits; the timeout tests below show the app group and the worker group with different limits.
- One thing runs **outside** every route layer: axum's method router adds `Allow` to a wrong-method response after the route's layers ran, so the finished router is wrapped once more in `without_allow` ([error-responses 2](error-responses.md#2-the-compiled-example)).

## 2. The stack in order

| # | Layer | Why here |
|---|---|---|
| 0–2 | `map_request` removing a client's `x-request-id`, `SetRequestIdLayer::x_request_id(MakeReqId)`, `PropagateRequestIdLayer::x_request_id()` | First, so every later layer, the trace span and the error body see the same `req_<ULID>`, and every response (preflights and rendered errors included) carries it. Owned by [rust-tracing](../rust-tracing/request-id.md#1-the-layer-stack) |
| 3 | `TraceLayer` | Outside everything that can answer, so the span covers the final status. Its configuration is in [rust-tracing](../rust-tracing/request-id.md#2-the-code-and-its-test) |
| 4 | `CorsLayer` | Before auth: a preflight carries no `Authorization`, and `Cors` answers every preflight itself without calling the inner service (`Cors::call` in [`cors/mod.rs`](https://docs.rs/crate/tower-http/0.7.1/source/src/cors/mod.rs)) |
| 5 | `from_fn(render_errors)` | Writes every error body ([error-responses 1](error-responses.md#1-the-design-one-renderer)). Everything below it, including tower-http's own responses, gets an `ErrorBody` |
| 6 | `CatchPanicLayer::custom(panic_response)` | Inside the renderer, so a panic's `internal` body gets its `request_id` ([error-responses 2](error-responses.md#2-the-compiled-example)) |
| 7 | `RequestBodyLimitLayer` | The global byte cap, inside the renderer so its plain-text 413 becomes `upload_too_large` ([error-responses 4](error-responses.md#4-body-limits)) |
| per group | `route_layer(auth)`, the rate limiter, `TimeoutLayer`, `DefaultBodyLimit` | Different for app, worker, webhook and ops routes ([auth 4](auth.md#4-which-routes-get-which-check)) |
| outside | `without_allow` | Wraps the finished router (section 1) |

The `api` binary keeps the router in a function (`fn app(state: &Arc<AppState>) -> Router`) so tests drive the same stack with `oneshot`. The block carries copies of `ApiError`, `render_errors` and `MakeReqId`, so the stack under test is the real one.

```rust
use std::{convert::Infallible, sync::Arc, time::Duration};

use axum::{
    Router,
    extract::{Request, State},
    http::{HeaderName, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{
        Response,
        sse::{Event, Sse},
    },
    routing::{get, post},
};
use futures::Stream;
use tower::ServiceBuilder;
use tower_http::{
    catch_panic::CatchPanicLayer,
    cors::{AllowOrigin, CorsLayer},
    limit::RequestBodyLimitLayer,
    request_id::{PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

pub struct AppState {
    /// The web build's origins (frontend 17), from configuration.
    pub web_origins: Vec<HeaderValue>,
}

/// auth.md: verify the Clerk token, put the user in the request's extensions.
async fn require_user(State(_): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    next.run(request).await
}

/// auth.md: map the worker token to a worker.
async fn require_worker(State(_): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    next.run(request).await
}

pub const UPLOAD_REQUEST_MAX: usize = 12 * 1024 * 1024 + 2 * 4 * 1024 * 1024 + 64 * 1024;
/// Ordinary requests: well above any query, far below a stuck one (this skill's choice).
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// The claim long poll waits up to 30 s (contracts 5), so its group needs more.
pub const CLAIM_TIMEOUT: Duration = Duration::from_secs(35);

/// A per-group timeout whose response `render_errors` turns into `500 internal`, logged.
// Not in the contracts yet: a timeout is `internal` (contracts 12, questions 16 and 17).
fn timeout(limit: Duration) -> TimeoutLayer {
    TimeoutLayer::with_status_code(StatusCode::GATEWAY_TIMEOUT, limit)
}

pub fn cors(origins: &[HeaderValue]) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins.iter().cloned()))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        // Exactly the request headers of frontend 17.
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            HeaderName::from_static("idempotency-key"),
            HeaderName::from_static("last-event-id"),
        ])
        .max_age(Duration::from_secs(600))
}

async fn slow() -> StatusCode {
    tokio::time::sleep(Duration::from_secs(20)).await;
    StatusCode::OK
}

async fn events() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = async_stream::stream! {
        for seq in 1..=3_u64 {
            tokio::time::sleep(Duration::from_secs(4)).await;
            yield Ok(Event::default().id(seq.to_string()).data("{}"));
        }
    };
    Sse::new(stream)
}

async fn claim() -> StatusCode {
    tokio::time::sleep(Duration::from_secs(25)).await;
    StatusCode::NO_CONTENT
}

pub fn app(state: &Arc<AppState>) -> Router {
    let cors = cors(&state.web_origins);
    let app_routes = Router::new()
        .route("/slow", get(slow))
        .route("/scans/{id}/events", get(events))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_user))
        .layer(timeout(REQUEST_TIMEOUT));
    let worker_routes = Router::new()
        .route("/internal/jobs/claim", post(claim))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_worker,
        ))
        .layer(timeout(CLAIM_TIMEOUT));
    let ops_routes = Router::new().route("/healthz", get(|| async { "{\"ok\":true}" }));

    let routes = Router::new()
        .merge(app_routes)
        .merge(worker_routes)
        .merge(ops_routes)
        .fallback(|| async { ApiError::NotFound })
        .layer(
            ServiceBuilder::new()
                .map_request(|mut request: Request| {
                    request.headers_mut().remove("x-request-id");
                    request
                })
                .layer(SetRequestIdLayer::x_request_id(MakeReqId))
                .layer(PropagateRequestIdLayer::x_request_id())
                // rust-tracing configures its span (`make_span_with`, `on_response`).
                .layer(TraceLayer::new_for_http())
                .layer(cors)
                .layer(middleware::from_fn(render_errors))
                .layer(CatchPanicLayer::custom(panic_response))
                .layer(RequestBodyLimitLayer::new(UPLOAD_REQUEST_MAX)),
        );
    without_allow(routes)
}

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
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    const WEB: &str = "https://app.example.org";

    fn router() -> Router {
        app(&Arc::new(AppState {
            web_origins: vec![HeaderValue::from_static(WEB)],
        }))
    }

    fn preflight(origin: &str) -> axum::http::Request<Body> {
        axum::http::Request::builder()
            .method(Method::OPTIONS)
            .uri("/scans/01JAX/events")
            .header(header::ORIGIN, origin)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .header(
                header::ACCESS_CONTROL_REQUEST_HEADERS,
                "authorization,last-event-id",
            )
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn preflight_allows_the_web_origin_and_the_four_headers() {
        let response = router().oneshot(preflight(WEB)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let headers = response.headers();
        assert_eq!(headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], WEB);
        assert_eq!(
            headers[header::ACCESS_CONTROL_ALLOW_HEADERS],
            "authorization,content-type,idempotency-key,last-event-id"
        );
        assert_eq!(headers[header::ACCESS_CONTROL_MAX_AGE], "600");
        assert!(
            headers["x-request-id"]
                .to_str()
                .unwrap()
                .starts_with("req_")
        );
    }

    #[tokio::test]
    async fn other_origins_get_no_allow_origin() {
        let response = router()
            .oneshot(preflight("https://evil.example"))
            .await
            .unwrap();
        assert!(
            !response
                .headers()
                .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        );
    }

    fn get_request(uri: &str) -> axum::http::Request<Body> {
        axum::http::Request::get(uri).body(Body::empty()).unwrap()
    }

    #[tokio::test(start_paused = true)]
    async fn a_slow_handler_is_cut_at_the_timeout_and_rendered_as_internal() {
        let start = tokio::time::Instant::now();
        let response = router().oneshot(get_request("/slow")).await.unwrap();
        assert_eq!(start.elapsed(), REQUEST_TIMEOUT);
        // The layer's 504 reached the renderer, which answers `internal`.
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"]["code"], "internal");
        assert_eq!(body["error"]["retryable"], false);
    }

    #[tokio::test(start_paused = true)]
    async fn the_timeout_does_not_cut_an_sse_body() {
        let start = tokio::time::Instant::now();
        let response = router()
            .oneshot(get_request("/scans/01JAX/events"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        // Three events, the last at 12 s: past the 10 s request timeout.
        assert_eq!(body.windows(4).filter(|w| w == b"id: ").count(), 3);
        assert_eq!(start.elapsed(), Duration::from_secs(12));
    }

    #[tokio::test(start_paused = true)]
    async fn the_claim_long_poll_has_its_own_longer_timeout() {
        let request = axum::http::Request::post("/internal/jobs/claim")
            .body(Body::empty())
            .unwrap();
        let response = router().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn a_wrong_method_is_a_404_without_allow() {
        let request = axum::http::Request::delete("/healthz")
            .body(Body::empty())
            .unwrap();
        let response = router().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(!response.headers().contains_key(header::ALLOW));
    }
}
```

## 3. CORS

The web build may have its own origin; then the API must answer CORS for it, allowing exactly the request headers `Authorization`, `Content-Type`, `Idempotency-Key` and `Last-Event-ID`, and every call is preflighted ([frontend 17](../../../docs/frontend/README.md#17-builds-environments-and-delivery)).

- **Origins** from configuration, as a list: [`AllowOrigin::list`](https://docs.rs/tower-http/0.7.1/tower_http/cors/struct.AllowOrigin.html#method.list). Never `Any` or `mirror_request()`: frontend 17 asks for CORS for the web build's origin only, and an open policy would let any site's scripts read the API's responses (this skill's reading of frontend 17).
- **Headers**: exactly the four, as in the example; the preflight test checks the `Access-Control-Allow-Headers` value. No `allow_credentials`: the app sends a bearer token in `Authorization`, not cookies ([contracts 1, authentication headers](../../../docs/architecture/contracts.md#authentication-headers)).
- **Methods**: `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, the methods in [architecture 9](../../../docs/architecture/README.md#9-api-surface).
- **`max_age`**: 600 s (this skill's choice) caches a preflight for ten minutes; browsers cap it lower or higher on their own ([MDN, `Access-Control-Max-Age`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Access-Control-Max-Age)).
- A disallowed origin gets a response without `Access-Control-Allow-Origin`, and the browser blocks it; the test checks that too.
- tower-http 0.7.0 relaxed the default `Vary` header ([CHANGELOG 0.7.0](https://docs.rs/crate/tower-http/0.7.1/source/CHANGELOG.md), #674); nothing to set for PDM.

Docs: [`CorsLayer`](https://docs.rs/tower-http/0.7.1/tower_http/cors/struct.CorsLayer.html).

## 4. Timeouts

- [`TimeoutLayer`](https://docs.rs/tower-http/0.7.1/tower_http/timeout/struct.TimeoutLayer.html) bounds the time until the response **head** is ready: its `ResponseFuture` races the handler's future against one `sleep` and answers with its status if the sleep wins ([`timeout/service.rs`](https://docs.rs/crate/tower-http/0.7.1/source/src/timeout/service.rs)). Once the handler has returned a response, the body is outside it, so an SSE body is never cut: the `the_timeout_does_not_cut_an_sse_body` test runs 12 s under a 10 s timeout and gets every event. So it is safe on the SSE routes, and still useful there for the work done before the stream starts.
- **Construct it with `TimeoutLayer::with_status_code(StatusCode::GATEWAY_TIMEOUT, …)`.** `TimeoutLayer::new` is deprecated since tower-http 0.6.7, and its default status is `408 Request Timeout` (`timeout/service.rs`). Its response has an empty body; `render_errors` maps `408` and `504` to `500 internal` and logs it with the `request_id` ([error-responses 2](error-responses.md#2-the-compiled-example), `from_status`; the `a_slow_handler_is_cut_at_the_timeout_and_rendered_as_internal` test). A handler that runs past the limit is a bug or a hung dependency, and calls to dependencies (LLM API, weather) have their own shorter timeouts that map to `503 upstream_unavailable` ([rust-errors](../rust-errors/SKILL.md#the-mapping-table)). 504 rather than 500 keeps the log line's cause ("the request timed out") distinct from an unrendered 500. 10 s is this skill's choice. **Not in the contracts yet:** contracts 2 has no timeout code, and `internal` is `retryable: false`, so the app doesn't retry a timed-out request ([contracts 12, questions 16 and 17](../../../docs/architecture/contracts.md#12-open-questions)).
- **The claim long poll** (`POST /internal/jobs/claim`) waits up to `wait_s`, default 25 s, max 30 s ([contracts 5](../../../docs/architecture/contracts.md#post-internaljobsclaim)). Its group gets 35 s. The handler also ends its wait when the shutdown token fires and answers `204` ([rust-workers](../rust-workers/SKILL.md)).
- **Never** put `ResponseBodyTimeoutLayer` or `ResponseBodyDeadlineLayer` (new in 0.7.0) on the SSE routes. [`ResponseBodyTimeoutLayer`](https://docs.rs/tower-http/0.7.1/tower_http/timeout/struct.ResponseBodyTimeoutLayer.html) is an idle timeout: it starts a timer when a frame is polled for and resets it after each frame ([`timeout/body.rs`](https://docs.rs/crate/tower-http/0.7.1/source/src/timeout/body.rs)). The stream sends at least a keepalive every 15 s ([sse 2](sse.md#2-what-axum-writes)), so a timeout over 15 s never fires and adds nothing, and one under 15 s ends a quiet stream with a body error, which aborts the response instead of closing it (the test below). [`ResponseBodyDeadlineLayer`](https://docs.rs/tower-http/0.7.1/tower_http/timeout/struct.ResponseBodyDeadlineLayer.html) is a hard deadline from the response's creation, which cuts every long stream. `RequestBodyTimeoutLayer` on the upload routes is fine.
- tower's own `tower::timeout::TimeoutLayer` returns an *error* instead of a response, which axum then needs `HandleErrorLayer` for ([axum error handling, applying fallible middleware](https://docs.rs/axum/0.8.9/axum/error_handling/index.html#applying-fallible-middleware)); use tower-http's, which answers with a status.

```rust
#[cfg(test)]
mod tests {
    use std::{convert::Infallible, time::Duration};

    use axum::{
        Router,
        body::Body,
        response::sse::{Event, KeepAlive, Sse},
        routing::get,
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use tower_http::timeout::ResponseBodyTimeoutLayer;

    /// A scan with no events for a while: only keepalives go out, every 15 s.
    fn quiet_stream() -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
        Sse::new(futures::stream::pending()).keep_alive(KeepAlive::new().text("keepalive"))
    }

    async fn first_frame_after(body_timeout: Duration) -> Result<String, String> {
        let app = Router::new()
            .route("/events", get(|| async { quiet_stream() }))
            .layer(ResponseBodyTimeoutLayer::new(body_timeout));
        let request = axum::http::Request::get("/events")
            .body(Body::empty())
            .unwrap();
        let mut body = app.oneshot(request).await.unwrap().into_body();
        match body.frame().await.unwrap() {
            Ok(frame) => Ok(String::from_utf8(frame.into_data().unwrap().to_vec()).unwrap()),
            Err(error) => Err(error.to_string()),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_body_timeout_under_the_keepalive_aborts_a_quiet_stream() {
        let error = first_frame_after(Duration::from_secs(10))
            .await
            .unwrap_err();
        assert_eq!(error, "data was not received within the designated timeout");
        // Over the keepalive interval it never fires: the keepalive is the first frame.
        let frame = first_frame_after(Duration::from_secs(20)).await.unwrap();
        assert_eq!(frame, ": keepalive\n\n");
    }
}
```

## 5. Rate limits

[architecture 11](../../../docs/architecture/README.md#11-security-privacy-and-safety): rate limits per user, and invite acceptance is rate-limited. [contracts 9](../../../docs/architecture/contracts.md#members-and-invites) fixes the invite limit at **10 attempts per user per minute**. Every limit answers `429 rate_limited` with `retryable: true` and `retry_after_s` ([contracts 2](../../../docs/architecture/contracts.md#2-errors)).

- One keyed limiter per limit, in `AppState`, keyed by the user's ID: [`RateLimiter::keyed`](https://docs.rs/governor/0.10.4/governor/struct.RateLimiter.html#method.keyed) with a [`Quota`](https://docs.rs/governor/0.10.4/governor/struct.Quota.html).
- **Don't use `Quota::per_minute(n)` for "n a minute".** It allows a burst of `n` and then replenishes one cell every `60 / n` seconds ([`Quota`, burst sizes](https://docs.rs/governor/0.10.4/governor/struct.Quota.html#burst-sizes)), so `per_minute(10)` lets 19 attempts through in the first 60 s (10 at once, then one every 6 s). A burst of `b` with one cell every `p` seconds lets `b` through at once and one more every `p` seconds after, so "at most `n` a minute" is a burst of `n / 2` and a period of `120 / n` seconds (`at_most_per_minute` below; this skill's choice of split). Invites get [`Quota::with_period`](https://docs.rs/governor/0.10.4/governor/struct.Quota.html#method.with_period)`(12 s)` with [`allow_burst`](https://docs.rs/governor/0.10.4/governor/struct.Quota.html#method.allow_burst)`(5)`: a client trying every 100 ms gets 9 attempts in any 60 s and the 10th exactly at the 60 s mark, then one every 12 s. The test drives the limiter with governor's [`FakeRelativeClock`](https://docs.rs/governor/0.10.4/governor/clock/struct.FakeRelativeClock.html) and checks every window.
- The general per-user limit runs as a middleware **after** auth (it needs the user). The invite limit is a `route_layer` on `POST /invites/{code}/accept` only ([section 1](#1-how-order-works)).
- [`check_key`](https://docs.rs/governor/0.10.4/governor/struct.RateLimiter.html#method.check_key) returns `Err(NotUntil)`; [`NotUntil::wait_time_from(clock.now())`](https://docs.rs/governor/0.10.4/governor/struct.NotUntil.html#method.wait_time_from) gives the wait. Round **up** to whole seconds, so a client that waits `retry_after_s` is allowed (the test retries after exactly that many seconds).
- A keyed limiter keeps one entry per key forever unless pruned: call [`retain_recent`](https://docs.rs/governor/0.10.4/governor/struct.RateLimiter.html#method.retain_recent) from a background task (every minute, in the `TaskTracker`; [rust-async](../rust-async/SKILL.md#3-tasks)).
- The state is in memory, which is right for the single `api` instance ([architecture 12, operations](../../../docs/architecture/README.md#operations)).
- The general limit's numbers aren't in the docs. 120 requests a minute below is a placeholder to agree with the team. The daily scan limit and chat token budget are not rate limits: they are counters in the database answering `budget_exceeded` ([database 4](../../../docs/database/README.md#limits-and-counters-)).

```rust
use std::{num::NonZeroU32, time::Duration};

use governor::{
    Quota, RateLimiter,
    clock::{Clock, DefaultClock},
    middleware::NoOpMiddleware,
    state::keyed::DefaultKeyedStateStore,
};

/// A keyed limiter on any clock, so tests can use governor's fake one.
pub type UserLimiter<C = DefaultClock> =
    RateLimiter<String, DefaultKeyedStateStore<String>, C, NoOpMiddleware<<C as Clock>::Instant>>;

/// At most `per_minute` cells in any 60 s: a burst of half, one cell every `120 / n` s.
pub fn at_most_per_minute(per_minute: u32) -> Quota {
    let burst = NonZeroU32::new(per_minute / 2).expect("per_minute is at least 2");
    let period = Duration::from_secs(120) / per_minute;
    Quota::with_period(period)
        .expect("the period is not zero")
        .allow_burst(burst)
}

/// contracts 9: 10 invite-code attempts per user per minute.
pub const INVITE_ATTEMPTS_PER_MINUTE: u32 = 10;
/// Placeholder: the docs give no general per-user number.
pub const REQUESTS_PER_MINUTE: u32 = 120;

pub struct RateLimits {
    pub per_user: UserLimiter,
    pub invite_accept: UserLimiter,
}

impl Default for RateLimits {
    fn default() -> Self {
        Self {
            per_user: RateLimiter::keyed(at_most_per_minute(REQUESTS_PER_MINUTE)),
            invite_accept: RateLimiter::keyed(at_most_per_minute(INVITE_ATTEMPTS_PER_MINUTE)),
        }
    }
}

/// `Err(retry_after_s)` becomes `ApiError::RateLimited { retry_after_s }`.
pub fn check<C: Clock>(limiter: &UserLimiter<C>, user_id: &str) -> Result<(), u64> {
    limiter.check_key(&user_id.to_owned()).map_err(|not_until| {
        let wait = not_until.wait_time_from(limiter.clock().now());
        // Round up, and never tell a client to retry after 0 s.
        (wait.as_secs() + u64::from(wait.subsec_nanos() > 0)).max(1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use governor::clock::FakeRelativeClock;

    fn invite_limiter(clock: &FakeRelativeClock) -> UserLimiter<FakeRelativeClock> {
        let quota = at_most_per_minute(INVITE_ATTEMPTS_PER_MINUTE);
        RateLimiter::dashmap_with_clock(quota, clock.clone())
    }

    #[test]
    fn no_60_second_window_lets_more_than_10_invite_attempts_through() {
        let clock = FakeRelativeClock::default();
        let limiter = invite_limiter(&clock);
        // A client that tries every 100 ms for 5 minutes.
        let mut allowed_at_ms = Vec::new();
        for tick in 0..3000_u64 {
            if check(&limiter, "user_a").is_ok() {
                allowed_at_ms.push(tick * 100);
            }
            clock.advance(Duration::from_millis(100));
        }
        // The most attempts let through in a 60 s window that starts at an allowed
        // attempt, where the count peaks: `closed` counts one at the window's end too.
        let most_in = |closed: bool| {
            (0..allowed_at_ms.len())
                .map(|i| {
                    let end = allowed_at_ms[i] + 60_000;
                    allowed_at_ms[i..]
                        .iter()
                        .take_while(|&&t| t < end || (closed && t == end))
                        .count()
                })
                .max()
        };
        assert_eq!(most_in(false), Some(9));
        assert_eq!(most_in(true), Some(10));
        // After the first burst, a steady client gets one attempt every 12 s.
        let last_minute = allowed_at_ms.iter().filter(|&&t| t >= 240_000).count();
        assert_eq!(last_minute, 5);
    }

    #[test]
    fn per_minute_10_lets_19_attempts_through_in_the_first_minute() {
        let clock = FakeRelativeClock::default();
        let quota = Quota::per_minute(NonZeroU32::new(10).unwrap());
        let limiter: UserLimiter<FakeRelativeClock> =
            RateLimiter::dashmap_with_clock(quota, clock.clone());
        let mut allowed = 0;
        for _ in 0..600 {
            allowed += usize::from(check(&limiter, "user_a").is_ok());
            clock.advance(Duration::from_millis(100));
        }
        assert_eq!(allowed, 19);
    }

    #[test]
    fn a_refused_attempt_says_how_long_to_wait_and_that_wait_is_enough() {
        let clock = FakeRelativeClock::default();
        let limiter = invite_limiter(&clock);
        for _ in 0..5 {
            assert_eq!(check(&limiter, "user_a"), Ok(()));
        }
        // One cell comes back every 12 s.
        let wait = check(&limiter, "user_a").unwrap_err();
        assert_eq!(wait, 12);
        // Keys are separate: another user is not affected.
        assert_eq!(check(&limiter, "user_b"), Ok(()));
        clock.advance(Duration::from_secs(wait));
        assert_eq!(check(&limiter, "user_a"), Ok(()));
    }
}
```

The middleware around `check` reads the user from the request's extensions (put there by the auth layer), calls `check(&state.limits.per_user, &user.id)`, and on `Err(retry_after_s)` returns `ApiError::RateLimited { retry_after_s }`, which renders as in [error-responses 2](error-responses.md#2-the-compiled-example).

## 6. Graceful shutdown

The process has one `CancellationToken` and one `TaskTracker`; their semantics, the signal handling and the drain deadline are in [rust-async shutdown](../rust-async/shutdown.md#5-api). For the HTTP server:

- `axum::serve(listener, app).with_graceful_shutdown(token.clone().cancelled_owned())`. After the signal it stops accepting, then **waits for every open connection to finish** ([`serve/mod.rs`](https://docs.rs/crate/axum/0.8.9/source/src/serve/mod.rs): `close_tx.closed().await`); it returns only then ([`Serve::with_graceful_shutdown`](https://docs.rs/axum/0.8.9/axum/serve/struct.Serve.html#method.with_graceful_shutdown)).
- So every long response must end on the token: the SSE streams ([sse 1](sse.md#1-what-the-endpoint-does)) and the claim long poll. The second test below shows a stream that ignores the token keeping the server up.
- After `serve` returns: `tracker.close()`, then `tracker.wait()` for the background tasks (scheduler, sweeps, `retain_recent`), each of which also watches the token ([rust-async shutdown 3](../rust-async/shutdown.md#3-tasktracker)).

```rust
use std::{convert::Infallible, time::Duration};

use axum::{
    Router,
    response::sse::{Event, Sse},
    routing::get,
};
use futures::{Stream, StreamExt};
use tokio_util::sync::CancellationToken;

fn events_until(token: CancellationToken) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let first = futures::stream::once(async { Ok(Event::default().data("hello")) });
    // Ends when the token fires: `take_until` drops the rest of the stream.
    let rest = futures::stream::pending().take_until(token.cancelled_owned());
    Sse::new(first.chain(rest))
}

fn events_forever() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let first = futures::stream::once(async { Ok(Event::default().data("hello")) });
    Sse::new(first.chain(futures::stream::pending()))
}

pub async fn serve(
    listener: tokio::net::TcpListener,
    app: Router,
    token: CancellationToken,
) -> std::io::Result<()> {
    axum::serve(listener, app)
        .with_graceful_shutdown(token.cancelled_owned())
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Starts the server, opens one SSE stream, reads its first event, then cancels.
    async fn shutdown_with_open_stream(app: Router, token: CancellationToken) -> bool {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/events", listener.local_addr().unwrap());
        let server = tokio::spawn(serve(listener, app, token.clone()));
        let mut response = reqwest::get(&url).await.unwrap();
        let first = response.chunk().await.unwrap().unwrap();
        assert_eq!(&first[..], b"data: hello\n\n");
        token.cancel();
        let finished = tokio::time::timeout(Duration::from_secs(2), server).await;
        drop(response);
        finished.is_ok()
    }

    #[tokio::test]
    async fn streams_that_watch_the_token_let_the_server_stop() {
        let token = CancellationToken::new();
        let stream_token = token.clone();
        let app = Router::new().route(
            "/events",
            get(move || async move { events_until(stream_token) }),
        );
        assert!(shutdown_with_open_stream(app, token).await);
    }

    #[tokio::test]
    async fn a_stream_that_ignores_the_token_holds_the_server_up() {
        let token = CancellationToken::new();
        let app = Router::new().route("/events", get(|| async { events_forever() }));
        assert!(!shutdown_with_open_stream(app, token).await);
    }
}
```
