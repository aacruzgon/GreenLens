//! Farm connectors ([connectors 5], [connectors 11]): connect, choose sources, sync now and
//! disconnect, owner-only; the list for any member.
//!
//! - **Off without `CONNECTOR_KEY`** (roadmap E3.1, decision 9): the list answers
//!   `available: false` and every other route `409 invalid_state` after its access checks, never
//!   `503`, which clients retry.
//! - **Connecting** (E3.3, E3.4): `POST /gardens/{id}/connectors` creates a `pending` connector
//!   holding the nonce, the device hash and Google's sealed PKCE verifier, and returns the
//!   vendor's consent URL with a signed state ([`OAuthState`]). The public callback checks the
//!   state and `302`s to the app; `complete` checks it again, checks the device, exchanges the
//!   code and activates the connector.
//! - **Demo connectors** (`endpoint: "era"`) refuse every owner route with `409 invalid_state`
//!   (decision 11): only server configuration manages them.
//! - **Vendor calls** go through [`ConnectVendors`](crate::connect::ConnectVendors), on the
//!   vendors' fixed hosts (decision 8). A disconnect is the deletion runner's
//!   `finish_connector`, the one revoke-then-delete path.
//!
//! [connectors 5]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#5-connecting-and-tokens
//! [connectors 11]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#11-changes-to-other-documents

use std::collections::BTreeSet;

