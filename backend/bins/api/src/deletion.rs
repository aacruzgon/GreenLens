//! The deletion runner ([architecture 5], deletion; [contracts 9], deletion cascades;
//! [database 4], deletion; [storage 4], deletes).
//!
//! The request marks the target as being deleted (`deleting_at`, through `DeletionRepo::start_*`,
//! which also cancels its jobs and clears its step claims) and answers `202`; this runner does
//! the rest in the background:
//!
//! 1. delete the objects through `AppState`'s one `blob::DeleteQueue`: detached, never cancelled
//!    (on `RustFS` 1.0.1 an interrupted `DeleteObjects` can leave objects that can never be listed
//!    or deleted again, storage 4), each attempt bounded only by the client's 120-second
//!    operation timeout, retried 5 times, drained at shutdown. When the objects can't all be
//!    deleted, the records stay marked and the cascade resumes on the next start ("a failure
//!    partway is retried from the start");
//! 2. for a garden or a plant, erase its scope in Agent Memory (`BeliefStore::forget`; a
//!    garden's takes its plants'), so no shared memory outlives its row there. A failure stops
//!    the cascade with the records still marked, and it resumes on the next start, the same
//!    rule as the objects. Account deletion needs nothing: shared memories stay with the garden
//!    and private ones were never sent;
//! 3. delete the records bottom-up in batches of 500, in `CascadeStep` order (database 4);
//! 4. send `garden_deleted` pushes.
//!
//! **Resumable.** After a restart, `DeletionRepo::unfinished_deletions` lists every target still
//! marked `deleting_at`, and the runner deletes its records again from the first step (each step
//! is idempotent). A garden's objects are one prefix, deleted again; a plant's or scan's objects
//! are left to the daily orphan sweep, which deletes objects no record points to.
//!
//! **Accounts.** An account cascade starts with the tombstone's compare-and-set
//! (`DeletionRepo::start_account_deletion`): a second `DELETE /me`, or a `user.deleted` webhook
//! arriving while the first cascade runs, is a no-op. The tombstone also refuses the account's
//! still-valid session tokens (`401`), and `unfinished_account_deletions` resumes the records and
//! the Clerk deletion after a restart, until `finish_account_deletion` records Clerk's answer.

use std::{sync::Arc, time::Duration};

use blob::{DeleteJob, DeleteQueue};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use domain::{
    authz::{self, AccessError, Owner, Viewer},
    garden::{account::AccountDeletion, push::PushPayload},
    id::{AuthId, GardenId, UserId},
    ports::{
        AccountDeletionMarker, AuthAdmin, BeliefStore, BeliefSubject, CascadeStep, Clock,
        DeletionStarted, DeletionTarget, ObjectKey, ObjectPrefix, ProviderError, Store, StoreError,
    },
    wire::V1,
};

use crate::{error::ApiError, push::Pusher};

/// The longest wait between retries of the Clerk account deletion.
const ADMIN_BACKOFF_MAX: Duration = Duration::from_secs(3600);

