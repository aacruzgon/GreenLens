//! Gardens, members, invites and the garden map ([contracts 9]; [architecture 8.5], 6.8).

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, response::Response};
use data_encoding::BASE32_NOPAD;
use rand::RngExt;

use domain::{
    authz::{Editor, Owner, Viewer},
    error::ErrorBody,
    garden::{
        gardens::{
            CreateGardenRequest, Garden, GardenLocationInput, GardenWeather, LocationQuery,
            UpdateGardenRequest,
        },
        map::{GardenMap, GardenMapInput, MapResponse, PinPlant},
        members::{CreateInviteRequest, Invite, Member, UpdateMemberRequest},
    },
    id::{GardenId, InviteCode, InviteId, PlantId, UserId, table},
    ports::{
        CellRequest, GardenPatch, GeocodeQuery, IdGenExt, MapWrite, NewGarden, NewInvite,
        ResolvedLocation,
    },
    rules::limits::INVITE_TTL,
    wire::{Page, PageRequest},
};

use crate::{
    error::ApiError,
    extract::{ApiJson, ApiPath, ApiQuery, access::Access, auth::AuthUser, idempotency::IdemJson},
    limits,
    reply::Wire,
    scheduler::hour_of,
    state::{AppState, SharedState},
};

/// Geocodes a location input once ([architecture 8.5]): coordinates are kept at the precision
/// the user chose and named by reverse geocoding; a postal code or city is forward geocoded. The
/// time zone comes with it. An unknown postal code or city is `400 invalid_request`.
async fn resolve(
    state: &AppState,
    input: GardenLocationInput,
) -> Result<ResolvedLocation, ApiError> {
    let query = input.query()?;
    let precision = query.precision();
    let field = match &query {
        LocationQuery::Coordinates { .. } => "location.lat",
        LocationQuery::PostalCode(_) => "location.postal_code",
        LocationQuery::City(_) => "location.city",
    };
    let geocoded = match &query {
        LocationQuery::Coordinates { at, .. } => {
            state
                .ports
                .geocoder
                .geocode(GeocodeQuery::Coordinates(*at))
                .await
        }
        LocationQuery::PostalCode(code) => {
            state
                .ports
                .geocoder
                .geocode(GeocodeQuery::PostalCode(code))
                .await
        }
        LocationQuery::City(city) => state.ports.geocoder.geocode(GeocodeQuery::City(city)).await,
    }
    .map_err(|e| match e {
        domain::ports::ProviderError::NotFound { .. } => {
            ApiError::invalid_request(field, "no place found for this location")
        }
        other => other.into(),
    })?;
    let coordinates = integrations::geocode::round_for(
        match &query {
            LocationQuery::Coordinates { at, .. } => *at,
            _ => geocoded.coordinates,
        },
        precision,
    );
    Ok(ResolvedLocation {
        coordinates,
        precision,
        label: geocoded.label,
        time_zone: geocoded.time_zone,
    })
}

/// `GET /gardens`: the caller's gardens, by name. Any signed-in user.
#[utoipa::path(get, path = "/gardens", tag = "gardens", params(PageRequest),
    responses((status = 200, body = Page<Garden>), (status = 401, body = ErrorBody)))]
pub async fn list_gardens(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    ApiQuery(page): ApiQuery<PageRequest>,
) -> Result<Wire<Page<Garden>>, ApiError> {
    Ok(Wire::ok(state.ports.store.gardens(user.id(), page).await?))
}

