//! Errors ([contracts 2]): the codes, their HTTP status and default `retryable` as data, the
//! response body with typed `details`, and [`DomainError`], the rule failures every crate
//! raises. HTTP is a `u16` here: only `api` turns it into an axum response.

use std::fmt;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    id::{JobId, PlanId, PlantId},
    rules::scan_state::TransitionError,
    scan::ImageKind,
    wire::{Role, Timestamp},
};

/// Every error code of contracts 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// 400: malformed JSON, missing or wrong-typed field.
    InvalidRequest,
    /// 401: missing, expired or invalid JWT.
    Unauthenticated,
    /// 403: a member whose role is too low.
    Forbidden,
    /// 403: expired or tampered signed image URL.
    InvalidSignature,
    /// 404: unknown id, or one the caller may not see.
    NotFound,
    /// 409: the record isn't in a state that allows this call.
    InvalidState,
    /// 409: same `Idempotency-Key` while the first request still runs.
    RequestInProgress,
    /// 409: worker call for a job whose lease is gone.
    LeaseLost,
    /// 413: image over 12 MB.
    UploadTooLarge,
    /// 415: not JPEG, PNG, WebP or HEIC, or an undecodable HEIC.
    UnsupportedMediaType,
    /// 422: the blur or exposure check failed; the image was not stored.
    ImageQuality,
    /// 422: submit without a `whole_plant` image.
    MissingRequiredShot,
    /// 422: kind limit reached.
    TooManyImages,
    /// 422: reused `Idempotency-Key` with a different body.
    IdempotencyConflict,
    /// 429: request rate limit, including invite-code attempts.
    RateLimited,
    /// 429: daily chat token budget or daily scan limit used up.
    BudgetExceeded,
    /// 500: a bug.
    Internal,
    /// 503: face blurring or another required upload step failed; nothing stored.
    ProcessingUnavailable,
    /// 503: LLM API, weather or another dependency down.
    UpstreamUnavailable,
}

impl ErrorCode {
    /// Every code, in the order of contracts 2.
    pub const ALL: [Self; 19] = [
        Self::InvalidRequest,
        Self::Unauthenticated,
        Self::Forbidden,
        Self::InvalidSignature,
        Self::NotFound,
        Self::InvalidState,
        Self::RequestInProgress,
        Self::LeaseLost,
        Self::UploadTooLarge,
        Self::UnsupportedMediaType,
        Self::ImageQuality,
        Self::MissingRequiredShot,
        Self::TooManyImages,
        Self::IdempotencyConflict,
        Self::RateLimited,
        Self::BudgetExceeded,
        Self::Internal,
        Self::ProcessingUnavailable,
        Self::UpstreamUnavailable,
    ];

    /// The HTTP status of contracts 2.
    pub const fn http_status(self) -> u16 {
        match self {
            Self::InvalidRequest => 400,
            Self::Unauthenticated => 401,
            Self::Forbidden | Self::InvalidSignature => 403,
            Self::NotFound => 404,
            Self::InvalidState | Self::RequestInProgress | Self::LeaseLost => 409,
            Self::UploadTooLarge => 413,
            Self::UnsupportedMediaType => 415,
            Self::ImageQuality
            | Self::MissingRequiredShot
            | Self::TooManyImages
            | Self::IdempotencyConflict => 422,
            Self::RateLimited | Self::BudgetExceeded => 429,
            Self::Internal => 500,
            Self::ProcessingUnavailable | Self::UpstreamUnavailable => 503,
        }
    }

    /// The default `retryable`: whether the same request may succeed later. `true` for
    /// `request_in_progress`, `rate_limited` and both `503`s (contracts 2), and for
    /// `image_quality` as in contracts 2's example (a retake succeeds). `budget_exceeded` is
    /// `false`: it lasts until `resets_at`.
    pub const fn default_retryable(self) -> bool {
        matches!(
            self,
            Self::RequestInProgress
                | Self::RateLimited
                | Self::ImageQuality
                | Self::ProcessingUnavailable
                | Self::UpstreamUnavailable
        )
    }

