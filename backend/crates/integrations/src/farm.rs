//! What the farm-source clients ([`crate::slack`], [`crate::google_drive`]) share: their error
//! and the fixed codes a connector's `last_error` stores ([connectors 6]).
//!
//! A vendor's response body is never kept in an error, logged or stored, because it may echo
//! the farm's own records ([connectors 8]). Errors carry the vendor's short error code (Slack's
//! `error`, Google's `reason`) at most.
//!
//! [connectors 6]: ../../../docs/connectors/README.md#6-sync
//! [connectors 8]: ../../../docs/connectors/README.md#8-privacy-and-security

use std::time::Duration;

use crate::http::TransportError;

/// The code stored in a connector's `last_error` ([connectors 6]).
///
/// [connectors 6]: ../../../docs/connectors/README.md#6-sync
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FarmErrorCode {
    /// The vendor throttled us; the sync resumes from its cursor on a later run.
    RateLimited,
    /// Down, timed out, a 5xx or an answer that didn't parse.
    VendorUnavailable,
    /// The token was refused or revoked: the owner must reconnect.
    AuthRefused,
    /// Over the garden's record limit. Set by the sync, never by a client.
    TooLarge,
}

impl FarmErrorCode {
    /// The stored string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RateLimited => "rate_limited",
            Self::VendorUnavailable => "vendor_unavailable",
            Self::AuthRefused => "auth_refused",
            Self::TooLarge => "too_large",
        }
    }
}

/// A failed call to Slack or Google Drive.
#[derive(Debug, thiserror::Error)]
pub enum FarmClientError {
    /// The token is missing, wrong, expired or revoked (Slack `not_authed`, `invalid_auth`,
    /// `token_revoked`, `account_inactive`; Google `401`, or `invalid_grant` on a refresh).
    #[error("{vendor}: token refused ({code})")]
    AuthRefused {
        /// `"slack"` or `"google_drive"`.
        vendor: &'static str,
        /// The vendor's short error code.
        code: String,
    },
    /// `429`, or Drive's `403` with reason `userRateLimitExceeded` or `rateLimitExceeded`,
    /// after the client's retries ran out.
    #[error("{vendor}: rate limited")]
    RateLimited {
        /// The vendor.
        vendor: &'static str,
        /// The vendor's `Retry-After`, if it sent one.
        retry_after: Option<Duration>,
    },
    /// Down, timed out or answering 5xx, after the client's retries ran out.
    #[error("{vendor}: unavailable")]
    Unavailable {
        /// The vendor.
        vendor: &'static str,
        /// The transport failure, when there was one.
        #[source]
        source: Option<TransportError>,
    },
    /// The channel, file or folder doesn't exist or isn't visible to the token
    /// (Slack `channel_not_found`, `not_in_channel`; Drive `404`).
    #[error("{vendor}: not found ({code})")]
    NotFound {
        /// The vendor.
        vendor: &'static str,
        /// The vendor's short error code.
        code: String,
    },
    /// Any other refusal: Slack's `ok: false` with another code, a Drive `403` that isn't a
    /// rate limit, a `400`. Never retried.
    #[error("{vendor}: refused ({code})")]
    Refused {
        /// The vendor.
        vendor: &'static str,
        /// The HTTP status.
        status: u16,
        /// The vendor's short error code.
        code: String,
    },
    /// The answer didn't parse into the expected shape.
    #[error("{vendor}: bad response")]
    Malformed {
        /// The vendor.
        vendor: &'static str,
    },
}

impl FarmClientError {
    /// The `last_error` code this failure leaves on a connector. `NotFound` and `Refused` are
    /// `vendor_unavailable`: the sync can't go on, and nothing the owner does to the token
    /// fixes it.
    pub const fn code(&self) -> FarmErrorCode {
        match self {
            Self::AuthRefused { .. } => FarmErrorCode::AuthRefused,
            Self::RateLimited { .. } => FarmErrorCode::RateLimited,
            Self::Unavailable { .. }
            | Self::NotFound { .. }
            | Self::Refused { .. }
            | Self::Malformed { .. } => FarmErrorCode::VendorUnavailable,
        }
    }
}
