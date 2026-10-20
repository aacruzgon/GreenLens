//! The connect flow and the owner routes with connectors on (roadmap E3.3, E3.4; T-E3.3,
//! T-E3.4, T-E3.8 in docs/EON). The vendors are a mock of `ConnectVendors`: no route test makes
//! real HTTP (decision 8); the real exchanges are tested in the clients.

use std::sync::Mutex;

use axum::http::{HeaderMap, Method};
use mockall::{Sequence, predicate::eq};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};

use domain::{
    connectors::{
        Connector, ConnectorAccount, ConnectorEndpoint, ConnectorKind, ConnectorSource,
        ConnectorStatus, SyncErrorCode,
    },
    id::ConnectorId,
    ports::{
        Conflict, ConnectorRow, NewConnector, PendingConnect, Sealed, StoreError, SyncRequest,
    },
};

use super::*;
use crate::{
    connect::{Exchanged, MockConnectVendors, OAuthState, VendorError},
    sealing::{ConnectorKey, SealPurpose, StoredTokens},
};

const CONNECTOR: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5C";
const OTHER_USER: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5X";
const KEY_HEX: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
/// RFC 7636 appendix B's verifier and its `S256` hash: the app's device binding.
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const HASH: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
const NONCE: &str = "AAAAAAAAAAAAAAAAAAAAAA";

fn key() -> ConnectorKey {
    ConnectorKey::from_hex(KEY_HEX).unwrap()
}

fn id() -> ConnectorId {
    CONNECTOR.parse().unwrap()
}

fn now() -> Timestamp {
    NOW.parse().unwrap()
}

/// Connectors on, with both vendors' OAuth clients.
fn on() -> Config {
    config_with(&[
        ("CONNECTOR_KEY", KEY_HEX),
        ("SLACK_CLIENT_ID", "123.456"),
        ("SLACK_CLIENT_SECRET", "slack-secret"),
        ("GOOGLE_CLIENT_ID", "pdm.apps.googleusercontent.com"),
        ("GOOGLE_CLIENT_SECRET", "google-secret"),
    ])
}

struct Answer {
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
}

/// Sends `request` as a member with `role` and waits for the background tasks it started.
async fn send(
    mut store: FakeStore,
    config: Config,
    role: Option<Role>,
    vendors: MockConnectVendors,
    request: Request<Body>,
) -> Answer {
    let auth = signed_in(&mut store, role);
    let state = state_with_vendors(
        store,
        auth,
        config,
        vendors,
        (MockBlobStore::new(), no_images(), Arc::new(NoBeliefs)),
    );
    let response = crate::router::app(Arc::clone(&state))
        .oneshot(request)
        .await
        .unwrap();
    state.tasks.close();
    state.tasks.wait().await;
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    Answer {
        status,
        headers,
        body,
    }
}