    /// Whether a response with this code is stored under its idempotency key ([contracts 1]:
    /// `2xx` and the deterministic `400`, `403`, `404`, `413`, `415`, `422`).
    pub const fn is_stored_for_idempotency(self) -> bool {
        matches!(self.http_status(), 400 | 403 | 404 | 413 | 415 | 422)
    }

    /// The wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::Unauthenticated => "unauthenticated",
            Self::Forbidden => "forbidden",
            Self::InvalidSignature => "invalid_signature",
            Self::NotFound => "not_found",
            Self::InvalidState => "invalid_state",
            Self::RequestInProgress => "request_in_progress",
            Self::LeaseLost => "lease_lost",
            Self::UploadTooLarge => "upload_too_large",
            Self::UnsupportedMediaType => "unsupported_media_type",
            Self::ImageQuality => "image_quality",
            Self::MissingRequiredShot => "missing_required_shot",
            Self::TooManyImages => "too_many_images",
            Self::IdempotencyConflict => "idempotency_conflict",
            Self::RateLimited => "rate_limited",
            Self::BudgetExceeded => "budget_exceeded",
            Self::Internal => "internal",
            Self::ProcessingUnavailable => "processing_unavailable",
            Self::UpstreamUnavailable => "upstream_unavailable",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One hint per `image_quality` problem, for the app's retake prompt ([contracts 2]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum QualityHint {
    /// `too_blurry`: hold the phone still and tap the subject to focus.
    TapToFocus,
    /// `too_dark`: move into better light or turn the plant to face it.
    MoreLight,
    /// `too_bright`: shade the plant or step out of direct sun.
    AvoidGlare,
}

/// `details.problem` of `image_quality`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum QualityProblem {
    /// The blur score is under the threshold.
    TooBlurry,
    /// Underexposed.
    TooDark,
    /// Overexposed.
    TooBright,
}

impl QualityProblem {
    /// The one hint for each problem.
    pub const fn hint(self) -> QualityHint {
        match self {
            Self::TooBlurry => QualityHint::TapToFocus,
            Self::TooDark => QualityHint::MoreLight,
            Self::TooBright => QualityHint::AvoidGlare,
        }
    }
}

/// `details.hint` of `invalid_state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InvalidStateHint {
    /// Submit of a scan that failed with `no_plant_found` or `photos_unusable`.
    NewScan,
    /// Confirming a plan revision whose step names a deleted neighbour.
    NeighbourRemoved,
    /// Confirming a `create_task` whose plant was deleted.
    PlantRemoved,
}

/// `details.budget` of `budget_exceeded`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Budget {
    /// The daily chat token budget.
    ChatTokens,
    /// The daily scan limit (30 per user).
    Scans,
}

/// `details` of `invalid_request`. `plant_ids` is set only for the map upload's
/// `problem: "unknown_plant"` ([contracts 9], garden map).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct InvalidRequestDetails {
    /// The field at fault, as a dotted path (`"location.lat"`, `"pins"`).
    pub field: String,
    /// What is wrong with it.
    pub problem: String,
    /// Every pinned plant that isn't a plant of this garden.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plant_ids: Option<Vec<PlantId>>,
}

/// `details` of `invalid_state`. Every field is optional; the record's state is reported as a
/// string because the record may be a scan, question, plan, task, invite, pending action or map.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct InvalidStateDetails {
    /// The record's current state, such as `"planned"` or `"rejected"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub status: Option<String>,
    /// The states that would allow the call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub allowed: Option<Vec<String>>,
    /// A plan revision's base is no longer current.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub current_plan_id: Option<PlanId>,
    /// A map upload's `base_version` is stale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub current_map_version: Option<u32>,
    /// What the app should offer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub hint: Option<InvalidStateHint>,
}

