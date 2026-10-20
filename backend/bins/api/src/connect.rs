//! Connecting a farm connector ([connectors 5], roadmap E3.3 and E3.4): the signed OAuth
//! state, the vendors' consent URLs, and the [`ConnectVendors`] seam the connector routes and
//! the deletion runner call for everything that talks to Slack or Google. The tokens as they
//! are sealed are `sealing::StoredTokens`.
//!
//! - **Fixed hosts (decision 8).** The consent URLs are constants here, and `wiring.rs` builds
//!   the vendor clients with the vendors' own hosts only. No route takes a URL, and no route
//!   reaches a connector whose `endpoint` is `era`: those refuse every owner change with
//!   `409 invalid_state` (decision 11), so a real token never goes to a simulator and a
//!   simulator's token never goes to a vendor.
//! - **The seam.** The routes call [`ConnectVendors`], held in `AppState` like the other seams,
//!   and the deletion runner revokes through the same instance, so their tests use a mock and
//!   never real HTTP. The real exchanges are tested in the clients (`integrations::slack`,
//!   `integrations::google_drive`; decision 2).
//!
//! [connectors 5]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#5-connecting-and-tokens

mod oauth;

use async_trait::async_trait;
use secrecy::SecretString;

use domain::{
    connectors::{ConnectorAccount, ConnectorKind, ConnectorSource},
    id::{ConnectorId, DeviceHash, DeviceVerifier},
};

pub use oauth::{OAuthState, StateError};

use crate::sealing::StoredTokens;

/// Slack's consent page ([connectors 5]). A constant: no configuration can move it.
///
/// [connectors 5]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#5-connecting-and-tokens
pub const SLACK_AUTHORIZE_URL: &str = "https://slack.com/oauth/v2/authorize";
/// The five bot scopes PDM asks Slack for, read-only.
pub const SLACK_SCOPES: &str =
    "channels:read,channels:history,groups:read,groups:history,users:read";
/// Google's consent page. A constant, like Slack's.
pub const GOOGLE_AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
/// The one Drive scope, `drive.readonly`.
pub const GOOGLE_SCOPE: &str = integrations::google_drive::SCOPE;
/// The callback's path under `API_PUBLIC_BASE`, registered with both vendors.
pub const CALLBACK_PATH: &str = "/connectors/oauth/callback";

/// The callback URL the vendors redirect to: `{API_PUBLIC_BASE}/connectors/oauth/callback`.
pub fn redirect_uri(public_base: &str) -> String {
    format!("{}{CALLBACK_PATH}", public_base.trim_end_matches('/'))
}

/// The vendor's consent URL for a pending connector. Google's carries the PKCE challenge and
/// asks for a refresh token (`access_type=offline`, `prompt=consent`, so a reconnect gets one
/// too); Slack's has no PKCE (connectors 5: the Slack app stays a confidential client).
pub fn authorize_url(
    kind: ConnectorKind,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    pkce_challenge: Option<&str>,
) -> String {
    let mut query: Vec<(&str, &str)> = vec![
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("state", state),
    ];
    let base = match kind {
        ConnectorKind::Slack => {
            query.push(("scope", SLACK_SCOPES));
            SLACK_AUTHORIZE_URL
        }
        ConnectorKind::GoogleDrive => {
            query.extend([
                ("response_type", "code"),
                ("scope", GOOGLE_SCOPE),
                ("access_type", "offline"),
                ("prompt", "consent"),
                ("include_granted_scopes", "false"),
                ("code_challenge_method", "S256"),
            ]);
            if let Some(challenge) = pkce_challenge {
                query.push(("code_challenge", challenge));
            }
            GOOGLE_AUTHORIZE_URL
        }
    };
    format!("{base}?{}", query_string(&query))
}

/// The callback's `302` target: `{CONNECTOR_RETURN_URL}/{connector}/callback?…`.
pub fn return_location(return_url: &str, connector: ConnectorId, query: &[(&str, &str)]) -> String {
    format!(
        "{}/{connector}/callback?{}",
        return_url.trim_end_matches('/'),
        query_string(query)
    )
}

/// `k=v&k=v`, each part percent-encoded.
fn query_string(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Percent-encodes everything but RFC 3986's unreserved characters.
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0x0f)]));
        }
    }
    out
}

/// `SHA-256(device_verifier) = device_hash`, compared in constant time.
pub fn device_matches(verifier: &DeviceVerifier, hash: &DeviceHash) -> bool {
    let digest =
        aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, verifier.as_str().as_bytes());
    let computed = data_encoding::BASE64URL_NOPAD.encode(digest.as_ref());
    aws_lc_rs::constant_time::verify_slices_are_equal(computed.as_bytes(), hash.as_str().as_bytes())
        .is_ok()
}

/// What a code exchange gives: the tokens to seal and the account to show.
#[derive(Debug)]
pub struct Exchanged {
    /// The vendor's tokens.
    pub tokens: StoredTokens,
    /// The connected workspace (`auth.test`) or Google account (`about.get`).
    pub account: ConnectorAccount,
}

