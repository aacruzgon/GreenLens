//! `ApiError` → `ErrorBody` ([contracts 2]). Every non-2xx response carries the contract body,
//! whatever produced it: a handler, an extractor rejection, a panic, a body limit, a timeout or
//! axum's own 404 and 405.
//!
//! `IntoResponse` can't see the request, so it can't know the `request_id`. The body is
//! therefore written in one place: [`ApiError::into_response`] leaves the status and an
//! [`ErrorParts`] value in the response's extensions, and the [`render_errors`] middleware, which
//! reads the request's `RequestId` on the way in, writes the JSON on the way out. Responses no
//! `ApiError` made get a body chosen from their status ([`from_status`]).

use std::{any::Any, borrow::Cow, fmt};

use axum::{
    body::Body,
    extract::Request,
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use tower_http::request_id::RequestId;

use domain::{
    authz::AccessError,
    error::{
        DomainError, ErrorBody, ErrorCode, ErrorKind, ErrorObject, InvalidRequestDetails,
        InvalidStateDetails, QualityProblem,
    },
    id::JobId,
    ports::{AuthError, BlobError, BoxError, Conflict, ModelError, ProviderError, StoreError},
    rules::uploads::{ACCEPTED_MEDIA_TYPES, IMAGE_MAX_BYTES},
};

use crate::seams::{PipelineError, TurnError, UploadError};

/// An error response: a contracts 2 code with its details, a message safe to show, `retryable`,
/// `retry_after_s`, and, for the log only, the cause. Boxed, so a `Result<T, ApiError>` stays
/// one pointer wider than `T` however large the details get.
pub struct ApiError(Box<Inner>);

struct Inner {
    kind: ErrorKind,
    message: Cow<'static, str>,
    retryable: bool,
    retry_after_s: Option<u32>,
    cause: Option<BoxError>,
}

impl fmt::Debug for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiError")
            .field("code", &self.0.kind.code())
            .field("message", &self.0.message)
            .field("cause", &self.0.cause.as_ref().map(ToString::to_string))
            .finish_non_exhaustive()
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.0.kind.code(), self.0.message)
    }
}

/// What [`render_errors`] needs to write the body, left in the response's extensions.
#[derive(Debug, Clone)]
pub struct ErrorParts {
    kind: ErrorKind,
    message: Cow<'static, str>,
    retryable: bool,
    retry_after_s: Option<u32>,
    /// The cause chain, rendered once, for the log line of a `5xx`.
    cause: Option<String>,
}

/// The default message of each code: plain English, safe to show ([contracts 2]).
pub const fn default_message(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::InvalidRequest => "The request is not valid.",
        ErrorCode::Unauthenticated => "Please sign in again.",
        ErrorCode::Forbidden => "Your role in this garden does not allow this.",
        ErrorCode::InvalidSignature => "This link has expired.",
        ErrorCode::NotFound => "Not found.",
        ErrorCode::InvalidState => "This can't be done right now.",
        ErrorCode::RequestInProgress => "The same request is still running.",
        ErrorCode::LeaseLost => "The job's lease is gone.",
        ErrorCode::UploadTooLarge => "This photo is too large.",
        ErrorCode::UnsupportedMediaType => "Use a JPEG, PNG, WebP or HEIC photo.",
        ErrorCode::ImageQuality => "This photo can't be used. Please take it again.",
        ErrorCode::MissingRequiredShot => "A whole-plant photo is needed first.",
        ErrorCode::TooManyImages => "There are already enough photos of this kind.",
        ErrorCode::IdempotencyConflict => "This request key was used for a different request.",
        ErrorCode::RateLimited => "Too many requests. Please wait a moment.",
        ErrorCode::BudgetExceeded => "The daily limit is used up.",
        ErrorCode::Internal => "Something went wrong on our side.",
        ErrorCode::ProcessingUnavailable => "The photo couldn't be processed. Please try again.",
        ErrorCode::UpstreamUnavailable => "A service the app needs is down. Please try again.",
    }
}

