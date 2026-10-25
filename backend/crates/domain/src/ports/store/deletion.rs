//! Deletion cascades ([contracts 9], deletion cascades; [database 4], deletion; architecture 5,
//! deletion). Bottom-up, in batches of 500, never one large transaction: mark, cancel, delete
//! objects, then delete records children first, each step repeated until it deletes nothing.

use crate::{
    authz::{Editor, Membership, Owner},
    garden::account::GardenTransfer,
    id::{AuthId, ConversationId, GardenId, PlantId, ScanId, UserId},
    ports::{ObjectKey, ObjectPrefix, StoreError},
    wire::Timestamp,
};

/// What is being deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeletionTarget {
    /// A garden and everything in it.
    Garden(GardenId),
    /// A plant and everything under it.
    Plant(PlantId),
    /// A `pending` scan.
    PendingScan(ScanId),
    /// One member's data in one garden (removal, or account deletion step 3).
    Member {
        /// The garden.
        garden: GardenId,
        /// The member.
        user: UserId,
    },
}

/// The record steps, in the order [database 4] fixes: each holds a
/// `REFERENCE ON DELETE REJECT` to something a later step deletes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CascadeStep {
    /// Scan events (one range delete per scan) and the scans' jobs.
    EventsAndJobs,
    /// Tasks and plan trigger rules, including neighbour ones in both directions.
    TasksAndTriggers,
    /// Plans and diagnoses, after unsetting `scan.plan`, `scan.diagnosis` and
    /// `scan.open_question`; plans selected by equality, never `WHERE scan IN …`.
    PlansAndDiagnoses,
    /// Masks and guided checks.
    MasksAndGuidedChecks,
    /// Observations, questions and scan feedback.
    ObservationsQuestionsFeedback,
    /// Scan images, then scans.
    ImagesAndScans,
    /// Conversations with their pending actions, message feedback, messages and chat images.
    Conversations,
    /// Memories, then plants (a single plant first loses its pin), then the garden map.
    MemoriesPlantsMap,
    /// Farm records, then the farm connectors ([connectors 8], deletion): a record holds a
    /// reference to its connector, and both to the garden. A garden's only.
    ///
    /// [connectors 8]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#8-privacy-and-security
    FarmRecordsAndConnectors,
    /// Invites, memberships and the garden itself.
    Garden,
}

impl CascadeStep {
    /// Every step, in order.
    pub const ORDER: [Self; 10] = [
        Self::EventsAndJobs,
        Self::TasksAndTriggers,
        Self::PlansAndDiagnoses,
        Self::MasksAndGuidedChecks,
        Self::ObservationsQuestionsFeedback,
        Self::ImagesAndScans,
        Self::Conversations,
        Self::MemoriesPlantsMap,
        Self::FarmRecordsAndConnectors,
        Self::Garden,
    ];

    /// The steps a target goes through.
    pub fn steps_for(target: DeletionTarget) -> &'static [Self] {
        match target {
            DeletionTarget::Garden(_) => &Self::ORDER,
            DeletionTarget::Plant(_) => &Self::ORDER[..8],
            DeletionTarget::PendingScan(_) => &[
                Self::EventsAndJobs,
                Self::MasksAndGuidedChecks,
                Self::ObservationsQuestionsFeedback,
                Self::Conversations,
                Self::ImagesAndScans,
            ],
            DeletionTarget::Member { .. } => &[Self::Conversations, Self::MemoriesPlantsMap],
        }
    }
}

/// What starting a deletion did: `deleting_at` is set, jobs are cancelled and step claims and
/// question steps cleared in the same transaction, so running steps fail their commit check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletionStarted {
    /// The target.
    pub target: DeletionTarget,
    /// Object prefixes to delete before the records (a garden's, or each scan's).
    pub prefixes: Vec<ObjectPrefix>,
    /// Single objects to delete.
    pub keys: Vec<ObjectKey>,
    /// Members to tell with a `garden_deleted` push (garden only, not the deleter).
    pub notify: Vec<UserId>,
    /// The garden's name, for that push.
    pub garden_name: Option<String>,
}

