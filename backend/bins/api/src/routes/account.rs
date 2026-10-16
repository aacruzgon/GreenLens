//! The account and push tokens ([contracts 9], account).

use axum::{extract::State, http::StatusCode};

use domain::{
    error::ErrorBody,
    garden::account::{AccountDeletion, Me, PushTokenRequest, UpdateMeRequest},
    id::PushToken,
};

use crate::{
    error::ApiError,
    extract::{ApiJson, ApiPath, auth::AuthUser},
    reply::Wire,
    state::SharedState,
};

/// `GET /me`. The user record was created on first sight by `AuthUser` if the webhook hadn't yet.
#[utoipa::path(get, path = "/me", tag = "account",
    responses((status = 200, body = Me), (status = 401, body = ErrorBody)))]
pub async fn get_me(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Wire<Me>, ApiError> {
    Ok(Wire::ok(state.ports.store.me(user.id()).await?))
}

/// `PATCH /me`: withdrawing location consent rounds every garden the user owns to about 10 km,
/// in the store's transaction.
#[utoipa::path(patch, path = "/me", tag = "account", request_body = UpdateMeRequest,
    responses((status = 200, body = Me), (status = 400, body = ErrorBody)))]
pub async fn update_me(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    ApiJson(patch): ApiJson<UpdateMeRequest>,
) -> Result<Wire<Me>, ApiError> {
    Ok(Wire::ok(
        state.ports.store.update_me(user.id(), patch).await?,
    ))
}

/// `DELETE /me`: `202` with what happens to each garden; the cascade runs in the background and
/// ends with the account's deletion at Clerk, retried until it succeeds (across restarts, through
/// the account's tombstone). A repeat while the cascade runs starts nothing; the token is
/// refused (`401`) once the tombstone exists, so a repeat usually never gets this far.
#[utoipa::path(delete, path = "/me", tag = "account",
    responses((status = 202, body = AccountDeletion), (status = 401, body = ErrorBody)))]
pub async fn delete_me(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Wire<AccountDeletion>, ApiError> {
    let plan = state.deletions.plan_account(user.id()).await?;
    state
        .deletions
        .start_account(user.id(), user.auth_id().clone())
        .await?;
    Ok(Wire::accepted(plan))
}

/// `POST /me/push-tokens`: `204`; registering the same token again is a no-op.
#[utoipa::path(post, path = "/me/push-tokens", tag = "account", request_body = PushTokenRequest,
    responses((status = 204), (status = 400, body = ErrorBody)))]
pub async fn register_push_token(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    ApiJson(body): ApiJson<PushTokenRequest>,
) -> Result<StatusCode, ApiError> {
    state
        .ports
        .store
        .register_push_token(user.id(), body)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /me/push-tokens/{token}`: the token is URL-encoded in the path (`[` and `]`); axum's
/// path extractor percent-decodes it.
#[utoipa::path(delete, path = "/me/push-tokens/{token}", tag = "account", params(("token" = String, Path)),
    responses((status = 204), (status = 401, body = ErrorBody)))]
pub async fn remove_push_token(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    ApiPath(token): ApiPath<PushToken>,
) -> Result<StatusCode, ApiError> {
    state
        .ports
        .store
        .remove_push_token(user.id(), &token)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
