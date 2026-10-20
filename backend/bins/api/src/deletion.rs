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
//! 2. delete the records bottom-up in batches of 500, in `CascadeStep` order (database 4);
//! 3. send `garden_deleted` pushes.
//!
//! **Resumable.** After a restart, `DeletionRepo::unfinished_deletions` lists every target still
//! marked `deleting_at`, and the runner deletes its records again from the first step (each step
//! is idempotent). A garden's objects are one prefix, deleted again; a plant's or scan's objects
//! are left to the daily orphan sweep, which deletes objects no record points to.
//!
//! **Farm connectors** ([connectors 8]; EON E3.6). A connector goes with its garden, with the
//! owner who created it when that owner stops being one: demoted (`change_role`), removed or
//! leaving (`remove_member`), or deleting the account (`transfer_ownership`, then the `Member`
//! cascade, or the garden's own cascade), and when its owner disconnects it
//! (`DELETE /connectors/{id}`). The store marks those connectors `deleting` in the transaction
//! that changes the garden, the membership or the connector, so the sync stops reading them at
//! once and the row keeps its sealed token. After that commit, never inside a transaction,
//! [`DeletionRunner::finish_connector`], the one revoke-then-delete path, takes each one: it
//! unseals the token (`sealing::StoredTokens`) and asks the vendor to revoke it through the
//! `ConnectVendors` seam, in a task of its own so a panic is one failed revoke; logs the outcome
//! without the token or the vendor's body; and then deletes the records and the row in batches
//! (`delete_connector_batch`), whether the revoke worked or not. No revoke for an `era`
//! connector (the simulator's demo token), for one without a token (still pending), or while
//! another live connector holds the same account's token (`shared_account_connectors`: one
//! Slack bot token per workspace, and Google's refresh token ends the whole grant). Revoking
//! before the delete, not after it, is what makes this resumable: a restart anywhere between
//! the mark and the last batch leaves the row `deleting` with its token, and `resume` (or the
//! garden's resumed cascade) finishes it. The worst case is a second revoke of a token already
//! revoked, which the vendors answer as revoked.
//!
//! **Without `CONNECTOR_KEY`** no token can be opened. A garden's cascade still deletes its
//! connectors, unrevoked, because the garden goes, and so does an account's: the user record
//! can't go while a connector names it (`connector.created_by`, `ON DELETE REJECT`), and the
//! account can't wait for a key, so each is deleted unrevoked with a warning by its id and
//! kind. A demoted, removed or leaving owner's connectors stay `deleting`, hidden from the
//! list and the sync, with a warning at each start, so their tokens are revoked once the key
//! is configured.
//!
//! **Accounts.** An account cascade starts with the tombstone's compare-and-set
//! (`DeletionRepo::start_account_deletion`): a second `DELETE /me`, or a `user.deleted` webhook
//! arriving while the first cascade runs, is a no-op. The tombstone also refuses the account's
//! still-valid session tokens (`401`), and `unfinished_account_deletions` resumes the records and
//! the Clerk deletion after a restart, until `finish_account_deletion` records Clerk's answer.
//!
//! [connectors 8]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#8-privacy-and-security

use std::{sync::Arc, time::Duration};

use blob::{DeleteJob, DeleteQueue};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use domain::{
    authz::{self, AccessError, Owner, Viewer},
    connectors::{ConnectorAccount, ConnectorEndpoint, ConnectorKind},
    garden::{account::AccountDeletion, push::PushPayload},
    id::{AuthId, ConnectorId, GardenId, UserId},
    ports::{
        AccountDeletionMarker, AuthAdmin, CascadeStep, Clock, ConnectorRow, DeletionStarted,
        DeletionTarget, ObjectKey, ObjectPrefix, Store, StoreError,
    },
    wire::V1,
};

use crate::{
    connect::ConnectVendors,
    error::ApiError,
    push::Pusher,
    sealing::{ConnectorKey, StoredTokens},
};

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
    /// Shutdown interrupted the Clerk retries; the tombstone resumes them.
    #[error("shut down before the Clerk account was deleted")]
    ShutDown,
}