/// Why a background cascade stopped; it resumes on the next start.
#[derive(Debug, thiserror::Error)]
pub enum DeletionError {
    /// The database.
    #[error("store failed")]
    Store(#[from] StoreError),
    /// The membership read of an account cascade.
    #[error("access check failed")]
    Access(#[from] AccessError),
    /// Some objects are still there after every attempt: the records stay so a later run
    /// deletes the objects first.
    #[error("objects not deleted; the records stay for a resumed run")]
    Objects,
    /// Agent Memory didn't erase the target's scope: the records stay so a later run erases it
    /// first.
    #[error("agent memory not erased; the records stay for a resumed run")]
    Beliefs(#[source] ProviderError),
    /// Shutdown interrupted the Clerk retries; the tombstone resumes them.
    #[error("shut down before the Clerk account was deleted")]
    ShutDown,
}

/// Runs cascades on the tracker.
#[derive(Clone)]
pub struct DeletionRunner {
    store: Arc<dyn Store>,
    deletes: DeleteQueue,
    beliefs: Arc<dyn BeliefStore>,
    auth_admin: Arc<dyn AuthAdmin>,
    pusher: Pusher,
    clock: Arc<dyn Clock>,
    tasks: TaskTracker,
    shutdown: CancellationToken,
}

impl DeletionRunner {
    /// A runner over the ports and `AppState`'s delete queue.
    #[expect(
        clippy::too_many_arguments,
        reason = "one parameter per port the cascade touches"
    )]
    pub fn new(
        store: Arc<dyn Store>,
        deletes: DeleteQueue,
        beliefs: Arc<dyn BeliefStore>,
        auth_admin: Arc<dyn AuthAdmin>,
        pusher: Pusher,
        clock: Arc<dyn Clock>,
        tasks: TaskTracker,
        shutdown: CancellationToken,
    ) -> Self {
        Self {
            store,
            deletes,
            beliefs,
            auth_admin,
            pusher,
            clock,
            tasks,
            shutdown,
        }
    }

    /// Runs a started cascade in the background. Not tied to the shutdown token: shutdown waits
    /// for it up to its drain deadline, and a cascade cut off there resumes on the next start.
    pub fn spawn(&self, started: DeletionStarted) {
        self.spawn_in(started, None);
    }

    /// [`Self::spawn`] for a plant: `DeletionTarget::Plant` doesn't name the garden, and the
    /// garden is part of the plant's scope path in Agent Memory.
    pub fn spawn_plant(&self, garden: GardenId, started: DeletionStarted) {
        self.spawn_in(started, Some(garden));
    }

    fn spawn_in(&self, started: DeletionStarted, garden: Option<GardenId>) {
        let runner = self.clone();
        self.tasks.spawn(async move {
            let target = started.target;
            if let Err(error) = runner.run(started, garden).await {
                tracing::error!(?target, error = %error, "deletion failed; it resumes on restart");
            }
        });
    }

    /// Objects, then the target's scope in Agent Memory, then records, then pushes. Records go
    /// only after every object is gone and the scope is erased. `garden` is a plant's garden,
    /// which its target doesn't carry.
    pub async fn run(
        &self,
        started: DeletionStarted,
        garden: Option<GardenId>,
    ) -> Result<(), DeletionError> {
        if !self.delete_objects(started.prefixes, started.keys).await {
            return Err(DeletionError::Objects);
        }
        self.forget_beliefs(started.target, garden).await?;
        self.delete_records(started.target).await?;
        if let Some(name) = started.garden_name
            && !started.notify.is_empty()
        {
            let payload = PushPayload::GardenDeleted {
                v: V1,
                garden_name: name.clone(),
            };
            self.pusher
                .send(
                    &started.notify,
                    "A garden was deleted",
                    &format!("{name} was deleted."),
                    payload,
                )
                .await;
        }
        Ok(())
    }

    /// Erases a garden's or a plant's scope in Agent Memory. Forgetting is idempotent, so a
    /// resumed run repeats it.
    async fn forget_beliefs(
        &self,
        target: DeletionTarget,
        garden: Option<GardenId>,
    ) -> Result<(), DeletionError> {
        let subject = match (target, garden) {
            (DeletionTarget::Garden(garden), _) => BeliefSubject {
                garden,
                plant: None,
            },
            (DeletionTarget::Plant(plant), Some(garden)) => BeliefSubject {
                garden,
                plant: Some(plant),
            },
            (DeletionTarget::Plant(plant), None) => {
                // A resumed plant cascade: the target doesn't name the garden, and `garden_of`
                // hides a plant being deleted. With no row left, an earlier run already passed
                // this step, since the records go only after it.
                if !self.beliefs.enabled() {
                    return Ok(());
                }
                match self.store.deleting_plant_garden(plant).await? {
                    Some(garden) => BeliefSubject {
                        garden,
                        plant: Some(plant),
                    },
                    None => return Ok(()),
                }
            }
            (DeletionTarget::PendingScan(_) | DeletionTarget::Member { .. }, _) => return Ok(()),
        };
        if !self.beliefs.enabled() {
            return Ok(());
        }
        self.beliefs
            .forget(subject)
            .await
            .map_err(DeletionError::Beliefs)
    }

    /// The object deletes, submitted to the delete queue and awaited. Dropping this future (the
    /// cascade's task) never stops a delete: the queue owns it. `true` when every object is gone.
    async fn delete_objects(&self, prefixes: Vec<ObjectPrefix>, keys: Vec<ObjectKey>) -> bool {
        let mut handles: Vec<_> = prefixes
            .into_iter()
            .map(|prefix| self.deletes.submit(DeleteJob::Prefix(prefix)))
            .collect();
        if !keys.is_empty() {
            handles.push(self.deletes.submit(DeleteJob::Keys(keys)));
        }
        let mut all_done = true;
        for handle in handles {
            all_done &= handle.outcome().await.is_some_and(|o| o.is_done());
        }
        all_done
    }

    /// Every cascade step of the target, each repeated until a batch deletes nothing.
    pub async fn delete_records(&self, target: DeletionTarget) -> Result<(), StoreError> {
        for &step in CascadeStep::steps_for(target) {
            while self.store.delete_batch(target, step).await? > 0 {}
        }
        Ok(())
    }

    /// Resumes every cascade a restart cut off: targets still marked `deleting_at`, and account
    /// deletions whose tombstone has no `clerk_deleted_at`.
    pub async fn resume(&self) -> Result<usize, StoreError> {
        let targets = self.store.unfinished_deletions().await?;
        let accounts = self.store.unfinished_account_deletions().await?;
        let count = targets.len() + accounts.len();
        for marker in accounts {
            self.spawn_account_run(marker);
        }
        for target in targets {
            let prefixes = match target {
                DeletionTarget::Garden(garden) => vec![ObjectPrefix::garden(garden)],
                _ => Vec::new(),
            };
            self.spawn(DeletionStarted {
                target,
                prefixes,
                keys: Vec::new(),
                notify: Vec::new(),
                garden_name: None,
            });
        }
        Ok(count)
    }

    /// The synchronous part of `DELETE /me` and `user.deleted`: what happens to each garden
    /// (architecture 5, deletion steps 1–3), for the `202` body.
    pub async fn plan_account(&self, user: UserId) -> Result<AccountDeletion, ApiError> {
        let plan = self.store.plan_account_deletion(user).await?;
        Ok(AccountDeletion {
            deleted_gardens: plan.delete,
            transferred_gardens: plan.transfer,
        })
    }

    /// `DELETE /me` and `user.deleted`: starts the account cascade unless one already runs for
    /// this account (the tombstone's compare-and-set). `false` when it was already running.
    pub async fn start_account(&self, user: UserId, auth_id: AuthId) -> Result<bool, StoreError> {
        let started_at = self.clock.now();
        if !self
            .store
            .start_account_deletion(user, &auth_id, started_at)
            .await?
        {
            tracing::info!(%user, "account deletion already running");
            return Ok(false);
        }
        self.spawn_account_run(AccountDeletionMarker {
            user,
            auth_id,
            started_at,
            user_deleted: false,
        });
        Ok(true)
    }

    fn spawn_account_run(&self, marker: AccountDeletionMarker) {
        let runner = self.clone();
        self.tasks.spawn(async move {
            let user = marker.user;
            if let Err(error) = runner.run_account(marker).await {
                tracing::error!(%user, error = %error, "account deletion stopped; it resumes on restart");
            }
        });
    }

    /// The account cascade, in the background: ownership transfers, the gardens the user alone
    /// used, the shared gardens they leave (their chats there), their scan feedback, the user
    /// record, and last the account at Clerk, retried until it succeeds or the process stops
    /// (the tombstone resumes it). Every step is idempotent, so a resumed run repeats them; a
    /// garden whose own cascade a restart cut off is resumed by `unfinished_deletions`.
    async fn run_account(&self, marker: AccountDeletionMarker) -> Result<(), DeletionError> {
        let user = marker.user;
        if !marker.user_deleted {
            let plan = self.store.plan_account_deletion(user).await?;
            for transfer in plan.transfer {
                self.store.transfer_ownership(transfer, user).await?;
            }
            let now = self.clock.now();
            for garden in plan.delete {
                let owner = authz::check_membership_of::<Owner>(&*self.store, user, garden).await?;
                let started = self.store.start_garden_deletion(owner, now).await?;
                self.run(started, None).await?;
            }
            for garden in plan.leave {
                let member =
                    authz::check_membership_of::<Viewer>(&*self.store, user, garden).await?;
                let removed = self.store.remove_member(member, user).await?;
                if !self.delete_objects(removed.chat_prefixes, Vec::new()).await {
                    return Err(DeletionError::Objects);
                }
                self.delete_records(DeletionTarget::Member { garden, user })
                    .await?;
            }
            self.store.delete_user_feedback(user).await?;
            self.store.delete_user(user).await?;
        }
        let mut wait = Duration::from_secs(1);
        while let Err(error) = self.auth_admin.delete_account(&marker.auth_id).await {
            tracing::warn!(%user, error = %error, ?wait, "Clerk account deletion failed; retrying");
            tokio::select! {
                () = tokio::time::sleep(wait) => {}
                () = self.shutdown.cancelled() => return Err(DeletionError::ShutDown),
            }
            wait = (wait * 2).min(ADMIN_BACKOFF_MAX);
        }
        self.store
            .finish_account_deletion(&marker.auth_id, self.clock.now())
            .await?;
        Ok(())
    }
}
