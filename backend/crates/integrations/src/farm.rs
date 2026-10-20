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

use domain::{connectors::ConnectorKind, ports::FarmSourceError};

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
    /// A chosen channel or folder is gone or no longer visible to the token: nothing more runs
    /// until the owner changes the sources.
    SourceMissing,
}

impl FarmErrorCode {
    /// The stored string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RateLimited => "rate_limited",
            Self::VendorUnavailable => "vendor_unavailable",
            Self::AuthRefused => "auth_refused",
            Self::TooLarge => "too_large",
            Self::SourceMissing => "source_missing",
        }
    }
}

/// A failed call to Slack or Google Drive.
#[derive(Debug, thiserror::Error)]
pub enum FarmClientError {
    /// The token is missing, wrong, expired, revoked or lacks a scope (Slack `not_authed`,
    /// `invalid_auth`, `token_revoked`, `account_inactive`, `missing_scope`; Google `401`, a
    /// `403` for an insufficient scope, or `invalid_grant` on a refresh). It is also a bad code
    /// the user brought to an OAuth exchange (Slack `invalid_code`, `code_already_used`,
    /// `code_expired`; Google `invalid_grant`). The rule: a bad user code or token is
    /// `AuthRefused`; a fault in our own configuration is [`Self::Refused`].
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
    /// rate limit, a `400`. Never retried. Includes our own misconfiguration of an OAuth exchange
    /// (Slack `bad_redirect_uri`, `invalid_client_id`, `bad_client_secret`, `invalid_client`;
    /// Google `401 invalid_client`), which is not the user's to fix.
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
    /// The `last_error` code this failure leaves on a connector when it stops a sync. A
    /// `NotFound` on a chosen source is `source_missing`. A `Refused` is `vendor_unavailable`:
    /// nothing the owner does to the token fixes it, and the sync skips a refused single file
    /// rather than stopping.
    pub const fn code(&self) -> FarmErrorCode {
        match self {
            Self::AuthRefused { .. } => FarmErrorCode::AuthRefused,
            Self::RateLimited { .. } => FarmErrorCode::RateLimited,
            Self::NotFound { .. } => FarmErrorCode::SourceMissing,
            Self::Unavailable { .. } | Self::Refused { .. } | Self::Malformed { .. } => {
                FarmErrorCode::VendorUnavailable
            }
        }
    }

    /// The `FarmSource` port's error for this failure, for a call that read `source_id` (a
    /// channel or the folder) or, with `None`, a call about the token or the account. A
    /// `NotFound` on a source is `SourceMissing`; on anything else it says nothing about the
    /// owner's choice and counts as a bad answer. The result keeps no response body: its
    /// source is this error at most, which holds none.
    pub fn into_source_error(
        self,
        vendor: ConnectorKind,
        source_id: Option<&str>,
    ) -> FarmSourceError {
        match self {
            Self::AuthRefused { .. } => FarmSourceError::AuthRefused { vendor },
            Self::RateLimited { retry_after, .. } => FarmSourceError::RateLimited {
                vendor,
                retry_after_s: retry_after.map(|d| u32::try_from(d.as_secs()).unwrap_or(u32::MAX)),
            },
            Self::NotFound { .. } => match source_id {
                Some(id) => FarmSourceError::SourceMissing {
                    vendor,
                    source_id: id.to_owned(),
                },
                None => FarmSourceError::BadResponse {
                    vendor,
                    source: Box::new(self),
                },
            },
            Self::Malformed { .. } => FarmSourceError::BadResponse {
                vendor,
                source: Box::new(self),
            },
            Self::Unavailable { .. } | Self::Refused { .. } => FarmSourceError::Unavailable {
                vendor,
                source: Box::new(self),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use domain::{connectors::ConnectorKind, connectors::SyncErrorCode};

    use super::FarmClientError;

    const SLACK: ConnectorKind = ConnectorKind::Slack;

    #[test]
    fn each_client_error_maps_to_the_same_code_through_the_port() {
        let cases = [
            FarmClientError::AuthRefused {
                vendor: "slack",
                code: "invalid_auth".into(),
            },
            FarmClientError::RateLimited {
                vendor: "slack",
                retry_after: Some(Duration::from_secs(30)),
            },
            FarmClientError::Unavailable {
                vendor: "slack",
                source: None,
            },
            FarmClientError::NotFound {
                vendor: "slack",
                code: "channel_not_found".into(),
            },
            FarmClientError::Refused {
                vendor: "slack",
                status: 400,
                code: "invalid_cursor".into(),
            },
            FarmClientError::Malformed { vendor: "slack" },
        ];
        for error in cases {
            let code = error.code().as_str();
            let port = error.into_source_error(SLACK, Some("C_FIELD"));
            assert_eq!(port.code().as_str(), code, "{port:?}");
        }
    }

    #[test]
    fn a_retry_after_is_kept_in_whole_seconds() {
        let error = FarmClientError::RateLimited {
            vendor: "slack",
            retry_after: Some(Duration::from_millis(1_500)),
        };
        assert!(matches!(
            error.into_source_error(SLACK, None),
            domain::ports::FarmSourceError::RateLimited {
                retry_after_s: Some(1),
                ..
            }
        ));
    }

    #[test]
    fn not_found_off_a_source_is_not_source_missing() {
        let error = FarmClientError::NotFound {
            vendor: "slack",
            code: "user_not_found".into(),
        };
        assert_eq!(
            error.into_source_error(SLACK, None).code(),
            SyncErrorCode::VendorUnavailable
        );
    }
}
