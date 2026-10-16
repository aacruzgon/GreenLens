//! Users and push tokens ([contracts 9], account; [database 3], identity).

use chrono::{DateTime, Utc};
use domain::{
    garden::account::{Me, PushTokenRequest, UpdateMeRequest},
    id::{AuthId, PushToken, UserId},
    ports::{IdentityRepo, PushTarget, StoreError},
};
use surrealdb::types::{RecordId, SurrealValue};

use crate::{
    SurrealStore,
    error::{DbFailure, RowError, take_failed},
    ids::{deleted_user_rid, from_rid, rid},
    rows::ts,
    tx::run_tx,
};

/// The statements.
pub mod sql {
    /// The user of an auth id (unique index `user_auth`), and whether the auth id has an
    /// account-deletion tombstone. Statements: 0 the user, 1 the tombstone check.
    pub const USER_BY_AUTH: &str = "SELECT VALUE id FROM user WHERE auth_id = $auth_id LIMIT 1;
RETURN array::len(SELECT VALUE id FROM $tomb) > 0;";
    /// A first sight of an auth id, refused while a tombstone exists (a deleted account never
    /// comes back by signing in). The key is a ULID `SurrealDB` generates (the store has no
    /// `IdGen` for users); a racing webhook loses on the unique `user_auth` index and reads the
    /// winner. Statements: 0 `BEGIN`, 1 the tombstone `IF`, 2 `CREATE`, 3 `COMMIT`.
    pub const CREATE_USER: &str = "BEGIN;
IF array::len(SELECT VALUE id FROM $tomb) > 0 { THROW 'not_found' };
CREATE type::record('user', rand::ulid()) SET auth_id = $auth_id, display_name = $name ?? 'Gardener' RETURN VALUE id;
COMMIT;";
    /// `GET /me`.
    pub const ME: &str = "SELECT * FROM ONLY $user;";
    /// `PATCH /me`; withdrawing consent rounds every owned garden to `about_10_km` in the same
    /// transaction (contracts 9, account). Rounding the stored centroid to 0.1° is done in Rust
    /// before the update.
    pub const UPDATE_ME: &str = "BEGIN;
UPDATE $user SET display_name = $name ?? display_name, location_consent = $consent ?? location_consent;
IF $consent = false {
  FOR $g IN (SELECT VALUE out FROM member_of WHERE in = $user AND role = 'owner') {
    UPDATE $g SET centroid = type::point([math::fixed(centroid.coordinates[0], 1), math::fixed(centroid.coordinates[1], 1)]),
                  location_precision = 'about_10_km'
      WHERE location_precision IN ['exact', 'about_1_km'];
  };
};
SELECT * FROM ONLY $user;
COMMIT;";
    /// `POST /me/push-tokens`: an upsert on `push_token:[token]`; `created_at` keeps its
    /// first value (`READONLY`, set by its default).
    pub const REGISTER_PUSH_TOKEN: &str =
        "UPSERT $push_token SET user = $user, platform = $platform;";
    /// `DELETE /me/push-tokens/{token}`.
    pub const REMOVE_PUSH_TOKEN: &str = "DELETE $push_token WHERE user = $user;";
    /// The deliverable tokens of these users (iOS and Android only, contracts 9).
    pub const PUSH_TARGETS: &str =
        "SELECT record::id(id)[0] AS token, user, platform FROM push_token
  WHERE user IN $users AND platform IN ['ios', 'android'];";
    /// Tokens Expo reported as `DeviceNotRegistered`.
    pub const FORGET_PUSH_TOKENS: &str = "DELETE $tokens;";
}

/// A `user` row.
#[derive(Debug, Clone, SurrealValue)]
pub struct UserRow {
    /// `user:<ulid>`.
    pub id: RecordId,
    /// Shown to the garden's members.
    pub display_name: String,
    /// Whether exact locations may be stored.
    pub location_consent: bool,
    /// When the user was created.
    pub created_at: DateTime<Utc>,
}

/// One row of [`sql::PUSH_TARGETS`].
#[derive(Debug, Clone, SurrealValue)]
pub struct PushTargetRow {
    /// The Expo token (the key of `push_token:[token]`).
    pub token: String,
    /// Its user.
    pub user: RecordId,
    /// `ios` or `android`.
    pub platform: String,
}

impl TryFrom<UserRow> for Me {
    type Error = RowError;

    fn try_from(row: UserRow) -> Result<Self, RowError> {
        Ok(Self {
            id: from_rid(&row.id)?,
            display_name: row.display_name,
            location_consent: row.location_consent,
            created_at: ts(row.created_at),
        })
    }
}

