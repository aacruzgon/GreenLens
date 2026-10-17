//! Plants, their timeline and growth series, and species confirmation ([contracts 9], plants;
//! [contracts 3], `POST /plants/{id}/species`).

use std::time::Duration;

use axum::{extract::State, http::StatusCode, response::Response};

use domain::{
    authz::{Editor, Owner, Viewer},
    chat::PlantBeliefs,
    error::ErrorBody,
    garden::plants::{
        CreatePlantRequest, MeasurementPoint, MeasurementsQuery, PlantsQuery, TimelineEntry,
        UpdatePlantRequest,
    },
    id::table,
    ports::{BeliefSubject, IdGenExt, NewPlant},
    scan::{ConfirmSpeciesRequest, Plant},
    url::Sign,
    wire::{Page, PageRequest},
};

use crate::{
    error::ApiError,
    extract::{ApiJson, ApiQuery, access::Access, auth::AuthUser, idempotency::IdemJson},
    reply::Wire,
    state::SharedState,
};

/// `GET /plants?garden_id=`: by name. Any member.
#[utoipa::path(get, path = "/plants", tag = "plants", params(PlantsQuery, PageRequest),
    responses((status = 200, body = Page<Plant>), (status = 404, body = ErrorBody)))]
pub async fn list_plants(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    ApiQuery(query): ApiQuery<PlantsQuery>,
    ApiQuery(page): ApiQuery<PageRequest>,
) -> Result<Wire<Page<Plant>>, ApiError> {
    let member = state.member::<Viewer>(&user, query.garden_id).await?;
    let plants = state.ports.store.plants(member, page).await?;
    Ok(Wire::ok(plants.sign((), &state.signer())))
}

/// `POST /plants`. Editor.
#[utoipa::path(post, path = "/plants", tag = "plants", request_body = CreatePlantRequest,
    params(("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    responses((status = 201, body = Plant), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub async fn create_plant(
    State(state): State<SharedState>,
    request: IdemJson<CreatePlantRequest>,
) -> Response {
    let IdemJson { user, body, begun } = request;
    let result = async {
        let editor = state.member::<Editor>(&user, body.garden_id).await?;
        let name = body.name.trim().to_owned();
        if name.is_empty() || name.chars().count() > 80 {
            return Err(ApiError::invalid_request("name", "1-80 characters"));
        }
        let plant = state
            .ports
            .store
            .create_plant(
                editor,
                NewPlant {
                    id: state.ports.ids.id(),
                    name,
                    species: body.species_id,
                },
            )
            .await?;
        Ok(Wire::created(plant.sign((), &state.signer())))
    }
    .await;
    begun.finish(result).await
}

/// `GET /plants/{id}`. Any member.
#[utoipa::path(get, path = "/plants/{id}", tag = "plants", params(("id" = String, Path)),
    responses((status = 200, body = Plant), (status = 404, body = ErrorBody)))]
pub async fn get_plant(
    State(state): State<SharedState>,
    access: Access<Viewer, table::Plant>,
) -> Result<Wire<Plant>, ApiError> {
    let plant = state.ports.store.plant(access.member, access.id).await?;
    Ok(Wire::ok(plant.sign((), &state.signer())))
}

/// `PATCH /plants/{id}`: name and status. Editor.
#[utoipa::path(patch, path = "/plants/{id}", tag = "plants", params(("id" = String, Path)),
    request_body = UpdatePlantRequest,
    responses((status = 200, body = Plant), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub async fn update_plant(
    State(state): State<SharedState>,
    access: Access<Editor, table::Plant>,
    ApiJson(body): ApiJson<UpdatePlantRequest>,
) -> Result<Wire<Plant>, ApiError> {
    let plant = state
        .ports
        .store
        .update_plant(access.member, access.id, body)
        .await?;
    Ok(Wire::ok(plant.sign((), &state.signer())))
}

/// `DELETE /plants/{id}`: `202`; the cascade (its scans, tasks, the tasks other plans made for
/// it, conversations, memories, its pin) runs in the background. Owner.
#[utoipa::path(delete, path = "/plants/{id}", tag = "plants", params(("id" = String, Path)),
    responses((status = 202), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub async fn delete_plant(
    State(state): State<SharedState>,
    access: Access<Owner, table::Plant>,
) -> Result<StatusCode, ApiError> {
    let started = state
        .ports
        .store
        .start_plant_deletion(access.member, access.id, state.ports.clock.now())
        .await?;
    state.deletions.spawn_plant(access.member.garden(), started);
    Ok(StatusCode::ACCEPTED)
}

/// `GET /plants/{id}/timeline`: newest first, with signed thumbnails. Any member.
#[utoipa::path(get, path = "/plants/{id}/timeline", tag = "plants", params(("id" = String, Path), PageRequest),
    responses((status = 200, body = Page<TimelineEntry>), (status = 404, body = ErrorBody)))]
