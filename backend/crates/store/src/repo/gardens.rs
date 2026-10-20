//! Gardens, members and invites ([contracts 9]; [database 4], membership ✓).

use chrono::{DateTime, Utc};
use domain::{
    authz::{Membership, Owner, Viewer},
    garden::{
        gardens::{Garden, SoilProfileStatus, TimeZone},
        members::{Invite, Member},
    },
    id::{ConversationId, GardenId, InviteCode, InviteId, UserId},
    knowledge::SoilProfile,
    ports::{
        AcceptedInvite, GardenContext, GardenPatch, GardenRepo, NewGarden, NewInvite, ObjectPrefix,
        RemovedMember, RoleChange, StoreError,
    },
    records::weather::Coordinates,
    wire::{Page, PageRequest, Role, Timestamp},
};
use surrealdb::types::{RecordId, SurrealValue};

use crate::{
    SurrealStore,
    error::{RowError, take_failed},
    ids::{from_opt_rid, from_rid, member_rid, rid},
    read::{page as page_of, take_all, take_one},
    rows::{LocationPrecisionRow, RoleRow, SoilProfileStatusRow, dt, to_json, ts, u32_of},
    tx::run_tx,
};

/// The statements.
pub mod sql {
    /// `POST /gardens`: the garden and the caller's `owner` membership in one transaction, the
    /// membership with [database 4]'s `INSERT RELATION` ✓ (it fails on an existing edge;
    /// `RELATE` with a fixed id would overwrite it). The centroid is `type::point([lon, lat])`
    /// ✓: a `(lon, lat)` literal doesn't take binds (a parse error on 3.3.4). Statements:
    /// 0 `BEGIN`, 1 `CREATE`, 2 `INSERT RELATION`, 3 the garden read, 4 `COMMIT`.
    pub const CREATE_GARDEN: &str = "BEGIN;
CREATE $garden SET name = $name, centroid = type::point([$lon, $lat]), location_precision = $precision,
  location_label = $label, time_zone = $time_zone;
INSERT RELATION INTO member_of { id: type::record('member_of', [$user, $garden]), in: $user, out: $garden, role: 'owner' };
SELECT *, 'owner' AS my_role, 1 AS member_count FROM ONLY $garden;
COMMIT;";
    /// The index of the garden read in [`CREATE_GARDEN`].
    pub const CREATE_GARDEN_RESULT: usize = 3;

    /// A garden as a member reads it (`GET /gardens/{id}`).
    pub const GARDEN: &str = "SELECT *, (SELECT VALUE role FROM ONLY type::record('member_of', [$user, $parent.id])) AS my_role,
       count(SELECT VALUE id FROM member_of WHERE out = $parent.id) AS member_count
FROM ONLY $garden WHERE deleting_at = NONE;";

    /// `GET /gardens`: the caller's gardens, by name, paged by `(name, id)`.
    pub const GARDENS: &str = "SELECT out AS id, out.name AS name, role AS my_role, out.location_label AS location_label,
       out.location_precision AS location_precision, out.time_zone AS time_zone,
       out.soil_profile_status AS soil_profile_status,
       count(SELECT VALUE id FROM member_of WHERE out = $parent.out) AS member_count, out.created_at AS created_at
FROM member_of WHERE in = $user AND out.deleting_at = NONE AND ($after = NONE OR [out.name, out] > $after)
ORDER BY name, id LIMIT $limit;";
    /// The cursor key of a garden.
    pub const GARDEN_CURSOR: &str = "SELECT VALUE [name, id] FROM ONLY $garden;";