/// `POST /gardens`: the caller becomes owner; `SoilGrids` is fetched in the background, and the
/// scheduler retries it when it fails.
#[utoipa::path(post, path = "/gardens", tag = "gardens", request_body = CreateGardenRequest,
    params(("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    responses((status = 201, body = Garden), (status = 400, body = ErrorBody), (status = 503, body = ErrorBody)))]
pub async fn create_garden(
    State(state): State<SharedState>,
    request: IdemJson<CreateGardenRequest>,
) -> Response {
    let IdemJson { user, body, begun } = request;
    let result = async {
        let name = body.name.trim().to_owned();
        if name.is_empty() || name.chars().count() > 80 {
            return Err(ApiError::invalid_request("name", "1-80 characters"));
        }
        let location = resolve(&state, body.location).await?;
        let at = location.coordinates;
        let garden = state
            .ports
            .store
            .create_garden(
                user.id(),
                NewGarden {
                    id: state.ports.ids.id(),
                    name,
                    location,
                },
            )
            .await?;
        spawn_soil_fetch(&state, garden.id, at);
        Ok(Wire::created(garden))
    }
    .await;
    begun.finish(result).await
}

/// The first `SoilGrids` attempt, in the background; a failure sets the garden's next attempt
/// (15 minutes after an outage, 7 days after no data) for the scheduler's `soil` task.
fn spawn_soil_fetch(
    state: &SharedState,
    garden: GardenId,
    at: domain::records::weather::Coordinates,
) {
    let state = Arc::clone(state);
    state.tasks.clone().spawn(async move {
        if let Err(error) = crate::scheduler::fetch_soil_profile(&state, garden, at).await {
            tracing::warn!(%garden, error = %error, "soil profile not stored");
        }
    });
}

/// `GET /gardens/{id}`. Any member.
#[utoipa::path(get, path = "/gardens/{id}", tag = "gardens", params(("id" = String, Path)),
    responses((status = 200, body = Garden), (status = 404, body = ErrorBody)))]
pub async fn get_garden(
    State(state): State<SharedState>,
    access: Access<Viewer, table::Garden>,
) -> Result<Wire<Garden>, ApiError> {
    Ok(Wire::ok(state.ports.store.garden(access.member).await?))
}

/// `PATCH /gardens/{id}`: name and location. Owner.
#[utoipa::path(patch, path = "/gardens/{id}", tag = "gardens", params(("id" = String, Path)),
    request_body = UpdateGardenRequest,
    responses((status = 200, body = Garden), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub async fn update_garden(
    State(state): State<SharedState>,
    access: Access<Owner, table::Garden>,
    ApiJson(body): ApiJson<UpdateGardenRequest>,
) -> Result<Wire<Garden>, ApiError> {
    let location = match body.location {
        Some(input) => Some(resolve(&state, input).await?),
        None => None,
    };
    let patch = GardenPatch {
        name: body.name.map(|n| n.trim().to_owned()),
        location,
    };
    Ok(Wire::ok(
        state
            .ports
            .store
            .update_garden(access.member, patch)
            .await?,
    ))
}

/// `DELETE /gardens/{id}`: `202`; the cascade runs in the background and the other members get
/// a `garden_deleted` push. Owner.
#[utoipa::path(delete, path = "/gardens/{id}", tag = "gardens", params(("id" = String, Path)),
    responses((status = 202), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub async fn delete_garden(
    State(state): State<SharedState>,
    access: Access<Owner, table::Garden>,
) -> Result<StatusCode, ApiError> {
    let started = state
        .ports
        .store
        .start_garden_deletion(access.member, state.ports.clock.now())
        .await?;
    state.deletions.spawn(started);
    Ok(StatusCode::ACCEPTED)
}

/// `GET /gardens/{id}/weather`: the garden's cache cell for this hour, fetched on a miss as a scan
/// does. `503 upstream_unavailable` when the fetch fails. Any member.
#[utoipa::path(get, path = "/gardens/{id}/weather", tag = "gardens", params(("id" = String, Path)),
    responses((status = 200, body = GardenWeather), (status = 404, body = ErrorBody), (status = 503, body = ErrorBody)))]
pub async fn garden_weather(
    State(state): State<SharedState>,
    access: Access<Viewer, table::Garden>,
) -> Result<Wire<GardenWeather>, ApiError> {
    let context = state
        .ports
        .store
        .garden_context(access.member.garden())
        .await?;
    let hour = hour_of(state.ports.clock.now());
    let cell = if let Some(cached) = state.ports.store.cached(context.weather_cell, hour).await? {
        cached
    } else {
        let request = CellRequest {
            cell: context.weather_cell,
            time_zone: context.time_zone,
        };
        let fetched = state.ports.weather.fetch_cell(&request).await?;
        state
            .ports
            .store
            .put(context.weather_cell, hour, fetched.clone())
            .await?;
        fetched
    };
    Ok(Wire::ok(GardenWeather {
        weather: cell.for_garden(context.precision),
    }))
}

/// `GET /gardens/{id}/members`, by `since`. Any member.
#[utoipa::path(get, path = "/gardens/{id}/members", tag = "members", params(("id" = String, Path), PageRequest),
    responses((status = 200, body = Page<Member>), (status = 404, body = ErrorBody)))]