fn request(method: Method, uri: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, "Bearer session-token");
    match body {
        Some(body) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

fn connector(kind: ConnectorKind, status: ConnectorStatus) -> Connector {
    Connector {
        id: id(),
        garden_id: GARDEN.parse().unwrap(),
        kind,
        status,
        endpoint: ConnectorEndpoint::Vendor,
        account: None,
        sources: Vec::new(),
        synced_at: None,
        record_count: 0,
        created_at: now(),
    }
}

fn slack_account() -> ConnectorAccount {
    ConnectorAccount::Slack {
        team_id: "T0123".to_owned(),
        team_name: "Maple Row Farm".to_owned(),
        url: "https://maplerow.slack.com/".to_owned(),
    }
}

fn source(id: &str) -> ConnectorSource {
    ConnectorSource {
        id: id.to_owned(),
        name: format!("name of {id}"),
    }
}

/// A pending connector started by `USER` with `HASH`, its state good for 10 minutes.
fn pending_row(kind: ConnectorKind) -> ConnectorRow {
    ConnectorRow {
        connector: connector(kind, ConnectorStatus::Pending),
        created_by: USER.parse().unwrap(),
        secret: None,
        oauth: Some(PendingConnect {
            nonce: NONCE.to_owned(),
            device_hash: HASH.parse().unwrap(),
            verifier: (kind == ConnectorKind::GoogleDrive)
                .then(|| key().seal(SealPurpose::Pkce, id(), VERIFIER.as_bytes())),
            expires_at: now().plus(Duration::from_mins(10)),
            claimed_at: None,
        }),
        cursors: BTreeMap::new(),
        next_sync_at: now(),
        last_error: None,
        sync_requested_at: None,
    }
}

/// An active Slack connector with channels `C1` and `C2` and a sealed bot token.
fn active_row() -> ConnectorRow {
    let mut c = connector(ConnectorKind::Slack, ConnectorStatus::Active);
    c.account = Some(slack_account());
    c.sources = vec![source("C1"), source("C2")];
    ConnectorRow {
        connector: c,
        created_by: USER.parse().unwrap(),
        secret: Some(StoredTokens::bearer(SecretString::from("xoxb-bot-token")).seal(&key(), id())),
        oauth: None,
        cursors: BTreeMap::new(),
        next_sync_at: now(),
        last_error: None,
        sync_requested_at: None,
    }
}

/// The demo garden's `era` connector.
fn era_row() -> ConnectorRow {
    let mut row = active_row();
    row.connector.endpoint = ConnectorEndpoint::Era;
    row
}

/// The vendors with no refresh due: Slack's bot token doesn't expire.
fn no_refresh(vendors: &mut MockConnectVendors) {
    vendors.expect_refresh().returning(|_, _| Ok(None));
}

/// The store lets `complete` claim the pending state once, and release it `released` times.
fn claims(store: &mut FakeStore, released: usize) {
    store
        .farm_record_store
        .expect_claim_connect()
        .withf(|_, c, nonce, at| *c == id() && nonce == NONCE && *at == now())
        .times(1)
        .returning(|_, _, _, _| Ok(()));
    store
        .farm_record_store
        .expect_release_connect()
        .withf(|_, c, nonce| *c == id() && nonce == NONCE)
        .times(released)
        .returning(|_, _, _| Ok(()));
}

/// A store whose `connector_row` answers `row` for `CONNECTOR`.
fn store_with(row: ConnectorRow) -> FakeStore {
    let mut store = FakeStore::default();
    store
        .farm_record_store
        .expect_connector_row()
        .with(eq(id()))
        .returning(move |_| Ok(Some(row.clone())));
    store
}

fn state_for(nonce: &str, expires_at: Timestamp) -> String {
    OAuthState {
        connector: id(),
        nonce: nonce.to_owned(),
        expires_at,
    }
    .encode(&key())
}

fn good_state() -> String {
    state_for(NONCE, now().plus(Duration::from_mins(10)))
}

fn complete_body(state: &str, verifier: &str) -> Value {
    json!({ "code": "the-code", "state": state, "device_verifier": verifier })
}

fn complete_request(state: &str, verifier: &str) -> Request<Body> {
    request(
        Method::POST,
        &format!("/connectors/{CONNECTOR}/complete"),
        Some(complete_body(state, verifier)),
    )
}

fn callback(query: &str) -> Request<Body> {
    Request::get(format!("/connectors/oauth/callback?{query}"))
        .body(Body::empty())
        .unwrap()
}

/// The `state` query parameter of a URL.
fn state_param(url: &str) -> String {
    url.split(['?', '&'])
        .find_map(|p| p.strip_prefix("state="))
        .unwrap()
        .to_owned()
}

// ---------------------------------------------------------------------------------------------
// Starting a connect
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_drive_connect_is_pending_with_a_sealed_verifier_and_a_signed_state() {
    let mut store = FakeStore::default();
    let created: Arc<Mutex<Option<NewConnector>>> = Arc::default();
    let seen = Arc::clone(&created);
    store
        .farm_record_store
        .expect_create_connector()
        .withf(|owner, c| {
            owner.garden().to_string() == GARDEN
                && c.status == ConnectorStatus::Pending
                && c.endpoint == ConnectorEndpoint::Vendor
                && c.kind == ConnectorKind::GoogleDrive
                && c.created_by.to_string() == USER
                && c.secret.is_none()
                && c.sources.is_empty()
        })
        .times(1)
        .returning(move |_, c| {
            let mut out = connector(c.kind, c.status);
            out.id = c.id;
            *seen.lock().unwrap() = Some(c);
            Ok(out)
        });
    let answer = send(
        store,
        on(),
        Some(Role::Owner),
        MockConnectVendors::new(),
        request(
            Method::POST,
            &format!("/gardens/{GARDEN}/connectors"),
            Some(json!({ "kind": "google_drive", "device_hash": HASH })),
        ),
    )
    .await;
    assert_eq!(answer.status, StatusCode::CREATED, "{}", answer.body);
    assert_eq!(answer.body["connector"]["status"], "pending");
    let url = answer.body["authorize_url"].as_str().unwrap();
    assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?client_id=pdm.apps.googleusercontent.com&redirect_uri=https%3A%2F%2Fapi.example.org%2Fconnectors%2Foauth%2Fcallback&"), "{url}");

    let new = created.lock().unwrap().take().unwrap();
    let oauth = new.oauth.unwrap();
    assert_eq!(oauth.device_hash.as_str(), HASH);
    assert_eq!(oauth.expires_at, now().plus(Duration::from_mins(10)));
    // The state names the connector and carries the stored nonce and expiry.
    let state = OAuthState::decode(&state_param(url), &key(), now()).unwrap();
    assert_eq!(state.connector, new.id);
    assert_eq!(state.nonce, oauth.nonce);
    assert_eq!(state.expires_at, oauth.expires_at);
    // The verifier is sealed for this connector as PKCE, and the URL carries its challenge.
    let verifier = key()
        .unseal(SealPurpose::Pkce, new.id, &oauth.verifier.unwrap())
        .unwrap();
    let pkce = integrations::google_drive::Pkce::from_verifier(SecretString::from(
        std::str::from_utf8(verifier.expose_secret()).unwrap(),
    ))
    .unwrap();
    assert!(
        url.contains(&format!("&code_challenge={}", pkce.challenge())),
        "{url}"
    );
}

#[tokio::test]
async fn a_slack_connect_has_no_pkce() {
    let mut store = FakeStore::default();
    store
        .farm_record_store
        .expect_create_connector()
        .withf(|_, c| c.oauth.as_ref().is_some_and(|o| o.verifier.is_none()))
        .returning(|_, c| Ok(connector(c.kind, c.status)));
    let answer = send(
        store,
        on(),
        Some(Role::Owner),
        MockConnectVendors::new(),
        request(
            Method::POST,
            &format!("/gardens/{GARDEN}/connectors"),
            Some(json!({ "kind": "slack", "device_hash": HASH })),
        ),
    )
    .await;
    assert_eq!(answer.status, StatusCode::CREATED);
    let url = answer.body["authorize_url"].as_str().unwrap();
    assert!(url.starts_with("https://slack.com/oauth/v2/authorize?client_id=123.456&"));
    assert!(!url.contains("code_challenge"));
}

#[tokio::test]
async fn a_connect_to_a_vendor_without_an_oauth_client_is_409() {
    let mut store = FakeStore::default();
    store.farm_record_store.expect_create_connector().never();
    let answer = send(
        store,
        config_with(&[("CONNECTOR_KEY", KEY_HEX)]),
        Some(Role::Owner),
        MockConnectVendors::new(),
        request(
            Method::POST,
            &format!("/gardens/{GARDEN}/connectors"),
            Some(json!({ "kind": "slack", "device_hash": HASH })),
        ),
    )
    .await;
    assert_eq!(answer.status, StatusCode::CONFLICT);
    assert_eq!(answer.body["error"]["code"], "invalid_state");
    assert_eq!(answer.body["error"]["retryable"], false);
}

// ---------------------------------------------------------------------------------------------
// T-E3.3: the state at the callback and at complete
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_callback_redirects_to_the_app_with_the_code_and_state() {
    let state = good_state();
    let answer = send(
        store_with(pending_row(ConnectorKind::Slack)),
        on(),
        None,
        MockConnectVendors::new(),
        callback(&format!("code=c%2F1&state={state}&scope=x&authuser=0")),
    )
    .await;
    assert_eq!(answer.status, StatusCode::FOUND);
    assert_eq!(
        answer.headers[header::LOCATION],
        format!("pdm://connectors/{CONNECTOR}/callback?code=c%2F1&state={state}").as_str()
    );
    assert_eq!(answer.headers[header::CACHE_CONTROL], "no-store");
}

#[tokio::test]
async fn a_declined_consent_returns_the_vendors_error_to_the_app() {
    let state = good_state();
    let answer = send(
        store_with(pending_row(ConnectorKind::GoogleDrive)),
        on(),
        None,
        MockConnectVendors::new(),
        callback(&format!("error=access_denied&state={state}")),
    )
    .await;
    assert_eq!(answer.status, StatusCode::FOUND);
    assert_eq!(
        answer.headers[header::LOCATION],
        format!("pdm://connectors/{CONNECTOR}/callback?error=access_denied&state={state}").as_str()
    );
}

/// A10: the vendor's `error` reaches the app only as a short `[a-z_]` code; anything else
/// becomes `vendor_error`.
#[tokio::test]
async fn the_callback_passes_on_only_a_short_vendor_error_code() {
    let longest = "a".repeat(64);
    let long = "a".repeat(65);
    for (error, expected) in [
        ("server_error", "server_error"),
        ("Access%20Denied", "vendor_error"),
        ("javascript%3Aalert(1)", "vendor_error"),
        (longest.as_str(), longest.as_str()),
        (long.as_str(), "vendor_error"),
        ("", "vendor_error"),
    ] {
        let state = good_state();
        let answer = send(
            store_with(pending_row(ConnectorKind::GoogleDrive)),
            on(),
            None,
            MockConnectVendors::new(),
            callback(&format!("error={error}&state={state}")),
        )
        .await;
        assert_eq!(answer.status, StatusCode::FOUND, "{error}");
        assert_eq!(
            answer.headers[header::LOCATION],
            format!("pdm://connectors/{CONNECTOR}/callback?error={expected}&state={state}")
                .as_str(),
            "{error}"
        );
    }
}

/// The three ways a state goes bad: a changed character, its expiry passed, and a state already
/// used (the exchange cleared the pending connector's nonce and activated it).
fn bad_states() -> Vec<(&'static str, String, ConnectorRow)> {
    let good = good_state();
    let mut tampered = good.clone().into_bytes();
    let at = good.find('.').unwrap() + 3;
    tampered[at] = if tampered[at] == b'B' { b'C' } else { b'B' };
    let mut used = active_row();
    used.connector.status = ConnectorStatus::Active;
    vec![
        (
            "tampered",
            String::from_utf8(tampered).unwrap(),
            pending_row(ConnectorKind::Slack),
        ),
        (
            "expired",
            state_for(NONCE, now()),
            pending_row(ConnectorKind::Slack),
        ),
        ("reused", good, used),
    ]
}

#[tokio::test]
async fn a_tampered_expired_or_reused_state_is_400_at_the_callback() {
    for (case, state, row) in bad_states() {
        let answer = send(
            store_with(row),
            on(),
            None,
            MockConnectVendors::new(),
            callback(&format!("code=c&state={state}")),
        )
        .await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{case}");
        assert_eq!(answer.body["error"]["code"], "invalid_request", "{case}");
        assert_eq!(answer.body["error"]["details"]["field"], "state", "{case}");
        assert!(answer.headers.get(header::LOCATION).is_none(), "{case}");
    }
}

#[tokio::test]
async fn a_tampered_expired_or_reused_state_is_400_at_complete() {
    for (case, state, row) in bad_states() {
        let mut vendors = MockConnectVendors::new();
        vendors.expect_exchange().never();
        let mut store = store_with(row);
        store.farm_record_store.expect_complete_connect().never();
        store
            .farm_record_store
            .expect_start_connector_deletion()
            .never();
        let answer = send(
            store,
            on(),
            Some(Role::Owner),
            vendors,
            complete_request(&state, VERIFIER),
        )
        .await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{case}");
        assert_eq!(answer.body["error"]["details"]["field"], "state", "{case}");
    }
}

#[tokio::test]
async fn a_state_for_another_connector_is_400_at_complete() {
    let other: ConnectorId = "01JAX3K9T6R2B8V0QZ4M7D1E5D".parse().unwrap();
    let state = OAuthState {
        connector: other,
        nonce: NONCE.to_owned(),
        expires_at: now().plus(Duration::from_mins(10)),
    }
    .encode(&key());
    let answer = send(
        store_with(pending_row(ConnectorKind::Slack)),
        on(),
        Some(Role::Owner),
        MockConnectVendors::new(),
        complete_request(&state, VERIFIER),
    )
    .await;
    assert_eq!(answer.status, StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------------------------
// Completing
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn complete_exchanges_the_code_and_seals_the_tokens() {
    let mut vendors = MockConnectVendors::new();
    vendors
        .expect_exchange()
        .withf(|kind, code, verifier, redirect| {
            *kind == ConnectorKind::GoogleDrive
                && code == "the-code"
                && verifier.as_ref().map(ExposeSecret::expose_secret) == Some(VERIFIER)
                && redirect == "https://api.example.org/connectors/oauth/callback"
        })
        .times(1)
        .returning(|_, _, _, _| {
            Ok(Exchanged {
                tokens: StoredTokens {
                    access_token: SecretString::from("ya29.access"),
                    refresh_token: Some(SecretString::from("1//refresh")),
                    expires_at: Some("2026-10-18T15:00:00Z".parse().unwrap()),
                },
                account: ConnectorAccount::GoogleDrive {
                    email: "owner@example.org".to_owned(),
                },
            })
        });
    let mut store = store_with(pending_row(ConnectorKind::GoogleDrive));
    claims(&mut store, 0);
    store
        .farm_record_store
        .expect_complete_connect()
        .withf(|_, c, done| {
            let tokens = StoredTokens::unseal(&key(), *c, &done.secret).unwrap();
            *c == id()
                && done.nonce == NONCE
                && done.now == now()
                && tokens.access_token.expose_secret() == "ya29.access"
                && tokens
                    .refresh_token
                    .as_ref()
                    .is_some_and(|t| t.expose_secret() == "1//refresh")
        })
        .times(1)
        .returning(|_, _, done| {
            let mut c = connector(ConnectorKind::GoogleDrive, ConnectorStatus::Active);
            c.account = Some(done.account);
            Ok(c)
        });
    let answer = send(
        store,
        on(),
        Some(Role::Owner),
        vendors,
        complete_request(&good_state(), VERIFIER),
    )
    .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    assert_eq!(answer.body["status"], "active");
    assert_eq!(answer.body["account"]["email"], "owner@example.org");
}

#[tokio::test]
async fn a_code_the_vendor_refuses_is_400_and_the_connector_stays_pending() {
    let mut vendors = MockConnectVendors::new();
    vendors.expect_exchange().returning(|kind, _, _, _| {
        Err(VendorError::AuthRefused {
            vendor: kind,
            code: "invalid_code".to_owned(),
        })
    });
    let mut store = store_with(pending_row(ConnectorKind::Slack));
    claims(&mut store, 1);
    store.farm_record_store.expect_complete_connect().never();
    let answer = send(
        store,
        on(),
        Some(Role::Owner),
        vendors,
        complete_request(&good_state(), VERIFIER),
    )
    .await;
    assert_eq!(answer.status, StatusCode::BAD_REQUEST);
    assert_eq!(answer.body["error"]["details"]["field"], "code");
}

/// A3: of two `complete`s with the same state, the one that loses the claim is refused
/// before the exchange, so the code is spent at most once. (The claim's race itself runs on
/// the embedded store: `two_claims_at_once_spend_the_state_once`.)
#[tokio::test]
async fn a_complete_that_loses_the_claim_never_exchanges_the_code() {
    let mut vendors = MockConnectVendors::new();
    vendors.expect_exchange().never();
    let mut store = store_with(pending_row(ConnectorKind::GoogleDrive));
    store
        .farm_record_store
        .expect_claim_connect()
        .times(1)
        .returning(|_, _, _, _| Err(StoreError::Conflict(Conflict::AlreadyExists)));
    store.farm_record_store.expect_release_connect().never();
    store.farm_record_store.expect_complete_connect().never();
    let answer = send(
        store,
        on(),
        Some(Role::Owner),
        vendors,
        complete_request(&good_state(), VERIFIER),
    )
    .await;
    assert_eq!(answer.status, StatusCode::BAD_REQUEST);
    assert_eq!(answer.body["error"]["details"]["field"], "state");
}

/// A3: the exchange gave tokens, but the final compare-and-set lost (the connector was deleted
/// or cleaned up meanwhile): nothing stores them, so the grant is revoked.
#[tokio::test]
async fn tokens_the_store_no_longer_takes_are_revoked() {
    let mut vendors = MockConnectVendors::new();
    vendors.expect_exchange().times(1).returning(|_, _, _, _| {
        Ok(Exchanged {
            tokens: StoredTokens {
                access_token: SecretString::from("ya29.access"),
                refresh_token: Some(SecretString::from("1//refresh")),
                expires_at: Some("2026-10-18T15:00:00Z".parse().unwrap()),
            },
            account: ConnectorAccount::GoogleDrive {
                email: "owner@example.org".to_owned(),
            },
        })
    });
    vendors
        .expect_revoke()
        .withf(|kind, tokens| {
            *kind == ConnectorKind::GoogleDrive
                && tokens
                    .refresh_token
                    .as_ref()
                    .is_some_and(|t| t.expose_secret() == "1//refresh")
        })
        .times(1)
        .returning(|_, _| Ok(()));
    let mut store = store_with(pending_row(ConnectorKind::GoogleDrive));
    claims(&mut store, 0);
    store
        .farm_record_store
        .expect_complete_connect()
        .times(1)
        .returning(|_, _, _| Err(StoreError::NotFound));
    store
        .farm_record_store
        .expect_shared_account_connectors()
        .with(
            eq(ConnectorKind::GoogleDrive),
            eq("owner@example.org".to_owned()),
            eq(id()),
        )
        .times(1)
        .returning(|_, _, _| Ok(0));
    let answer = send(
        store,
        on(),
        Some(Role::Owner),
        vendors,
        complete_request(&good_state(), VERIFIER),
    )
    .await;
    assert_eq!(answer.status, StatusCode::NOT_FOUND);
}

/// A store error after which the commit may have applied: the connection lost on the way
/// back.
fn lost_answer() -> StoreError {
    StoreError::Unavailable("connection reset".into())
}

/// A Google exchange that gives tokens, with the store's `connector_row` answering the
/// pending row for the route's checks and then `reread` for the read after the failed write,
/// which gets the sealed value the write sent. The write fails with `lost_answer`.
fn ambiguous_write(
    vendors: &mut MockConnectVendors,
    reread: impl Fn(Sealed) -> Result<Option<ConnectorRow>, StoreError> + Send + 'static,
) -> FakeStore {
    vendors
        .expect_exchange()
        .times(1)
        .returning(|_, _, _, _| Ok(google_exchanged()));
    let mut store = FakeStore::default();
    let mut seq = Sequence::new();
    store
        .farm_record_store
        .expect_connector_row()
        .with(eq(id()))
        .times(2)
        .in_sequence(&mut seq)
        .returning(|_| Ok(Some(pending_row(ConnectorKind::GoogleDrive))));
    let sent = Arc::new(Mutex::new(None::<Sealed>));
    let read = Arc::clone(&sent);
    store
        .farm_record_store
        .expect_connector_row()
        .with(eq(id()))
        .times(1)
        .in_sequence(&mut seq)
        .returning(move |_| reread(read.lock().unwrap().clone().unwrap()));
    claims(&mut store, 0);
    store
        .farm_record_store
        .expect_complete_connect()
        .times(1)
        .returning(move |_, _, done| {
            *sent.lock().unwrap() = Some(done.secret);
            Err(lost_answer())
        });
    store
}

/// Audit of PR 72, finding 2: a write whose answer was lost but whose commit applied leaves
/// the connector `active` with these tokens; they are kept, and the connect is answered.
#[tokio::test]
async fn a_lost_answer_after_the_commit_keeps_the_tokens_and_answers_200() {
    let mut vendors = MockConnectVendors::new();
    vendors.expect_revoke().never();
    let store = ambiguous_write(&mut vendors, |secret| {
        let mut row = active_row();
        row.connector.kind = ConnectorKind::GoogleDrive;
        row.connector.account = Some(ConnectorAccount::GoogleDrive {
            email: "owner@example.org".to_owned(),
        });
        row.secret = Some(secret);
        Ok(Some(row))
    });
    let answer = send(
        store,
        on(),
        Some(Role::Owner),
        vendors,
        complete_request(&good_state(), VERIFIER),
    )
    .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    assert_eq!(answer.body["status"], "active");
}

/// Finding 2: a lost answer whose commit didn't apply (the row is still pending): nothing
/// holds the tokens, so they are revoked.
#[tokio::test]
async fn a_lost_answer_before_the_commit_revokes_the_tokens() {
    let mut vendors = MockConnectVendors::new();
    vendors.expect_revoke().times(1).returning(|_, _| Ok(()));
    let mut store = ambiguous_write(&mut vendors, |_| {
        Ok(Some(pending_row(ConnectorKind::GoogleDrive)))
    });
    store
        .farm_record_store
        .expect_shared_account_connectors()
        .times(1)
        .returning(|_, _, _| Ok(0));
    let answer = send(
        store,
        on(),
        Some(Role::Owner),
        vendors,
        complete_request(&good_state(), VERIFIER),
    )
    .await;
    assert_eq!(
        answer.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        answer.body
    );
}

/// Finding 2: when the row can't be read either, the outcome is unknown, and tokens a live
/// connector may hold are left alone.
#[tokio::test]
async fn a_lost_answer_with_no_row_to_read_leaves_the_tokens() {
    let mut vendors = MockConnectVendors::new();
    vendors.expect_revoke().never();
    let store = ambiguous_write(&mut vendors, |_| Err(lost_answer()));
    let answer = send(
        store,
        on(),
        Some(Role::Owner),
        vendors,
        complete_request(&good_state(), VERIFIER),
    )
    .await;
    assert_eq!(
        answer.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        answer.body
    );
}

/// Our own request rejected (a misconfigured OAuth client: Slack's `bad_client_secret`,
/// Google's `invalid_client`) is a server error, never the owner's `400`.
#[tokio::test]
async fn a_rejected_client_at_complete_is_500_not_400() {
    let mut vendors = MockConnectVendors::new();
    vendors.expect_exchange().returning(|kind, _, _, _| {
        Err(VendorError::Rejected {
            vendor: kind,
            code: "bad_client_secret".to_owned(),
        })
    });
    let mut store = store_with(pending_row(ConnectorKind::Slack));
    claims(&mut store, 1);
    store.farm_record_store.expect_complete_connect().never();
    let answer = send(
        store,
        on(),
        Some(Role::Owner),
        vendors,
        complete_request(&good_state(), VERIFIER),
    )
    .await;
    assert_eq!(answer.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(answer.body["error"]["code"], "internal");
    assert!(!answer.body.to_string().contains("bad_client_secret"));
}

#[tokio::test]
async fn a_vendor_outage_at_complete_is_503_upstream_unavailable() {
    let mut vendors = MockConnectVendors::new();
    vendors.expect_exchange().returning(|kind, _, _, _| {
        Err(VendorError::Unavailable {
            vendor: kind,
            retry_after_s: Some(30),
            source: None,
        })
    });
    // The claim is released, so a retry of the same `complete` can claim again.
    let mut store = store_with(pending_row(ConnectorKind::Slack));
    claims(&mut store, 1);
    let answer = send(
        store,
        on(),
        Some(Role::Owner),
        vendors,
        complete_request(&good_state(), VERIFIER),
    )
    .await;
    assert_eq!(answer.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(answer.body["error"]["details"]["upstream"], "slack");
    assert_eq!(answer.body["error"]["retry_after_s"], 30);
}

/// Google's answer to a good code.
fn google_exchanged() -> Exchanged {
    Exchanged {
        tokens: StoredTokens {
            access_token: SecretString::from("ya29.access"),
            refresh_token: Some(SecretString::from("1//refresh")),
            expires_at: Some("2026-10-18T15:00:00Z".parse().unwrap()),
        },
        account: ConnectorAccount::GoogleDrive {
            email: "owner@example.org".to_owned(),
        },
    }
}

const IDEMPOTENCY_KEY: &str = "7d444840-9dc0-11d1-b245-5ffdce74fad2";

fn keyed_complete_request() -> Request<Body> {
    let mut request = complete_request(&good_state(), VERIFIER);
    request.headers_mut().insert(
        "idempotency-key",
        axum::http::HeaderValue::from_static(IDEMPOTENCY_KEY),
    );
    request
}

/// A vendor exchange that signals when it starts and then blocks its thread until the test
/// lets it answer `answer`, so a test can drop the request while the exchange is in flight.
/// Needs the multi-thread runtime.
fn held_exchange(
    vendors: &mut MockConnectVendors,
    seq: &mut Sequence,
    answer: fn() -> Result<Exchanged, VendorError>,
) -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
    let (started, on_start) = std::sync::mpsc::channel();
    let (release, on_release) = std::sync::mpsc::channel::<()>();
    let on_release = Mutex::new(on_release);
    vendors
        .expect_exchange()
        .times(1)
        .in_sequence(seq)
        .returning(move |_, _, _, _| {
            started.send(()).unwrap();
            on_release.lock().unwrap().recv().unwrap();
            answer()
        });
    (on_start, release)
}

/// Sends `request` and drops it once the exchange has started, as the route's timeout or a
/// client that went away does; then lets the exchange answer and waits for the tasks.
async fn drop_mid_exchange(
    state: &Arc<AppState>,
    request: Request<Body>,
    (on_start, release): (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>),
) {
    let call = tokio::spawn(crate::router::app(Arc::clone(state)).oneshot(request));
    tokio::task::spawn_blocking(move || on_start.recv().unwrap())
        .await
        .unwrap();
    // The handler only awaits the work's task, so it is dropped at once; a handler running
    // the exchange itself would still be blocked in it.
    call.abort();
    let dropped = tokio::time::timeout(Duration::from_secs(5), call)
        .await
        .expect("the handler doesn't run the exchange itself");
    assert!(dropped.unwrap_err().is_cancelled());
    release.send(()).unwrap();
}

/// Audit of PR 72, finding 1: the handler dropped mid-exchange doesn't cancel the claimed
/// work. The tokens are stored, and the answer is recorded under the idempotency key, so the
/// app's retry with that key gets it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_complete_dropped_mid_exchange_still_stores_and_records_its_answer() {
    let mut vendors = MockConnectVendors::new();
    let held = held_exchange(
        &mut vendors,
        &mut Sequence::new(),
        || Ok(google_exchanged()),
    );
    let mut store = store_with(pending_row(ConnectorKind::GoogleDrive));
    claims(&mut store, 0);
    store
        .farm_record_store
        .expect_complete_connect()
        .times(1)
        .returning(|_, _, done| {
            let mut c = connector(ConnectorKind::GoogleDrive, ConnectorStatus::Active);
            c.account = Some(done.account);
            Ok(c)
        });
    vendors.expect_revoke().never();
    store
        .idempotency_repo
        .expect_begin()
        .times(1)
        .returning(|_, _, _| Ok(IdempotencyBegin::Started));
    store
        .idempotency_repo
        .expect_finish()
        .withf(|scope, response| {
            scope.route == format!("POST /connectors/{CONNECTOR}/complete")
                && response.status == 200
                && response.body["status"] == "active"
        })
        .times(1)
        .returning(|_, _| Ok(()));
    store.idempotency_repo.expect_release().never();
    let auth = signed_in(&mut store, Some(Role::Owner));
    let state = state_with_vendors(
        store,
        auth,
        on(),
        vendors,
        (MockBlobStore::new(), no_images(), Arc::new(NoBeliefs)),
    );
    drop_mid_exchange(&state, keyed_complete_request(), held).await;
    state.tasks.close();
    state.tasks.wait().await;
}

/// Finding 1: an exchange that fails after its request was dropped still releases the claim
/// and the idempotency key, so the retry runs again and connects.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_complete_dropped_mid_outage_releases_its_claim_and_a_retry_connects() {
    let mut vendors = MockConnectVendors::new();
    let mut seq = Sequence::new();
    let held = held_exchange(&mut vendors, &mut seq, || {
        Err(VendorError::Unavailable {
            vendor: ConnectorKind::GoogleDrive,
            retry_after_s: None,
            source: None,
        })
    });
    vendors
        .expect_exchange()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_, _, _, _| Ok(google_exchanged()));
    let mut store = store_with(pending_row(ConnectorKind::GoogleDrive));
    store
        .farm_record_store
        .expect_claim_connect()
        .times(2)
        .returning(|_, _, _, _| Ok(()));
    store
        .farm_record_store
        .expect_release_connect()
        .withf(|_, c, nonce| *c == id() && nonce == NONCE)
        .times(1)
        .returning(|_, _, _| Ok(()));
    store
        .farm_record_store
        .expect_complete_connect()
        .times(1)
        .returning(|_, _, done| {
            let mut c = connector(ConnectorKind::GoogleDrive, ConnectorStatus::Active);
            c.account = Some(done.account);
            Ok(c)
        });
    store
        .idempotency_repo
        .expect_begin()
        .times(2)
        .returning(|_, _, _| Ok(IdempotencyBegin::Started));
    store
        .idempotency_repo
        .expect_release()
        .times(1)
        .returning(|_| Ok(()));
    store
        .idempotency_repo
        .expect_finish()
        .withf(|_, response| response.status == 200)
        .times(1)
        .returning(|_, _| Ok(()));
    let auth = signed_in(&mut store, Some(Role::Owner));
    let state = state_with_vendors(
        store,
        auth,
        on(),
        vendors,
        (MockBlobStore::new(), no_images(), Arc::new(NoBeliefs)),
    );
    drop_mid_exchange(&state, keyed_complete_request(), held).await;
    // The first task is done once its claim and key are released.
    while !state.tasks.is_empty() {
        tokio::task::yield_now().await;
    }
    let response = crate::router::app(Arc::clone(&state))
        .oneshot(keyed_complete_request())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    state.tasks.close();
    state.tasks.wait().await;
}

/// Finding 1: a panic in the claimed exchange (a vendor client's bug) is `500`, and the claim
/// is released, so a retry can claim again.
#[tokio::test]
async fn a_panicking_exchange_releases_the_claim() {
    let mut vendors = MockConnectVendors::new();
    vendors
        .expect_exchange()
        .times(1)
        .returning(|_, _, _, _| panic!("a vendor client bug"));
    let mut store = store_with(pending_row(ConnectorKind::Slack));
    claims(&mut store, 1);
    store.farm_record_store.expect_complete_connect().never();
    let answer = send(
        store,
        on(),
        Some(Role::Owner),
        vendors,
        complete_request(&good_state(), VERIFIER),
    )
    .await;
    assert_eq!(answer.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(answer.body["error"]["code"], "internal");
}

/// T-E3.4: a `complete` from another owner than the starter, or with the wrong
/// `device_verifier`, is `404` and deletes the pending connector; the code is never exchanged.
#[tokio::test]
async fn complete_from_another_user_or_device_is_404_and_deletes_the_pending_connector() {
    let mut by_other = pending_row(ConnectorKind::Slack);
    by_other.created_by = OTHER_USER.parse().unwrap();
    let cases = [
        ("another user", by_other, VERIFIER.to_owned()),
        (
            "another device",
            pending_row(ConnectorKind::Slack),
            "B".repeat(43),
        ),
    ];
    for (case, row, verifier) in cases {
        let mut vendors = MockConnectVendors::new();
        vendors.expect_exchange().never();
        vendors.expect_revoke().never();
        let mut store = store_with(row.clone());
        let mut seq = Sequence::new();
        let mut deleting = row;
        deleting.connector.status = ConnectorStatus::Deleting;
        store
            .farm_record_store
            .expect_start_connector_deletion()
            .withf(|_, c| *c == id())
            .times(1)
            .in_sequence(&mut seq)
            .returning(move |_, _| Ok(deleting.clone()));
        store
            .farm_record_store
            .expect_delete_connector_batch()
            .with(eq(id()))
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_| Ok(0));
        store.farm_record_store.expect_complete_connect().never();
        let answer = send(
            store,
            on(),
            Some(Role::Owner),
            vendors,
            complete_request(&good_state(), &verifier),
        )
        .await;
        assert_eq!(answer.status, StatusCode::NOT_FOUND, "{case}");
        assert_eq!(answer.body["error"]["code"], "not_found", "{case}");
    }
}

// ---------------------------------------------------------------------------------------------
// T-E3.4: owners only, and never an `era` connector
// ---------------------------------------------------------------------------------------------

/// Every owner route, with a body each accepts.
fn owner_routes() -> Vec<Request<Body>> {
    vec![
        complete_request(&good_state(), VERIFIER),
        request(
            Method::GET,
            &format!("/connectors/{CONNECTOR}/available-sources"),
            None,
        ),
        request(
            Method::PUT,
            &format!("/connectors/{CONNECTOR}/sources"),
            Some(json!({ "sources": [{ "id": "C1" }] })),
        ),
        request(Method::POST, &format!("/connectors/{CONNECTOR}/sync"), None),
        request(Method::DELETE, &format!("/connectors/{CONNECTOR}"), None),
    ]
}

#[tokio::test]
async fn non_owners_get_403_on_every_owner_route() {
    for role in [Role::Editor, Role::Viewer] {
        let mut routes = owner_routes();
        routes.push(request(
            Method::POST,
            &format!("/gardens/{GARDEN}/connectors"),
            Some(json!({ "kind": "slack", "device_hash": HASH })),
        ));
        for request in routes {
            let route = format!("{role:?} {} {}", request.method(), request.uri());
            let mut store = FakeStore::default();
            store.farm_record_store.expect_connector_row().never();
            store.farm_record_store.expect_create_connector().never();
            let answer = send(store, on(), Some(role), MockConnectVendors::new(), request).await;
            assert_eq!(answer.status, StatusCode::FORBIDDEN, "{route}");
            assert_eq!(
                answer.body["error"]["details"]["required_role"], "owner",
                "{route}"
            );
        }
    }
}

#[tokio::test]
async fn every_owner_route_refuses_an_era_connector() {
    for request in owner_routes() {
        let route = format!("{} {}", request.method(), request.uri());
        let mut vendors = MockConnectVendors::new();
        vendors.expect_exchange().never();
        vendors.expect_available_sources().never();
        vendors.expect_revoke().never();
        let mut store = store_with(era_row());
        store.farm_record_store.expect_set_sources().never();
        store.farm_record_store.expect_request_sync().never();
        store
            .farm_record_store
            .expect_start_connector_deletion()
            .never();
        let answer = send(store, on(), Some(Role::Owner), vendors, request).await;
        assert_eq!(answer.status, StatusCode::CONFLICT, "{route}");
        assert_eq!(answer.body["error"]["code"], "invalid_state", "{route}");
    }
}

// ---------------------------------------------------------------------------------------------
// Sources
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn available_sources_lists_the_vendors_channels() {
    let mut vendors = MockConnectVendors::new();
    no_refresh(&mut vendors);
    vendors
        .expect_available_sources()
        .withf(|kind, tokens| {
            *kind == ConnectorKind::Slack && tokens.access_token.expose_secret() == "xoxb-bot-token"
        })
        .returning(|_, _| Ok(vec![source("C1"), source("C3")]));
    let answer = send(
        store_with(active_row()),
        on(),
        Some(Role::Owner),
        vendors,
        request(
            Method::GET,
            &format!("/connectors/{CONNECTOR}/available-sources"),
            None,
        ),
    )
    .await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(
        answer.body,
        json!({ "sources": [
            { "id": "C1", "name": "name of C1" }, { "id": "C3", "name": "name of C3" }
        ] })
    );
}

/// A2: Google's access token about to expire is refreshed for the call and stored over the
/// sealed value it came from. Here another refresh was stored first (the compare-and-set
/// answers `false`), and this call still uses its own token.
#[tokio::test]
async fn an_expiring_google_token_is_refreshed_and_stored_over_the_one_read() {
    let mut row = active_row();
    row.connector.kind = ConnectorKind::GoogleDrive;
    row.connector.account = Some(ConnectorAccount::GoogleDrive {
        email: "owner@example.org".to_owned(),
    });
    let read = StoredTokens {
        access_token: SecretString::from("ya29.old"),
        refresh_token: Some(SecretString::from("1//refresh")),
        expires_at: Some(now().plus(Duration::from_secs(60))),
    }
    .seal(&key(), id());
    row.secret = Some(read.clone());
    let mut vendors = MockConnectVendors::new();
    vendors
        .expect_refresh()
        .withf(|kind, tokens| {
            *kind == ConnectorKind::GoogleDrive && tokens.access_token.expose_secret() == "ya29.old"
        })
        .times(1)
        .returning(|_, _| {
            Ok(Some(StoredTokens {
                access_token: SecretString::from("ya29.new"),
                refresh_token: Some(SecretString::from("1//refresh")),
                expires_at: Some(now().plus(Duration::from_secs(3600))),
            }))
        });
    vendors
        .expect_available_sources()
        .withf(|_, tokens| tokens.access_token.expose_secret() == "ya29.new")
        .times(1)
        .returning(|_, _| Ok(vec![source("folder-1")]));
    let mut store = store_with(row);
    store
        .farm_record_store
        .expect_replace_secret()
        .withf(move |c, old, new| {
            let stored = StoredTokens::unseal(&key(), *c, new).unwrap();
            *c == id() && *old == read && stored.access_token.expose_secret() == "ya29.new"
        })
        .times(1)
        .returning(|_, _, _| Ok(false));
    let answer = send(
        store,
        on(),
        Some(Role::Owner),
        vendors,
        request(
            Method::GET,
            &format!("/connectors/{CONNECTOR}/available-sources"),
            None,
        ),
    )
    .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    assert_eq!(answer.body["sources"][0]["id"], "folder-1");
}

#[tokio::test]
async fn a_refused_token_makes_the_connector_reauth_needed() {
    let mut vendors = MockConnectVendors::new();
    no_refresh(&mut vendors);
    vendors.expect_available_sources().returning(|kind, _| {
        Err(VendorError::AuthRefused {
            vendor: kind,
            code: "token_revoked".to_owned(),
        })
    });
    let mut store = store_with(active_row());
    store
        .farm_record_store
        .expect_set_status()
        .with(
            eq(id()),
            eq(ConnectorStatus::ReauthNeeded),
            eq(Some(SyncErrorCode::AuthRefused)),
        )
        .times(1)
        .returning(|_, _, _| Ok(()));
    let answer = send(
        store,
        on(),
        Some(Role::Owner),
        vendors,
        request(
            Method::GET,
            &format!("/connectors/{CONNECTOR}/available-sources"),
            None,
        ),
    )
    .await;
    assert_eq!(answer.status, StatusCode::CONFLICT);
    assert_eq!(answer.body["error"]["details"]["status"], "reauth_needed");
}

#[tokio::test]
async fn a_pending_connector_has_no_sources_to_offer() {
    let mut vendors = MockConnectVendors::new();
    vendors.expect_available_sources().never();
    let answer = send(
        store_with(pending_row(ConnectorKind::Slack)),
        on(),
        Some(Role::Owner),
        vendors,
        request(
            Method::GET,
            &format!("/connectors/{CONNECTOR}/available-sources"),
            None,
        ),
    )
    .await;
    assert_eq!(answer.status, StatusCode::CONFLICT);
    assert_eq!(answer.body["error"]["details"]["status"], "pending");
}

/// T-E3.4: an 11th channel, or a 2nd folder, is refused before the vendor or the store is asked.
#[tokio::test]
async fn too_many_sources_are_refused() {
    let mut drive = active_row();
    drive.connector.kind = ConnectorKind::GoogleDrive;
    drive.connector.sources = vec![source("F1")];
    let eleven: Vec<Value> = (0..11).map(|i| json!({ "id": format!("C{i}") })).collect();
    let cases = [
        ("11 channels", active_row(), json!({ "sources": eleven })),
        (
            "2 folders",
            drive,
            json!({ "sources": [{ "id": "F1" }, { "id": "F2" }] }),
        ),
        (
            "a channel twice",
            active_row(),
            json!({ "sources": [{ "id": "C1" }, { "id": "C1" }] }),
        ),
    ];
    for (case, row, body) in cases {
        let mut vendors = MockConnectVendors::new();
        vendors.expect_available_sources().never();
        let mut store = store_with(row);
        store.farm_record_store.expect_set_sources().never();
        let answer = send(
            store,
            on(),
            Some(Role::Owner),
            vendors,
            request(
                Method::PUT,
                &format!("/connectors/{CONNECTOR}/sources"),
                Some(body),
            ),
        )
        .await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{case}");
        assert_eq!(answer.body["error"]["code"], "invalid_request", "{case}");
    }
}

#[tokio::test]
async fn ten_channels_are_allowed() {
    let ten: Vec<Value> = (0..10).map(|i| json!({ "id": format!("C{i}") })).collect();
    let mut vendors = MockConnectVendors::new();
    no_refresh(&mut vendors);
    vendors
        .expect_available_sources()
        .returning(|_, _| Ok((0..12).map(|i| source(&format!("C{i}"))).collect()));
    let mut store = store_with(active_row());
    store
        .farm_record_store
        .expect_set_sources()
        .withf(|_, _, sources, _| sources.len() == 10 && sources[3] == source("C3"))
        .returning(|_, _, sources, _| {
            let mut c = active_row().connector;
            c.sources = sources;
            Ok(c)
        });
    // C1 and C2 stay chosen: nothing is deleted.
    store
        .farm_record_store
        .expect_delete_source_records()
        .never();
    let answer = send(
        store,
        on(),
        Some(Role::Owner),
        vendors,
        request(
            Method::PUT,
            &format!("/connectors/{CONNECTOR}/sources"),
            Some(json!({ "sources": ten })),
        ),
    )
    .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    assert_eq!(answer.body["sources"].as_array().unwrap().len(), 10);
}

#[tokio::test]
async fn a_source_the_vendor_doesnt_offer_is_refused() {
    let mut vendors = MockConnectVendors::new();
    no_refresh(&mut vendors);
    vendors
        .expect_available_sources()
        .returning(|_, _| Ok(vec![source("C1")]));
    let mut store = store_with(active_row());
    store.farm_record_store.expect_set_sources().never();
    let answer = send(
        store,
        on(),
        Some(Role::Owner),
        vendors,
        request(
            Method::PUT,
            &format!("/connectors/{CONNECTOR}/sources"),
            Some(json!({ "sources": [{ "id": "C1" }, { "id": "C9" }] })),
        ),
    )
    .await;
    assert_eq!(answer.status, StatusCode::BAD_REQUEST);
    assert_eq!(answer.body["error"]["details"]["field"], "sources[1].id");
}

/// T-E3.8 at the route (with a mocked store; the embedded-store half is in `store`'s
/// `dropping_a_channel_deletes_its_records_and_keeps_the_others`): dropping `C1` stores the new
/// choice, then deletes `C1`'s records in batches; `C2`'s are never touched.
#[tokio::test]
async fn dropping_a_channel_deletes_only_its_records() {
    let mut vendors = MockConnectVendors::new();
    no_refresh(&mut vendors);
    vendors
        .expect_available_sources()
        .returning(|_, _| Ok(vec![source("C1"), source("C2")]));
    let mut store = FakeStore::default();
    let mut seq = Sequence::new();
    store
        .farm_record_store
        .expect_connector_row()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| Ok(Some(active_row())));
    store
        .farm_record_store
        .expect_set_sources()
        .withf(|_, c, sources, at| *c == id() && *sources == [source("C2")] && *at == now())
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_, _, sources, _| {
            let mut c = active_row().connector;
            c.sources = sources;
            c.record_count = 3;
            Ok(c)
        });
    for deleted in [500, 12, 0] {
        store
            .farm_record_store
            .expect_delete_source_records()
            .with(eq(id()), eq("C1".to_owned()))
            .times(1)
            .in_sequence(&mut seq)
            .returning(move |_, _| Ok(deleted));
    }
    store
        .farm_record_store
        .expect_connector_row()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut row = active_row();
            row.connector.sources = vec![source("C2")];
            row.connector.record_count = 40;
            Ok(Some(row))
        });
    let answer = send(
        store,
        on(),
        Some(Role::Owner),
        vendors,
        request(
            Method::PUT,
            &format!("/connectors/{CONNECTOR}/sources"),
            Some(json!({ "sources": [{ "id": "C2" }] })),
        ),
    )
    .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    assert_eq!(
        answer.body["sources"],
        json!([{ "id": "C2", "name": "name of C2" }])
    );
    assert_eq!(answer.body["record_count"], 40);
}