/// A vendor call that failed. The vendor's response body is never kept ([connectors 8]).
///
/// [connectors 8]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#8-privacy-and-security
#[derive(Debug, thiserror::Error)]
pub enum VendorError {
    /// This server has no OAuth client for the vendor.
    #[error("{0} isn't configured")]
    NotConfigured(ConnectorKind),
    /// The vendor refused the owner's code or token (the client's `AuthRefused`): a used,
    /// expired or wrong code at the exchange (Slack's `invalid_code`, Google's `invalid_grant`);
    /// a revoked token, or a missing scope, afterwards. The owner fixes it by connecting again.
    #[error("{vendor} refused the code or token ({code})")]
    AuthRefused {
        /// The vendor.
        vendor: ConnectorKind,
        /// The vendor's short error code.
        code: String,
    },
    /// The vendor rejected our own request (the client's `Refused` or `NotFound`): a
    /// misconfigured OAuth client (Slack's `bad_redirect_uri`, `invalid_client_id`,
    /// `bad_client_secret`; Google's `invalid_client`) or another refusal no owner can fix. A
    /// server-side error, never the owner's `400`.
    #[error("{vendor} rejected the request ({code})")]
    Rejected {
        /// The vendor.
        vendor: ConnectorKind,
        /// The vendor's short error code.
        code: String,
    },
    /// Down, timing out, throttling or answering something unreadable.
    #[error("{vendor} is unavailable")]
    Unavailable {
        /// The vendor.
        vendor: ConnectorKind,
        /// The vendor's `Retry-After`, when it throttled.
        retry_after_s: Option<u32>,
        /// The client's error, without any body.
        #[source]
        source: Option<domain::ports::BoxError>,
    },
}

/// Everything the connector routes ask Slack or Google, on the vendors' fixed hosts. `wiring.rs`
/// implements it over `integrations`' `SlackClient` and `DriveClient`; the route tests mock it.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait ConnectVendors: Send + Sync {
    /// Exchanges an authorization code (Slack's `oauth.v2.access` then `auth.test`; Google's
    /// token endpoint with the PKCE verifier, then `about.get`). Never retried: a code works
    /// once.
    async fn exchange(
        &self,
        kind: ConnectorKind,
        code: &str,
        pkce_verifier: Option<SecretString>,
        redirect_uri: &str,
    ) -> Result<Exchanged, VendorError>;

    /// Google's tokens with a new access token when the stored one has less than 5 minutes
    /// left and a refresh token is there to renew it; `None` when they are fine as they are
    /// (always for Slack, whose bot token doesn't expire). The caller stores what comes back
    /// with `FarmRecordStore::replace_secret`, a compare-and-set against the sealed value it
    /// read, so two refreshes at once keep one.
    async fn refresh(
        &self,
        kind: ConnectorKind,
        tokens: &StoredTokens,
    ) -> Result<Option<StoredTokens>, VendorError>;

    /// The channels the bot is a member of, or the folders the Google account can open, with
    /// the access token as given: refresh it first.
    async fn available_sources(
        &self,
        kind: ConnectorKind,
        tokens: &StoredTokens,
    ) -> Result<Vec<ConnectorSource>, VendorError>;

    /// Revokes the tokens: Slack's `auth.revoke`; Google's `revoke` with the refresh token,
    /// which ends the whole grant of that account to this OAuth client, or the access token
    /// when there is none. A token the vendor already forgot counts as revoked. Never call it
    /// directly for a stored connector: `DeletionRunner::finish_connector` decides whether a
    /// shared account's token may go.
    async fn revoke(&self, kind: ConnectorKind, tokens: &StoredTokens) -> Result<(), VendorError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONNECTOR: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5C";

    fn id() -> ConnectorId {
        CONNECTOR.parse().unwrap()
    }

    #[test]
    fn the_consent_urls_are_the_vendors_own() {
        let slack = authorize_url(
            ConnectorKind::Slack,
            "123.456",
            "https://api.example.org/connectors/oauth/callback",
            "s.t.1.m",
            None,
        );
        assert_eq!(
            slack,
            "https://slack.com/oauth/v2/authorize?client_id=123.456\
             &redirect_uri=https%3A%2F%2Fapi.example.org%2Fconnectors%2Foauth%2Fcallback\
             &state=s.t.1.m\
             &scope=channels%3Aread%2Cchannels%3Ahistory%2Cgroups%3Aread%2Cgroups%3Ahistory%2Cusers%3Aread"
        );
        let google = authorize_url(
            ConnectorKind::GoogleDrive,
            "id.apps.googleusercontent.com",
            "https://api.example.org/connectors/oauth/callback",
            "s",
            Some("E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"),
        );
        assert!(google.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        for part in [
            "response_type=code",
            "scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fdrive.readonly",
            "access_type=offline",
            "prompt=consent",
            "code_challenge_method=S256",
            "code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
        ] {
            assert!(google.contains(part), "{part} in {google}");
        }
    }

    #[test]
    fn the_callback_and_the_return_are_built_from_configuration() {
        assert_eq!(
            redirect_uri("https://api.example.org/"),
            "https://api.example.org/connectors/oauth/callback"
        );
        assert_eq!(
            return_location(
                "pdm://connectors",
                id(),
                &[("code", "a/b c"), ("state", "s")]
            ),
            format!("pdm://connectors/{CONNECTOR}/callback?code=a%2Fb%20c&state=s")
        );
    }

    #[test]
    fn the_device_hash_is_sha256_of_the_verifier() {
        // RFC 7636 appendix B: the same encoding as PKCE's S256.
        let verifier: DeviceVerifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
            .parse()
            .unwrap();
        let hash: DeviceHash = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
            .parse()
            .unwrap();
        assert!(device_matches(&verifier, &hash));
        let other: DeviceHash = "A".repeat(43).parse().unwrap();
        assert!(!device_matches(&verifier, &other));
    }
}
