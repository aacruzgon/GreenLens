//! The auth provider (Clerk, with Svix webhooks): verifying session tokens and webhooks, and
//! deleting the account at the provider.

use crate::{
    garden::webhook::{AuthWebhookEvent, WebhookHeaders},
    id::AuthId,
    ports::{AuthError, ProviderError},
    wire::Timestamp,
};

/// A verified session token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedToken {
    /// The `sub` claim.
    pub auth_id: AuthId,
    /// A display name, when the token carries one; the user record's default.
    pub display_name: Option<String>,
    /// The `exp` claim.
    pub expires_at: Timestamp,
}

/// Verifies `Authorization: Bearer <Clerk session token>` against the provider's JWKS
/// ([contracts 1], authentication headers; architecture 4, identity).
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait AuthVerifier: Send + Sync {
    /// Checks the signature, issuer, `azp`, `exp` and `nbf` at `now`. Fetches and caches the
    /// JWKS, refreshing on an unknown `kid`.
    async fn verify(&self, bearer: &str, now: Timestamp) -> Result<VerifiedToken, AuthError>;
}

/// Verifies a Svix-delivered webhook before the body is parsed ([contracts 9], auth webhook):
/// the signature over `id.timestamp.body`, and a timestamp within 5 minutes of `now`.
/// Synchronous: an HMAC over a few kilobytes.
#[cfg_attr(feature = "mock", mockall::automock)]
pub trait WebhookVerifier: Send + Sync {
    /// Verifies, then parses `user.created` and `user.deleted`; other event types are
    /// `Ok(None)`.
    fn verify<'a>(
        &self,
        headers: WebhookHeaders<'a>,
        body: &'a [u8],
        now: Timestamp,
    ) -> Result<Option<AuthWebhookEvent>, AuthError>;
}

/// The provider's Backend API, for the last step of account deletion (architecture 5,
/// deletion step 5): retried until it succeeds.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait AuthAdmin: Send + Sync {
    /// Deletes the provider account. An account that is already gone is success, so retries are
    /// idempotent.
    async fn delete_account(&self, auth_id: &AuthId) -> Result<(), ProviderError>;
}