impl SurrealStore {
    /// The user of an auth id and whether it is tombstoned.
    async fn find_user(&self, auth_id: &AuthId) -> Result<(Option<UserId>, bool), StoreError> {
        const WHAT: &str = "user by auth id";
        let mut results = run_tx(&self.db, sql::USER_BY_AUTH, |q| {
            q.bind(("auth_id", auth_id.as_str()))
                .bind(("tomb", deleted_user_rid(auth_id)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let ids: Vec<RecordId> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        let tombstoned: Option<bool> = results.take(1).map_err(|e| take_failed(WHAT, e))?;
        let user = ids
            .first()
            .map(from_rid::<UserId>)
            .transpose()
            .map_err(|e| e.into_store(WHAT))?;
        Ok((user, tombstoned.unwrap_or(false)))
    }
}

#[async_trait::async_trait]
impl IdentityRepo for SurrealStore {
    async fn upsert_user(
        &self,
        auth_id: &AuthId,
        display_name: Option<String>,
    ) -> Result<UserId, StoreError> {
        const WHAT: &str = "upsert user";
        // A deleted account (its tombstone exists) is `NotFound`, which `authenticate` maps to
        // `401`, on both paths.
        match self.find_user(auth_id).await? {
            (_, true) => return Err(StoreError::NotFound),
            (Some(user), false) => return Ok(user),
            (None, false) => {}
        }
        let created = run_tx(&self.db, sql::CREATE_USER, |q| {
            q.bind(("auth_id", auth_id.as_str()))
                .bind(("name", display_name.clone()))
                .bind(("tomb", deleted_user_rid(auth_id)))
        })
        .await;
        match created {
            Ok(mut results) => {
                let ids: Vec<RecordId> = results.take(2).map_err(|e| take_failed(WHAT, e))?;
                ids.first()
                    .ok_or(StoreError::NotFound)
                    .and_then(|id| from_rid(id).map_err(|e| e.into_store(WHAT)))
            }
            // The webhook and a first request raced: the other one created the user.
            Err(DbFailure::UniqueIndex { index, .. }) if index == "user_auth" => {
                self.find_user(auth_id).await?.0.ok_or(StoreError::NotFound)
            }
            Err(failure) => Err(failure.into_store(WHAT)),
        }
    }

    async fn user_by_auth(&self, auth_id: &AuthId) -> Result<Option<UserId>, StoreError> {
        Ok(self.find_user(auth_id).await?.0)
    }

    async fn me(&self, user: UserId) -> Result<Me, StoreError> {
        const WHAT: &str = "me";
        let mut r = run_tx(&self.db, sql::ME, |q| q.bind(("user", rid(&user))))
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let row: Option<UserRow> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        Me::try_from(row.ok_or(StoreError::NotFound)?).map_err(|e| e.into_store(WHAT))
    }

    async fn update_me(&self, user: UserId, patch: UpdateMeRequest) -> Result<Me, StoreError> {
        const WHAT: &str = "update me";
        let mut r = run_tx(&self.db, sql::UPDATE_ME, |q| {
            q.bind(("user", rid(&user)))
                .bind(("name", patch.display_name.clone()))
                .bind(("consent", patch.location_consent))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let last = r.num_statements().saturating_sub(2);
        let row: Option<UserRow> = r.take(last).map_err(|e| take_failed(WHAT, e))?;
        Me::try_from(row.ok_or(StoreError::NotFound)?).map_err(|e| e.into_store(WHAT))
    }

    async fn register_push_token(
        &self,
        user: UserId,
        request: PushTokenRequest,
    ) -> Result<(), StoreError> {
        const WHAT: &str = "register push token";
        let platform =
            crate::rows::to_json("platform", &request.platform).map_err(|e| e.into_store(WHAT))?;
        run_tx(&self.db, sql::REGISTER_PUSH_TOKEN, |q| {
            q.bind(("push_token", crate::ids::push_token_rid(&request.token)))
                .bind(("user", rid(&user)))
                .bind(("platform", platform.clone()))
        })
        .await
        .map(drop)
        .map_err(|f| f.into_store(WHAT))
    }

    async fn remove_push_token(&self, user: UserId, token: &PushToken) -> Result<(), StoreError> {
        const WHAT: &str = "remove push token";
        run_tx(&self.db, sql::REMOVE_PUSH_TOKEN, |q| {
            q.bind(("push_token", crate::ids::push_token_rid(token)))
                .bind(("user", rid(&user)))
        })
        .await
        .map(drop)
        .map_err(|f| f.into_store(WHAT))
    }

    async fn push_targets(&self, users: &[UserId]) -> Result<Vec<PushTarget>, StoreError> {
        const WHAT: &str = "push targets";
        let users: Vec<RecordId> = users.iter().map(rid).collect();
        let mut r = run_tx(&self.db, sql::PUSH_TARGETS, |q| {
            q.bind(("users", users.clone()))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<PushTargetRow> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        rows.into_iter()
            .map(|row| {
                Ok(PushTarget {
                    user: from_rid(&row.user)?,
                    token: row.token.parse().map_err(|_| RowError::BadField {
                        field: "token",
                        problem: "not a push token".to_owned(),
                    })?,
                    platform: crate::rows::from_json(
                        "platform",
                        serde_json::Value::String(row.platform),
                    )?,
                })
            })
            .collect::<Result<_, RowError>>()
            .map_err(|e| e.into_store(WHAT))
    }

    async fn forget_push_tokens(&self, tokens: &[PushToken]) -> Result<(), StoreError> {
        const WHAT: &str = "forget push tokens";
        if tokens.is_empty() {
            return Ok(());
        }
        let ids: Vec<RecordId> = tokens.iter().map(crate::ids::push_token_rid).collect();
        run_tx(&self.db, sql::FORGET_PUSH_TOKENS, |q| {
            q.bind(("tokens", ids.clone()))
        })
        .await
        .map(drop)
        .map_err(|f| f.into_store(WHAT))
    }
}