use axum::{
    extract::State,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use secrecy::{ExposeSecret, SecretString};

use domain::{
    authz::{Membership, Owner, Viewer},
    connectors::{
        AvailableSources, CompleteConnectorRequest, Connector, ConnectorEndpoint, ConnectorKind,
        ConnectorList, ConnectorSource, ConnectorStatus, CreateConnectorRequest,
        CreateConnectorResponse, OAuthCallbackQuery, SetSourcesRequest,
    },
    error::{ErrorBody, InvalidStateDetails},
    id::{ConnectorId, table},
    ports::{
        CompletedConnect, Conflict, ConnectorRow, IdGenExt, NewConnector, PendingConnect,
        StoreError, SyncRequest,
    },
    rules::limits::PENDING_CONNECT_TTL,
};
use integrations::google_drive::Pkce;

use crate::{
    config::OAuthClient,
    connect::{
        OAuthState, StateError, VendorError, authorize_url, device_matches, redirect_uri,
        return_location,
    },
    error::ApiError,
    extract::{ApiJson, ApiQuery, access::Access, idempotency::IdemJson},
    reply::Wire,
    sealing::{ConnectorKey, SealPurpose, StoredTokens, TokensError},
    state::{AppState, SharedState},
};

/// Most Slack channels one connector syncs ([contracts 9], farm connectors).
///
/// [contracts 9]: https://github.com/aacruzgon/GreenLens/blob/main/docs/architecture/contracts.md#farm-connectors
const MAX_SLACK_CHANNELS: usize = 10;
/// Most Drive folders one connector syncs.
const MAX_DRIVE_FOLDERS: usize = 1;
/// Batches of a dropped source's records deleted inline before giving up: 10,000 records a
/// garden at 500 a batch, with room to spare.
const MAX_SOURCE_BATCHES: u32 = 40;

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

/// The vendor's OAuth client, or `409 invalid_state` when this server has none for it.
fn vendor_client(state: &SharedState, kind: ConnectorKind) -> Result<&OAuthClient, ApiError> {
    let client = match kind {
        ConnectorKind::Slack => state.config.connectors.slack.as_ref(),
        ConnectorKind::GoogleDrive => state.config.connectors.google.as_ref(),
    };
    client.ok_or_else(|| {
        ApiError::invalid_state(InvalidStateDetails::default()).message(match kind {
            ConnectorKind::Slack => "Slack connections aren't set up on this server.",
            ConnectorKind::GoogleDrive => "Google Drive connections aren't set up on this server.",
        })
    })
}

/// `400 invalid_request` on `state`: tampered, expired, for another connector, or used.
fn bad_state(why: &str) -> ApiError {
    tracing::info!(why, "OAuth state refused");
    ApiError::invalid_request(
        "state",
        "The connection link is invalid or has expired. Start again.",
    )
}

/// The connector a `/connectors/{id}` route names, for an owner change: `404` when it is gone
/// or being deleted, `409 invalid_state` when it is a demo connector (decision 11).
async fn owned_row(state: &AppState, id: ConnectorId) -> Result<ConnectorRow, ApiError> {
    let row = state
        .ports
        .store
        .connector_row(id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    if row.connector.status == ConnectorStatus::Deleting {
        return Err(ApiError::not_found());
    }
    if row.connector.endpoint == ConnectorEndpoint::Era {
        return Err(ApiError::invalid_state(InvalidStateDetails::default())
            .message("Demo connectors are managed by the server and can't be changed."));
    }
    Ok(row)
}

/// `409 invalid_state` unless the connector holds a token (`active` or `reauth_needed`).
fn require_connected(row: &ConnectorRow) -> Result<(), ApiError> {
    match row.connector.status {
        ConnectorStatus::Active | ConnectorStatus::ReauthNeeded if row.secret.is_some() => Ok(()),
        status => Err(ApiError::invalid_state(InvalidStateDetails {
            status: Some(status.as_str().to_owned()),
            allowed: Some(vec!["active".to_owned(), "reauth_needed".to_owned()]),
            ..InvalidStateDetails::default()
        })),
    }
}

/// The connector's tokens; unreadable ones (another `CONNECTOR_KEY`) are `500`.
fn tokens_of(key: &ConnectorKey, row: &ConnectorRow) -> Result<StoredTokens, ApiError> {
    let sealed = row
        .secret
        .as_ref()
        .ok_or_else(|| ApiError::invalid_state(InvalidStateDetails::default()))?;
    StoredTokens::unseal(key, row.connector.id, sealed)
        .map_err(|e: TokensError| ApiError::internal(e))
}

/// A vendor failure as the app sees it. A refused token is `409` with
/// `status: "reauth_needed"` (the caller marks the connector first, so the app shows
/// **Reconnect**); a request the vendor rejected as ours (a misconfigured OAuth client) is
/// `500`, logged; an outage is `503 upstream_unavailable`.
fn vendor_failure(error: VendorError) -> ApiError {
    match error {
        VendorError::NotConfigured(kind) => {
            tracing::warn!(%kind, "vendor call without its OAuth client");
            ApiError::invalid_state(InvalidStateDetails::default())
        }
        error @ VendorError::Rejected { .. } => {
            tracing::error!(error = %error, "the vendor rejected our request; check its OAuth client");
            ApiError::internal(error)
        }
        VendorError::AuthRefused { vendor, code } => {
            tracing::info!(%vendor, code, "vendor refused the token");
            ApiError::invalid_state(InvalidStateDetails {
                status: Some(ConnectorStatus::ReauthNeeded.as_str().to_owned()),
                ..InvalidStateDetails::default()
            })
            .message("The connection was refused. Reconnect it.")
        }
        VendorError::Unavailable {
            vendor,
            retry_after_s,
            source,
        } => {
            let mut error = ApiError::upstream(
                vendor.as_str(),
                VendorError::Unavailable {
                    vendor,
                    retry_after_s,
                    source,
                },
            );
            if let Some(wait) = retry_after_s.filter(|&s| s > 5) {
                error = error.retry_after(wait);
            }
            error
        }
    }
}

/// A vendor failure on a stored token: a refused one makes the connector `reauth_needed`
/// first, so the app shows **Reconnect**.
async fn token_failure(state: &AppState, row: &ConnectorRow, error: VendorError) -> ApiError {
    if matches!(error, VendorError::AuthRefused { .. })
        && let Err(store) = state
            .ports
            .store
            .set_status(
                row.connector.id,
                ConnectorStatus::ReauthNeeded,
                Some(domain::connectors::SyncErrorCode::AuthRefused),
            )
            .await
    {
        return store.into();
    }
    vendor_failure(error)
}

/// The connector's sources at the vendor; a refused token makes it `reauth_needed`.
async fn vendor_sources(
    state: &AppState,
    key: &ConnectorKey,
    row: &ConnectorRow,
) -> Result<Vec<ConnectorSource>, ApiError> {
    let tokens = tokens_of(key, row)?;
    match state
        .connect
        .available_sources(row.connector.kind, &tokens)
        .await
    {
        Ok(sources) => Ok(sources),
        Err(error) => Err(token_failure(state, row, error).await),
    }
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
/// URL. Owner. `409 invalid_state` while connectors are off, when this server has no OAuth
/// client for the vendor, or when the garden already has 2 connectors.
#[utoipa::path(post, path = "/gardens/{id}/connectors", tag = "connectors",
    params(("id" = String, Path), ("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    request_body = CreateConnectorRequest,
    responses((status = 201, body = CreateConnectorResponse), (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn create_connector(
    State(state): State<SharedState>,
    access: Access<Owner, table::Garden>,
    request: IdemJson<CreateConnectorRequest>,
) -> Response {
    let IdemJson { body, begun, .. } = request;
    let result = start_connect(&state, &access, body).await;
    begun.finish(result).await
}

async fn start_connect(
    state: &SharedState,
    access: &Access<Owner, table::Garden>,
    body: CreateConnectorRequest,
) -> Result<Wire<CreateConnectorResponse>, ApiError> {
    let key = require_connectors(state)?;
    let client = vendor_client(state, body.kind)?;
    let id: ConnectorId = state.ports.ids.id();
    let now = state.ports.clock.now();
    let oauth = OAuthState::new(id, now.plus(PENDING_CONNECT_TTL));
    // Google's PKCE: the verifier sealed in `oauth`, the challenge in the consent URL. Slack's app
    // is a confidential client without PKCE (connectors 5).
    let pkce = (body.kind == ConnectorKind::GoogleDrive).then(Pkce::generate);
    let verifier = pkce.as_ref().map(|p| {
        key.seal(
            SealPurpose::Pkce,
            id,
            p.verifier().expose_secret().as_bytes(),
        )
    });
    let connector = state
        .ports
        .store
        .create_connector(
            access.member,
            NewConnector {
                id,
                kind: body.kind,
                status: ConnectorStatus::Pending,
                endpoint: ConnectorEndpoint::Vendor,
                created_by: access.user.id(),
                account: None,
                secret: None,
                oauth: Some(PendingConnect {
                    nonce: oauth.nonce.clone(),
                    device_hash: body.device_hash,
                    verifier,
                    expires_at: oauth.expires_at,
                }),
                sources: Vec::new(),
                created_at: now,
            },
        )
        .await?;
    let authorize_url = authorize_url(
        body.kind,
        &client.client_id,
        &redirect_uri(&state.config.public_base),
        &oauth.encode(key),
        pkce.as_ref().map(Pkce::challenge),
    );
    tracing::info!(connector = %id, kind = %body.kind, "connect started");
    Ok(Wire::created(CreateConnectorResponse {
        connector,
        authorize_url,
    }))
}

/// Checks a state against the pending connector it names: signed under this key, not expired,
/// for `expected` when given, and still the pending connector's nonce (a used state no longer
/// is: the exchange clears it). Returns the state and the connector.
async fn check_state(
    state: &AppState,
    key: &ConnectorKey,
    value: &str,
    expected: Option<ConnectorId>,
) -> Result<(OAuthState, ConnectorRow), ApiError> {
    let now = state.ports.clock.now();
    let oauth =
        OAuthState::decode(value, key, now).map_err(|e: StateError| bad_state(&e.to_string()))?;
    if expected.is_some_and(|id| id != oauth.connector) {
        return Err(bad_state("the state is for another connector"));
    }
    let row = state
        .ports
        .store
        .connector_row(oauth.connector)
        .await?
        .ok_or_else(|| bad_state("the connector is gone"))?;
    let live = row.connector.status == ConnectorStatus::Pending
        && row
            .oauth
            .as_ref()
            .is_some_and(|p| p.nonce == oauth.nonce && p.expires_at > now);
    if !live {
        return Err(bad_state("the state was used or replaced"));
    }
    Ok((oauth, row))
}

/// `GET /connectors/oauth/callback?code&state&error`: the vendor's redirect. Public: no session
/// can say who consented, so it checks the state (signature, expiry, and that the pending
/// connector still holds its nonce), stores nothing, and answers `302` to the app with the
/// code and state, or the vendor's `error` (the owner declined). `400 invalid_request` for a
/// bad, expired or used state; `409 invalid_state` while connectors are off.
#[utoipa::path(get, path = "/connectors/oauth/callback", tag = "connectors", security(()),
    params(OAuthCallbackQuery),
    responses((status = 302, description = "To the app with `code` and `state`, or `error` and `state`"),
        (status = 400, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn oauth_callback(
    State(state): State<SharedState>,
    ApiQuery(query): ApiQuery<OAuthCallbackQuery>,
) -> Result<Response, ApiError> {
    let key = require_connectors(&state)?;
    let value = query
        .state
        .as_deref()
        .ok_or_else(|| ApiError::invalid_request("state", "missing"))?;
    let (oauth, _) = check_state(&state, key, value, None).await?;
    let params: Vec<(&str, &str)> = match (&query.code, &query.error) {
        (_, Some(error)) => vec![("error", error.as_str()), ("state", value)],
        (Some(code), None) => vec![("code", code.as_str()), ("state", value)],
        (None, None) => return Err(ApiError::invalid_request("code", "missing")),
    };
    let location = return_location(
        &state.config.connectors.return_url,
        oauth.connector,
        &params,
    );
    let location = HeaderValue::from_str(&location).map_err(ApiError::internal)?;
    Ok((
        StatusCode::FOUND,
        [
            (header::LOCATION, location),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            (
                header::REFERRER_POLICY,
                HeaderValue::from_static("no-referrer"),
            ),
        ],
    )
        .into_response())
}

/// `POST /connectors/{id}/complete`: exchanges the code. The owner who started the connect, with
/// the `device_verifier` whose hash started it; anyone else, or a wrong verifier, gets
/// `404 not_found` and the pending connector is deleted. `400 invalid_request` for a bad,
/// expired or used state, or a code the vendor refused; `409 invalid_state` while connectors are
/// off.
#[utoipa::path(post, path = "/connectors/{id}/complete", tag = "connectors",
    params(("id" = String, Path), ("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    request_body = CompleteConnectorRequest,
    responses((status = 200, body = Connector), (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody),
        (status = 503, body = ErrorBody)))]
pub async fn complete_connector(
    State(state): State<SharedState>,
    access: Access<Owner, table::Connector>,
    request: IdemJson<CompleteConnectorRequest>,
) -> Response {
    let IdemJson { body, begun, .. } = request;
    let result = complete(&state, &access, body).await;
    begun.finish(result).await
}

async fn complete(
    state: &SharedState,
    access: &Access<Owner, table::Connector>,
    body: CompleteConnectorRequest,
) -> Result<Wire<Connector>, ApiError> {
    let key = require_connectors(state)?;
    let row = owned_row(state, access.id).await?;
    let (oauth, row) = check_state(state, key, &body.state, Some(row.connector.id)).await?;
    let pending = row
        .oauth
        .as_ref()
        .ok_or_else(|| bad_state("no pending state"))?;
    if row.created_by != access.user.id()
        || !device_matches(&body.device_verifier, &pending.device_hash)
    {
        tracing::warn!(connector = %access.id, "connect completed by another user or device; deleted");
        let doomed = state
            .ports
            .store
            .start_connector_deletion(access.member, access.id)
            .await?;
        state.deletions.spawn_connector(doomed);
        return Err(ApiError::not_found());
    }
    let kind = row.connector.kind;
    vendor_client(state, kind)?;
    let verifier = match &pending.verifier {
        Some(sealed) => {
            let opened = key
                .unseal(SealPurpose::Pkce, access.id, sealed)
                .map_err(ApiError::internal)?;
            let text = std::str::from_utf8(opened.expose_secret()).map_err(ApiError::internal)?;
            Some(SecretString::from(text))
        }
        None => None,
    };
    let exchanged = match state
        .connect
        .exchange(
            kind,
            &body.code,
            verifier,
            &redirect_uri(&state.config.public_base),
        )
        .await
    {
        Ok(exchanged) => exchanged,
        // The owner's code: used, expired or wrong. A rejection of our own request (a
        // misconfigured client) is not the owner's to fix and goes to `vendor_failure` (500).
        Err(VendorError::AuthRefused { vendor, code }) => {
            tracing::info!(connector = %access.id, %vendor, code, "the vendor refused the code");
            return Err(ApiError::invalid_request(
                "code",
                "The authorization code was refused. Start the connection again.",
            ));
        }
        Err(error) => return Err(vendor_failure(error)),
    };
    let secret = exchanged.tokens.seal(key, access.id);
    let connector = state
        .ports
        .store
        .complete_connect(
            access.member,
            access.id,
            CompletedConnect {
                nonce: oauth.nonce,
                account: exchanged.account,
                secret,
                now: state.ports.clock.now(),
            },
        )
        .await
        .map_err(|e| match e {
            // Another `complete` with the same state won the compare-and-set.
            StoreError::Conflict(Conflict::AlreadyExists) => bad_state("the state was used"),
            e => e.into(),
        })?;
    tracing::info!(connector = %access.id, %kind, "connector connected");
    Ok(Wire::ok(connector))
}

/// `GET /connectors/{id}/available-sources`: the channels the bot is in, or the folders. Owner.
/// `409 invalid_state` while connectors are off, for a demo connector, or one with no token
/// yet; a token the vendor refuses makes the connector `reauth_needed` and is `409` too.
#[utoipa::path(get, path = "/connectors/{id}/available-sources", tag = "connectors", params(("id" = String, Path)),
    responses((status = 200, body = AvailableSources), (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody), (status = 409, body = ErrorBody), (status = 503, body = ErrorBody)))]
pub async fn available_sources(
    State(state): State<SharedState>,
    access: Access<Owner, table::Connector>,
) -> Result<Wire<AvailableSources>, ApiError> {
    let key = require_connectors(&state)?;
    let row = owned_row(&state, access.id).await?;
    require_connected(&row)?;
    let sources = vendor_sources(&state, key, &row).await?;
    Ok(Wire::ok(AvailableSources { sources }))
}

/// `PUT /connectors/{id}/sources`: Slack up to 10 channels, Drive 1 folder, each from
/// `available-sources` (its name comes from there). Owner. The records of a source no longer
/// chosen are deleted before the answer. `409 invalid_state` while connectors are off, for a
/// demo connector, or one with no token yet.
#[utoipa::path(put, path = "/connectors/{id}/sources", tag = "connectors", params(("id" = String, Path)),
    request_body = SetSourcesRequest,
    responses((status = 200, body = Connector), (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody),
        (status = 503, body = ErrorBody)))]
pub async fn set_sources(
    State(state): State<SharedState>,
    access: Access<Owner, table::Connector>,
    ApiJson(request): ApiJson<SetSourcesRequest>,
) -> Result<Wire<Connector>, ApiError> {
    let key = require_connectors(&state)?;
    let row = owned_row(&state, access.id).await?;
    require_connected(&row)?;
    let max = match row.connector.kind {
        ConnectorKind::Slack => MAX_SLACK_CHANNELS,
        ConnectorKind::GoogleDrive => MAX_DRIVE_FOLDERS,
    };
    if request.sources.len() > max {
        return Err(ApiError::invalid_request(
            "sources",
            match row.connector.kind {
                ConnectorKind::Slack => "Choose at most 10 channels.",
                ConnectorKind::GoogleDrive => "Choose one folder.",
            },
        ));
    }
    let mut seen = BTreeSet::new();
    for (i, choice) in request.sources.iter().enumerate() {
        if !seen.insert(choice.id.as_str()) {
            return Err(ApiError::invalid_request(
                format!("sources[{i}].id"),
                "This source is chosen twice.",
            ));
        }
    }
    let chosen = if request.sources.is_empty() {
        Vec::new()
    } else {
        let available = vendor_sources(&state, key, &row).await?;
        request
            .sources
            .iter()
            .enumerate()
            .map(|(i, choice)| {
                available
                    .iter()
                    .find(|s| s.id == choice.id)
                    .cloned()
                    .ok_or_else(|| {
                        ApiError::invalid_request(
                            format!("sources[{i}].id"),
                            "Not one of the available sources.",
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    let now = state.ports.clock.now();
    let store = &state.ports.store;
    let mut connector = store
        .set_sources(access.member, access.id, chosen.clone(), now)
        .await?;
    let dropped: Vec<&ConnectorSource> = row
        .connector
        .sources
        .iter()
        .filter(|old| !chosen.iter().any(|new| new.id == old.id))
        .collect();
    if !dropped.is_empty() {
        for source in dropped {
            drop_source_records(&state, access.id, &source.id).await?;
        }
        if let Some(fresh) = store.connector_row(access.id).await? {
            connector = fresh.connector;
        }
    }
    Ok(Wire::ok(connector))
}

/// Deletes a dropped source's records, 500 at a time.
async fn drop_source_records(
    state: &AppState,
    connector: ConnectorId,
    source: &str,
) -> Result<(), ApiError> {
    for _ in 0..MAX_SOURCE_BATCHES {
        if state
            .ports
            .store
            .delete_source_records(connector, source.to_owned())
            .await?
            == 0
        {
            return Ok(());
        }
    }
    Err(ApiError::internal(StoreError::Query {
        what: "delete a dropped source's records",
        source: "more batches than a garden can hold".into(),
    }))
}

/// `POST /connectors/{id}/sync`: **Sync now**, `202`; `429 rate_limited` with `retry_after_s`
/// within 5 minutes of the last accepted request (decision 12, a compare-and-set in the store).
/// Owner. `409 invalid_state` while connectors are off, for a demo connector, one that isn't
/// `active`, or one stopped until its sources change. It has no body and holds no idempotency
/// record: a repeat inside 5 minutes is the `429`, so it is naturally idempotent.
#[utoipa::path(post, path = "/connectors/{id}/sync", tag = "connectors",
    params(("id" = String, Path), ("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    responses((status = 202), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody), (status = 429, body = ErrorBody)))]
pub async fn sync_connector(
    State(state): State<SharedState>,
    access: Access<Owner, table::Connector>,
) -> Result<StatusCode, ApiError> {
    require_connectors(&state)?;
    owned_row(&state, access.id).await?;
    let now = state.ports.clock.now();
    match state
        .ports
        .store
        .request_sync(access.member, access.id, now)
        .await?
    {
        SyncRequest::Accepted => Ok(StatusCode::ACCEPTED),
        SyncRequest::TooSoon { allowed_at } => {
            let wait = allowed_at.since(now);
            let secs = wait.as_secs() + u64::from(wait.subsec_nanos() > 0);
            Err(ApiError::rate_limited(
                u32::try_from(secs.max(1)).unwrap_or(u32::MAX),
            ))
        }
    }
}

/// `DELETE /connectors/{id}`: `202`. The store marks the connector `deleting` (the list and the
/// sync skip it), then a background task revokes the tokens with the vendor and deletes the
/// records and the connector. Owner. `409 invalid_state` while connectors are off and for a
/// demo connector.
#[utoipa::path(delete, path = "/connectors/{id}", tag = "connectors", params(("id" = String, Path)),
    responses((status = 202), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody)))]
pub async fn delete_connector(
    State(state): State<SharedState>,
    access: Access<Owner, table::Connector>,
) -> Result<StatusCode, ApiError> {
    require_connectors(&state)?;
    owned_row(&state, access.id).await?;
    let member: Membership<Owner> = access.member;
    let row = state
        .ports
        .store
        .start_connector_deletion(member, access.id)
        .await?;
    state.deletions.spawn_connector(row);
    Ok(StatusCode::ACCEPTED)
}