pub async fn timeline(
    State(state): State<SharedState>,
    access: Access<Viewer, table::Plant>,
    ApiQuery(page): ApiQuery<PageRequest>,
) -> Result<Wire<Page<TimelineEntry>>, ApiError> {
    let entries = state
        .ports
        .store
        .timeline(access.member, access.id, page)
        .await?;
    Ok(Wire::ok(entries.sign((), &state.signer())))
}

/// How long `GET /plants/{id}/beliefs` waits for Agent Memory: the adapter's own deadline of an
/// entity read, for both reads together.
const BELIEFS_TIMEOUT: Duration = Duration::from_secs(3);

/// `GET /plants/{id}/beliefs`: what is believed about the plant now (its current beliefs and its
/// garden's) and its superseded beliefs, newest first, from Agent Memory. Shared memories only,
/// so any member. Never a 5xx: Agent Memory off, failing or slower than 3 s is
/// `available: false` with empty lists.
#[utoipa::path(get, path = "/plants/{id}/beliefs", tag = "plants", params(("id" = String, Path)),
    responses((status = 200, body = PlantBeliefs), (status = 404, body = ErrorBody)))]
pub async fn plant_beliefs(
    State(state): State<SharedState>,
    access: Access<Viewer, table::Plant>,
) -> Result<Wire<PlantBeliefs>, ApiError> {
    let unavailable = PlantBeliefs {
        available: false,
        current: Vec::new(),
        history: Vec::new(),
    };
    let beliefs = &state.ports.beliefs;
    if !beliefs.enabled() {
        return Ok(Wire::ok(unavailable));
    }
    let garden = access.member.garden();
    let plant = access.id;
    let subject = BeliefSubject {
        garden,
        plant: Some(plant),
    };
    let reads = async {
        tokio::try_join!(
            beliefs.current(garden, Some(plant)),
            beliefs.history(subject)
        )
    };
    match tokio::time::timeout(BELIEFS_TIMEOUT, reads).await {
        Ok(Ok((current, history))) => Ok(Wire::ok(PlantBeliefs {
            available: true,
            current,
            history: history
                .into_iter()
                .filter(|b| b.superseded_at.is_some())
                .collect(),
        })),
        Ok(Err(e)) => {
            tracing::warn!(%plant, error = %crate::error::chain(&e), "plant beliefs unavailable");
            Ok(Wire::ok(unavailable))
        }
        Err(_) => {
            tracing::warn!(%plant, "plant beliefs timed out");
            Ok(Wire::ok(unavailable))
        }
    }
}

/// `GET /plants/{id}/measurements?quantity=`: the growth series, oldest first. Any member.
#[utoipa::path(get, path = "/plants/{id}/measurements", tag = "plants",
    params(("id" = String, Path), MeasurementsQuery, PageRequest),
    responses((status = 200, body = Page<MeasurementPoint>), (status = 404, body = ErrorBody)))]
pub async fn measurements(
    State(state): State<SharedState>,
    access: Access<Viewer, table::Plant>,
    ApiQuery(query): ApiQuery<MeasurementsQuery>,
    ApiQuery(page): ApiQuery<PageRequest>,
) -> Result<Wire<Page<MeasurementPoint>>, ApiError> {
    Ok(Wire::ok(
        state
            .ports
            .store
            .growth_series(access.member, access.id, query.quantity, page)
            .await?,
    ))
}

/// `POST /plants/{id}/species`: confirm or correct; the plant's most recent scan follows it, a
/// new run when it is past `queued` ([contracts 3]). Editor.
#[utoipa::path(post, path = "/plants/{id}/species", tag = "plants", params(("id" = String, Path)),
    request_body = ConfirmSpeciesRequest,
    responses((status = 200, body = Plant), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
        (status = 503, body = ErrorBody)))]
pub async fn confirm_species(
    State(state): State<SharedState>,
    access: Access<Editor, table::Plant>,
    ApiJson(body): ApiJson<ConfirmSpeciesRequest>,
) -> Result<Wire<Plant>, ApiError> {
    let plant = state
        .pipeline
        .species_confirmed(access.member, access.id, body.choice()?)
        .await?;
    Ok(Wire::ok(plant.sign((), &state.signer())))
}
