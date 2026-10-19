//! The farm connector routes while connectors are off (roadmap E1.4, decision 9): the list is
//! `available: false`, every other route `409 invalid_state`, after the same access checks the
//! working routes will make.

use axum::http::Method;
use serde_json::{Value, json};

use super::*;

const CONNECTOR: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5C";
const HASH: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

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

async fn call(role: Option<Role>, request: Request<Body>) -> (StatusCode, Value) {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, role);
    let app = crate::router::app(state(store, auth, MockBlobStore::new(), no_images()));
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    (status, json(response).await)
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