/// The `code` and `details` pair of an error body, tagged as contracts 2 lays them out: `code`
/// beside the other fields, and `details` only for the codes that define it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "code", content = "details", rename_all = "snake_case")]
pub enum ErrorKind {
    /// 400 `{ field, problem }`.
    InvalidRequest(InvalidRequestDetails),
    /// 401.
    Unauthenticated,
    /// 403 `{ required_role }`.
    Forbidden {
        /// The lowest role that may make the call.
        required_role: Role,
    },
    /// 403.
    InvalidSignature,
    /// 404.
    NotFound,
    /// 409, with the state details.
    InvalidState(InvalidStateDetails),
    /// 409.
    RequestInProgress,
    /// 409 `{ job_id }`.
    LeaseLost {
        /// The job whose lease is gone.
        job_id: JobId,
    },
    /// 413 `{ max_bytes }`.
    UploadTooLarge {
        /// The limit.
        max_bytes: u64,
    },
    /// 415 `{ accepted }`.
    UnsupportedMediaType {
        /// The accepted media types.
        accepted: Vec<String>,
    },
    /// 422 `{ problem, blur_score, exposure, hint }`.
    ImageQuality {
        /// Which check failed.
        problem: QualityProblem,
        /// The blur score (variance of the Laplacian).
        blur_score: f64,
        /// Mean exposure, 0–1.
        exposure: f64,
        /// The retake hint.
        hint: QualityHint,
    },
    /// 422 `{ missing }`.
    MissingRequiredShot {
        /// The required kinds that are missing.
        missing: Vec<ImageKind>,
    },
    /// 422 `{ kind, max }`.
    TooManyImages {
        /// The kind at its limit.
        kind: ImageKind,
        /// Its limit.
        max: u32,
    },
    /// 422.
    IdempotencyConflict,
    /// 429.
    RateLimited,
    /// 429 `{ budget, resets_at }`.
    BudgetExceeded {
        /// Which budget.
        budget: Budget,
        /// When it resets.
        resets_at: Timestamp,
    },
    /// 500.
    Internal,
    /// 503.
    ProcessingUnavailable,
    /// 503 `{ upstream }`.
    UpstreamUnavailable {
        /// The dependency that failed, such as `"llm"` or `"open_meteo"`.
        upstream: String,
    },
}

impl ErrorKind {
    /// The code.
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::InvalidRequest(_) => ErrorCode::InvalidRequest,
            Self::Unauthenticated => ErrorCode::Unauthenticated,
            Self::Forbidden { .. } => ErrorCode::Forbidden,
            Self::InvalidSignature => ErrorCode::InvalidSignature,
            Self::NotFound => ErrorCode::NotFound,
            Self::InvalidState(_) => ErrorCode::InvalidState,
            Self::RequestInProgress => ErrorCode::RequestInProgress,
            Self::LeaseLost { .. } => ErrorCode::LeaseLost,
            Self::UploadTooLarge { .. } => ErrorCode::UploadTooLarge,
            Self::UnsupportedMediaType { .. } => ErrorCode::UnsupportedMediaType,
            Self::ImageQuality { .. } => ErrorCode::ImageQuality,
            Self::MissingRequiredShot { .. } => ErrorCode::MissingRequiredShot,
            Self::TooManyImages { .. } => ErrorCode::TooManyImages,
            Self::IdempotencyConflict => ErrorCode::IdempotencyConflict,
            Self::RateLimited => ErrorCode::RateLimited,
            Self::BudgetExceeded { .. } => ErrorCode::BudgetExceeded,
            Self::Internal => ErrorCode::Internal,
            Self::ProcessingUnavailable => ErrorCode::ProcessingUnavailable,
            Self::UpstreamUnavailable { .. } => ErrorCode::UpstreamUnavailable,
        }
    }
}

/// `ErrorBody.error` ([contracts 2]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ErrorObject {
    /// `code` and `details`.
    #[serde(flatten)]
    pub kind: ErrorKind,
    /// Human-readable, safe to show; English for now.
    pub message: String,
    /// The same request may succeed later.
    pub retryable: bool,
    /// With `409 request_in_progress`, `429 rate_limited` and `503`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub retry_after_s: Option<u32>,
    /// Matches the server log line: `req_<ULID>`.
    pub request_id: String,
}

