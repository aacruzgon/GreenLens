//! Revoking a farm connector's vendor token when the connector is deleted ([connectors 8]):
//! Slack's `auth.revoke`, Google's `revoke`. It is an HTTP call, so the deletion runner makes it
//! outside every store transaction, after the transaction that marked the connector `deleting`
//! has committed, and before the records and the row that holds the sealed token are deleted
//! (EON E3.6).
//!
//! [`TokenRevoker`] is the seam: [`VendorRevoker`] calls the `integrations` clients at the
//! vendors' constant hosts, and the runner's tests record the calls instead. An `era`
//! connector's token is a fixed demo string, so the runner never asks for its revoke.
//!
//! [connectors 8]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#8-privacy-and-security

use domain::{connectors::ConnectorKind, ports::VendorToken};
use integrations::{farm::FarmClientError, google_drive::DriveClient, slack::SlackClient};
use secrecy::{ExposeSecret, SecretSlice, SecretString};

/// Revokes one vendor token. A failure is logged by the caller, never retried, and never stops
/// the records' deletion: the token is gone from our side either way.
#[async_trait::async_trait]
pub trait TokenRevoker: Send + Sync {
    /// Revokes `token` at the vendor `kind` names. A token the vendor already forgot counts as
    /// revoked where the client says so (Google's `invalid_token`).
    async fn revoke(&self, kind: ConnectorKind, token: &VendorToken)
    -> Result<(), FarmClientError>;
}

/// The real vendors, through the shared `reqwest` client. Revoking needs only the token, not
/// the OAuth client's id or secret, so it works whatever vendor settings are configured.
pub struct VendorRevoker {
    slack: SlackClient,
    drive: DriveClient,
}

impl VendorRevoker {
    /// Over clients built with their default configurations, which are the vendors' constant
    /// hosts ([connectors 8]: never configuration). `wiring.rs` builds them on the process's one
    /// HTTP client.
    ///
    /// [connectors 8]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#8-privacy-and-security
    pub fn new(slack: SlackClient, drive: DriveClient) -> Self {
        Self { slack, drive }
    }
}

#[async_trait::async_trait]
impl TokenRevoker for VendorRevoker {
    async fn revoke(
        &self,
        kind: ConnectorKind,
        token: &VendorToken,
    ) -> Result<(), FarmClientError> {
        let secret = SecretString::from(token.expose());
        match kind {
            ConnectorKind::Slack => self.slack.auth_revoke(&secret).await,
            ConnectorKind::GoogleDrive => self.drive.revoke(&secret).await,
        }
    }
}

/// The token to revoke from an unsealed `connector.secret`. The sealed plaintext is the token
/// itself (Slack's bot token; the seeded `era` strings, EON E4.6), or a JSON object of a Google
/// grant, whose `refresh_token` revokes the whole grant and whose `access_token` is the fallback.
/// `None` when it holds neither, or isn't UTF-8.
pub fn token_to_revoke(plaintext: &SecretSlice<u8>) -> Option<VendorToken> {
    let text = std::str::from_utf8(plaintext.expose_secret()).ok()?.trim();
    if text.starts_with('{') {
        let grant: serde_json::Value = serde_json::from_str(text).ok()?;
        let token = ["refresh_token", "access_token"]
            .iter()
            .find_map(|field| grant.get(field)?.as_str().filter(|t| !t.is_empty()))?;
        return Some(VendorToken::new(token));
    }
    (!text.is_empty()).then(|| VendorToken::new(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(text: &str) -> Option<String> {
        token_to_revoke(&SecretSlice::from(text.as_bytes().to_vec())).map(|t| t.expose().to_owned())
    }

    #[test]
    fn a_plain_token_is_revoked_as_it_is() {
        assert_eq!(token("xoxb-123\n").as_deref(), Some("xoxb-123"));
        assert_eq!(token("ya29.era-demo").as_deref(), Some("ya29.era-demo"));
        assert_eq!(token("  "), None);
    }

    #[test]
    fn a_google_grant_revokes_its_refresh_token_first() {
        assert_eq!(
            token(r#"{"access_token":"ya29.a","refresh_token":"1//r"}"#).as_deref(),
            Some("1//r")
        );
        assert_eq!(
            token(r#"{"access_token":"ya29.a"}"#).as_deref(),
            Some("ya29.a")
        );
        assert_eq!(token(r#"{"expires_in":3599}"#), None);
        assert_eq!(token("{not json"), None);
    }
}
