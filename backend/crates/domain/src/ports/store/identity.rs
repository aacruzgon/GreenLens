//! Users and push tokens ([contracts 9], account; architecture 4, identity).

use crate::{
    garden::account::{Me, Platform, PushTokenRequest, UpdateMeRequest},
    id::{AuthId, PushToken, UserId},
    ports::StoreError,
};

/// A device to push to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushTarget {
    /// Its user.
    pub user: UserId,
    /// The Expo token.
    pub token: PushToken,
    /// Its platform; only iOS and Android are delivered.
    pub platform: Platform,
}

/// The `user` and `push_token` tables.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait IdentityRepo: Send + Sync {
    /// Creates or finds the user for an auth id: an upsert on the unique `auth_id`, so two
    /// first requests racing each other agree (the `user.created` webhook creates nothing).
    /// Refused with `StoreError::NotFound` while the account's tombstone
    /// (`deleted_user:[auth_id]`, [`AccountDeletionMarker`](crate::ports::AccountDeletionMarker))
    /// exists, checked in the same transaction as the create and before an existing user is
    /// returned, so a deleted account's still-valid session token neither recreates the user
    /// nor keeps using it during the cascade.
    async fn upsert_user(
        &self,
        auth_id: &AuthId,
        display_name: Option<String>,
    ) -> Result<UserId, StoreError>;

    /// The user of an auth id, without creating one (the `user.deleted` webhook).
    async fn user_by_auth(&self, auth_id: &AuthId) -> Result<Option<UserId>, StoreError>;

    /// `GET /me`.
    async fn me(&self, user: UserId) -> Result<Me, StoreError>;

    /// `PATCH /me`. Withdrawing consent rounds the location of every garden the user owns to
    /// about 10 km (`about_10_km`) in the same transaction.
    async fn update_me(&self, user: UserId, patch: UpdateMeRequest) -> Result<Me, StoreError>;

    /// `POST /me/push-tokens`: an upsert on `push_token:[token]`, so registering again is a
    /// no-op.
    async fn register_push_token(
        &self,
        user: UserId,
        request: PushTokenRequest,
    ) -> Result<(), StoreError>;

    /// `DELETE /me/push-tokens/{token}`.
    async fn remove_push_token(&self, user: UserId, token: &PushToken) -> Result<(), StoreError>;

    /// The deliverable tokens of these users.
    async fn push_targets(&self, users: &[UserId]) -> Result<Vec<PushTarget>, StoreError>;

    /// Removes tokens Expo reported as `DeviceNotRegistered`.
    async fn forget_push_tokens(&self, tokens: &[PushToken]) -> Result<(), StoreError>;
}
