//! Farm connectors ([connectors]): a garden's Slack channels or Google Drive folder, synced into
//! `farm_record` for the chat and the plan writer. The connector as the app sees it, the
//! requests of the connect flow, and the citation of a synced record.
//!
//! What never appears here: the sealed tokens (`connector.secret`), the pending connect's
//! `oauth` state (nonce, device hash, PKCE verifier), the sync cursor and the record vectors.
//! They stay in the store's own rows.
//!
//! [connectors]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    id::{ConnectorId, DeviceHash, DeviceVerifier, FarmRecordId, GardenId},
    wire::Timestamp,
};

/// `Connector.kind`, and `FarmCitation.system`: which vendor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorKind {
    /// Slack channels the bot was invited to.
    Slack,
    /// One Google Drive folder.
    GoogleDrive,
}

impl ConnectorKind {
    /// The wire and stored name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Slack => "slack",
            Self::GoogleDrive => "google_drive",
        }
    }
}

impl std::fmt::Display for ConnectorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `Connector.status` ([connectors 4], `connector.status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorStatus {
    /// Started; the owner hasn't finished the vendor's consent yet. Deleted by the cleanup after
    /// 10 minutes.
    Pending,
    /// Connected and syncing on its schedule.
    Active,
    /// The vendor refused the token (Slack's `invalid_auth` or `token_revoked`, Google's
    /// `invalid_grant`): the sync stopped, records stay searchable, owners see **Reconnect**.
    ReauthNeeded,
    /// Disconnected or its garden is being deleted: tokens revoked, records being deleted.
    Deleting,
}

impl ConnectorStatus {
    /// The wire and stored name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::ReauthNeeded => "reauth_needed",
            Self::Deleting => "deleting",
        }
    }
}

/// `Connector.endpoint` ([connectors 9.4]): where the connector's vendor calls go. Set by server
/// configuration only; no route creates an `era` connector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorEndpoint {
    /// The real vendor: every connector an owner creates.
    Vendor,
    /// Era by Eon's simulators: the demo garden's connectors, seeded at start-up. The app labels
    /// them "Demo data from Era by Eon (synthetic)".
    Era,
}

impl ConnectorEndpoint {
    /// The wire and stored name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vendor => "vendor",
            Self::Era => "era",
        }
    }
}

/// `Connector.account`: whose workspace or Drive was connected, tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConnectorAccount {
    /// From Slack's `auth.test` at connect.
    Slack {
        /// The workspace's team id; one pacer and one token per team.
        team_id: String,
        /// The workspace's name.
        team_name: String,
        /// The workspace URL, which permalinks are built from.
        url: String,
    },
    /// From the token response's `id_token`, or `about.get`.
    GoogleDrive {
        /// The Google account.
        email: String,
    },
}

/// A chosen source: a Slack channel or the Drive folder.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub struct ConnectorSource {
    /// The vendor's id: a channel id (`"C024BE91L"`) or a folder id.
    pub id: String,
    /// `"field-reports"`, `"Farm records"`.
    pub name: String,
}

/// `Connector` ([connectors 5]): what `GET /gardens/{id}/connectors` lists and the connect
/// calls return. Tokens and the pending connect's state are never part of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Connector {
    /// The connector.
    pub id: ConnectorId,
    /// Its garden.
    pub garden_id: GardenId,
    /// Slack or Google Drive.
    pub kind: ConnectorKind,
    /// Where it stands.
    pub status: ConnectorStatus,
    /// `vendor`, or `era` for the demo garden's synthetic data.
    pub endpoint: ConnectorEndpoint,
    /// The connected workspace or account; absent while `pending`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub account: Option<ConnectorAccount>,
    /// The chosen channels (Slack, up to 10) or folder (Drive, 1); empty until chosen.
    pub sources: Vec<ConnectorSource>,
    /// When the last sync finished; absent before the first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub synced_at: Option<Timestamp>,
    /// Synced records (chunks) held for the garden by this connector.
    pub record_count: u32,
    /// When the connect started.
    pub created_at: Timestamp,
}

/// `GET /gardens/{id}/connectors` ([connectors 11]). `available: false` when connectors are off
/// on this server (no `CONNECTOR_KEY`), with an empty list, as beliefs do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ConnectorList {
    /// Connectors are configured on this server.
    pub available: bool,
    /// The garden's connectors, oldest first; empty when not available.
    pub connectors: Vec<Connector>,
}

impl ConnectorList {
    /// Connectors are off: `{ available: false, connectors: [] }`.
    pub const fn unavailable() -> Self {
        Self {
            available: false,
            connectors: Vec::new(),
        }
    }
}

/// `POST /gardens/{id}/connectors` (owner): starts a connect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateConnectorRequest {
    /// Which vendor.
    pub kind: ConnectorKind,
    /// `SHA-256(device_verifier)`; `complete` must bring the verifier.
    pub device_hash: DeviceHash,
}

/// `POST /gardens/{id}/connectors`' answer: the `pending` connector and the vendor's consent
/// URL, opened with `openAuthSessionAsync`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CreateConnectorResponse {
    /// The pending connector.
    pub connector: Connector,
    /// The vendor's consent page, with `state` = HMAC(connector, nonce, expiry).
    pub authorize_url: String,
}

/// `GET /connectors/oauth/callback?code&state` (public): what the vendor sends back. Not
/// `deny_unknown_fields`: the vendors add their own parameters (Google's `scope`, `authuser`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct OAuthCallbackQuery {
    /// The authorization code; absent when the owner declined.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// The state the API signed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// The vendor's error, such as `access_denied`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// `POST /connectors/{id}/complete` (the owner who started it): exchanges the code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CompleteConnectorRequest {
    /// The vendor's authorization code, from the redirect to the app.
    pub code: String,
    /// The state from the same redirect.
    pub state: String,
    /// The 32 random bytes whose hash started the connect.
    pub device_verifier: DeviceVerifier,
}

/// `GET /connectors/{id}/available-sources` (owner): the channels the bot is a member of, or
/// the folders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AvailableSources {
    /// What the owner may choose from.
    pub sources: Vec<ConnectorSource>,
}

/// One chosen source in `PUT /connectors/{id}/sources`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceChoice {
    /// A channel id or a folder id from `available-sources`.
    pub id: String,
}

/// `PUT /connectors/{id}/sources` (owner): Slack up to 10 channels, Drive 1 folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SetSourcesRequest {
    /// The chosen sources; replaces the previous choice.
    pub sources: Vec<SourceChoice>,
}

/// `connector.last_error` ([connectors 6]): a fixed code, never the vendor's text, which may
/// echo record text. Stored, not sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncErrorCode {
    /// The vendor answered `429`; the cursor is saved and the next run continues.
    RateLimited,
    /// The vendor was down; the next attempt is 15 minutes later.
    VendorUnavailable,
    /// The vendor refused the token: `reauth_needed`, nothing runs until a reconnect.
    AuthRefused,
    /// The garden would exceed 10,000 records: nothing runs until the sources change.
    TooLarge,
}

impl SyncErrorCode {
    /// The stored name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RateLimited => "rate_limited",
            Self::VendorUnavailable => "vendor_unavailable",
            Self::AuthRefused => "auth_refused",
            Self::TooLarge => "too_large",
        }
    }
}

/// `farm_record.kind`, and `search_farm_records`' `kind?` filter.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FarmRecordKind {
    /// A Slack message, its thread's replies joined to it.
    Message,
    /// A chunk of a Drive document.
    Document,
}

/// `FarmCitation` ([connectors 7]): a synced farm record cited by a plan step or an assistant
/// message. Stored with the plan or message as it was, so a deleted record's citation stays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct FarmCitation {
    /// The `farm_record` cited.
    pub record_id: FarmRecordId,
    /// Slack or Google Drive.
    pub system: ConnectorKind,
    /// `"#field-reports"`, `"Spray log 2026"`.
    pub title: String,
    /// The author's display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub author: Option<String>,
    /// The message's time, or the document's `modifiedTime`.
    pub at: Timestamp,
    /// Opens the record in Slack or Drive (or, for an `era` connector, the simulator).
    pub link: String,
}

/// One `search_farm_records` result: the record's text, framed as data for the model, and its
/// citation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FarmRecordHit {
    /// One chunk, at most 2,000 characters.
    pub text: String,
    /// What the answer cites.
    pub citation: FarmCitation,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const CONNECTOR: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5C";
    const GARDEN: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5G";
    const RECORD: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5C.0.C024BE91L/1729000000.000100";
    const HASH: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

    fn connector() -> Connector {
        Connector {
            id: CONNECTOR.parse().unwrap(),
            garden_id: GARDEN.parse().unwrap(),
            kind: ConnectorKind::Slack,
            status: ConnectorStatus::Active,
            endpoint: ConnectorEndpoint::Vendor,
            account: Some(ConnectorAccount::Slack {
                team_id: "T0123".to_owned(),
                team_name: "Maple Row Farm".to_owned(),
                url: "https://maplerow.slack.com/".to_owned(),
            }),
            sources: vec![ConnectorSource {
                id: "C024BE91L".to_owned(),
                name: "field-reports".to_owned(),
            }],
            synced_at: Some("2026-10-19T09:15:00Z".parse().unwrap()),
            record_count: 118,
            created_at: "2026-10-19T09:00:00Z".parse().unwrap(),
        }
    }

    #[test]
    fn a_connector_is_exactly_its_contract() {
        assert_eq!(
            serde_json::to_value(connector()).unwrap(),
            json!({
                "id": CONNECTOR,
                "garden_id": GARDEN,
                "kind": "slack",
                "status": "active",
                "endpoint": "vendor",
                "account": {
                    "kind": "slack", "team_id": "T0123", "team_name": "Maple Row Farm",
                    "url": "https://maplerow.slack.com/"
                },
                "sources": [{"id": "C024BE91L", "name": "field-reports"}],
                "synced_at": "2026-10-19T09:15:00Z",
                "record_count": 118,
                "created_at": "2026-10-19T09:00:00Z"
            })
        );
    }

    #[test]
    fn a_pending_connector_leaves_out_account_and_synced_at() {
        let pending = Connector {
            kind: ConnectorKind::GoogleDrive,
            status: ConnectorStatus::Pending,
            endpoint: ConnectorEndpoint::Era,
            account: None,
            sources: Vec::new(),
            synced_at: None,
            record_count: 0,
            ..connector()
        };
        assert_eq!(
            serde_json::to_value(&pending).unwrap(),
            json!({
                "id": CONNECTOR, "garden_id": GARDEN, "kind": "google_drive",
                "status": "pending", "endpoint": "era", "sources": [], "record_count": 0,
                "created_at": "2026-10-19T09:00:00Z"
            })
        );
        let drive = ConnectorAccount::GoogleDrive {
            email: "owner@example.org".to_owned(),
        };
        assert_eq!(
            serde_json::to_value(drive).unwrap(),
            json!({"kind": "google_drive", "email": "owner@example.org"})
        );
    }

    #[test]
    fn statuses_are_the_designs_four() {
        let all = [
            ConnectorStatus::Pending,
            ConnectorStatus::Active,
            ConnectorStatus::ReauthNeeded,
            ConnectorStatus::Deleting,
        ];
        for status in all {
            assert_eq!(
                serde_json::to_value(status).unwrap(),
                json!(status.as_str())
            );
        }
        assert_eq!(
            serde_json::to_value(ConnectorStatus::ReauthNeeded).unwrap(),
            json!("reauth_needed")
        );
        assert!(serde_json::from_value::<ConnectorStatus>(json!("syncing")).is_err());
        for kind in [ConnectorKind::Slack, ConnectorKind::GoogleDrive] {
            assert_eq!(serde_json::to_value(kind).unwrap(), json!(kind.as_str()));
        }
        for endpoint in [ConnectorEndpoint::Vendor, ConnectorEndpoint::Era] {
            assert_eq!(
                serde_json::to_value(endpoint).unwrap(),
                json!(endpoint.as_str())
            );
        }
        for code in [
            SyncErrorCode::RateLimited,
            SyncErrorCode::VendorUnavailable,
            SyncErrorCode::AuthRefused,
            SyncErrorCode::TooLarge,
        ] {
            assert_eq!(serde_json::to_value(code).unwrap(), json!(code.as_str()));
        }
    }

    #[test]
    fn connectors_off_is_available_false_with_an_empty_list() {
        assert_eq!(
            serde_json::to_value(ConnectorList::unavailable()).unwrap(),
            json!({"available": false, "connectors": []})
        );
    }

    #[test]
    fn the_connect_requests_refuse_unknown_fields() {
        let create: CreateConnectorRequest =
            serde_json::from_value(json!({"kind": "slack", "device_hash": HASH})).unwrap();
        assert_eq!(create.kind, ConnectorKind::Slack);
        assert_eq!(serde_json::to_value(&create).unwrap()["device_hash"], HASH);
        assert!(
            serde_json::from_value::<CreateConnectorRequest>(
                json!({"kind": "slack", "device_hash": HASH, "endpoint": "era"})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<CreateConnectorRequest>(
                json!({"kind": "slack", "device_hash": "short"})
            )
            .is_err()
        );
        let complete: CompleteConnectorRequest =
            serde_json::from_value(json!({"code": "c-1", "state": "s-1", "device_verifier": HASH}))
                .unwrap();
        assert_eq!(
            serde_json::to_value(&complete).unwrap(),
            json!({"code": "c-1", "state": "s-1", "device_verifier": HASH})
        );
        let sources: SetSourcesRequest =
            serde_json::from_value(json!({"sources": [{"id": "C024BE91L"}]})).unwrap();
        assert_eq!(sources.sources[0].id, "C024BE91L");
        assert!(
            serde_json::from_value::<SetSourcesRequest>(
                json!({"sources": [{"id": "C1", "name": "x"}]})
            )
            .is_err()
        );
    }

    #[test]
    fn the_callback_ignores_the_vendors_extra_parameters() {
        let q: OAuthCallbackQuery = serde_json::from_value(
            json!({"code": "4/0A", "state": "s", "scope": "drive.readonly", "authuser": "0"}),
        )
        .unwrap();
        assert_eq!(q.code.as_deref(), Some("4/0A"));
        assert_eq!(q.error, None);
    }

    #[test]
    fn a_farm_citation_is_exactly_its_contract() {
        let citation = FarmCitation {
            record_id: RECORD.parse().unwrap(),
            system: ConnectorKind::Slack,
            title: "#field-reports".to_owned(),
            author: Some("Sam".to_owned()),
            at: "2026-10-12T07:30:00Z".parse().unwrap(),
            link: "https://maplerow.slack.com/archives/C024BE91L/p1729000000000100".to_owned(),
        };
        assert_eq!(
            serde_json::to_value(&citation).unwrap(),
            json!({
                "record_id": RECORD,
                "system": "slack",
                "title": "#field-reports",
                "author": "Sam",
                "at": "2026-10-12T07:30:00Z",
                "link": "https://maplerow.slack.com/archives/C024BE91L/p1729000000000100"
            })
        );
        let anonymous = FarmCitation {
            author: None,
            ..citation.clone()
        };
        assert!(
            serde_json::to_value(&anonymous)
                .unwrap()
                .get("author")
                .is_none()
        );
        let hit = FarmRecordHit {
            text: "Rings on bed 3 again.".to_owned(),
            citation,
        };
        assert_eq!(
            serde_json::to_value(&hit).unwrap()["citation"]["record_id"],
            RECORD
        );
        assert_eq!(
            serde_json::to_value(FarmRecordKind::Document).unwrap(),
            json!("document")
        );
    }
}