// ---------------------------------------------------------------------------------------------
// Sync now and disconnect
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn sync_now_is_202_then_429_with_the_wait() {
    for (outcome, status, wait) in [
        (SyncRequest::Accepted, StatusCode::ACCEPTED, None),
        (
            SyncRequest::TooSoon {
                allowed_at: now().plus(Duration::from_secs(90)),
            },
            StatusCode::TOO_MANY_REQUESTS,
            Some(90),
        ),
    ] {
        let mut store = store_with(active_row());
        store
            .farm_record_store
            .expect_request_sync()
            .withf(|_, c, at| *c == id() && *at == now())
            .times(1)
            .returning(move |_, _, _| Ok(outcome));
        let answer = send(
            store,
            on(),
            Some(Role::Owner),
            MockConnectVendors::new(),
            request(Method::POST, &format!("/connectors/{CONNECTOR}/sync"), None),
        )
        .await;
        assert_eq!(answer.status, status);
        if let Some(wait) = wait {
            assert_eq!(answer.body["error"]["code"], "rate_limited");
            assert_eq!(answer.body["error"]["retry_after_s"], wait);
            assert_eq!(answer.body["error"]["retryable"], true);
            assert_eq!(answer.headers[header::RETRY_AFTER], "90");
        }
    }
}

