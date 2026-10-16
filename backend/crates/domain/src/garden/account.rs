//! The account endpoints ([contracts 9], account).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    id::{GardenId, PushToken, UserId},
    wire::Timestamp,
};

/// `GET /me` and `PATCH /me`: `{ id, display_name, location_consent, created_at }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Me {
    /// The user.
    pub id: UserId,
    /// Their display name.
    pub display_name: String,
    /// Whether gardens may store GPS coordinates.
    pub location_consent: bool,
    /// When the account was created.
    pub created_at: Timestamp,
}

/// `PATCH /me`. Withdrawing consent rounds every owned garden to `about_10_km`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateMeRequest {
    /// New display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub display_name: Option<String>,
    /// New consent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub location_consent: Option<bool>,
}

/// `DELETE /me` `transferred_gardens[]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GardenTransfer {
    /// The garden.
    pub garden_id: GardenId,
    /// The longest-standing editor, now owner.
    pub new_owner_user_id: UserId,
}

/// `202` of `DELETE /me`. The cascade runs in the background (architecture 5, deletion).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct AccountDeletion {
    /// Gardens deleted outright.
    pub deleted_gardens: Vec<GardenId>,
    /// Gardens handed to another member.
    pub transferred_gardens: Vec<GardenTransfer>,
}

/// `POST /me/push-tokens` `platform`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    /// Delivered through Expo Push.
    Ios,
    /// Delivered through Expo Push.
    Android,
    /// Stored, not yet used: Expo Push doesn't deliver to the web.
    Web,
}

impl Platform {
    /// Whether Expo Push delivers to it.
    pub const fn deliverable(self) -> bool {
        matches!(self, Self::Ios | Self::Android)
    }
}

/// `POST /me/push-tokens`: registering the same token again is a no-op.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PushTokenRequest {
    /// The Expo push token.
    pub token: PushToken,
    /// Its platform.
    pub platform: Platform,
}
