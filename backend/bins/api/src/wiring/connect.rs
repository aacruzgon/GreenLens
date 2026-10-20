//! [`ConnectVendors`] over `integrations`' Slack and Drive clients, on the vendors' own hosts
//! only (decision 8 of docs/EON): `SlackConfig::default()` is `https://slack.com`, and
//! `DriveConfig::default()` is Google's API, token and revoke endpoints. Nothing here takes a
//! base URL, so no configuration and no request can point a route's vendor call elsewhere. The
//! `era` connectors' simulator clients belong to the sync (E4), never to a route.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use secrecy::SecretString;

use domain::{
    connectors::{ConnectorAccount, ConnectorKind, ConnectorSource},
    ports::Clock,
};
use integrations::{
    farm::FarmClientError,
    google_drive::{DriveClient, DriveConfig},
    slack::{SlackClient, SlackConfig},
};

use crate::{
    config::{ConnectorSettings, OAuthClient},
    connect::{ConnectVendors, Exchanged, VendorError},
    sealing::StoredTokens,
};

/// Google's access token is refreshed when it has less than this left ([connectors 5]).
///
/// [connectors 5]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#5-connecting-and-tokens
const REFRESH_BEFORE: Duration = Duration::from_mins(5);

/// The vendors' clients and this server's OAuth clients.
pub struct VendorClients {
    slack: SlackClient,
    drive: DriveClient,
    slack_client: Option<OAuthClient>,
    google_client: Option<OAuthClient>,
    clock: Arc<dyn Clock>,
}

impl VendorClients {
    /// The clients on the vendors' fixed hosts, over the shared HTTP client: `SlackConfig` and
    /// `DriveConfig` are always their defaults here.
    pub fn new(
        slack: impl FnOnce(SlackConfig) -> SlackClient,
        drive: impl FnOnce(DriveConfig) -> DriveClient,
        settings: &ConnectorSettings,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            slack: slack(SlackConfig::default()),
            drive: drive(DriveConfig::default()),
            slack_client: settings.slack.clone(),
            google_client: settings.google.clone(),
            clock,
        }
    }

    fn client(&self, kind: ConnectorKind) -> Result<&OAuthClient, VendorError> {
        match kind {
            ConnectorKind::Slack => self.slack_client.as_ref(),
            ConnectorKind::GoogleDrive => self.google_client.as_ref(),
        }
        .ok_or(VendorError::NotConfigured(kind))
    }

    /// Google's access token, refreshed for this call when it is about to expire. The refreshed
    /// token isn't stored: the sync keeps the stored one current (E4), and Google's refresh
    /// token stays valid.
    async fn google_access(&self, tokens: &StoredTokens) -> Result<SecretString, VendorError> {
        let now = self.clock.now();
        let fresh = tokens
            .expires_at
            .is_none_or(|at| at > now.plus(REFRESH_BEFORE));
        match (&tokens.refresh_token, fresh) {
            (Some(refresh), false) => {
                let client = self.client(ConnectorKind::GoogleDrive)?;
                let grant = self
                    .drive
                    .refresh(refresh, &client.client_id, &client.client_secret)
                    .await
                    .map_err(|e| vendor_error(ConnectorKind::GoogleDrive, e))?;
                Ok(grant.access_token)
            }
            _ => Ok(tokens.access_token.clone()),
        }
    }
}

#[async_trait]
impl ConnectVendors for VendorClients {
    async fn exchange(
        &self,
        kind: ConnectorKind,
        code: &str,
        pkce_verifier: Option<SecretString>,
        redirect_uri: &str,
    ) -> Result<Exchanged, VendorError> {
        let client = self.client(kind)?;
        let error = |e| vendor_error(kind, e);
        match kind {
            ConnectorKind::Slack => {
                let access = self
                    .slack
                    .oauth_v2_access(code, redirect_uri, &client.client_id, &client.client_secret)
                    .await
                    .map_err(error)?;
                let who = self
                    .slack
                    .auth_test(&access.access_token)
                    .await
                    .map_err(error)?;
                Ok(Exchanged {
                    tokens: StoredTokens::bearer(access.access_token),
                    account: ConnectorAccount::Slack {
                        team_id: who.team_id,
                        team_name: who.team,
                        url: who.url,
                    },
                })
            }
            ConnectorKind::GoogleDrive => {
                let verifier = pkce_verifier.ok_or_else(|| VendorError::Rejected {
                    vendor: kind,
                    code: "missing_pkce_verifier".to_owned(),
                })?;
                let grant = self
                    .drive
                    .exchange_code(
                        code,
                        &verifier,
                        redirect_uri,
                        &client.client_id,
                        &client.client_secret,
                    )
                    .await
                    .map_err(error)?;
                let email = self
                    .drive
                    .about_email(&grant.access_token)
                    .await
                    .map_err(error)?;
                let expires_at = self.clock.now().plus(grant.expires_in);
                Ok(Exchanged {
                    tokens: StoredTokens {
                        access_token: grant.access_token,
                        refresh_token: grant.refresh_token,
                        expires_at: Some(expires_at),
                    },
                    account: ConnectorAccount::GoogleDrive { email },
                })
            }
        }
    }