/// Every non-2xx response body ([contracts 2]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ErrorBody {
    /// The error.
    pub error: ErrorObject,
}

impl ErrorBody {
    /// A body with the code's default `retryable`.
    pub fn new(kind: ErrorKind, message: impl Into<String>, request_id: impl Into<String>) -> Self {
        let retryable = kind.code().default_retryable();
        Self {
            error: ErrorObject {
                kind,
                message: message.into(),
                retryable,
                retry_after_s: None,
                request_id: request_id.into(),
            },
        }
    }

    /// The HTTP status.
    pub fn http_status(&self) -> u16 {
        self.error.kind.code().http_status()
    }
}

/// A rule refused the call. Raised by `domain::rules`, `authz` and every crate's own logic, and
/// mapped to a contract code in one place ([`DomainError::kind`]). Infrastructure failures are
/// not `DomainError`s: they live in each port's error enum (`StoreError`, `BlobError`, …),
/// which wraps this one with `#[from]`.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DomainError {
    /// The record doesn't exist, is being deleted, or the caller may not see it. Always 404,
    /// never 403, for non-members ([contracts 1], roles and access).
    #[error("not found")]
    NotFound,
    /// A member whose role is below `required`.
    #[error("requires the {required} role")]
    Forbidden {
        /// The lowest role that may make the call.
        required: Role,
    },
    /// A request field breaks a rule that serde can't express (exactly one of two fields, a
    /// length, a range).
    #[error("{field}: {problem}")]
    InvalidRequest {
        /// The field, as a dotted path.
        field: String,
        /// What is wrong.
        problem: String,
    },
    /// Map pins name plants that aren't plants of this garden ([contracts 9]).
    #[error("unknown plants in pins")]
    UnknownPlants {
        /// Every such plant.
        plant_ids: Vec<PlantId>,
    },
    /// The record isn't in a state that allows the call.
    #[error("invalid state")]
    InvalidState(InvalidStateDetails),
    /// A scan status transition the state machine refuses.
    #[error(transparent)]
    Transition(#[from] TransitionError),
    /// A kind limit before submit ([contracts 3]).
    #[error("at most {max} {kind:?} images")]
    TooManyImages {
        /// The kind.
        kind: ImageKind,
        /// Its limit.
        max: u32,
    },
    /// Submit without the required shots.
    #[error("missing required shots")]
    MissingRequiredShot {
        /// The missing kinds.
        missing: Vec<ImageKind>,
    },
    /// A daily budget is used up.
    #[error("{budget:?} budget used up until {resets_at}")]
    BudgetExceeded {
        /// Which budget.
        budget: Budget,
        /// When it resets.
        resets_at: Timestamp,
    },
    /// The same idempotency key is still running.
    #[error("request in progress")]
    RequestInProgress {
        /// When to retry.
        retry_after_s: u32,
    },
    /// The same idempotency key was used with another body.
    #[error("idempotency key reused with a different body")]
    IdempotencyConflict,
    /// A worker's lease is gone ([contracts 5]).
    #[error("lease lost for job {job_id}")]
    LeaseLost {
        /// The job.
        job_id: JobId,
    },
}

impl DomainError {
    /// A shorthand for [`DomainError::InvalidRequest`].
    pub fn invalid(field: impl Into<String>, problem: impl Into<String>) -> Self {
        Self::InvalidRequest {
            field: field.into(),
            problem: problem.into(),
        }
    }

    /// A shorthand for `InvalidState` with a hint.
    pub fn invalid_state_hint(hint: InvalidStateHint) -> Self {
        Self::InvalidState(InvalidStateDetails {
            hint: Some(hint),
            ..InvalidStateDetails::default()
        })
    }