const fn quality_message(problem: QualityProblem) -> &'static str {
    match problem {
        QualityProblem::TooBlurry => {
            "This photo is too blurry. Hold the phone still and tap the leaf to focus."
        }
        QualityProblem::TooDark => "This photo is too dark. Move into better light.",
        QualityProblem::TooBright => {
            "This photo is too bright. Shade the plant or step out of direct sun."
        }
    }
}

impl ApiError {
    /// An error with the code's default message and `retryable`.
    pub fn new(kind: ErrorKind) -> Self {
        let code = kind.code();
        Self(Box::new(Inner {
            message: Cow::Borrowed(default_message(code)),
            retryable: code.default_retryable(),
            retry_after_s: None,
            cause: None,
            kind,
        }))
    }

    /// Replaces the message.
    #[must_use]
    pub fn message(mut self, message: impl Into<Cow<'static, str>>) -> Self {
        self.0.message = message.into();
        self
    }

    /// Sets `retry_after_s` (only `request_in_progress`, `rate_limited` and the 503s carry it).
    #[must_use]
    pub fn retry_after(mut self, seconds: u32) -> Self {
        self.0.retry_after_s = Some(seconds.max(1));
        self
    }

    /// Keeps the cause for the log; it is never sent.
    #[must_use]
    pub fn cause(mut self, cause: impl Into<BoxError>) -> Self {
        self.0.cause = Some(cause.into());
        self
    }

    /// `404 not_found`.
    pub fn not_found() -> Self {
        Self::new(ErrorKind::NotFound)
    }

    /// `400 invalid_request` with `{ field, problem }`.
    pub fn invalid_request(field: impl Into<String>, problem: impl Into<String>) -> Self {
        let problem = problem.into();
        Self::new(ErrorKind::InvalidRequest(InvalidRequestDetails {
            field: field.into(),
            problem: problem.clone(),
            plant_ids: None,
        }))
        .message(problem)
    }

    /// `409 invalid_state` with details.
    pub fn invalid_state(details: InvalidStateDetails) -> Self {
        Self::new(ErrorKind::InvalidState(details))
    }

    /// `500 internal`, keeping the cause for the log.
    pub fn internal(cause: impl Into<BoxError>) -> Self {
        Self::new(ErrorKind::Internal).cause(cause)
    }

    /// `503 upstream_unavailable` with `{ upstream }`.
    pub fn upstream(upstream: &str, cause: impl Into<BoxError>) -> Self {
        Self::new(ErrorKind::UpstreamUnavailable {
            upstream: upstream.to_owned(),
        })
        .retry_after(5)
        .cause(cause)
    }

    /// `429 rate_limited`, `retryable: true`.
    pub fn rate_limited(retry_after_s: u32) -> Self {
        Self::new(ErrorKind::RateLimited).retry_after(retry_after_s)
    }

    /// `409 lease_lost` with `{ job_id }`.
    pub fn lease_lost(job_id: JobId) -> Self {
        Self::new(ErrorKind::LeaseLost { job_id })
    }

    /// `401 unauthenticated`.
    pub fn unauthenticated() -> Self {
        Self::new(ErrorKind::Unauthenticated)
    }

    /// The code.
    pub fn code(&self) -> ErrorCode {
        self.0.kind.code()
    }

