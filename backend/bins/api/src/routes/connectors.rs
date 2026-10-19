//! Farm connectors ([connectors 5], [connectors 11]): connect, choose sources, sync now and
//! disconnect, owner-only; the list for any member.
//!
//! Without `CONNECTOR_KEY` connectors are off (roadmap E3.1, decision 9): the list answers
//! `available: false` and every other route `409 invalid_state`, never `503`, which clients
//! retry. With it, the list comes from the store; the other routes call
//! [`require_connectors`] first and still answer `409 invalid_state` until the OAuth state and the routes are built (E3.3, E3.4). The access
//! checks already run, so a non-member gets `404` and a non-owner `403` as they will then.
//!
//! [connectors 5]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#5-connecting-and-tokens
//! [connectors 11]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#11-changes-to-other-documents

use axum::{extract::State, http::StatusCode, response::Response};

use domain::{
    authz::{Owner, Viewer},
    connectors::{
        AvailableSources, CompleteConnectorRequest, Connector, ConnectorList,
        CreateConnectorRequest, CreateConnectorResponse, OAuthCallbackQuery, SetSourcesRequest,
    },
    error::{ErrorBody, InvalidStateDetails},
    id::table,
};

use crate::{
    error::ApiError,
    extract::{ApiJson, ApiQuery, access::Access, idempotency::IdemJson},
    reply::Wire,
    sealing::ConnectorKey,
    state::SharedState,
};

/// The answer of every connector write while connectors are off.
fn connectors_off() -> ApiError {
    ApiError::invalid_state(InvalidStateDetails::default())
        .message("Farm connectors aren't available on this server yet.")
}

/// The guard every write route calls first: connectors are off without `CONNECTOR_KEY`, and
/// then the route answers `409 invalid_state`. Returns the key for the handler that needs it.
fn require_connectors(state: &SharedState) -> Result<&ConnectorKey, ApiError> {
    state.connector_key().ok_or_else(connectors_off)
}

/// `GET /gardens/{id}/connectors`: the garden's connectors with their sources, last sync and
/// record counts. Any member. `available: false` with an empty list while connectors are off.
#[utoipa::path(get, path = "/gardens/{id}/connectors", tag = "connectors", params(("id" = String, Path)),
    responses((status = 200, body = ConnectorList), (status = 404, body = ErrorBody)))]
pub async fn list_connectors(
    State(state): State<SharedState>,
    access: Access<Viewer, table::Garden>,
) -> Result<Wire<ConnectorList>, ApiError> {
    // Off without `CONNECTOR_KEY` (the one switch, `AppState::connector_key`): the store isn't
    // asked.
    if state.connector_key().is_none() {
        return Ok(Wire::ok(ConnectorList::unavailable()));
    }
    let connectors = state.ports.store.connectors(access.member).await?;
    Ok(Wire::ok(ConnectorList {
        available: true,
        connectors,
    }))
}

