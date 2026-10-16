//! The auth webhook ([contracts 9], auth webhook): Clerk's events through Svix, verified before
//! the body is parsed.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::id::AuthId;

/// `AuthWebhookEvent.type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub enum AuthEventType {
    /// Upsert the user on `auth_id`.
    #[serde(rename = "user.created")]
    UserCreated,
    /// Run the account deletion cascade.
    #[serde(rename = "user.deleted")]
    UserDeleted,
}

/// `AuthWebhookEvent.data`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AuthEventData {
    /// The provider's user id, stored as `user.auth_id`.
    pub id: AuthId,
}

/// `AuthWebhookEvent` ([contracts 9]). `204`, idempotent. Clerk's payload has more fields; they
/// are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AuthWebhookEvent {
    /// `user.created` or `user.deleted`.
    #[serde(rename = "type")]
    pub event_type: AuthEventType,
    /// The user.
    pub data: AuthEventData,
}

/// Svix's three headers, borrowed from the request for verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebhookHeaders<'a> {
    /// `svix-id`.
    pub id: &'a str,
    /// `svix-timestamp`, Unix seconds.
    pub timestamp: &'a str,
    /// `svix-signature`: space-separated `v1,<base64>` entries.
    pub signature: &'a str,
}

/// Svix rejects a timestamp more than 5 minutes from now.
pub const WEBHOOK_TOLERANCE: std::time::Duration = std::time::Duration::from_mins(5);