    /// The HTTP status.
    pub fn status(&self) -> StatusCode {
        StatusCode::from_u16(self.code().http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }

    /// The contract body with a `request_id`: what [`render_errors`] writes, and what the
    /// idempotency layer stores for a deterministic `4xx`.
    pub fn body(&self, request_id: &str) -> ErrorBody {
        ErrorBody {
            error: ErrorObject {
                kind: self.0.kind.clone(),
                message: self.0.message.to_string(),
                retryable: self.0.retryable,
                retry_after_s: self.0.retry_after_s,
                request_id: request_id.to_owned(),
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let inner = *self.0;
        let mut response = status.into_response();
        if let Some(seconds) = inner.retry_after_s {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from(seconds));
        }
        response.extensions_mut().insert(ErrorParts {
            cause: inner.cause.as_deref().map(|e| chain(e)),
            kind: inner.kind,
            message: inner.message,
            retryable: inner.retryable,
            retry_after_s: inner.retry_after_s,
        });
        response
    }
}

/// Renders an error's `source()` chain on one line: `a: b: c`.
fn chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut out = error.to_string();
    let mut next = error.source();
    while let Some(e) = next {
        out.push_str(": ");
        out.push_str(&e.to_string());
        next = e.source();
    }
    out
}

/// The middleware that writes every error body. It sits inside the request-id layers and the
/// trace span, and outside everything that can answer with an error.
pub async fn render_errors(request: Request, next: Next) -> Response {
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .and_then(|id| id.header_value().to_str().ok())
        .unwrap_or("req_unknown")
        .to_owned();
    let mut response = next.run(request).await;
    let parts = match response.extensions_mut().remove::<ErrorParts>() {
        Some(parts) => parts,
        None if response.status().as_u16() < 400 || is_json(&response) => return response,
        None => {
            // axum's 404/405 and rejections, tower-http's 413 and timeouts.
            let error = from_status(response.status());
            *response.status_mut() = error.status();
            response.headers_mut().remove(header::ALLOW);
            let inner = *error.0;
            ErrorParts {
                cause: None,
                kind: inner.kind,
                message: inner.message,
                retryable: inner.retryable,
                retry_after_s: inner.retry_after_s,
            }
        }
    };
    if let Some(cause) = &parts.cause {
        // contracts 2: `internal` is logged with request_id, as a field of the event itself.
        if parts.kind.code() == ErrorCode::Internal {
            tracing::error!(request_id, error = %cause, "internal error");
        } else {
            tracing::warn!(request_id, code = %parts.kind.code(), error = %cause, "request failed");
        }
    }
    let body = ErrorBody {
        error: ErrorObject {
            kind: parts.kind,
            message: parts.message.into_owned(),
            retryable: parts.retryable,
            retry_after_s: parts.retry_after_s,
            request_id,
        },
    };
    let bytes = serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec());
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    // Here, not only in `into_response`, so the errors built from a bare status (the timeout's
    // 503) carry the header too.
    if let Some(seconds) = body.error.retry_after_s {
        headers.insert(header::RETRY_AFTER, HeaderValue::from(seconds));
    }
    headers.remove(header::CONTENT_LENGTH);
    *response.body_mut() = Body::from(bytes);
    response
}

fn is_json(response: &Response) -> bool {
    response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|v| v.as_bytes().starts_with(b"application/json"))
}

/// The error for a response no `ApiError` made, chosen from its status.
pub fn from_status(status: StatusCode) -> ApiError {
    match status.as_u16() {
        401 => ApiError::unauthenticated(),
        // An unknown path and a known path with the wrong method both look like an unknown ID.
        404 | 405 => ApiError::not_found(),
        413 => ApiError::new(ErrorKind::UploadTooLarge {
            max_bytes: IMAGE_MAX_BYTES,
        }),
        415 => ApiError::new(ErrorKind::UnsupportedMediaType {
            accepted: ACCEPTED_MEDIA_TYPES
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
        }),
        400 | 422 => ApiError::invalid_request("body", "the request body could not be read"),
        // Only the timeout layer answers 503 without an `ApiError` (contracts 2, timeouts).
        503 => ApiError::new(ErrorKind::UpstreamUnavailable {
            upstream: "api".to_owned(),
        })
        .message("The request took too long. Please try again.")
        .retry_after(5),
        _ => ApiError::new(ErrorKind::Internal),
    }
}

/// `CatchPanicLayer::custom`: a panic becomes `500 internal`, rendered by [`render_errors`].
#[expect(
    clippy::needless_pass_by_value,
    reason = "CatchPanicLayer::custom passes the payload by value"
)]
pub fn panic_response(panic: Box<dyn Any + Send + 'static>) -> Response {
    let what = panic
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic".to_owned());
    ApiError::internal(format!("handler panicked: {what}")).into_response()
}