/// What account deletion will do to each garden of the user (architecture 5, deletion steps
/// 1–3).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccountDeletionPlan {
    /// Gardens where the user is the only member, or the only owner with no editor to inherit.
    pub delete: Vec<GardenId>,
    /// Gardens handed to the longest-standing editor.
    pub transfer: Vec<GardenTransfer>,
    /// Shared gardens the user leaves (step 3).
    pub leave: Vec<GardenId>,
}

/// A garden that still holds a record of an account being deleted, found from the records
/// themselves (the user's conversations and unshared chat memories) and from the memberships in
/// gardens being deleted, never from the remaining memberships alone: a leave cut off after
/// `remove_member` committed has no membership left, and `conversation.user` refuses
/// `delete_user` while a conversation names the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountLeftover {
    /// The garden.
    pub garden: GardenId,
    /// The garden is being deleted: its own cascade deletes the user's records there.
    pub deleting: bool,
    /// The user's conversations in it, whose chat image prefixes go before their records.
    pub conversations: Vec<ConversationId>,
}

/// An account deletion's marker and tombstone, `deleted_user:[auth_id]` ([database 4],
/// deletion): written first, in the compare-and-set that starts the cascade, and kept until the
/// account is gone at the auth provider plus [`crate::rules::limits::TOMBSTONE_KEEP`]. While it
/// exists, `IdentityRepo::upsert_user` refuses the auth id, so a session token still valid after
/// the deletion can't create the user again; and an unfinished one resumes after a restart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountDeletionMarker {
    /// The user being deleted (kept as an id, not a record link: the record goes first).
    pub user: UserId,
    /// The auth provider's id, the tombstone's key.
    pub auth_id: AuthId,
    /// When the cascade started.
    pub started_at: Timestamp,
    /// The `user` record is gone (cascade steps 1–5 done); only the auth provider's deletion is
    /// left.
    pub user_deleted: bool,
}

