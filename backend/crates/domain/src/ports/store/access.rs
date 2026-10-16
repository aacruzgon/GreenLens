//! The lookups behind [`crate::authz`].

use crate::{
    id::{
        ConversationId, GardenId, InviteId, MemoryId, PendingActionId, PlanId, PlantId, ScanId,
        TaskId, UserId,
    },
    ports::StoreError,
    wire::Role,
};

/// A record whose garden decides access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GardenResource {
    /// A plant.
    Plant(PlantId),
    /// A scan.
    Scan(ScanId),
    /// A plan version.
    Plan(PlanId),
    /// A task.
    Task(TaskId),
    /// A memory.
    Memory(MemoryId),
    /// An invite.
    Invite(InviteId),
}

/// A conversation's owner and garden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConversationOwnerRow {
    /// The owner (`conversation.user`).
    pub user: UserId,
    /// The garden.
    pub garden: GardenId,
}

/// The reads every authorization check makes ([database 4], membership).
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait AccessRepo: Send + Sync {
    /// A direct read of `member_of:[user, garden]`; `None` for a non-member or a garden being
    /// deleted.
    async fn role_in(&self, user: UserId, garden: GardenId) -> Result<Option<Role>, StoreError>;

    /// The record's garden; `None` when it doesn't exist or is being deleted (its own
    /// `deleting_at`, its plant's or its garden's).
    async fn garden_of(&self, resource: GardenResource) -> Result<Option<GardenId>, StoreError>;

    /// The conversation's owner and garden; `None` when it doesn't exist.
    async fn conversation_owner(
        &self,
        conversation: ConversationId,
    ) -> Result<Option<ConversationOwnerRow>, StoreError>;

    /// The conversation a pending action belongs to.
    async fn pending_action_conversation(
        &self,
        action: PendingActionId,
    ) -> Result<Option<ConversationId>, StoreError>;
}