    /// `POST /invites/{code}/accept` ([database 4], membership ✓): a live invite of a live
    /// garden, then `INSERT RELATION` unless the caller is a member already, whose role is
    /// never changed. Statements: 0 `BEGIN`, 1 `LET $inv`, 2 `IF` (unknown → `not_found`),
    /// 3 `LET $member`, 4 `LET $joined`, 5 the result, 6 `COMMIT`.
    pub const ACCEPT_INVITE: &str = "BEGIN;
LET $inv = (SELECT id, garden, role FROM invite WHERE code = $code AND used_at = NONE AND revoked_at = NONE
            AND expires_at > $now AND garden.deleting_at = NONE LIMIT 1)[0];
IF $inv = NONE { THROW 'not_found' };
LET $member = type::record('member_of', [$user, $inv.garden]);
LET $joined = IF array::len(SELECT VALUE id FROM $member) > 0 { false } ELSE {
  INSERT RELATION INTO member_of { id: $member, in: $user, out: $inv.garden, role: $inv.role };
  UPDATE $inv.id SET used_at = $now, used_by = $user;
  true
};
RETURN { garden: $inv.garden, joined: $joined };
COMMIT;";
    /// The index of the result in [`ACCEPT_INVITE`].
    pub const ACCEPT_INVITE_RESULT: usize = 5;

    /// `PATCH /gardens/{id}/members/{user_id}`; demoting the last owner throws
    /// `invalid_state`. Demoting an owner marks the connectors they created in the garden
    /// `deleting` in the same transaction ([connectors 8]: the consent was theirs); `api`
    /// revokes their tokens and deletes them after the commit (EON E3.6).
    ///
    /// [connectors 8]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#8-privacy-and-security
    pub const CHANGE_ROLE: &str = "BEGIN;
LET $owners = count(SELECT VALUE id FROM member_of WHERE out = $garden AND role = 'owner');
LET $current = (SELECT VALUE role FROM ONLY $member);
IF $current = NONE { THROW 'not_found' };
IF $current = 'owner' AND $role != 'owner' AND $owners <= 1 { THROW 'invalid_state' };
UPDATE $member SET role = $role;
IF $current = 'owner' AND $role != 'owner' { UPDATE connector SET status = 'deleting' WHERE garden = $garden AND created_by = $user AND status != 'deleting' };
SELECT in AS user_id, in.display_name AS display_name, role, since FROM ONLY $member;
RETURN $current = 'owner' AND $role != 'owner';
COMMIT;";

    /// `DELETE /gardens/{id}/members/{user_id}`; the last owner can't leave. The member's
    /// connectors in the garden are marked `deleting` in the same transaction, as for a
    /// demotion; the `Member` cascade revokes and deletes them.
    pub const REMOVE_MEMBER: &str = "BEGIN;
LET $current = (SELECT VALUE role FROM ONLY $member);
IF $current = NONE { THROW 'not_found' };
IF $current = 'owner' AND count(SELECT VALUE id FROM member_of WHERE out = $garden AND role = 'owner') <= 1 { THROW 'invalid_state' };
DELETE $member;
UPDATE connector SET status = 'deleting' WHERE garden = $garden AND created_by = $user AND status != 'deleting';
SELECT VALUE id FROM conversation WHERE garden = $garden AND user = $user;
COMMIT;";