/// The cascade's writes.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait DeletionRepo: Send + Sync {
    /// `DELETE /gardens/{id}`: starts the cascade; `202` follows.
    async fn start_garden_deletion(
        &self,
        owner: Membership<Owner>,
        now: Timestamp,
    ) -> Result<DeletionStarted, StoreError>;

    /// `DELETE /plants/{id}`: also removes its pin from the current map in place.
    async fn start_plant_deletion(
        &self,
        owner: Membership<Owner>,
        plant: PlantId,
        now: Timestamp,
    ) -> Result<DeletionStarted, StoreError>;

    /// `DELETE /scans/{id}` of a `pending` scan (any other status is
    /// `Rejected(InvalidState)` with `status`).
    async fn start_scan_deletion(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
        now: Timestamp,
    ) -> Result<DeletionStarted, StoreError>;

    /// One batch of one step: `DELETE (SELECT VALUE id FROM … LIMIT 500) RETURN VALUE
    /// $before.id` ([database 4], deletion). Returns how many records it deleted (the ids
    /// returned); the caller repeats the step until 0, then moves on. `RETURN NONE` can't be
    /// used: it returns `[]` for a full batch and an empty one alike.
    async fn delete_batch(
        &self,
        target: DeletionTarget,
        step: CascadeStep,
    ) -> Result<u32, StoreError>;

    /// Deletions started but not finished (after a restart), to resume.
    async fn unfinished_deletions(&self) -> Result<Vec<DeletionTarget>, StoreError>;

    /// Decides each garden's fate for `DELETE /me` and the `user.deleted` webhook.
    async fn plan_account_deletion(&self, user: UserId) -> Result<AccountDeletionPlan, StoreError>;

    /// Makes `to` an owner in place of `from` (step 2).
    async fn transfer_ownership(
        &self,
        transfer: GardenTransfer,
        from: UserId,
    ) -> Result<(), StoreError>;

    /// A member's conversations in a garden and their chat image prefixes, before the
    /// `Member` cascade.
    async fn member_conversations(
        &self,
        garden: GardenId,
        user: UserId,
    ) -> Result<Vec<(ConversationId, ObjectPrefix)>, StoreError>;

    /// Account deletion, before step 4: every garden still holding the user's conversations or
    /// unshared chat memories, and every garden being deleted that they are a member of
    /// ([`AccountLeftover`]). Empty once the account's gardens are dealt with.
    async fn account_leftovers(&self, user: UserId) -> Result<Vec<AccountLeftover>, StoreError>;

    /// Step 4: the user's scan feedback.
    async fn delete_user_feedback(&self, user: UserId) -> Result<u32, StoreError>;

    /// After step 5: one batch (500 of each) of the rows keyed by the user, which hold no
    /// reference and so outlive the `user` record: `usage_daily:[user, day]` and
    /// `idempotency_key:[user, …]`. Returns how many it deleted; the caller repeats until 0.
    async fn delete_user_rows(&self, user: UserId) -> Result<u32, StoreError>;

    /// Step 5: the `user` record (push tokens cascade; author links unset).
    async fn delete_user(&self, user: UserId) -> Result<(), StoreError>;

    /// Starts an account deletion (`DELETE /me` or `user.deleted`): creates the tombstone
    /// `deleted_user:[auth_id]` with `user` and `started_at`, as a compare-and-set on its
    /// absence. `true` when this call created it, so the caller runs the cascade; `false` when
    /// a cascade for this account already started, so the second request is a no-op.
    ///
    /// The default body always answers `true` (no marker, no protection against a second
    /// cascade); the store overrides it.
    async fn start_account_deletion(
        &self,
        user: UserId,
        auth_id: &AuthId,
        now: Timestamp,
    ) -> Result<bool, StoreError> {
        let _ = (user, auth_id, now);
        Ok(true)
    }

    /// Account deletions whose auth-provider deletion hasn't been confirmed
    /// (`clerk_deleted_at` unset), to resume after a restart.
    ///
    /// The default body finds none; the store overrides it.
    async fn unfinished_account_deletions(&self) -> Result<Vec<AccountDeletionMarker>, StoreError> {
        Ok(Vec::new())
    }

    /// The account is gone at the auth provider: sets the tombstone's `clerk_deleted_at`. The
    /// tombstone itself stays for [`crate::rules::limits::TOMBSTONE_KEEP`].
    ///
    /// The default body does nothing; the store overrides it.
    async fn finish_account_deletion(
        &self,
        auth_id: &AuthId,
        now: Timestamp,
    ) -> Result<(), StoreError> {
        let _ = (auth_id, now);
        Ok(())
    }

    /// Cleanup: tombstones whose `clerk_deleted_at` is before the cutoff, in batches of 500.
    ///
    /// The default body deletes nothing; the store overrides it.
    async fn delete_finished_tombstones(&self, before: Timestamp) -> Result<u32, StoreError> {
        let _ = before;
        Ok(0)
    }

    /// The orphan sweep: which of these keys no record points to (every key field counts:
    /// `storage_key`, `depth_key`, `depth_confidence_key`, masks and chat images).
    async fn unreferenced_keys(&self, keys: &[ObjectKey]) -> Result<Vec<ObjectKey>, StoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plants_stop_before_the_garden_step() {
        let p: PlantId = "01JAX3K9T6R2B8V0QZ4M7D1E5P".parse().unwrap();
        let steps = CascadeStep::steps_for(DeletionTarget::Plant(p));
        assert_eq!(steps.last(), Some(&CascadeStep::MemoriesPlantsMap));
        assert!(CascadeStep::ORDER.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn a_garden_deletes_its_farm_records_and_connectors_before_itself() {
        let g: GardenId = "01JAX3K9T6R2B8V0QZ4M7D1E5G".parse().unwrap();
        let steps = CascadeStep::steps_for(DeletionTarget::Garden(g));
        assert_eq!(
            &steps[steps.len() - 2..],
            &[CascadeStep::FarmRecordsAndConnectors, CascadeStep::Garden]
        );
        let p: PlantId = "01JAX3K9T6R2B8V0QZ4M7D1E5P".parse().unwrap();
        assert!(
            !CascadeStep::steps_for(DeletionTarget::Plant(p))
                .contains(&CascadeStep::FarmRecordsAndConnectors)
        );
    }
}
