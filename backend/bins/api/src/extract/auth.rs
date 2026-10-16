//! Who is calling: an app user (Clerk session token) or a worker (worker token plus
//! `X-Lease-Token`) ([contracts 1], authentication headers).

use std::sync::Arc;

use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};

use domain::{
    authz::{self, CurrentUser},
    id::{JobId, LeaseToken, WorkerId},
    ports::Lease,
};

use crate::{error::ApiError, extract, limits, state::AppState};

/// A signed-in user. Verifies the bearer token through the `AuthVerifier` port and maps its
/// `sub` to a user, creating the user on first sight (`authz::authenticate`: an upsert on
/// `auth_id`, so two first requests racing each other agree; the `user.created` webhook
/// creates nothing, api 22 question 24). A deleted account's token, still valid, is refused
/// with `401` while its tombstone exists, so it never creates the user again. Then the general
/// per-user rate limit. The user is cached in the request's extensions, so a second extractor
/// on the same request (an access extractor and the idempotency extractor) doesn't verify twice.
#[derive(Debug, Clone)]
pub struct AuthUser(pub CurrentUser);

impl<S> FromRequestParts<S> for AuthUser
where
    Arc<AppState>: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        if let Some(user) = parts.extensions.get::<CurrentUser>() {
            return Ok(Self(user.clone()));
        }
        let app = Arc::<AppState>::from_ref(state);
        let token = extract::bearer(parts).ok_or_else(ApiError::unauthenticated)?;
        let user = authz::authenticate(
            &*app.ports.auth,
            &*app.ports.store,
            token,
            app.ports.clock.now(),
        )
        .await?;
        limits::check(&app.limits.per_user, user.id())?;
        parts.extensions.insert(user.clone());
        Ok(Self(user))
    }
}

/// A worker, identified by its bearer token ([`crate::workers::WorkerTokens`]).
#[derive(Debug, Clone)]
pub struct WorkerAuth(pub WorkerId);

impl<S> FromRequestParts<S> for WorkerAuth
where
    Arc<AppState>: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    /// In memory, so a ready future rather than an `async fn` with nothing to await.
    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let app = Arc::<AppState>::from_ref(state);
        std::future::ready(
            extract::bearer(parts)
                .and_then(|token| app.workers.identify(token))
                .map(Self)
                .ok_or_else(ApiError::unauthenticated),
        )
    }
}

/// A worker's hold on the job in the path: the worker token, the `{id}` capture and
/// `X-Lease-Token` ([contracts 5]). The store compares all three inside its write, so a stale
/// token is `409 lease_lost` there, never a separate read here.
#[derive(Debug, Clone)]
pub struct JobLease(pub Lease);

impl<S> FromRequestParts<S> for JobLease
where
    Arc<AppState>: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let WorkerAuth(worker) = WorkerAuth::from_request_parts(parts, state).await?;
        let job: JobId = extract::path_param(parts, state, "id").await?;
        let token: LeaseToken = parts
            .headers
            .get("x-lease-token")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| ApiError::invalid_request("X-Lease-Token", "missing or not a ULID"))?;
        Ok(Self(Lease { job, token, worker }))
    }
}