/// `POST /gardens/{id}/connectors`: starts a connect, `pending`, and returns the vendor's consent
/// URL. Owner. `409 invalid_state` while connectors are off.
#[utoipa::path(post, path = "/gardens/{id}/connectors", tag = "connectors",
    params(("id" = String, Path), ("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    request_body = CreateConnectorRequest,
    responses((status = 201, body = CreateConnectorResponse), (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn create_connector(
    State(state): State<SharedState>,
    _access: Access<Owner, table::Garden>,
    request: IdemJson<CreateConnectorRequest>,
) -> Response {
    let IdemJson { begun, .. } = request;
    let result: Result<Wire<CreateConnectorResponse>, ApiError> = require_connectors(&state)
        .and_then(|_key| Err(ApiError::invalid_state(InvalidStateDetails::default())));
    begun.finish(result).await
}

/// `GET /connectors/oauth/callback?code&state`: the vendor's redirect. Public: it checks only the
/// state's signature and expiry, stores nothing, and answers `302` to the app with the code and
/// state. `409 invalid_state` while connectors are off.
#[utoipa::path(get, path = "/connectors/oauth/callback", tag = "connectors", security(()),
    params(OAuthCallbackQuery),
    responses((status = 302, description = "To the app with `code` and `state`"),
        (status = 400, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn oauth_callback(
    State(state): State<SharedState>,
    ApiQuery(_query): ApiQuery<OAuthCallbackQuery>,
) -> Result<StatusCode, ApiError> {
    require_connectors(&state)?;
    Err(ApiError::invalid_state(InvalidStateDetails::default()))
}

/// `POST /connectors/{id}/complete`: exchanges the code. The owner who started the connect, with
/// the `device_verifier` whose hash started it. `409 invalid_state` while connectors are off.
#[utoipa::path(post, path = "/connectors/{id}/complete", tag = "connectors",
    params(("id" = String, Path), ("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    request_body = CompleteConnectorRequest,
    responses((status = 200, body = Connector), (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn complete_connector(
    State(state): State<SharedState>,
    _access: Access<Owner, table::Connector>,
    request: IdemJson<CompleteConnectorRequest>,
) -> Response {
    let IdemJson { begun, .. } = request;
    let result: Result<Wire<Connector>, ApiError> = require_connectors(&state)
        .and_then(|_key| Err(ApiError::invalid_state(InvalidStateDetails::default())));
    begun.finish(result).await
}

/// `GET /connectors/{id}/available-sources`: the channels the bot is in, or the folders. Owner.
/// `409 invalid_state` while connectors are off.
#[utoipa::path(get, path = "/connectors/{id}/available-sources", tag = "connectors", params(("id" = String, Path)),
    responses((status = 200, body = AvailableSources), (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn available_sources(
    State(state): State<SharedState>,
    _access: Access<Owner, table::Connector>,
) -> Result<Wire<AvailableSources>, ApiError> {
    require_connectors(&state)?;
    Err(ApiError::invalid_state(InvalidStateDetails::default()))
}

/// `PUT /connectors/{id}/sources`: Slack up to 10 channels, Drive 1 folder. Owner.
/// `409 invalid_state` while connectors are off.
#[utoipa::path(put, path = "/connectors/{id}/sources", tag = "connectors", params(("id" = String, Path)),
    request_body = SetSourcesRequest,
    responses((status = 200, body = Connector), (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn set_sources(
    State(state): State<SharedState>,
    _access: Access<Owner, table::Connector>,
    ApiJson(_request): ApiJson<SetSourcesRequest>,
) -> Result<Wire<Connector>, ApiError> {
    require_connectors(&state)?;
    Err(ApiError::invalid_state(InvalidStateDetails::default()))
}

/// `POST /connectors/{id}/sync`: **Sync now**, `202`; `429` within 5 minutes of the last. Owner.
/// `409 invalid_state` while connectors are off. It has no body, and no body-less route holds an
/// idempotency record yet, so `Idempotency-Key` is documented here and honoured with the handler
/// (E3): sync-now is naturally idempotent, since a repeat inside 5 minutes is the `429`.
#[utoipa::path(post, path = "/connectors/{id}/sync", tag = "connectors",
    params(("id" = String, Path), ("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    responses((status = 202), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody), (status = 429, body = ErrorBody)))]
pub async fn sync_connector(
    State(state): State<SharedState>,
    _access: Access<Owner, table::Connector>,
) -> Result<StatusCode, ApiError> {
    require_connectors(&state)?;
    Err(ApiError::invalid_state(InvalidStateDetails::default()))
}

/// `DELETE /connectors/{id}`: `202`; the tokens are revoked, then the records deleted. Owner.
/// `409 invalid_state` while connectors are off.
#[utoipa::path(delete, path = "/connectors/{id}", tag = "connectors", params(("id" = String, Path)),
    responses((status = 202), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody)))]
pub async fn delete_connector(
    State(state): State<SharedState>,
    _access: Access<Owner, table::Connector>,
) -> Result<StatusCode, ApiError> {
    require_connectors(&state)?;
    Err(ApiError::invalid_state(InvalidStateDetails::default()))
}
