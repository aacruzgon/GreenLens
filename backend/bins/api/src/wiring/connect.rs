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

    async fn refresh(
        &self,
        kind: ConnectorKind,
        tokens: &StoredTokens,
        force: bool,
    ) -> Result<Option<StoredTokens>, VendorError> {
        let now = self.clock.now();
        let fresh = !force
            && tokens
                .expires_at
                .is_none_or(|at| at > now.plus(REFRESH_BEFORE));
        let (ConnectorKind::GoogleDrive, Some(refresh), false) =
            (kind, &tokens.refresh_token, fresh)
        else {
            return Ok(None);
        };
        let client = self.client(kind)?;
        let grant = self
            .drive
            .refresh(refresh, &client.client_id, &client.client_secret)
            .await
            .map_err(|e| vendor_error(kind, e))?;
        Ok(Some(StoredTokens {
            access_token: grant.access_token,
            // Google sends a new refresh token only sometimes; the old one stays good otherwise.
            refresh_token: grant.refresh_token.or_else(|| Some(refresh.clone())),
            expires_at: Some(now.plus(grant.expires_in)),
        }))
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
            ConnectorKind::GoogleDrive => Ok(self
                .drive
                .folders(&tokens.access_token)
                .await
                .map_err(error)?
                .into_iter()
                .map(|f| ConnectorSource {
                    id: f.id,
                    name: f.name,
                })
                .collect()),
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

    /// `refresh` against a mocked token endpoint: an expiring token is renewed and a fresh one
    /// isn't, `force` renews a fresh one, an answer without a refresh token keeps the old one,
    /// and `expires_at` is the clock plus `expires_in`.
    #[tokio::test]
    async fn refresh_renews_an_expiring_token_or_a_forced_one() {
        use domain::{testing::FixedClock, wire::Timestamp};
        use integrations::http::{HttpConfig, client};
        use secrecy::ExposeSecret;
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{body_string_contains, method, path},
        };

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("grant_type=refresh_token"))
            .and(body_string_contains("refresh_token=1%2F%2Fkeep"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({"access_token": "ya29.new", "expires_in": 1800}),
                ),
            )
            .mount(&server)
            .await;
        let now: Timestamp = "2026-10-15T12:00:00Z".parse().expect("a timestamp");
        let settings = ConnectorSettings {
            key: None,
            slack: None,
            google: Some(OAuthClient {
                client_id: "id.apps.googleusercontent.com".into(),
                client_secret: SecretString::from("secret"),
            }),
            return_url: "pdm://connectors".into(),
            era_slack_base: None,
            era_google_base: None,
            era_demo_garden: None,
        };
        let uri = server.uri();
        let http = client(&HttpConfig::default()).expect("the HTTP client builds");
        let clients = VendorClients::new(
            |c| SlackClient::new(http.clone(), c),
            |_| {
                DriveClient::new(
                    http.clone(),
                    DriveConfig {
                        api_base: uri.clone(),
                        token_url: format!("{uri}/token"),
                        revoke_url: format!("{uri}/revoke"),
                        ..DriveConfig::default()
                    },
                )
            },
            &settings,
            Arc::new(FixedClock::at(now)),
        );
        let tokens = |expires_in: Duration| StoredTokens {
            access_token: SecretString::from("ya29.old"),
            refresh_token: Some(SecretString::from("1//keep")),
            expires_at: Some(now.plus(expires_in)),
        };
        let kind = ConnectorKind::GoogleDrive;

        let fresh = tokens(Duration::from_mins(30));
        assert!(
            clients
                .refresh(kind, &fresh, false)
                .await
                .expect("ok")
                .is_none()
        );
        assert!(
            server
                .received_requests()
                .await
                .expect("recorded")
                .is_empty()
        );

        for (stored, force) in [(tokens(Duration::from_mins(4)), false), (fresh, true)] {
            let renewed = clients
                .refresh(kind, &stored, force)
                .await
                .expect("ok")
                .expect("renewed");
            assert_eq!(renewed.access_token.expose_secret(), "ya29.new");
            assert_eq!(
                renewed
                    .refresh_token
                    .as_ref()
                    .map(ExposeSecret::expose_secret),
                Some("1//keep"),
                "Google sent no refresh token: the old one stays"
            );
            assert_eq!(renewed.expires_at, Some(now.plus(Duration::from_mins(30))));
        }
        assert_eq!(server.received_requests().await.expect("recorded").len(), 2);
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
