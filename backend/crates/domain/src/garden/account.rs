//! The account endpoints ([contracts 9], account).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::DomainError,
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
    /// The Expo push token (`ExponentPushToken[…]` or `ExpoPushToken[…]`) for `ios` and
    /// `android`; for `web`, any printable string of at most 255 bytes, stored and never sent.
    pub token: PushToken,
    /// Its platform.
    pub platform: Platform,
}

impl PushTokenRequest {
    /// Checks the request ([contracts 9], account): an `ios` or `android` token must be an Expo
    /// push token, since Expo refuses a whole batch over one malformed `to` and one bad token
    /// would silence the pushes of every member sent with it. A `web` token is never sent, so
    /// it is kept as given.
    pub fn check(&self) -> Result<(), DomainError> {
        if self.platform.deliverable() && !self.token.is_expo() {
            return Err(DomainError::invalid(
                "token",
                "expected an Expo push token, ExponentPushToken[…] or ExpoPushToken[…]",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_token_request_shape_and_check() {
        let body = serde_json::json!({ "token": "ExponentPushToken[xxxxxxxxxxxxxxxxxxxxxx]", "platform": "ios" });
        let request: PushTokenRequest = serde_json::from_value(body.clone()).unwrap();
        assert_eq!(serde_json::to_value(&request).unwrap(), body);
        assert!(request.check().is_ok());

        let raw_apns = PushTokenRequest {
            token: "740f4707bebcf74f9b7c25d48e3358945f6aa01da5ddb387462c7eaf61bb78ad"
                .parse()
                .unwrap(),
            platform: Platform::Android,
        };
        let Err(DomainError::InvalidRequest { field, .. }) = raw_apns.check() else {
            panic!("a raw device token is refused");
        };
        assert_eq!(field, "token");

        let web = PushTokenRequest {
            platform: Platform::Web,
            ..raw_apns
        };
        assert!(web.check().is_ok(), "web tokens are stored as given");
    }
}