    async fn available_sources(
        &self,
        kind: ConnectorKind,
        tokens: &StoredTokens,
    ) -> Result<Vec<ConnectorSource>, VendorError> {
        let error = |e| vendor_error(kind, e);
        match kind {
            ConnectorKind::Slack => Ok(self
                .slack
                .conversations_list(&tokens.access_token)
                .await
                .map_err(error)?
                .into_iter()
                .map(|c| ConnectorSource {
                    id: c.id,
                    name: c.name,
                })
                .collect()),
            ConnectorKind::GoogleDrive => {
                let access = self.google_access(tokens).await?;
                Ok(self
                    .drive
                    .folders(&access)
                    .await
                    .map_err(error)?
                    .into_iter()
                    .map(|f| ConnectorSource {
                        id: f.id,
                        name: f.name,
                    })
                    .collect())
            }
        }
    }

    async fn revoke(&self, kind: ConnectorKind, tokens: &StoredTokens) -> Result<(), VendorError> {
        let error = |e| vendor_error(kind, e);
        match kind {
            // The clients count an already-revoked token as revoked (Slack's `token_revoked` and
            // `invalid_auth`, Google's `invalid_token`).
            ConnectorKind::Slack => self
                .slack
                .auth_revoke(&tokens.access_token)
                .await
                .map_err(error),
            ConnectorKind::GoogleDrive => {
                let token = tokens
                    .refresh_token
                    .as_ref()
                    .unwrap_or(&tokens.access_token);
                self.drive.revoke(token).await.map_err(error)
            }
        }
    }
}

/// A client error as the routes answer it. A refusal of the owner's code or token
/// (`AuthRefused`) is the owner's to fix; a refusal of our own request (`Refused`: a
/// misconfigured OAuth client; `NotFound`) is the server's; everything else is the vendor
/// being away.
fn vendor_error(vendor: ConnectorKind, error: FarmClientError) -> VendorError {
    match error {
        FarmClientError::AuthRefused { code, .. } => VendorError::AuthRefused { vendor, code },
        FarmClientError::NotFound { code, .. } | FarmClientError::Refused { code, .. } => {
            VendorError::Rejected { vendor, code }
        }
        FarmClientError::RateLimited { retry_after, .. } => VendorError::Unavailable {
            vendor,
            retry_after_s: retry_after
                .map(|d| u32::try_from(d.as_secs().max(1)).unwrap_or(u32::MAX)),
            source: None,
        },
        e @ (FarmClientError::Unavailable { .. } | FarmClientError::Malformed { .. }) => {
            VendorError::Unavailable {
                vendor,
                retry_after_s: None,
                source: Some(Box::new(e)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusals_are_the_owners_and_outages_the_vendors() {
        let refused = vendor_error(
            ConnectorKind::Slack,
            FarmClientError::Refused {
                vendor: "slack",
                status: 200,
                code: "bad_client_secret".into(),
            },
        );
        assert!(
            matches!(refused, VendorError::Rejected { code, .. } if code == "bad_client_secret")
        );
        let auth = vendor_error(
            ConnectorKind::GoogleDrive,
            FarmClientError::AuthRefused {
                vendor: "google_drive",
                code: "invalid_grant".into(),
            },
        );
        assert!(matches!(auth, VendorError::AuthRefused { code, .. } if code == "invalid_grant"));
        let limited = vendor_error(
            ConnectorKind::GoogleDrive,
            FarmClientError::RateLimited {
                vendor: "google_drive",
                retry_after: Some(Duration::from_secs(30)),
            },
        );
        assert!(matches!(
            limited,
            VendorError::Unavailable {
                retry_after_s: Some(30),
                ..
            }
        ));
        let down = vendor_error(
            ConnectorKind::GoogleDrive,
            FarmClientError::Malformed {
                vendor: "google_drive",
            },
        );
        assert!(matches!(
            down,
            VendorError::Unavailable {
                source: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn the_clients_use_the_vendors_hosts() {
        assert_eq!(SlackConfig::default().base_url, "https://slack.com");
        let drive = DriveConfig::default();
        assert_eq!(drive.api_base, "https://www.googleapis.com");
        assert_eq!(drive.token_url, "https://oauth2.googleapis.com/token");
        assert_eq!(drive.revoke_url, "https://oauth2.googleapis.com/revoke");
    }
}
