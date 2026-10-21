//! The reads behind `domain::authz` ([database 4], membership: "a direct read of
//! `member_of:[$user, $garden]`").

use domain::{
    id::{ConversationId, GardenId, PendingActionId, UserId},
    ports::{AccessRepo, ConversationOwnerRow, GardenResource, StoreError},
    wire::Role,
};
use surrealdb::types::{RecordId, SurrealValue};

use crate::{
    SurrealStore,
    error::take_failed,
    ids::{from_rid, member_rid, rid},
    rows::RoleRow,
    tx::run_tx,
};

/// The statements. Each lookup skips a record being deleted, its plant's and its garden's
/// deletion included, so a record being deleted reads as missing (`404`).
pub mod sql {
    /// The membership check: a direct read of `member_of:[$user, $garden]`.
    pub const ROLE_IN: &str = "SELECT VALUE role FROM $member WHERE out.deleting_at = NONE;";
    /// A plant's garden.
    pub const GARDEN_OF_PLANT: &str =
        "SELECT VALUE garden FROM $id WHERE deleting_at = NONE AND garden.deleting_at = NONE;";
    /// A scan's garden.
    pub const GARDEN_OF_SCAN: &str = "SELECT VALUE garden FROM $id WHERE deleting_at = NONE AND plant.deleting_at = NONE AND garden.deleting_at = NONE;";
    /// A plan's, task's or memory's garden (their plant, when they have one, must be live).
    pub const GARDEN_OF_PLANT_CHILD: &str = "SELECT VALUE garden FROM $id WHERE plant.deleting_at = NONE AND garden.deleting_at = NONE;";
    /// An invite's garden.
    pub const GARDEN_OF_INVITE: &str =
        "SELECT VALUE garden FROM $id WHERE garden.deleting_at = NONE;";
    /// A farm connector's garden; a connector being deleted reads as missing.
    pub const GARDEN_OF_CONNECTOR: &str =
        "SELECT VALUE garden FROM $id WHERE status != 'deleting' AND garden.deleting_at = NONE;";
    /// A conversation's owner and garden.
    pub const CONVERSATION_OWNER: &str =
        "SELECT user, garden FROM $id WHERE garden.deleting_at = NONE;";
    /// A pending action's conversation.
    pub const PENDING_ACTION_CONVERSATION: &str = "SELECT VALUE conversation FROM $id;";
}

/// `SELECT user, garden FROM conversation`.
#[derive(Debug, Clone, SurrealValue)]
pub struct ConversationOwnerDbRow {
    /// The owner.
    pub user: RecordId,
    /// The garden.
    pub garden: RecordId,
}

#[async_trait::async_trait]
impl AccessRepo for SurrealStore {
    async fn role_in(&self, user: UserId, garden: GardenId) -> Result<Option<Role>, StoreError> {
        const WHAT: &str = "membership check";
        let mut results = run_tx(&self.db, sql::ROLE_IN, |q| {
            q.bind(("member", member_rid(user, garden)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let roles: Vec<RoleRow> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        Ok(roles.into_iter().next().map(Role::from))
    }

    async fn garden_of(&self, resource: GardenResource) -> Result<Option<GardenId>, StoreError> {
        const WHAT: &str = "garden of a record";
        let (statement, id) = match resource {
            GardenResource::Plant(id) => (sql::GARDEN_OF_PLANT, rid(&id)),
            GardenResource::Scan(id) => (sql::GARDEN_OF_SCAN, rid(&id)),
            GardenResource::Plan(id) => (sql::GARDEN_OF_PLANT_CHILD, rid(&id)),
            GardenResource::Task(id) => (sql::GARDEN_OF_PLANT_CHILD, rid(&id)),
            GardenResource::Memory(id) => (sql::GARDEN_OF_PLANT_CHILD, rid(&id)),
            GardenResource::Invite(id) => (sql::GARDEN_OF_INVITE, rid(&id)),
            GardenResource::Connector(id) => (sql::GARDEN_OF_CONNECTOR, rid(&id)),
        };
        let mut results = run_tx(&self.db, statement, |q| q.bind(("id", id.clone())))
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let gardens: Vec<RecordId> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        gardens
            .first()
            .map(from_rid::<GardenId>)
            .transpose()
            .map_err(|e| e.into_store(WHAT))
    }

    async fn conversation_owner(
        &self,
        conversation: ConversationId,
    ) -> Result<Option<ConversationOwnerRow>, StoreError> {
        const WHAT: &str = "conversation owner";
        let mut results = run_tx(&self.db, sql::CONVERSATION_OWNER, |q| {
            q.bind(("id", rid(&conversation)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<ConversationOwnerDbRow> =
            results.take(0).map_err(|e| take_failed(WHAT, e))?;
        rows.into_iter()
            .next()
            .map(|row| {
                Ok(ConversationOwnerRow {
                    user: from_rid(&row.user)?,
                    garden: from_rid(&row.garden)?,
                })
            })
            .transpose()
            .map_err(|e: crate::error::RowError| e.into_store(WHAT))
    }

    async fn pending_action_conversation(
        &self,
        action: PendingActionId,
    ) -> Result<Option<ConversationId>, StoreError> {
        const WHAT: &str = "pending action's conversation";
        let mut results = run_tx(&self.db, sql::PENDING_ACTION_CONVERSATION, |q| {
            q.bind(("id", rid(&action)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let ids: Vec<RecordId> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        ids.first()
            .map(from_rid::<ConversationId>)
            .transpose()
            .map_err(|e| e.into_store(WHAT))
    }
}