    /// `POST /gardens/{id}/invites`.
    pub const CREATE_INVITE: &str = "CREATE $invite SET garden = $garden, code = $code, role = $role, created_by = $user, expires_at = $expires_at;";
    /// `GET /gardens/{id}/invites`: unused and unexpired, newest first (ULIDs sort by time).
    pub const INVITES: &str = "SELECT id, code, role, created_by, expires_at FROM invite WHERE garden = $garden AND used_at = NONE
  AND revoked_at = NONE AND expires_at > $now AND ($after = NONE OR id < $after) ORDER BY id DESC LIMIT $limit;";
    /// `DELETE /gardens/{id}/invites/{invite_id}`.
    pub const REVOKE_INVITE: &str = "UPDATE $invite SET revoked_at = $now WHERE garden = $garden;";
    /// `GET /gardens/{id}/members`, by `since` (index `member_garden`).
    pub const MEMBERS: &str = "SELECT in AS user_id, in.display_name AS display_name, role, since FROM member_of
WHERE out = $garden AND ($after = NONE OR [since, in] > $after) ORDER BY since, user_id LIMIT $limit;";
    /// The cursor key of a member.
    pub const MEMBER_CURSOR: &str = "SELECT VALUE [since, in] FROM ONLY $member;";
    /// Every member's id.
    pub const MEMBER_IDS: &str = "SELECT VALUE in FROM member_of WHERE out = $garden;";
    /// The garden as the pipeline and scheduler read it.
    pub const GARDEN_CONTEXT: &str = crate::repo::load::sql::GARDEN_CONTEXT;
    /// Stores the `SoilGrids` result.
    pub const SET_SOIL_PROFILE: &str =
        "UPDATE $garden SET soil_profile_status = $status, soil_profile = $profile,
  soil_retry_at = IF $status = 'ready' { NONE } ELSE { soil_retry_at };";
    /// A failed `SoilGrids` fetch: `unavailable` until `$retry_at`; the profile is kept.
    pub const SOIL_PROFILE_FAILED: &str =
        "UPDATE $garden SET soil_profile_status = 'unavailable', soil_retry_at = $retry_at;";
    /// Gardens due a `SoilGrids` attempt: `pending`, or `unavailable` whose retry time has
    /// come; the oldest retry first (`NONE` sorts first).
    pub const DUE_SOIL_PROFILE: &str = "SELECT id, centroid.coordinates[0] AS lon, centroid.coordinates[1] AS lat, soil_retry_at FROM garden
WHERE deleting_at = NONE AND (soil_profile_status = 'pending'
  OR (soil_profile_status = 'unavailable' AND (soil_retry_at = NONE OR soil_retry_at <= $now)))
ORDER BY soil_retry_at LIMIT $limit;";
    /// Gardens whose `SoilGrids` fetch is pending or failed.
    pub const WITHOUT_SOIL_PROFILE: &str =
        "SELECT id, centroid.coordinates[0] AS lon, centroid.coordinates[1] AS lat FROM garden
WHERE soil_profile_status IN ['pending', 'unavailable'] AND deleting_at = NONE LIMIT $limit;";
    /// `PATCH /gardens/{id}`.
    pub const UPDATE_GARDEN: &str = "UPDATE $garden SET name = $name ?? name, centroid = IF $lon != NONE { type::point([$lon, $lat]) } ELSE { centroid },
  location_precision = $precision ?? location_precision, location_label = $label ?? location_label, time_zone = $time_zone ?? time_zone
WHERE deleting_at = NONE;";
}

/// A `garden` row as [`sql::GARDEN`] and [`sql::CREATE_GARDEN`] read it.
#[derive(Debug, Clone, SurrealValue)]
pub struct GardenRow {
    /// `garden:<ulid>`.
    pub id: RecordId,
    /// Its name.
    pub name: String,
    /// The caller's role.
    pub my_role: RoleRow,
    /// "Toronto, ON".
    pub location_label: String,
    /// How precise the stored location is.
    pub location_precision: LocationPrecisionRow,
    /// IANA time zone.
    pub time_zone: String,
    /// The `SoilGrids` fetch.
    pub soil_profile_status: SoilProfileStatusRow,
    /// Members.
    pub member_count: i64,
    /// When it was created.
    pub created_at: DateTime<Utc>,
}

impl TryFrom<GardenRow> for Garden {
    type Error = RowError;

    fn try_from(row: GardenRow) -> Result<Self, RowError> {
        Ok(Self {
            id: from_rid(&row.id)?,
            name: row.name,
            my_role: row.my_role.into(),
            location_label: row.location_label,
            location_precision: row.location_precision.into(),
            time_zone: TimeZone(row.time_zone),
            soil_profile_status: row.soil_profile_status.into(),
            member_count: u32_of("member_count", row.member_count)?,
            created_at: ts(row.created_at),
        })
    }
}

