//! Gardens, members and invites ([contracts 9], gardens, members and invites).

use crate::{
    authz::{Membership, Owner, Viewer},
    garden::{
        gardens::{Garden, SoilProfileStatus, TimeZone},
        members::{Invite, Member},
    },
    id::{ConversationId, GardenId, InviteCode, InviteId, UserId},
    knowledge::SoilProfile,
    ports::{ObjectPrefix, StoreError},
    records::weather::{Coordinates, LocationPrecision, WeatherCell},
    wire::{Page, PageRequest, Role, Timestamp},
};

/// A location the geocoder resolved, ready to store.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedLocation {
    /// `garden.centroid`.
    pub coordinates: Coordinates,
    /// `garden.location_precision`.
    pub precision: LocationPrecision,
    /// `garden.location_label`.
    pub label: String,
    /// `garden.time_zone`.
    pub time_zone: TimeZone,
}

/// `POST /gardens`, resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct NewGarden {
    /// Its id.
    pub id: GardenId,
    /// Its name.
    pub name: String,
    /// Its location.
    pub location: ResolvedLocation,
}

/// `PATCH /gardens/{id}`, resolved.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GardenPatch {
    /// A new name.
    pub name: Option<String>,
    /// A new location.
    pub location: Option<ResolvedLocation>,
}

/// What the pipeline and the scheduler need of a garden.
#[derive(Debug, Clone, PartialEq)]
pub struct GardenContext {
    /// The garden.
    pub id: GardenId,
    /// Its name.
    pub name: String,
    /// Its centroid.
    pub coordinates: Coordinates,
    /// How precise it is.
    pub precision: LocationPrecision,
    /// Its weather cache cell.
    pub weather_cell: WeatherCell,
    /// Its time zone.
    pub time_zone: TimeZone,
    /// `SoilGrids`, when fetched.
    pub soil_profile: Option<SoilProfile>,
    /// The current map version, 0 for none.
    pub map_version: u32,
}

/// A new invite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewInvite {
    /// Its id.
    pub id: InviteId,
    /// 26 random base32 characters.
    pub code: InviteCode,
    /// The role it grants.
    pub role: Role,
    /// 7 days from now.
    pub expires_at: Timestamp,
}

/// What accepting an invite did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcceptedInvite {
    /// The garden.
    pub garden: GardenId,
    /// False for an existing member, whose role is unchanged.
    pub joined: bool,
}

/// What removing a member leaves for the shared-garden step of the cascade ([contracts 9],
/// removing a member): their conversations in the garden, with their chat image prefixes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedMember {
    /// The garden.
    pub garden: GardenId,
    /// The member.
    pub user: UserId,
    /// Their conversations in the garden.
    pub conversations: Vec<ConversationId>,
    /// Their chat images' prefixes, deleted before the records.
    pub chat_prefixes: Vec<ObjectPrefix>,
}

/// What a role change did: the member as they are now, and whether an owner lost the role, in
/// which case the transaction also marked the farm connectors they created in the garden
/// `deleting` ([connectors 8]), for the deletion runner to finish after the commit.
///
/// [connectors 8]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#8-privacy-and-security
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleChange {
    /// The member after the change.
    pub member: Member,
    /// The member was an owner and no longer is.
    pub demoted_owner: bool,
}

