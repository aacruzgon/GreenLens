//! Ops and the auth webhook ([contracts 9], ops and auth webhook). No sign-in.

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
};

use domain::{
    error::ErrorBody,
    garden::{
        ops::{Healthz, Status},
        webhook::{AuthEventType, WebhookHeaders},
    },
    ports::ObjectPrefix,
    wire::BoolLit,
};

use crate::{error::ApiError, reply::Wire, state::SharedState};

/// `GET /healthz`: `200 { ok: true }` when `SurrealDB` and `RustFS` answer; `503` otherwise.
#[utoipa::path(get, path = "/healthz", tag = "ops", security(()),
    responses((status = 200, body = Healthz), (status = 503, body = ErrorBody)))]
pub async fn healthz(State(state): State<SharedState>) -> Result<Wire<Healthz>, ApiError> {
    state.ports.store.status(state.ports.clock.now()).await?;
    // A listing of an empty prefix: one cheap round trip to RustFS.
    let probe = ObjectPrefix::garden(domain::id::Id::from_ulid(ulid::Ulid::nil()));
    state.ports.blob.list(&probe).await?;
    Ok(Wire::ok(Healthz { ok: BoolLit }))
}

/// `GET /status`: each known worker's capabilities, health, last heartbeat and GPU name, and the
/// queue's counts. No user data, and no model versions: a heartbeat's `versions` are kept for
/// the label-version rule (contracts 5), and results carry their own `model_versions`.
#[utoipa::path(get, path = "/status", tag = "ops", security(()),
    responses((status = 200, body = Status), (status = 503, body = ErrorBody)))]
pub async fn status(State(state): State<SharedState>) -> Result<Wire<Status>, ApiError> {
    Ok(Wire::ok(
        state.ports.store.status(state.ports.clock.now()).await?,
    ))
}

/// `POST /webhooks/auth`: Clerk through Svix. The raw body is verified (signature over
/// `id.timestamp.body`, timestamp within 5 minutes either way) **before** it is parsed; a bad
/// signature or timestamp is `401 unauthenticated` (contracts 9, auth webhook). `user.deleted` starts the
/// account cascade. `user.created` is acknowledged and changes nothing: the user record is made
/// by the first authenticated request, and Clerk doesn't guarantee event order, so a late
/// `user.created` must never recreate a deleted user. Idempotent: `204` every time, also for
/// event types it doesn't handle and for a signed body the adapter can't read (Svix would retry
/// it forever).
#[utoipa::path(post, path = "/webhooks/auth", tag = "ops", security(()),
    params(("svix-id" = String, Header), ("svix-timestamp" = String, Header), ("svix-signature" = String, Header)),
    request_body(content = String, content_type = "application/json", description = "Clerk's event, Svix-signed"),
    responses((status = 204), (status = 401, body = ErrorBody)))]
pub async fn auth_webhook(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
    };
    let signed = WebhookHeaders {
        id: header("svix-id"),
        timestamp: header("svix-timestamp"),
        signature: header("svix-signature"),
    };
    let Some(event) = state
        .ports
        .webhooks
        .verify(signed, &body, state.ports.clock.now())?
    else {
        return Ok(StatusCode::NO_CONTENT);
    };
    // A cascade already running for this account (a `DELETE /me`) makes this a no-op.
    if event.event_type == AuthEventType::UserDeleted
        && let Some(user) = state.ports.store.user_by_auth(&event.data.id).await?
    {
        state.deletions.start_account(user, event.data.id).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}