/// One row of [`sql::WITHOUT_SOIL_PROFILE`].
#[derive(Debug, Clone, SurrealValue)]
pub struct PointRow {
    /// The garden.
    pub id: RecordId,
    /// Longitude.
    pub lon: f64,
    /// Latitude.
    pub lat: f64,
}

/// `invalid_state` (the last owner) as `Rejected(InvalidState)`.
fn last_owner(f: crate::error::DbFailure, what: &'static str) -> StoreError {
    if f.thrown() == Some(crate::error::thrown::INVALID_STATE) {
        StoreError::Rejected(domain::error::DomainError::InvalidState(
            domain::error::InvalidStateDetails::default(),
        ))
    } else {
        f.into_store(what)
    }
}

impl SurrealStore {
    /// The cursor key of a keyset page: the record named by `cursor`, read with `statement`
    /// (which binds it as `$<name>`). `None` on the first page.
    pub(crate) async fn cursor_key(
        &self,
        statement: &'static str,
        name: &'static str,
        cursor: Option<&domain::id::Cursor>,
        id_of: impl Fn(&str) -> Option<RecordId> + Send,
    ) -> Result<Option<surrealdb::types::Value>, StoreError> {
        const WHAT: &str = "cursor";
        let Some(cursor) = cursor else {
            return Ok(None);
        };
        let id = id_of(cursor.as_str()).ok_or(StoreError::NotFound)?;
        let mut r = run_tx(&self.db, statement, |q| q.bind((name, id.clone())))
            .await
            .map_err(|f| f.into_store(WHAT))?;
        r.take(0).map_err(|e| take_failed(WHAT, e))
    }
}

/// An `invite` row.
#[derive(Debug, Clone, SurrealValue)]
pub struct InviteRow {
    /// `invite:<ulid>`.
    pub id: RecordId,
    /// The code.
    pub code: String,
    /// The role it grants.
    pub role: RoleRow,
    /// Its creator; `NONE` once that account is deleted (`ON DELETE UNSET`).
    pub created_by: Option<RecordId>,
    /// 7 days after creation.
    pub expires_at: DateTime<Utc>,
}

impl TryFrom<InviteRow> for Invite {
    type Error = RowError;

    fn try_from(row: InviteRow) -> Result<Self, RowError> {
        Ok(Self {
            id: from_rid(&row.id)?,
            code: row
                .code
                .parse()
                .map_err(|e: domain::id::IdParseError| RowError::BadField {
                    field: "code",
                    problem: e.to_string(),
                })?,
            role: row.role.into(),
            // contracts v0.9: optional, absent once the creator is deleted.
            created_by: from_opt_rid(row.created_by.as_ref())?,
            expires_at: ts(row.expires_at),
        })
    }
}

/// The result of [`sql::ACCEPT_INVITE`].
#[derive(Debug, Clone, SurrealValue)]
pub struct AcceptRow {
    /// The garden joined.
    pub garden: RecordId,
    /// False for an existing member.
    pub joined: bool,
}