// ---------------------------------------------------------------------------------------------
// Conversions: every lower error to one contract code (the mapping table of api.md section 7)
// ---------------------------------------------------------------------------------------------

impl From<DomainError> for ApiError {
    fn from(e: DomainError) -> Self {
        let retry = e.retry_after_s();
        let message = match &e {
            DomainError::InvalidRequest { problem, .. } => Some(problem.clone()),
            _ => None,
        };
        let mut error = Self::new(e.kind());
        if let Some(message) = message {
            error = error.message(message);
        }
        if let Some(seconds) = retry {
            error = error.retry_after(seconds);
        }
        error
    }
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::NotFound => Self::not_found(),
            StoreError::Rejected(rule) => rule.into(),
            StoreError::Conflict(conflict) => match conflict {
                Conflict::StalePlan { current } => Self::invalid_state(InvalidStateDetails {
                    current_plan_id: current,
                    ..InvalidStateDetails::default()
                }),
                Conflict::StaleMap { current_version } => {
                    Self::invalid_state(InvalidStateDetails {
                        current_map_version: Some(current_version),
                        ..InvalidStateDetails::default()
                    })
                }
                Conflict::LeaseLost { job_id } => Self::lease_lost(job_id),
                Conflict::AlreadyExists | Conflict::ClaimLost | Conflict::QuestionStepLost => {
                    Self::invalid_state(InvalidStateDetails::default())
                }
                Conflict::Retries => Self::new(ErrorKind::UpstreamUnavailable {
                    upstream: "database".to_owned(),
                })
                .retry_after(1),
            },
            StoreError::Unavailable(cause) => Self::upstream("database", cause),
            e @ StoreError::Query { .. } => Self::internal(e),
        }
    }
}

impl From<BlobError> for ApiError {
    fn from(e: BlobError) -> Self {
        match e {
            BlobError::NotFound => Self::not_found(),
            BlobError::Unavailable(cause) => Self::upstream("storage", cause),
            e @ (BlobError::AlreadyExists | BlobError::Rejected { .. }) => Self::internal(e),
        }
    }
}

impl From<ProviderError> for ApiError {
    fn from(e: ProviderError) -> Self {
        let upstream = e.provider();
        match e {
            ProviderError::RateLimited { retry_after_s, .. } => {
                let error = Self::upstream(upstream, e);
                match retry_after_s {
                    Some(s) => error.retry_after(s),
                    None => error,
                }
            }
            other => Self::upstream(upstream, other),
        }
    }
}

impl From<ModelError> for ApiError {
    fn from(e: ModelError) -> Self {
        match e {
            ModelError::Unavailable { model, .. } => Self::upstream(model, e),
            other => Self::internal(other),
        }
    }
}

impl From<AuthError> for ApiError {
    fn from(e: AuthError) -> Self {
        match e {
            AuthError::InvalidToken(_) | AuthError::InvalidWebhook | AuthError::AccountDeleted => {
                Self::unauthenticated().cause(e)
            }
            AuthError::Unavailable(cause) => Self::upstream("auth", cause),
        }
    }
}

impl From<AccessError> for ApiError {
    fn from(e: AccessError) -> Self {
        match e {
            AccessError::NotFound => Self::not_found(),
            AccessError::Forbidden { required } => Self::new(ErrorKind::Forbidden {
                required_role: required,
            }),
            AccessError::Unauthenticated(auth) => auth.into(),
            AccessError::Store(store) => store.into(),
        }
    }
}

impl From<PipelineError> for ApiError {
    fn from(e: PipelineError) -> Self {
        match e {
            PipelineError::Store(e) => e.into(),
            PipelineError::Domain(e) => e.into(),
            PipelineError::Blob(e) => e.into(),
            PipelineError::Model(e) => e.into(),
            PipelineError::Provider(e) => e.into(),
            e @ PipelineError::LabelsNotLoaded { .. } => Self::upstream("perception", e),
            e @ (PipelineError::PlanWriter(_)
            | PipelineError::Join(_)
            | PipelineError::Inconsistent(_)) => Self::internal(e),
        }
    }
}