/// The `garden`, `member_of` and `invite` tables.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait GardenRepo: Send + Sync {
    /// `POST /gardens`: the garden and the caller's `owner` membership in one transaction.
    async fn create_garden(&self, owner: UserId, garden: NewGarden) -> Result<Garden, StoreError>;

    /// `GET /gardens`: the caller's, by name.
    async fn gardens(&self, user: UserId, page: PageRequest) -> Result<Page<Garden>, StoreError>;

    /// `GET /gardens/{id}`.
    async fn garden(&self, member: Membership<Viewer>) -> Result<Garden, StoreError>;

    /// `PATCH /gardens/{id}`.
    async fn update_garden(
        &self,
        owner: Membership<Owner>,
        patch: GardenPatch,
    ) -> Result<Garden, StoreError>;

    /// The garden as the pipeline and scheduler read it.
    async fn garden_context(&self, garden: GardenId) -> Result<GardenContext, StoreError>;

    /// Stores the `SoilGrids` profile, or marks it unavailable for a later retry.
    async fn set_soil_profile(
        &self,
        garden: GardenId,
        status: SoilProfileStatus,
        profile: Option<SoilProfile>,
    ) -> Result<(), StoreError>;

    /// Gardens whose `SoilGrids` fetch is pending or failed, for the background retry.
    async fn gardens_without_soil_profile(
        &self,
        limit: u32,
    ) -> Result<Vec<(GardenId, Coordinates)>, StoreError>;

    /// A failed `SoilGrids` fetch ([contracts 9], the soil profile): `soil_profile_status:
    /// unavailable` and the earliest next attempt in `garden.soil_retry_at`, which
    /// [`crate::rules::limits::soil_retry_after`] sets 15 minutes ahead after an outage and 7
    /// days ahead after no data. Storing a `ready` profile clears it.
    ///
    /// The default body stores the status without the time, so every failed garden is due again
    /// at once; the store overrides it.
    async fn soil_profile_failed(
        &self,
        garden: GardenId,
        retry_at: Timestamp,
    ) -> Result<(), StoreError> {
        let _ = retry_at;
        self.set_soil_profile(garden, SoilProfileStatus::Unavailable, None)
            .await
    }

    /// Gardens due a `SoilGrids` attempt, for the scheduler's `soil` task: `pending`, or
    /// `unavailable` with no `soil_retry_at` or one at or before `now`; never a garden being
    /// deleted; the longest-waiting first; at most `limit`.
    ///
    /// The default body ignores `soil_retry_at` (it is
    /// [`gardens_without_soil_profile`](Self::gardens_without_soil_profile)); the store
    /// overrides it.
    async fn gardens_due_soil_profile(
        &self,
        now: Timestamp,
        limit: u32,
    ) -> Result<Vec<(GardenId, Coordinates)>, StoreError> {
        let _ = now;
        self.gardens_without_soil_profile(limit).await
    }

    /// Which of these gardens still have a record, live or being deleted: the orphan sweep
    /// sweeps the prefix of a garden that has none.
    async fn existing_gardens(&self, gardens: &[GardenId]) -> Result<Vec<GardenId>, StoreError>;

    /// `GET /gardens/{id}/members`, by `since`.
    async fn members(
        &self,
        member: Membership<Viewer>,
        page: PageRequest,
    ) -> Result<Page<Member>, StoreError>;

    /// Every member's user id, for alert pushes and the garden-deleted push.
    async fn member_ids(&self, garden: GardenId) -> Result<Vec<UserId>, StoreError>;

    /// `GET /gardens/{id}/invites`: unused and unexpired at `now`, newest first.
    async fn invites(
        &self,
        owner: Membership<Owner>,
        page: PageRequest,
        now: Timestamp,
    ) -> Result<Page<Invite>, StoreError>;

    /// `POST /gardens/{id}/invites`.
    async fn create_invite(
        &self,
        owner: Membership<Owner>,
        invite: NewInvite,
    ) -> Result<Invite, StoreError>;

    /// `DELETE /gardens/{id}/invites/{invite_id}`: sets `revoked_at`.
    async fn revoke_invite(
        &self,
        owner: Membership<Owner>,
        invite: InviteId,
        now: Timestamp,
    ) -> Result<(), StoreError>;

    /// `POST /invites/{code}/accept`: `INSERT RELATION` into `member_of`, which fails on an
    /// existing member so a second accept never changes a role ([database 4], membership).
    /// Unknown, used, revoked or expired codes are `NotFound`.
    async fn accept_invite(
        &self,
        user: UserId,
        code: &InviteCode,
        now: Timestamp,
    ) -> Result<AcceptedInvite, StoreError>;

    /// `PATCH /gardens/{id}/members/{user_id}`. Demoting the last owner is
    /// `Rejected(InvalidState)`, checked in the transaction; demoting any other owner marks
    /// their connectors in the garden `deleting` in it ([`RoleChange::demoted_owner`]).
    async fn change_role(
        &self,
        owner: Membership<Owner>,
        member: UserId,
        role: Role,
    ) -> Result<RoleChange, StoreError>;

    /// `DELETE /gardens/{id}/members/{user_id}`: by an owner, or the member themselves (the
    /// caller checks which). The last owner can't leave. Deletes the membership and returns what
    /// the shared-garden cascade step must delete next.
    async fn remove_member(
        &self,
        member: Membership<Viewer>,
        user: UserId,
    ) -> Result<RemovedMember, StoreError>;
}