/// Runs cascades on the tracker.
#[derive(Clone)]
pub struct DeletionRunner {
    store: Arc<dyn Store>,
    deletes: DeleteQueue,
    auth_admin: Arc<dyn AuthAdmin>,
    pusher: Pusher,
    clock: Arc<dyn Clock>,
    tasks: TaskTracker,
    shutdown: CancellationToken,
    /// The key that opens connector tokens and the vendors that revoke them; `None` with
    /// connectors off (no `CONNECTOR_KEY`), when no token can be opened to revoke.
    revoke: Option<Revoke>,
}

/// What revoking a connector's token needs.
#[derive(Clone)]
struct Revoke {
    key: ConnectorKey,
    /// `AppState::connect`: the same seam the connector routes call.
    vendors: Arc<dyn ConnectVendors>,
}

impl DeletionRunner {
    /// A runner over the ports and `AppState`'s delete queue.
    pub fn new(
        store: Arc<dyn Store>,
        deletes: DeleteQueue,
        auth_admin: Arc<dyn AuthAdmin>,
        pusher: Pusher,
        clock: Arc<dyn Clock>,
        tasks: TaskTracker,
        shutdown: CancellationToken,
    ) -> Self {
        Self {
            store,
            deletes,
            auth_admin,
            pusher,
            clock,
            tasks,
            shutdown,
            revoke: None,
        }
    }

    /// With farm connectors on: `key` opens the tokens the cascades revoke through `vendors`.
    #[must_use]
    pub fn with_revoke(mut self, key: ConnectorKey, vendors: Arc<dyn ConnectVendors>) -> Self {
        self.revoke = Some(Revoke { key, vendors });
        self
    }

    /// Runs a started cascade in the background. Not tied to the shutdown token: shutdown waits
    /// for it up to its drain deadline, and a cascade cut off there resumes on the next start.
    pub fn spawn(&self, started: DeletionStarted) {
        let runner = self.clone();
        self.tasks.spawn(async move {
            let target = started.target;
            if let Err(error) = runner.run(started).await {
                tracing::error!(?target, error = %error, "deletion failed; it resumes on restart");
            }
        });
    }