pub async fn list_members(
    State(state): State<SharedState>,
    access: Access<Viewer, table::Garden>,
    ApiQuery(page): ApiQuery<PageRequest>,
) -> Result<Wire<Page<Member>>, ApiError> {
    Ok(Wire::ok(
        state.ports.store.members(access.member, page).await?,
    ))
}

/// `PATCH /gardens/{id}/members/{user_id}`: change a role; demoting the last owner is `409`.
/// Owner.
#[utoipa::path(patch, path = "/gardens/{id}/members/{user_id}", tag = "members",
    params(("id" = String, Path), ("user_id" = String, Path)), request_body = UpdateMemberRequest,
    responses((status = 200, body = Member), (status = 403, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn update_member(
    State(state): State<SharedState>,
    access: Access<Owner, table::Garden>,
    ApiPath(ids): ApiPath<(GardenId, UserId)>,
    ApiJson(body): ApiJson<UpdateMemberRequest>,
) -> Result<Wire<Member>, ApiError> {
    let garden = access.member.garden();
    let change = state
        .ports
        .store
        .change_role(access.member, ids.1, body.role)
        .await?;
    // Demoting an owner deletes the farm connectors they created in the garden (connectors 8):
    // the store marked them `deleting` in the role change's transaction, and the runner revokes
    // and deletes them in the background now that it committed (EON E3.6). Any other change
    // marks nothing, so nothing is started.
    if change.demoted_owner {
        state.deletions.spawn_connectors_of(garden, ids.1);
    }
    Ok(Wire::ok(change.member))
}

/// `DELETE /gardens/{id}/members/{user_id}`: an owner removing anyone, or a member leaving
/// (the role check depends on the request, so it is the run-time `require`). Their chats in the
/// garden go with them; the last owner can't leave (`409`).
#[utoipa::path(delete, path = "/gardens/{id}/members/{user_id}", tag = "members",
    params(("id" = String, Path), ("user_id" = String, Path)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn remove_member(
    State(state): State<SharedState>,
    access: Access<Viewer, table::Garden>,
    ApiPath(ids): ApiPath<(GardenId, UserId)>,
) -> Result<StatusCode, ApiError> {
    let target = ids.1;
    if target != access.user.id() {
        access.member.require::<Owner>()?;
    }
    let removed = state
        .ports
        .store
        .remove_member(access.member, target)
        .await?;
    let runner = state.deletions.clone();
    let garden = removed.garden;
    let started = domain::ports::DeletionStarted {
        target: domain::ports::DeletionTarget::Member {
            garden,
            user: target,
        },
        prefixes: removed.chat_prefixes,
        keys: Vec::new(),
        notify: Vec::new(),
        garden_name: None,
    };
    runner.spawn(started);
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /gardens/{id}/invites`: unused and unexpired, newest first. Owner.
#[utoipa::path(get, path = "/gardens/{id}/invites", tag = "members", params(("id" = String, Path), PageRequest),
    responses((status = 200, body = Page<Invite>), (status = 403, body = ErrorBody)))]
pub async fn list_invites(
    State(state): State<SharedState>,
    access: Access<Owner, table::Garden>,
    ApiQuery(page): ApiQuery<PageRequest>,
) -> Result<Wire<Page<Invite>>, ApiError> {
    Ok(Wire::ok(
        state
            .ports
            .store
            .invites(access.member, page, state.ports.clock.now())
            .await?,
    ))
}

/// A new invite code: 128 random bits as 26 base32 characters ([contracts 9]; rand 0.10's
/// `ThreadRng`, a CSPRNG reseeded from the OS).
pub fn new_invite_code() -> InviteCode {
    let mut bytes = [0_u8; 16];
    rand::rng().fill(&mut bytes[..]);
    // reason: 16 bytes always encode to 26 characters of A-Z2-7, the code's exact format.
    BASE32_NOPAD
        .encode(&bytes)
        .parse()
        .unwrap_or_else(|_| unreachable!("16 random bytes encode to a valid invite code"))
}

/// `POST /gardens/{id}/invites`: a single-use code, valid 7 days. Owner.
#[utoipa::path(post, path = "/gardens/{id}/invites", tag = "members", params(("id" = String, Path),
        ("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    request_body = CreateInviteRequest,
    responses((status = 201, body = Invite), (status = 403, body = ErrorBody)))]
pub async fn create_invite(
    State(state): State<SharedState>,
    access: Access<Owner, table::Garden>,
    request: IdemJson<CreateInviteRequest>,
) -> Response {
    let IdemJson { body, begun, .. } = request;
    let result = async {
        let invite = NewInvite {
            id: state.ports.ids.id(),
            code: new_invite_code(),
            role: body.role,
            expires_at: state.ports.clock.now().plus(INVITE_TTL),
        };
        Ok(Wire::created(
            state
                .ports
                .store
                .create_invite(access.member, invite)
                .await?,
        ))
    }
    .await;
    begun.finish(result).await
}

/// `DELETE /gardens/{id}/invites/{invite_id}`: the code stops working. Owner.
#[utoipa::path(delete, path = "/gardens/{id}/invites/{invite_id}", tag = "members",
    params(("id" = String, Path), ("invite_id" = String, Path)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub async fn revoke_invite(
    State(state): State<SharedState>,
    access: Access<Owner, table::Garden>,
    ApiPath(ids): ApiPath<(GardenId, InviteId)>,
) -> Result<StatusCode, ApiError> {
    state
        .ports
        .store
        .revoke_invite(access.member, ids.1, state.ports.clock.now())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /invites/{code}/accept`: any signed-in user, 10 attempts a minute (`429`). Unknown, used,
/// revoked or expired codes are `404`; an existing member gets `200` with their role unchanged.
#[utoipa::path(post, path = "/invites/{code}/accept", tag = "members", params(("code" = String, Path)),
    responses((status = 200, body = Garden), (status = 404, body = ErrorBody), (status = 429, body = ErrorBody)))]
pub async fn accept_invite(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    ApiPath(code): ApiPath<String>,
) -> Result<Wire<Garden>, ApiError> {
    limits::check(&state.limits.invite_accept, user.id())?;
    let code: InviteCode = code
        .trim()
        .to_ascii_uppercase()
        .parse()
        .map_err(|_| ApiError::not_found())?;
    let accepted = state
        .ports
        .store
        .accept_invite(user.id(), &code, state.ports.clock.now())
        .await?;
    let member = state.member::<Viewer>(&user, accepted.garden).await?;
    Ok(Wire::ok(state.ports.store.garden(member).await?))
}

/// `GET /gardens/{id}/map`: the current map or `null`. Any member.
#[utoipa::path(get, path = "/gardens/{id}/map", tag = "map", params(("id" = String, Path)),
    responses((status = 200, body = MapResponse), (status = 404, body = ErrorBody)))]