#[async_trait::async_trait]
impl GardenRepo for SurrealStore {
    async fn create_garden(&self, owner: UserId, garden: NewGarden) -> Result<Garden, StoreError> {
        const WHAT: &str = "create garden";
        let location = &garden.location;
        let mut results = run_tx(&self.db, sql::CREATE_GARDEN, |q| {
            q.bind(("garden", rid(&garden.id)))
                .bind(("user", rid(&owner)))
                .bind(("name", garden.name.clone()))
                .bind(("lon", location.coordinates.lon))
                .bind(("lat", location.coordinates.lat))
                .bind(("precision", LocationPrecisionRow::from(location.precision)))
                .bind(("label", location.label.clone()))
                .bind(("time_zone", location.time_zone.0.clone()))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let row: Option<GardenRow> = results
            .take(sql::CREATE_GARDEN_RESULT)
            .map_err(|e| take_failed(WHAT, e))?;
        row.ok_or(StoreError::NotFound)?
            .try_into()
            .map_err(|e: RowError| e.into_store(WHAT))
    }

    async fn gardens(&self, user: UserId, page: PageRequest) -> Result<Page<Garden>, StoreError> {
        const WHAT: &str = "gardens";
        let after = self
            .cursor_key(sql::GARDEN_CURSOR, "garden", page.cursor.as_ref(), |c| {
                c.parse::<GardenId>().ok().map(|g| rid(&g))
            })
            .await?;
        let limit = page.limit();
        let mut r = run_tx(&self.db, sql::GARDENS, |q| {
            q.bind(("user", rid(&user)))
                .bind(("after", after.clone()))
                .bind(("limit", i64::from(limit) + 1))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<Garden> = take_all(&mut r, 0, WHAT)?;
        Ok(page_of(rows, limit, |g| g.id.to_string()))
    }

    async fn garden(&self, member: Membership<Viewer>) -> Result<Garden, StoreError> {
        const WHAT: &str = "read garden";
        let mut results = run_tx(&self.db, sql::GARDEN, |q| {
            q.bind(("garden", rid(&member.garden())))
                .bind(("user", rid(&member.user())))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let row: Option<GardenRow> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        row.ok_or(StoreError::NotFound)?
            .try_into()
            .map_err(|e: RowError| e.into_store(WHAT))
    }

    async fn update_garden(
        &self,
        owner: Membership<Owner>,
        patch: GardenPatch,
    ) -> Result<Garden, StoreError> {
        const WHAT: &str = "update garden";
        let location = patch.location.as_ref();
        run_tx(&self.db, sql::UPDATE_GARDEN, |q| {
            q.bind(("garden", rid(&owner.garden())))
                .bind(("name", patch.name.clone()))
                .bind(("lon", location.map(|l| l.coordinates.lon)))
                .bind(("lat", location.map(|l| l.coordinates.lat)))
                .bind((
                    "precision",
                    location.map(|l| LocationPrecisionRow::from(l.precision)),
                ))
                .bind(("label", location.map(|l| l.label.clone())))
                .bind(("time_zone", location.map(|l| l.time_zone.0.clone())))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        self.garden(owner.at_least()).await
    }

    async fn garden_context(&self, garden: GardenId) -> Result<GardenContext, StoreError> {
        self.load_garden_context(garden).await
    }

    async fn set_soil_profile(
        &self,
        garden: GardenId,
        status: SoilProfileStatus,
        profile: Option<SoilProfile>,
    ) -> Result<(), StoreError> {
        const WHAT: &str = "set soil profile";
        let profile = profile
            .as_ref()
            .map(|p| to_json("soil_profile", p))
            .transpose()
            .map_err(|e| e.into_store(WHAT))?;
        run_tx(&self.db, sql::SET_SOIL_PROFILE, |q| {
            q.bind(("garden", rid(&garden)))
                .bind(("status", SoilProfileStatusRow::from(status)))
                .bind(("profile", profile.clone()))
        })
        .await
        .map(drop)
        .map_err(|f| f.into_store(WHAT))
    }

    async fn soil_profile_failed(
        &self,
        garden: GardenId,
        retry_at: Timestamp,
    ) -> Result<(), StoreError> {
        const WHAT: &str = "soil profile failed";
        run_tx(&self.db, sql::SOIL_PROFILE_FAILED, |q| {
            q.bind(("garden", rid(&garden)))
                .bind(("retry_at", dt(retry_at)))
        })
        .await
        .map(drop)
        .map_err(|f| f.into_store(WHAT))
    }

    async fn gardens_due_soil_profile(
        &self,
        now: Timestamp,
        limit: u32,
    ) -> Result<Vec<(GardenId, Coordinates)>, StoreError> {
        const WHAT: &str = "gardens due a soil profile";
        let mut r = run_tx(&self.db, sql::DUE_SOIL_PROFILE, |q| {
            q.bind(("now", dt(now))).bind(("limit", i64::from(limit)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<PointRow> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        rows.into_iter()
            .map(|row| {
                Ok((
                    from_rid(&row.id)?,
                    Coordinates {
                        lat: row.lat,
                        lon: row.lon,
                    },
                ))
            })
            .collect::<Result<_, RowError>>()
            .map_err(|e| e.into_store(WHAT))
    }

    async fn gardens_without_soil_profile(
        &self,
        limit: u32,
    ) -> Result<Vec<(GardenId, Coordinates)>, StoreError> {
        const WHAT: &str = "gardens without soil profile";
        let mut r = run_tx(&self.db, sql::WITHOUT_SOIL_PROFILE, |q| {
            q.bind(("limit", i64::from(limit)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<PointRow> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        rows.into_iter()
            .map(|row| {
                Ok((
                    from_rid(&row.id)?,
                    Coordinates {
                        lat: row.lat,
                        lon: row.lon,
                    },
                ))
            })
            .collect::<Result<_, RowError>>()
            .map_err(|e| e.into_store(WHAT))
    }

    async fn members(
        &self,
        member: Membership<Viewer>,
        page: PageRequest,
    ) -> Result<Page<Member>, StoreError> {
        const WHAT: &str = "members";
        let garden = member.garden();
        let after = self
            .cursor_key(sql::MEMBER_CURSOR, "member", page.cursor.as_ref(), |c| {
                c.parse::<UserId>().ok().map(|u| member_rid(u, garden))
            })
            .await?;
        let limit = page.limit();
        let mut r = run_tx(&self.db, sql::MEMBERS, |q| {
            q.bind(("garden", rid(&garden)))
                .bind(("after", after.clone()))
                .bind(("limit", i64::from(limit) + 1))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<Member> = take_all(&mut r, 0, WHAT)?;
        Ok(page_of(rows, limit, |m| m.user_id.to_string()))
    }

    async fn member_ids(&self, garden: GardenId) -> Result<Vec<UserId>, StoreError> {
        const WHAT: &str = "member ids";
        let mut r = run_tx(&self.db, sql::MEMBER_IDS, |q| {
            q.bind(("garden", rid(&garden)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let ids: Vec<RecordId> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        ids.iter()
            .map(from_rid)
            .collect::<Result<_, _>>()
            .map_err(|e: RowError| e.into_store(WHAT))
    }

    async fn invites(
        &self,
        owner: Membership<Owner>,
        page: PageRequest,
        now: Timestamp,
    ) -> Result<Page<Invite>, StoreError> {
        const WHAT: &str = "invites";
        let after = page
            .cursor
            .as_ref()
            .map(|c| c.as_str().parse::<InviteId>().map(|i| rid(&i)))
            .transpose()
            .map_err(|_| StoreError::NotFound)?;
        let limit = page.limit();
        let mut r = run_tx(&self.db, sql::INVITES, |q| {
            q.bind(("garden", rid(&owner.garden())))
                .bind(("now", dt(now)))
                .bind(("after", after.clone()))
                .bind(("limit", i64::from(limit) + 1))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<InviteRow> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        let invites = rows
            .into_iter()
            .map(Invite::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.into_store(WHAT))?;
        Ok(page_of(invites, limit, |i| i.id.to_string()))
    }

    async fn create_invite(
        &self,
        owner: Membership<Owner>,
        invite: NewInvite,
    ) -> Result<Invite, StoreError> {
        const WHAT: &str = "create invite";
        run_tx(&self.db, sql::CREATE_INVITE, |q| {
            q.bind(("invite", rid(&invite.id)))
                .bind(("garden", rid(&owner.garden())))
                .bind(("code", invite.code.as_str()))
                .bind(("role", RoleRow::from(invite.role)))
                .bind(("user", rid(&owner.user())))
                .bind(("expires_at", dt(invite.expires_at)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        Ok(Invite {
            id: invite.id,
            code: invite.code,
            role: invite.role,
            created_by: Some(owner.user()),
            expires_at: invite.expires_at,
        })
    }

    async fn revoke_invite(
        &self,
        owner: Membership<Owner>,
        invite: InviteId,
        now: Timestamp,
    ) -> Result<(), StoreError> {
        const WHAT: &str = "revoke invite";
        let mut r = run_tx(&self.db, sql::REVOKE_INVITE, |q| {
            q.bind(("invite", rid(&invite)))
                .bind(("garden", rid(&owner.garden())))
                .bind(("now", dt(now)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<surrealdb::types::Value> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        if rows.is_empty() {
            Err(StoreError::NotFound)
        } else {
            Ok(())
        }
    }

    async fn accept_invite(
        &self,
        user: UserId,
        code: &InviteCode,
        now: Timestamp,
    ) -> Result<AcceptedInvite, StoreError> {
        const WHAT: &str = "accept invite";
        let mut results = run_tx(&self.db, sql::ACCEPT_INVITE, |q| {
            q.bind(("user", rid(&user)))
                .bind(("code", code.as_str()))
                .bind(("now", dt(now)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let row: Option<AcceptRow> = results
            .take(sql::ACCEPT_INVITE_RESULT)
            .map_err(|e| take_failed(WHAT, e))?;
        let row = row.ok_or(StoreError::NotFound)?;
        Ok(AcceptedInvite {
            garden: from_rid(&row.garden).map_err(|e| e.into_store(WHAT))?,
            joined: row.joined,
        })
    }

    async fn change_role(
        &self,
        owner: Membership<Owner>,
        member: UserId,
        role: Role,
    ) -> Result<RoleChange, StoreError> {
        const WHAT: &str = "change role";
        let mut r = run_tx(&self.db, sql::CHANGE_ROLE, |q| {
            q.bind(("garden", rid(&owner.garden())))
                .bind(("member", member_rid(member, owner.garden())))
                .bind(("user", rid(&member)))
                .bind(("role", RoleRow::from(role)))
        })
        .await
        .map_err(|f| last_owner(f, WHAT))?;
        // The member's row, then the demotion flag, then `COMMIT`.
        let row = r.num_statements().saturating_sub(3);
        let flag = r.num_statements().saturating_sub(2);
        let member = take_one(&mut r, row, WHAT)?.ok_or(StoreError::NotFound)?;
        let demoted_owner: Option<bool> = r.take(flag).map_err(|e| take_failed(WHAT, e))?;
        Ok(RoleChange {
            member,
            demoted_owner: demoted_owner.unwrap_or(false),
        })
    }

    async fn remove_member(
        &self,
        member: Membership<Viewer>,
        user: UserId,
    ) -> Result<RemovedMember, StoreError> {
        const WHAT: &str = "remove member";
        let garden = member.garden();
        let mut r = run_tx(&self.db, sql::REMOVE_MEMBER, |q| {
            q.bind(("garden", rid(&garden)))
                .bind(("member", member_rid(user, garden)))
                .bind(("user", rid(&user)))
        })
        .await
        .map_err(|f| last_owner(f, WHAT))?;
        let last = r.num_statements().saturating_sub(2);
        let ids: Vec<RecordId> = r.take(last).map_err(|e| take_failed(WHAT, e))?;
        let conversations: Vec<ConversationId> = ids
            .iter()
            .map(from_rid)
            .collect::<Result<_, _>>()
            .map_err(|e: RowError| e.into_store(WHAT))?;
        Ok(RemovedMember {
            garden,
            user,
            chat_prefixes: conversations
                .iter()
                .map(|c| ObjectPrefix::chat(garden, *c))
                .collect(),
            conversations,
        })
    }
}
