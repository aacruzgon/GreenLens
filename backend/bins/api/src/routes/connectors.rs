//! Farm connectors ([connectors 5], [connectors 11]): connect, choose sources, sync now and
//! disconnect, owner-only; the list for any member.
//!
//! Declared ahead of their work (roadmap E1.4): until connectors are configured (E3, with
//! `CONNECTOR_KEY`), the list answers `available: false` and every other route answers
//! `409 invalid_state`, never `503`, which clients retry (decision 9). The access checks already
//! run, so a non-member gets `404` and a non-owner `403` as they will then.
//!
//! [connectors 5]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#5-connecting-and-tokens
//! [connectors 11]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#11-changes-to-other-documents

use axum::{http::StatusCode, response::Response};

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
};

/// The answer of every connector write while connectors are off.
fn connectors_off() -> ApiError {
    ApiError::invalid_state(InvalidStateDetails::default())
        .message("Farm connectors aren't available on this server yet.")
}

/// `GET /gardens/{id}/connectors`: the garden's connectors with their sources, last sync and
/// record counts. Any member. `available: false` with an empty list while connectors are off.
#[utoipa::path(get, path = "/gardens/{id}/connectors", tag = "connectors", params(("id" = String, Path)),
    responses((status = 200, body = ConnectorList), (status = 404, body = ErrorBody)))]
pub async fn list_connectors(
    _access: Access<Viewer, table::Garden>,
) -> Result<Wire<ConnectorList>, ApiError> {
    Ok(Wire::ok(ConnectorList::unavailable()))
}

/// `POST /gardens/{id}/connectors`: starts a connect, `pending`, and returns the vendor's consent
/// URL. Owner. `409 invalid_state` while connectors are off.
#[utoipa::path(post, path = "/gardens/{id}/connectors", tag = "connectors",
    params(("id" = String, Path), ("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    request_body = CreateConnectorRequest,
    responses((status = 201, body = CreateConnectorResponse), (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn create_connector(
    _access: Access<Owner, table::Garden>,
    request: IdemJson<CreateConnectorRequest>,
) -> Response {
    let IdemJson { begun, .. } = request;
    let result: Result<Wire<CreateConnectorResponse>, ApiError> = Err(connectors_off());
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
    ApiQuery(_query): ApiQuery<OAuthCallbackQuery>,
) -> Result<StatusCode, ApiError> {
    Err(connectors_off())
}

/// `POST /connectors/{id}/complete`: exchanges the code. The owner who started the connect, with
/// the `device_verifier` whose hash started it. `409 invalid_state` while connectors are off.
#[utoipa::path(post, path = "/connectors/{id}/complete", tag = "connectors",
    params(("id" = String, Path), ("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    request_body = CompleteConnectorRequest,
    responses((status = 200, body = Connector), (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn complete_connector(
    _access: Access<Owner, table::Connector>,
    request: IdemJson<CompleteConnectorRequest>,
) -> Response {
    let IdemJson { begun, .. } = request;
    let result: Result<Wire<Connector>, ApiError> = Err(connectors_off());
    begun.finish(result).await
}

/// `GET /connectors/{id}/available-sources`: the channels the bot is in, or the folders. Owner.
/// `409 invalid_state` while connectors are off.
#[utoipa::path(get, path = "/connectors/{id}/available-sources", tag = "connectors", params(("id" = String, Path)),
    responses((status = 200, body = AvailableSources), (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn available_sources(
    _access: Access<Owner, table::Connector>,
) -> Result<Wire<AvailableSources>, ApiError> {
    Err(connectors_off())
}

/// `PUT /connectors/{id}/sources`: Slack up to 10 channels, Drive 1 folder. Owner.
/// `409 invalid_state` while connectors are off.
#[utoipa::path(put, path = "/connectors/{id}/sources", tag = "connectors", params(("id" = String, Path)),
    request_body = SetSourcesRequest,
    responses((status = 200, body = Connector), (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn set_sources(
    _access: Access<Owner, table::Connector>,
    ApiJson(_request): ApiJson<SetSourcesRequest>,
) -> Result<Wire<Connector>, ApiError> {
    Err(connectors_off())
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
    _access: Access<Owner, table::Connector>,
) -> Result<StatusCode, ApiError> {
    Err(connectors_off())
}

/// `DELETE /connectors/{id}`: `202`; the tokens are revoked, then the records deleted. Owner.
/// `409 invalid_state` while connectors are off.
#[utoipa::path(delete, path = "/connectors/{id}", tag = "connectors", params(("id" = String, Path)),
    responses((status = 202), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody)))]
pub async fn delete_connector(
    _access: Access<Owner, table::Connector>,
) -> Result<StatusCode, ApiError> {
    Err(connectors_off())
}