    /// The contract code and details.
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::NotFound => ErrorKind::NotFound,
            Self::Forbidden { required } => ErrorKind::Forbidden {
                required_role: *required,
            },
            Self::InvalidRequest { field, problem } => {
                ErrorKind::InvalidRequest(InvalidRequestDetails {
                    field: field.clone(),
                    problem: problem.clone(),
                    plant_ids: None,
                })
            }
            Self::UnknownPlants { plant_ids } => ErrorKind::InvalidRequest(InvalidRequestDetails {
                field: "pins".to_owned(),
                problem: "unknown_plant".to_owned(),
                plant_ids: Some(plant_ids.clone()),
            }),
            Self::InvalidState(details) => ErrorKind::InvalidState(details.clone()),
            Self::Transition(e) => ErrorKind::InvalidState(e.details()),
            Self::TooManyImages { kind, max } => ErrorKind::TooManyImages {
                kind: *kind,
                max: *max,
            },
            Self::MissingRequiredShot { missing } => ErrorKind::MissingRequiredShot {
                missing: missing.clone(),
            },
            Self::BudgetExceeded { budget, resets_at } => ErrorKind::BudgetExceeded {
                budget: *budget,
                resets_at: *resets_at,
            },
            Self::RequestInProgress { .. } => ErrorKind::RequestInProgress,
            Self::IdempotencyConflict => ErrorKind::IdempotencyConflict,
            Self::LeaseLost { job_id } => ErrorKind::LeaseLost { job_id: *job_id },
        }
    }

    /// `retry_after_s`, when the code carries one.
    pub fn retry_after_s(&self) -> Option<u32> {
        match self {
            Self::RequestInProgress { retry_after_s } => Some(*retry_after_s),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_body_matches_the_contract_example() {
        let json = r#"{
          "error": {
            "code": "image_quality",
            "message": "This photo is too blurry. Hold the phone still and tap the leaf to focus.",
            "retryable": true,
            "details": { "problem": "too_blurry", "blur_score": 41.2, "exposure": 0.52, "hint": "tap_to_focus" },
            "request_id": "req_01JAX4B2Q8"
          }
        }"#;
        let body: ErrorBody = serde_json::from_str(json).unwrap();
        assert_eq!(body.http_status(), 422);
        assert_eq!(
            body.error.kind,
            ErrorKind::ImageQuality {
                problem: QualityProblem::TooBlurry,
                blur_score: 41.2,
                exposure: 0.52,
                hint: QualityHint::TapToFocus,
            }
        );
        let again: serde_json::Value = serde_json::to_value(&body).unwrap();
        let expected: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(again, expected);
    }

    #[test]
    fn codes_without_details_omit_the_field() {
        let body = ErrorBody::new(ErrorKind::NotFound, "Not found.", "req_1");
        assert_eq!(
            serde_json::to_value(&body).unwrap(),
            serde_json::json!({"error": {
                "code": "not_found", "message": "Not found.", "retryable": false,
                "request_id": "req_1"
            }})
        );
    }

    #[test]
    fn status_and_retryable_table() {
        for code in ErrorCode::ALL {
            let status = code.http_status();
            assert!((400..600).contains(&status));
        }
        assert_eq!(ErrorCode::Forbidden.http_status(), 403);
        assert_eq!(ErrorCode::InvalidSignature.http_status(), 403);
        assert!(ErrorCode::RateLimited.default_retryable());
        assert!(!ErrorCode::BudgetExceeded.default_retryable());
        assert!(!ErrorCode::Internal.default_retryable());
        assert!(ErrorCode::NotFound.is_stored_for_idempotency());
        assert!(!ErrorCode::InvalidState.is_stored_for_idempotency());
        assert!(!ErrorCode::RateLimited.is_stored_for_idempotency());
    }

    #[test]
    fn unknown_plants_maps_to_invalid_request() {
        let kind = DomainError::UnknownPlants { plant_ids: vec![] }.kind();
        let json = serde_json::to_value(&kind).unwrap();
        assert_eq!(json["code"], "invalid_request");
        assert_eq!(json["details"]["problem"], "unknown_plant");
    }
}
