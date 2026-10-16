//! The Messages client's error, its retry rules and its mapping to [`ModelError`].

use std::time::Duration;

use domain::ports::{BoxError, ModelError};

use crate::http::TransportError;

/// Why a Messages call failed. No `reqwest` type appears in it.
#[derive(Debug, thiserror::Error)]
pub enum AnthropicError {
    /// No HTTP response: connect failure, timeout, a dropped stream.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// The API answered with an error status.
    #[error("Anthropic API {status} {kind}: {message}")]
    Api {
        /// The HTTP status.
        status: u16,
        /// `error.type`, such as `rate_limit_error`.
        kind: String,
        /// `error.message`.
        message: String,
        /// `request-id`.
        request_id: Option<String>,
        /// `retry-after`, seconds.
        retry_after: Option<Duration>,
    },
    /// A 429 with `error.details.error_code == "enforced_spend_limit_reached"`: the
    /// organisation's monthly spend cap. Never retried.
    #[error("Anthropic spend cap reached")]
    SpendCap {
        /// `request-id`.
        request_id: Option<String>,
    },
    /// An `error` event after HTTP 200 (`overloaded_error`, `api_error`).
    #[error("Anthropic stream error {kind}: {message}")]
    Stream {
        /// `error.type`.
        kind: String,
        /// `error.message`.
        message: String,
    },
    /// A body or event that didn't parse.
    #[error("Anthropic response did not parse")]
    Decode(#[source] BoxError),
    /// A tool call's streamed JSON didn't parse at its block's stop.
    #[error("tool input of block {index} did not parse")]
    ToolInput {
        /// The block.
        index: usize,
        /// The parse error.
        #[source]
        source: BoxError,
    },
    /// The stream ended before `message_start`.
    #[error("Anthropic stream ended early")]
    Truncated,
    /// The request couldn't be serialised (a non-string map key in a schema).
    #[error("request did not serialise")]
    Encode(#[source] BoxError),
}

impl AnthropicError {
    /// Whether the same request may succeed later: transport failures, 408, 409, 429 (not the
    /// spend cap), 5xx and 529, and the stream's `overloaded_error` and `api_error`.
    pub fn retryable(&self) -> bool {
        match self {
            Self::Transport(_) | Self::Truncated => true,
            Self::Api { status, .. } => matches!(status, 408 | 409 | 429 | 500..=599),
            Self::Stream { kind, .. } => {
                matches!(
                    kind.as_str(),
                    "overloaded_error" | "api_error" | "rate_limit_error"
                )
            }
            Self::SpendCap { .. } | Self::Decode(_) | Self::ToolInput { .. } | Self::Encode(_) => {
                false
            }
        }
    }

    /// The server's `retry-after`.
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::Api { retry_after, .. } => *retry_after,
            _ => None,
        }
    }

    /// The domain error, for an adapter that knows its model name. Rate limits, overload,
    /// 5xx, transport failures and the spend cap are [`ModelError::Unavailable`] (`503
    /// upstream_unavailable` for chat); a malformed answer is [`ModelError::InvalidOutput`];
    /// a refused request (400, 401, 403, 404, 413) is [`ModelError::Failed`], logged with its
    /// message because it is our bug.
    pub fn into_model_error(self, model: &'static str) -> ModelError {
        let unavailable = matches!(self, Self::SpendCap { .. }) || self.retryable();
        match self {
            Self::Decode(_) | Self::ToolInput { .. } => ModelError::InvalidOutput {
                model,
                source: Box::new(self),
            },
            other if unavailable => ModelError::Unavailable {
                model,
                source: Box::new(other),
            },
            other => ModelError::Failed {
                model,
                source: Box::new(other),
            },
        }
    }
}
