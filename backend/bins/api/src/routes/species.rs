//! The species list ([contracts 9], species): any signed-in user; species aren't garden data.

use axum::extract::State;

use domain::{
    error::ErrorBody,
    garden::species::{SpeciesQuery, SpeciesSummary},
    id::SpeciesId,
    ports::SpeciesEntry,
    wire::{Page, PageRequest},
};

use crate::{
    error::ApiError,
    extract::{ApiPath, ApiQuery, auth::AuthUser},
    reply::Wire,
    state::{AppState, SharedState},
};

fn summary(state: &AppState, entry: SpeciesEntry) -> SpeciesSummary {
    let reference_images =
        SpeciesSummary::signed_images(&entry.species.id, &entry.photos, &state.signer());
    SpeciesSummary {
        species: entry.species,
        profile: entry.profile,
        classifier_supported: entry.classifier_supported,
        reference_images,
    }
}

/// `GET /species?q=`: by common name; `q` matches common and scientific names anywhere.
#[utoipa::path(get, path = "/species", tag = "species", params(SpeciesQuery, PageRequest),
    responses((status = 200, body = Page<SpeciesSummary>), (status = 401, body = ErrorBody)))]
pub async fn list_species(
    State(state): State<SharedState>,
    _user: AuthUser,
    ApiQuery(query): ApiQuery<SpeciesQuery>,
    ApiQuery(page): ApiQuery<PageRequest>,
) -> Result<Wire<Page<SpeciesSummary>>, ApiError> {
    let entries = state.ports.knowledge.species_list(query.q, page).await?;
    Ok(Wire::ok(entries.map(|e| summary(&state, e))))
}

/// `GET /species/{id}`: an unknown slug is `404`.
#[utoipa::path(get, path = "/species/{id}", tag = "species", params(("id" = String, Path)),
    responses((status = 200, body = SpeciesSummary), (status = 404, body = ErrorBody)))]
pub async fn get_species(
    State(state): State<SharedState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<SpeciesId>,
) -> Result<Wire<SpeciesSummary>, ApiError> {
    let entry = state
        .ports
        .knowledge
        .species_entry(&id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    Ok(Wire::ok(summary(&state, entry)))
}
