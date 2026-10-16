//! The signed image routes ([contracts 3], images and masks; [contracts 8], chat images;
//! [contracts 9], species photos; [storage 3]). No `Authorization`: the signature is the
//! authorisation, so plain `<img>` tags work in the web build. A bad or expired signature is
//! `403 invalid_signature`; a valid one for a record that no longer exists is `404`.

use axum::{extract::State, response::Response};

use domain::{
    error::ErrorBody,
    id::{ChatImageId, ConversationId, ImageId, MaskId, ScanId, SpeciesId},
    url::{ImagePath, PhotoHash},
};

use crate::{
    error::ApiError,
    extract::{ApiPath, ApiQuery},
    media::{self, SignedQuery},
    state::SharedState,
};

/// `GET /scans/{id}/images/{image_id}?exp=&sig=`: the face-blurred photo, `image/webp`.
#[utoipa::path(get, path = "/scans/{id}/images/{image_id}", tag = "media", security(()),
    params(("id" = String, Path), ("image_id" = String, Path), SignedQuery),
    responses((status = 200, content_type = "image/webp"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub async fn scan_image(
    State(state): State<SharedState>,
    ApiPath((scan, image)): ApiPath<(ScanId, ImageId)>,
    ApiQuery(query): ApiQuery<SignedQuery>,
) -> Result<Response, ApiError> {
    media::verify(&state, &ImagePath::ScanImage { scan, image }, &query)?;
    let object = state
        .ports
        .store
        .image_object(scan, image)
        .await?
        .ok_or_else(ApiError::not_found)?;
    media::stream_object(&*state.ports.blob, object, &state.shutdown).await
}

/// `GET /scans/{id}/masks/{mask_id}?exp=&sig=`: a segmentation mask, `image/png`.
#[utoipa::path(get, path = "/scans/{id}/masks/{mask_id}", tag = "media", security(()),
    params(("id" = String, Path), ("mask_id" = String, Path), SignedQuery),
    responses((status = 200, content_type = "image/png"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub async fn scan_mask(
    State(state): State<SharedState>,
    ApiPath((scan, mask)): ApiPath<(ScanId, MaskId)>,
    ApiQuery(query): ApiQuery<SignedQuery>,
) -> Result<Response, ApiError> {
    media::verify(&state, &ImagePath::Mask { scan, mask }, &query)?;
    let object = state
        .ports
        .store
        .mask_object(scan, mask)
        .await?
        .ok_or_else(ApiError::not_found)?;
    media::stream_object(&*state.ports.blob, object, &state.shutdown).await
}

/// `GET /conversations/{id}/images/{image_id}?exp=&sig=`: a chat image.
#[utoipa::path(get, path = "/conversations/{id}/images/{image_id}", tag = "media", security(()),
    params(("id" = String, Path), ("image_id" = String, Path), SignedQuery),
    responses((status = 200, content_type = "image/webp"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub async fn chat_image(
    State(state): State<SharedState>,
    ApiPath((conversation, image)): ApiPath<(ConversationId, ChatImageId)>,
    ApiQuery(query): ApiQuery<SignedQuery>,
) -> Result<Response, ApiError> {
    media::verify(
        &state,
        &ImagePath::ChatImage {
            conversation,
            image,
        },
        &query,
    )?;
    let object = state
        .ports
        .store
        .chat_image_object(conversation, image)
        .await?
        .ok_or_else(ApiError::not_found)?;
    media::stream_object(&*state.ports.blob, object, &state.shutdown).await
}

/// `GET /species/{id}/images/{hash}?exp=&sig=`: a species reference photo from `knowledge/`.
#[utoipa::path(get, path = "/species/{id}/images/{hash}", tag = "media", security(()),
    params(("id" = String, Path), ("hash" = String, Path), SignedQuery),
    responses((status = 200, content_type = "image/webp"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub async fn species_photo(
    State(state): State<SharedState>,
    ApiPath((species, hash)): ApiPath<(SpeciesId, String)>,
    ApiQuery(query): ApiQuery<SignedQuery>,
) -> Result<Response, ApiError> {
    let hash = PhotoHash::parse(&hash).ok_or_else(ApiError::not_found)?;
    media::verify(
        &state,
        &ImagePath::SpeciesPhoto {
            species: species.clone(),
            hash,
        },
        &query,
    )?;
    let object = state
        .ports
        .store
        .species_photo_object(&species, hash)
        .await?
        .ok_or_else(ApiError::not_found)?;
    media::stream_object(&*state.ports.blob, object, &state.shutdown).await
}
