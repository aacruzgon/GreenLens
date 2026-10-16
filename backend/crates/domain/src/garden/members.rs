//! Members and invites ([contracts 9], members and invites).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    id::{InviteCode, InviteId, UserId},
    wire::{Role, Timestamp},
};

/// `GET /gardens/{id}/members` items: `{ user_id, display_name, role, since }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Member {
    /// The member.
    pub user_id: UserId,
    /// Their display name.
    pub display_name: String,
    /// Their role.
    pub role: Role,
    /// When they joined.
    pub since: Timestamp,
}

/// `Invite` ([contracts 9]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Invite {
    /// The invite.
    pub id: InviteId,
    /// 26 random base32 characters (128 bits); shared as a link.
    pub code: InviteCode,
    /// The role the invitee gets.
    pub role: Role,
    /// Who made it; absent once that user is deleted (contracts v0.9, `created_by?: Id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub created_by: Option<UserId>,
    /// 7 days after it was made.
    pub expires_at: Timestamp,
}

/// `POST /gardens/{id}/invites`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateInviteRequest {
    /// The role.
    pub role: Role,
}

/// `PATCH /gardens/{id}/members/{user_id}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateMemberRequest {
    /// The new role; making someone `owner` adds an owner.
    pub role: Role,
}