    /// A removed member's connectors, then objects, then records, then pushes. Records go only
    /// after every object is gone.
    pub async fn run(&self, started: DeletionStarted) -> Result<(), DeletionError> {
        if let DeletionTarget::Member { garden, user } = started.target {
            self.finish_member_connectors(garden, user).await;
        }
        if !self.delete_objects(started.prefixes, started.keys).await {
            return Err(DeletionError::Objects);
        }
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

    /// Every cascade step of the target, each repeated until a batch deletes nothing. A
    /// garden's connectors are revoked and deleted just before its `FarmRecordsAndConnectors`
    /// step, which then deletes anything left; a removed member's go before their objects
    /// ([`run`](Self::run)).
    pub async fn delete_records(&self, target: DeletionTarget) -> Result<(), StoreError> {
        for &step in CascadeStep::steps_for(target) {
            if step == CascadeStep::FarmRecordsAndConnectors
                && let DeletionTarget::Garden(garden) = target
            {
                for row in self.store.deleting_connectors(Some(garden)).await? {
                    self.finish_connector(row).await?;
                }
            }
            while self.store.delete_batch(target, step).await? > 0 {}
        }
        Ok(())
    }

    /// After a demotion (`PATCH /gardens/{id}/members/{user_id}`) of an owner committed:
    /// revokes and deletes, in the background, the connectors the demoted owner created in the
    /// garden, which the store marked `deleting` in that transaction.
    pub fn spawn_connectors_of(&self, garden: GardenId, owner: UserId) {
        let runner = self.clone();
        self.tasks.spawn(async move {
            runner.finish_member_connectors(garden, owner).await;
        });
    }

    /// After `DELETE /connectors/{id}`, or a `complete` from the wrong user or device,
    /// committed the `deleting` mark: [`finish_connector`](Self::finish_connector) in the
    /// background.
    pub fn spawn_connector(&self, row: ConnectorRow) {
        let runner = self.clone();
        self.tasks.spawn(async move {
            let connector = row.connector.id;
            if let Err(error) = runner.finish_connector(row).await {
                tracing::error!(%connector, error = %error, "connector deletion stopped; it resumes on restart");
            }
        });
    }

    /// The connectors `owner` created in `garden` and the store marked `deleting` when they
    /// stopped being its owner, each finished in turn. Never fails: a store error is logged
    /// and the rest of the cascade goes on, since `resume` finishes what is left. Without
    /// `CONNECTOR_KEY` they stay `deleting`, so their tokens can be revoked once it is set.
    async fn finish_member_connectors(&self, garden: GardenId, owner: UserId) {
        if self.revoke.is_none() {
            tracing::warn!(%garden, "connectors off: a former owner's connectors stay `deleting` until CONNECTOR_KEY is set");
            return;
        }
        let rows = match self.store.deleting_connectors(Some(garden)).await {
            Ok(rows) => rows,
            Err(error) => {
                tracing::error!(%garden, error = %error, "listing a former owner's connectors failed; they resume on restart");
                return;
            }
        };
        for row in rows.into_iter().filter(|r| r.created_by == owner) {
            let connector = row.connector.id;
            if let Err(error) = self.finish_connector(row).await {
                tracing::error!(%connector, error = %error, "connector deletion stopped; it resumes on restart");
            }
        }
    }

    /// Before an account's user record goes, which `connector.created_by` refuses while a
    /// connector names it: every connector the user created that is still marked `deleting`
    /// (this cascade's transfers and removals, or a run a restart cut off), revoked and
    /// deleted. Without `CONNECTOR_KEY` they are deleted unrevoked, as a garden's are, since the
    /// account deletion can't wait for a key; `revoke_stored` logs each by its id and kind.
    async fn finish_account_connectors(&self, user: UserId) -> Result<(), StoreError> {
        let rows = self.store.deleting_connectors(None).await?;
        for row in rows.into_iter().filter(|r| r.created_by == user) {
            self.finish_connector(row).await?;
        }
        Ok(())
    }

    /// The one revoke-then-delete path of a connector marked `deleting`: revokes its token
    /// (unless another live connector holds the account's), then deletes its records and the
    /// row, 500 records a batch. A failed revoke is logged and the deletion goes on.
    pub async fn finish_connector(&self, row: ConnectorRow) -> Result<(), StoreError> {
        self.revoke_stored(&row).await;
        let connector = row.connector.id;
        while self.store.delete_connector_batch(connector).await? > 0 {}
        tracing::info!(%connector, "connector deleted");
        Ok(())
    }

    /// Unseals a stored connector's tokens and revokes them. Never fails: every outcome is
    /// logged by the connector's id and kind only, never the token or the vendor's body.
    async fn revoke_stored(&self, row: &ConnectorRow) {
        let connector = row.connector.id;
        let kind = row.connector.kind;
        if row.connector.endpoint == ConnectorEndpoint::Era {
            tracing::debug!(%connector, ?kind, "era connector: its demo token needs no revoke");
            return;
        }
        let Some(sealed) = &row.secret else {
            tracing::debug!(%connector, ?kind, "no token to revoke");
            return;
        };
        let Some(revoke) = &self.revoke else {
            tracing::warn!(%connector, ?kind, "connectors off: the token can't be opened to revoke");
            return;
        };
        let tokens = match StoredTokens::unseal(&revoke.key, connector, sealed) {
            Ok(tokens) => tokens,
            Err(error) => {
                tracing::warn!(%connector, ?kind, error = %error, "token not revoked: it doesn't open");
                return;
            }
        };
        self.revoke_tokens(connector, kind, row.connector.account.as_ref(), tokens)
            .await;
    }

    /// Revokes `tokens` for `connector` unless another live connector holds the same account's
    /// token, in a task of its own: a panic in the vendor call is a failed revoke, never the
    /// end of the cascade that asked. Also the `complete` route's revoke of tokens it got but
    /// couldn't store.
    pub async fn revoke_tokens(
        &self,
        connector: ConnectorId,
        kind: ConnectorKind,
        account: Option<&ConnectorAccount>,
        tokens: StoredTokens,
    ) {
        let Some(revoke) = &self.revoke else {
            tracing::warn!(%connector, ?kind, "connectors off: no vendor to revoke with");
            return;
        };
        if self.account_held_elsewhere(connector, kind, account).await {
            return;
        }
        let vendors = Arc::clone(&revoke.vendors);
        let call = self
            .tasks
            .spawn(async move { vendors.revoke(kind, &tokens).await });
        match call.await {
            Ok(Ok(())) => tracing::info!(%connector, ?kind, "token revoked"),
            Ok(Err(error)) => tracing::warn!(
                %connector,
                ?kind,
                error = %error,
                "token revoke failed; the records are deleted anyway"
            ),
            Err(error) => tracing::error!(
                %connector,
                ?kind,
                error = %error,
                "token revoke task failed; the records are deleted anyway"
            ),
        }
    }

    /// `true` when the token must stay: another live `vendor` connector holds this Slack
    /// workspace's bot token or this Google account's grant, or the store couldn't say.
    async fn account_held_elsewhere(
        &self,
        connector: ConnectorId,
        kind: ConnectorKind,
        account: Option<&ConnectorAccount>,
    ) -> bool {
        let key = match account {
            Some(ConnectorAccount::Slack { team_id, .. }) => team_id.clone(),
            Some(ConnectorAccount::GoogleDrive { email }) => email.clone(),
            None => return false,
        };
        match self
            .store
            .shared_account_connectors(kind, key, connector)
            .await
        {
            Ok(0) => false,
            Ok(_) => {
                tracing::info!(%connector, ?kind, "another connector holds this account's token; not revoked");
                true
            }
            Err(error) => {
                tracing::warn!(%connector, ?kind, error = %error, "couldn't check the account's other connectors; not revoked");
                true
            }
        }
    }

    /// Resumes every cascade a restart cut off: targets still marked `deleting_at`, account
    /// deletions whose tombstone has no `clerk_deleted_at`, and connectors still marked
    /// `deleting` outside a garden being deleted (a demotion's or a removal's, cut off between
    /// the store's commit and the last batch).
    pub async fn resume(&self) -> Result<usize, StoreError> {
        let targets = self.store.unfinished_deletions().await?;
        let accounts = self.store.unfinished_account_deletions().await?;
        let mut connectors = self.store.deleting_connectors(None).await?;
        if self.revoke.is_none() && !connectors.is_empty() {
            // A former owner's connectors wait for the key, so their tokens can be revoked.
            tracing::warn!(
                count = connectors.len(),
                "connectors off: connectors marked `deleting` stay until CONNECTOR_KEY is set"
            );
            connectors.clear();
        }
        let count = targets.len() + accounts.len() + connectors.len();
        if !connectors.is_empty() {
            let runner = self.clone();
            self.tasks.spawn(async move {
                for row in connectors {
                    let connector = row.connector.id;
                    if let Err(error) = runner.finish_connector(row).await {
                        tracing::error!(%connector, error = %error, "connector deletion stopped; it resumes on restart");
                    }
                }
            });
        }
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
                self.run(started).await?;
            }
            for garden in plan.leave {
                let member =
                    authz::check_membership_of::<Viewer>(&*self.store, user, garden).await?;
                let removed = self.store.remove_member(member, user).await?;
                if self.revoke.is_some() {
                    self.finish_member_connectors(garden, user).await;
                }
                if !self.delete_objects(removed.chat_prefixes, Vec::new()).await {
                    return Err(DeletionError::Objects);
                }
                self.delete_records(DeletionTarget::Member { garden, user })
                    .await?;
            }
            self.store.delete_user_feedback(user).await?;
            self.finish_account_connectors(user).await?;
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