/// The store marks the connector first, then the token is revoked, then the records go.
#[tokio::test]
async fn a_disconnect_revokes_after_the_store_marks_it_then_deletes_the_records() {
    let mut seq = Sequence::new();
    let mut store = store_with(active_row());
    let mut vendors = MockConnectVendors::new();
    store
        .farm_record_store
        .expect_start_connector_deletion()
        .withf(|_, c| *c == id())
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_, _| {
            let mut row = active_row();
            row.connector.status = ConnectorStatus::Deleting;
            Ok(row)
        });
    store
        .farm_record_store
        .expect_shared_account_connectors()
        .with(eq(ConnectorKind::Slack), eq("T0123".to_owned()), eq(id()))
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_, _, _| Ok(0));
    vendors
        .expect_revoke()
        .withf(|kind, tokens| {
            *kind == ConnectorKind::Slack && tokens.access_token.expose_secret() == "xoxb-bot-token"
        })
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_, _| Ok(()));
    for deleted in [500, 3, 0] {
        store
            .farm_record_store
            .expect_delete_connector_batch()
            .with(eq(id()))
            .times(1)
            .in_sequence(&mut seq)
            .returning(move |_| Ok(deleted));
    }
    let answer = send(
        store,
        on(),
        Some(Role::Owner),
        vendors,
        request(Method::DELETE, &format!("/connectors/{CONNECTOR}"), None),
    )
    .await;
    assert_eq!(answer.status, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn a_workspace_token_another_connector_holds_is_not_revoked() {
    let mut store = store_with(active_row());
    let mut vendors = MockConnectVendors::new();
    store
        .farm_record_store
        .expect_start_connector_deletion()
        .returning(|_, _| Ok(active_row()));
    store
        .farm_record_store
        .expect_shared_account_connectors()
        .returning(|_, _, _| Ok(1));
    vendors.expect_revoke().never();
    store
        .farm_record_store
        .expect_delete_connector_batch()
        .times(1)
        .returning(|_| Ok(0));
    let answer = send(
        store,
        on(),
        Some(Role::Owner),
        vendors,
        request(Method::DELETE, &format!("/connectors/{CONNECTOR}"), None),
    )
    .await;
    assert_eq!(answer.status, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn a_failed_revoke_still_deletes_the_records() {
    let mut store = store_with(active_row());
    let mut vendors = MockConnectVendors::new();
    store
        .farm_record_store
        .expect_start_connector_deletion()
        .returning(|_, _| Ok(active_row()));
    store
        .farm_record_store
        .expect_shared_account_connectors()
        .returning(|_, _, _| Ok(0));
    vendors.expect_revoke().times(1).returning(|kind, _| {
        Err(VendorError::Unavailable {
            vendor: kind,
            retry_after_s: None,
            source: None,
        })
    });
    let mut seq = Sequence::new();
    for deleted in [7, 0] {
        store
            .farm_record_store
            .expect_delete_connector_batch()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move |_| Ok(deleted));
    }
    let answer = send(
        store,
        on(),
        Some(Role::Owner),
        vendors,
        request(Method::DELETE, &format!("/connectors/{CONNECTOR}"), None),
    )
    .await;
    assert_eq!(answer.status, StatusCode::ACCEPTED);
}

// ---------------------------------------------------------------------------------------------
// E3.5: the cleanup
// ---------------------------------------------------------------------------------------------

/// T-E3.6 at the task (the embedded-store half is in `store`'s
/// `pending_connectors_older_than_ten_minutes_are_cleaned_up`): the cutoff is 10 minutes before
/// now, and full batches repeat until one comes back short.
#[tokio::test]
async fn the_cleanup_deletes_pending_connectors_older_than_ten_minutes() {
    let mut store = FakeStore::default();
    let mut seq = Sequence::new();
    for deleted in [500, 3] {
        store
            .farm_record_store
            .expect_delete_expired_pending()
            .with(
                eq(now().minus(Duration::from_mins(10))),
                eq(now().minus(Duration::from_mins(10))),
            )
            .times(1)
            .in_sequence(&mut seq)
            .returning(move |_, _| Ok(deleted));
    }
    let auth = signed_in(&mut store, None);
    let state = state_with_vendors(
        store,
        auth,
        on(),
        MockConnectVendors::new(),
        (MockBlobStore::new(), no_images(), Arc::new(NoBeliefs)),
    );
    crate::scheduler::delete_expired_connects(&state, now())
        .await
        .unwrap();
}