pub async fn get_map(
    State(state): State<SharedState>,
    access: Access<Viewer, table::Garden>,
) -> Result<Wire<MapResponse>, ApiError> {
    Ok(Wire::ok(MapResponse {
        map: state.ports.store.map(access.member).await?,
    }))
}

/// `PUT /gardens/{id}/map`: a new version, atomic; `409 invalid_state` with
/// `current_map_version` when `base_version` is stale, `400` with every unknown plant. Editor.
#[utoipa::path(put, path = "/gardens/{id}/map", tag = "map", params(("id" = String, Path),
        ("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    request_body = GardenMapInput,
    responses((status = 200, body = GardenMap), (status = 400, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn put_map(
    State(state): State<SharedState>,
    access: Access<Editor, table::Garden>,
    request: IdemJson<GardenMapInput>,
) -> Response {
    let IdemJson { body, begun, .. } = request;
    let result = async {
        // Shape rules (counts, polygons, distances, one pin per plant) are checked without the
        // database; the store then checks the version, the plants and deletions in one
        // transaction (database 4, garden map versions).
        let upload = body.validate()?;
        let new_plant_ids: Vec<PlantId> = upload
            .pins
            .iter()
            .filter(|pin| matches!(pin.plant, PinPlant::New(_)))
            .map(|_| state.ports.ids.id())
            .collect();
        let write = MapWrite {
            upload,
            new_plant_ids,
            created_at: state.ports.clock.now(),
        };
        Ok(Wire::ok(
            state.ports.store.put_map(access.member, write).await?,
        ))
    }
    .await;
    begun.finish(result).await
}