impl From<TurnError> for ApiError {
    fn from(e: TurnError) -> Self {
        match e {
            TurnError::Domain(e) => e.into(),
            TurnError::Store(e) => e.into(),
        }
    }
}

impl From<UploadError> for ApiError {
    fn from(e: UploadError) -> Self {
        match e {
            UploadError::Quality {
                problem,
                blur_score,
                exposure,
            } => Self::new(ErrorKind::ImageQuality {
                problem,
                blur_score,
                exposure,
                hint: problem.hint(),
            })
            .message(quality_message(problem)),
            e @ UploadError::UnsupportedMedia(_) => Self::new(ErrorKind::UnsupportedMediaType {
                accepted: ACCEPTED_MEDIA_TYPES
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect(),
            })
            .cause(e),
            e @ (UploadError::FaceBlur(_) | UploadError::Encode(_)) => {
                Self::new(ErrorKind::ProcessingUnavailable)
                    .retry_after(2)
                    .cause(e)
            }
        }
    }
}

impl From<tokio::task::JoinError> for ApiError {
    fn from(e: tokio::task::JoinError) -> Self {
        Self::internal(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{error::Budget, wire::Role};

    #[tokio::test]
    async fn a_timeout_503_carries_retry_after_like_any_other() {
        use tower::ServiceExt as _;
        // What `TimeoutLayer::with_status_code` answers: a bare 503 that no `ApiError` made.
        let app = axum::Router::new()
            .route(
                "/slow",
                axum::routing::get(|| async { StatusCode::SERVICE_UNAVAILABLE }),
            )
            .layer(axum::middleware::from_fn(render_errors));
        let response = app
            .oneshot(
                axum::http::Request::get("/slow")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()[header::RETRY_AFTER], "5");
    }

    #[test]
    fn every_code_keeps_its_status_and_retryable() {
        let samples = [
            ApiError::not_found(),
            ApiError::rate_limited(3),
            ApiError::new(ErrorKind::BudgetExceeded {
                budget: Budget::Scans,
                resets_at: "2026-10-19T00:00:00Z".parse().unwrap(),
            }),
            ApiError::new(ErrorKind::Forbidden {
                required_role: Role::Editor,
            }),
        ];
        let statuses: Vec<u16> = samples.iter().map(|e| e.status().as_u16()).collect();
        assert_eq!(statuses, [404, 429, 429, 403]);
        assert!(samples[1].0.retryable && !samples[2].0.retryable);
        let body = serde_json::to_value(samples[1].body("req_1")).unwrap();
        assert_eq!(body["error"]["retry_after_s"], 3);
        assert_eq!(body["error"]["code"], "rate_limited");
    }

    #[test]
    fn store_conflicts_map_to_their_codes() {
        let job: JobId = "01JAX3K9T6R2B8V0QZ4M7D1E5J".parse().unwrap();
        assert_eq!(
            ApiError::from(StoreError::Conflict(Conflict::LeaseLost { job_id: job })).code(),
            ErrorCode::LeaseLost
        );
        let stale = ApiError::from(StoreError::Conflict(Conflict::StaleMap {
            current_version: 4,
        }));
        let body = serde_json::to_value(stale.body("req_1")).unwrap();
        assert_eq!(body["error"]["details"]["current_map_version"], 4);
        assert_eq!(
            ApiError::from(StoreError::Rejected(DomainError::NotFound)).code(),
            ErrorCode::NotFound
        );
    }

    #[test]
    fn unknown_statuses_fall_back_to_a_contract_code() {
        assert_eq!(
            from_status(StatusCode::METHOD_NOT_ALLOWED).code(),
            ErrorCode::NotFound
        );
        assert_eq!(
            from_status(StatusCode::PAYLOAD_TOO_LARGE).code(),
            ErrorCode::UploadTooLarge
        );
        assert_eq!(
            from_status(StatusCode::REQUEST_TIMEOUT).code(),
            ErrorCode::Internal
        );
        let timeout = from_status(StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(timeout.code(), ErrorCode::UpstreamUnavailable);
        assert!(timeout.0.retryable);
    }
}
