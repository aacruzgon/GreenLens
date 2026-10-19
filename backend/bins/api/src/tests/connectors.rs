//! The farm connector routes, off and on (roadmap E1.4, E3.1, decision 9). Without
//! `CONNECTOR_KEY` the list is `available: false` and every other route `409 invalid_state`,
//! after the same access checks the working routes will make; with it, the list comes from the
//! store.

use axum::http::Method;
use serde_json::{Value, json};

use domain::connectors::{
    Connector, ConnectorAccount, ConnectorEndpoint, ConnectorKind, ConnectorSource, ConnectorStatus,
};

use super::*;

const CONNECTOR: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5C";
const HASH: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
const CONNECTOR_KEY: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

/// A configuration with connectors on.
fn connectors_on() -> Config {
    config_with(&[("CONNECTOR_KEY", CONNECTOR_KEY)])
}

/// Sends `request` as a member with `role`, over `store` and `config`.
async fn call_in(
    mut store: FakeStore,
    config: Config,
    role: Option<Role>,
    request: Request<Body>,
) -> (StatusCode, Value) {
    let auth = signed_in(&mut store, role);
    let state = state_with_config(
        store,
        auth,
        MockBlobStore::new(),
        no_images(),
        Arc::new(NoBeliefs),
        config,
    );
    let response = crate::router::app(state).oneshot(request).await.unwrap();
    let status = response.status();
    (status, json(response).await)
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

/// Sends `request` with connectors off; the connector store is never asked.
async fn call(role: Option<Role>, request: Request<Body>) -> (StatusCode, Value) {
    let mut store = FakeStore::default();
    store.farm_record_store.expect_connectors().never();
    call_in(store, config(), role, request).await
}

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

#[tokio::test]
async fn a_member_lists_no_connectors_while_they_are_off() {
    let (status, body) = call(
        Some(Role::Viewer),
        get(&format!("/gardens/{GARDEN}/connectors")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "available": false, "connectors": [] }));
}

#[tokio::test]
async fn a_member_lists_the_gardens_connectors_when_they_are_on() {
    let mut store = FakeStore::default();
    store
        .farm_record_store
        .expect_connectors()
        .withf(|member| member.garden().to_string() == GARDEN)
        .times(1)
        .returning(|_| Ok(vec![connector()]));
    let (status, body) = call_in(
        store,
        connectors_on(),
        Some(Role::Viewer),
        get(&format!("/gardens/{GARDEN}/connectors")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["available"], true);
    assert_eq!(
        body["connectors"],
        json!([serde_json::to_value(connector()).unwrap()])
    );
}

#[tokio::test]
async fn a_garden_without_connectors_lists_none_when_they_are_on() {
    let mut store = FakeStore::default();
    store
        .farm_record_store
        .expect_connectors()
        .returning(|_| Ok(Vec::new()));
    let (status, body) = call_in(
        store,
        connectors_on(),
        Some(Role::Owner),
        get(&format!("/gardens/{GARDEN}/connectors")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "available": true, "connectors": [] }));
}

#[tokio::test]
async fn a_non_member_cannot_see_a_gardens_connectors_when_they_are_on() {
    let mut store = FakeStore::default();
    store.farm_record_store.expect_connectors().never();
    let (status, body) = call_in(
        store,
        connectors_on(),
        None,
        get(&format!("/gardens/{GARDEN}/connectors")),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn a_non_member_cannot_see_a_gardens_connectors() {
    let (status, body) = call(None, get(&format!("/gardens/{GARDEN}/connectors"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn every_owner_write_is_409_invalid_state_while_connectors_are_off() {
    let calls = [
        request(
            Method::POST,
            &format!("/gardens/{GARDEN}/connectors"),
            Some(json!({ "kind": "slack", "device_hash": HASH })),
        ),
        request(
            Method::POST,
            &format!("/connectors/{CONNECTOR}/complete"),
            Some(json!({ "code": "c", "state": "s", "device_verifier": HASH })),
        ),
        request(
            Method::GET,
            &format!("/connectors/{CONNECTOR}/available-sources"),
            None,
        ),
        request(
            Method::PUT,
            &format!("/connectors/{CONNECTOR}/sources"),
            Some(json!({ "sources": [{ "id": "C024BE91L" }] })),
        ),
        request(Method::POST, &format!("/connectors/{CONNECTOR}/sync"), None),
        request(Method::DELETE, &format!("/connectors/{CONNECTOR}"), None),
    ];
    for request in calls {
        let route = format!("{} {}", request.method(), request.uri());
        let (status, body) = call(Some(Role::Owner), request).await;
        assert_eq!(status, StatusCode::CONFLICT, "{route}");
        assert_eq!(body["error"]["code"], "invalid_state", "{route}");
        assert_eq!(body["error"]["retryable"], false, "{route}");
    }
}

#[tokio::test]
async fn the_public_callback_is_409_invalid_state_while_connectors_are_off() {
    let request = Request::get("/connectors/oauth/callback?code=c&state=s&scope=x")
        .body(Body::empty())
        .unwrap();
    let (status, body) = call(None, request).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "invalid_state");
}

#[tokio::test]
async fn connector_writes_need_an_owner() {
    let (status, body) = call(
        Some(Role::Editor),
        request(Method::DELETE, &format!("/connectors/{CONNECTOR}"), None),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["details"]["required_role"], "owner");
    let (status, _) = call(
        Some(Role::Editor),
        request(
            Method::POST,
            &format!("/gardens/{GARDEN}/connectors"),
            Some(json!({ "kind": "slack", "device_hash": HASH })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_connect_with_an_unknown_field_is_400() {
    let (status, body) = call(
        Some(Role::Owner),
        request(
            Method::POST,
            &format!("/gardens/{GARDEN}/connectors"),
            Some(json!({ "kind": "slack", "device_hash": HASH, "endpoint": "era" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}
