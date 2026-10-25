//! S6's account cascade, the runner's side: a resumed account run (the user already gone, or
//! the whole cascade), a leave cut off after the membership went finished from the records, a
//! resume that starts an account and its own garden being deleted at once, the retries through
//! a `RustFS` outage, and `resume` skipping what is already running. The store's side runs on
//! the embedded engine in `crates/store/tests/account_deletion.rs`.
//!
//! Every store, blob and Clerk call writes to one [`Log`], so the tests assert the order.

use std::sync::{
    Mutex, PoisonError,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use domain::{
    id::{AuthId, ConversationId},
    ports::{
        AccountDeletionMarker, AccountDeletionPlan, AccountLeftover, AuthAdmin, BlobError,
        CascadeStep, DeleteReport, DeletionStarted, DeletionTarget, ObjectPrefix, StoreError,
    },
};

use super::*;
use crate::deletion::RETRY_WAITS;

const CONVERSATION: &str = "01JAX3K9T6R2B8V0QZ4M7D1EC1";

#[derive(Default)]
struct Log(Mutex<Vec<String>>);

impl Log {
    fn push(&self, event: impl Into<String>) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(event.into());
    }

    fn events(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn position(&self, event: &str) -> usize {
        let events = self.events();
        events
            .iter()
            .position(|e| e == event)
            .unwrap_or_else(|| panic!("{event} missing from {events:?}"))
    }
}

fn user() -> UserId {
    USER.parse().unwrap()
}

fn garden() -> GardenId {
    GARDEN.parse().unwrap()
}

fn marker(user_deleted: bool) -> AccountDeletionMarker {
    AccountDeletionMarker {
        user: user(),
        auth_id: "user_2abc".parse::<AuthId>().unwrap(),
        started_at: NOW.parse().unwrap(),
        user_deleted,
    }
}

/// A runner over `store`, `blob` and `admin` with no connector key, as `wiring.rs` builds it.
fn runner(
    store: FakeStore,
    blob: MockBlobStore,
    admin: MockAuthAdmin,
) -> (DeletionRunner, TaskTracker) {
    let store: Arc<FakeStore> = Arc::new(store);
    let tasks = TaskTracker::new();
    let admin: Arc<dyn AuthAdmin> = Arc::new(admin);
    let runner = DeletionRunner::new(
        store.clone(),
        blob::DeleteQueue::new(Arc::new(blob)),
        admin,
        Pusher::new(store, Arc::new(MockNotifier::new())),
        Arc::new(FixedClock::at(NOW.parse().unwrap())),
        tasks.clone(),
        CancellationToken::new(),
    );
    (runner, tasks)
}

async fn finish(tasks: &TaskTracker) {
    tasks.close();
    tasks.wait().await;
}

/// `resume` finds only the account marker; no target or connector is unfinished.
fn only_account(store: &mut FakeStore, user_deleted: bool) {
    store
        .deletion_repo
        .expect_unfinished_deletions()
        .returning(|| Ok(Vec::new()));
    store
        .deletion_repo
        .expect_unfinished_account_deletions()
        .returning(move || Ok(vec![marker(user_deleted)]));
    store
        .farm_record_store
        .expect_deleting_connectors()
        .returning(|_| Ok(Vec::new()));
}

/// The end of every account run: the rows keyed by the user (3, then none), then Clerk once,
/// then the tombstone's `clerk_deleted_at`.
fn the_end(store: &mut FakeStore, admin: &mut MockAuthAdmin, log: &Arc<Log>) {
    let left = Arc::new(AtomicUsize::new(3));
    let l = Arc::clone(log);
    store
        .deletion_repo
        .expect_delete_user_rows()
        .returning(move |u| {
            assert_eq!(u, user());
            let n = left.swap(0, Ordering::SeqCst);
            l.push(format!("user_rows:{n}"));
            Ok(u32::try_from(n).unwrap())
        });
    let l = Arc::clone(log);
    admin.expect_delete_account().times(1).returning(move |id| {
        assert_eq!(id.as_str(), "user_2abc");
        l.push("clerk");
        Ok(())
    });
    let l = Arc::clone(log);
    store
        .deletion_repo
        .expect_finish_account_deletion()
        .times(1)
        .returning(move |_, _| {
            l.push("clerk_deleted");
            Ok(())
        });
}

/// The steps between the leftovers and the user record: no feedback, no connectors.
fn before_the_user(store: &mut FakeStore, log: &Arc<Log>) {
    store
        .deletion_repo
        .expect_delete_user_feedback()
        .returning(|_| Ok(0));
    store
        .farm_record_store
        .expect_deleting_connectors_of_user()
        .returning(|_| Ok(Vec::new()));
    let l = Arc::clone(log);
    store
        .deletion_repo
        .expect_delete_user()
        .times(1)
        .returning(move |_| {
            l.push("delete_user");
            Ok(())
        });
}

#[tokio::test]
async fn a_resumed_account_whose_user_is_gone_deletes_its_rows_then_the_clerk_account() {
    let log = Arc::new(Log::default());
    let mut store = FakeStore::default();
    let mut admin = MockAuthAdmin::new();
    only_account(&mut store, true);
    store.deletion_repo.expect_plan_account_deletion().never();
    store.deletion_repo.expect_account_leftovers().never();
    store.deletion_repo.expect_delete_user().never();
    the_end(&mut store, &mut admin, &log);
    let (runner, tasks) = runner(store, MockBlobStore::new(), admin);

    assert_eq!(runner.resume().await.unwrap(), 1);
    finish(&tasks).await;

    assert_eq!(
        log.events(),
        ["user_rows:3", "user_rows:0", "clerk", "clerk_deleted"]
    );
}

/// The `user_deleted: false` path after a restart that cut a leave off: `remove_member` had
/// committed, so the plan lists nothing, but the conversation is still there. The run finds it
/// from the records, deletes its chat objects and then its records, and only then the user.
#[tokio::test]
async fn a_resumed_account_finishes_a_leave_cut_off_after_the_membership_went() {
    let log = Arc::new(Log::default());
    let mut store = FakeStore::default();
    let mut admin = MockAuthAdmin::new();
    let mut blob = MockBlobStore::new();
    only_account(&mut store, false);
    store
        .deletion_repo
        .expect_plan_account_deletion()
        .times(1)
        .returning(|_| Ok(AccountDeletionPlan::default()));
    let conversation: ConversationId = CONVERSATION.parse().unwrap();
    store
        .deletion_repo
        .expect_account_leftovers()
        .times(1)
        .returning(move |_| {
            Ok(vec![AccountLeftover {
                garden: garden(),
                deleting: false,
                conversations: vec![conversation],
            }])
        });
    let l = Arc::clone(&log);
    blob.expect_delete_prefix()
        .times(1)
        .returning(move |prefix| {
            assert_eq!(*prefix, ObjectPrefix::chat(garden(), conversation));
            l.push("chat_objects");
            Ok(DeleteReport::default())
        });
    // The first batch of each step deletes a record, the next finds nothing.
    let seen: Arc<Mutex<Vec<CascadeStep>>> = Arc::default();
    let l = Arc::clone(&log);
    store
        .deletion_repo
        .expect_delete_batch()
        .returning(move |target, step| {
            assert_eq!(
                target,
                DeletionTarget::Member {
                    garden: garden(),
                    user: user()
                }
            );
            let mut seen = seen.lock().unwrap_or_else(PoisonError::into_inner);
            if seen.contains(&step) {
                return Ok(0);
            }
            seen.push(step);
            l.push(format!("batch:{step:?}"));
            Ok(1)
        });
    before_the_user(&mut store, &log);
    the_end(&mut store, &mut admin, &log);
    let (runner, tasks) = runner(store, blob, admin);

    assert_eq!(runner.resume().await.unwrap(), 1);
    finish(&tasks).await;

    assert_eq!(
        log.events(),
        [
            "chat_objects",
            "batch:Conversations",
            "batch:MemoriesPlantsMap",
            "delete_user",
            "user_rows:3",
            "user_rows:0",
            "clerk",
            "clerk_deleted"
        ]
    );
}

/// The race of S6 work item 3: `resume` starts the account and its own garden, which a
/// restart cut off mid-cascade, together. The account must not reach `delete_user` while the
/// garden's records (its conversations) are there: it waits for the garden's run, or runs it
/// itself. `delete_user` refuses until the garden is gone and may be called once only.
#[tokio::test]
async fn one_resume_finishes_an_account_and_its_garden_being_deleted() {
    let log = Arc::new(Log::default());
    let mut store = FakeStore::default();
    let mut admin = MockAuthAdmin::new();
    let mut blob = MockBlobStore::new();
    store
        .deletion_repo
        .expect_unfinished_deletions()
        .returning(|| Ok(vec![DeletionTarget::Garden(garden())]));
    store
        .deletion_repo
        .expect_unfinished_account_deletions()
        .returning(|| Ok(vec![marker(false)]));
    store
        .farm_record_store
        .expect_deleting_connectors()
        .returning(|_| Ok(Vec::new()));
    // The garden is being deleted, so the plan skips it; the leftovers name it.
    store
        .deletion_repo
        .expect_plan_account_deletion()
        .returning(|_| Ok(AccountDeletionPlan::default()));
    let garden_gone = Arc::new(AtomicBool::new(false));
    let gone = Arc::clone(&garden_gone);
    store
        .deletion_repo
        .expect_account_leftovers()
        .returning(move |_| {
            Ok(if gone.load(Ordering::SeqCst) {
                Vec::new()
            } else {
                vec![AccountLeftover {
                    garden: garden(),
                    deleting: true,
                    conversations: Vec::new(),
                }]
            })
        });
    let l = Arc::clone(&log);
    blob.expect_delete_prefix().returning(move |prefix| {
        assert_eq!(*prefix, ObjectPrefix::garden(garden()));
        l.push("garden_objects");
        Ok(DeleteReport::default())
    });
    let gone = Arc::clone(&garden_gone);
    let l = Arc::clone(&log);
    store
        .deletion_repo
        .expect_delete_batch()
        .returning(move |target, step| {
            assert_eq!(target, DeletionTarget::Garden(garden()));
            if step == CascadeStep::Garden && !gone.swap(true, Ordering::SeqCst) {
                l.push("garden_gone");
            }
            Ok(0)
        });
    store
        .deletion_repo
        .expect_delete_user_feedback()
        .returning(|_| Ok(0));
    store
        .farm_record_store
        .expect_deleting_connectors_of_user()
        .returning(|_| Ok(Vec::new()));
    let gone = Arc::clone(&garden_gone);
    let l = Arc::clone(&log);
    store
        .deletion_repo
        .expect_delete_user()
        .times(1)
        .returning(move |_| {
            if !gone.load(Ordering::SeqCst) {
                // `conversation.user` is `REFERENCE ON DELETE REJECT`.
                return Err(StoreError::Unavailable(
                    "a conversation names the user".into(),
                ));
            }
            l.push("delete_user");
            Ok(())
        });
    the_end(&mut store, &mut admin, &log);
    let (runner, tasks) = runner(store, blob, admin);

    assert_eq!(runner.resume().await.unwrap(), 2);
    finish(&tasks).await;

    // The other run of the garden, after the account's, finds nothing left but its objects'
    // prefix, which it lists again.
    assert!(log.position("garden_gone") < log.position("delete_user"));
    assert!(log.position("delete_user") < log.position("clerk_deleted"));
}

/// A `RustFS` outage of over a minute: the chat objects' delete fails through the delete
/// queue's own five attempts twice, and the account run is retried in the process until it
/// goes through, with no restart and no second `resume`. While it waits, a `resume` (the
/// scheduler's `deletions` task) skips it.
#[tokio::test(start_paused = true)]
async fn an_account_run_rides_out_a_rustfs_outage_in_the_process() {
    let log = Arc::new(Log::default());
    let mut store = FakeStore::default();
    let mut admin = MockAuthAdmin::new();
    let mut blob = MockBlobStore::new();
    only_account(&mut store, false);
    store
        .deletion_repo
        .expect_plan_account_deletion()
        .returning(|_| Ok(AccountDeletionPlan::default()));
    let conversation: ConversationId = CONVERSATION.parse().unwrap();
    let leftover = Arc::new(AtomicBool::new(true));
    let left = Arc::clone(&leftover);
    store
        .deletion_repo
        .expect_account_leftovers()
        .returning(move |_| {
            Ok(if left.load(Ordering::SeqCst) {
                vec![AccountLeftover {
                    garden: garden(),
                    deleting: false,
                    conversations: vec![conversation],
                }]
            } else {
                Vec::new()
            })
        });
    let calls = Arc::new(AtomicUsize::new(0));
    let c = Arc::clone(&calls);
    let started = tokio::time::Instant::now();
    blob.expect_delete_prefix().returning(move |_| {
        // Down for the first two queue jobs, five attempts each.
        if c.fetch_add(1, Ordering::SeqCst) < 10 {
            return Err(BlobError::Unavailable("connection refused".into()));
        }
        Ok(DeleteReport::default())
    });
    let left = Arc::clone(&leftover);
    let l = Arc::clone(&log);
    store
        .deletion_repo
        .expect_delete_batch()
        .returning(move |_, step| {
            l.push(format!("batch:{step:?}"));
            left.store(false, Ordering::SeqCst);
            Ok(0)
        });
    before_the_user(&mut store, &log);
    the_end(&mut store, &mut admin, &log);
    let (runner, tasks) = runner(store, blob, admin);

    assert_eq!(runner.resume().await.unwrap(), 1);
    // The run is in its first retry's wait (the queue's attempts take 15 s).
    tokio::time::sleep(Duration::from_secs(20)).await;
    assert!(
        !log.events().iter().any(|e| e.starts_with("batch:")),
        "no record goes while the objects are there: {:?}",
        log.events()
    );
    assert_eq!(
        runner.resume().await.unwrap(),
        0,
        "the running account is skipped"
    );
    finish(&tasks).await;

    assert_eq!(calls.load(Ordering::SeqCst), 11);
    assert!(started.elapsed() >= Duration::from_mins(1));
    assert!(started.elapsed() < RETRY_WAITS.iter().sum::<Duration>());
    assert!(log.position("batch:Conversations") < log.position("delete_user"));
    assert_eq!(
        log.events().last().map(String::as_str),
        Some("clerk_deleted")
    );
}

/// A garden's cascade spawned by its route is retried the same way.
#[tokio::test(start_paused = true)]
async fn a_garden_cascade_is_retried_after_its_objects_failed() {
    let mut store = FakeStore::default();
    let mut blob = MockBlobStore::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let c = Arc::clone(&calls);
    blob.expect_delete_prefix().returning(move |_| {
        if c.fetch_add(1, Ordering::SeqCst) < 5 {
            return Err(BlobError::Unavailable("connection refused".into()));
        }
        Ok(DeleteReport::default())
    });
    store
        .farm_record_store
        .expect_deleting_connectors()
        .returning(|_| Ok(Vec::new()));
    let batches = Arc::new(AtomicUsize::new(0));
    let b = Arc::clone(&batches);
    store
        .deletion_repo
        .expect_delete_batch()
        .returning(move |_, _| {
            b.fetch_add(1, Ordering::SeqCst);
            Ok(0)
        });
    let (runner, tasks) = runner(store, blob, MockAuthAdmin::new());

    runner.spawn(DeletionStarted {
        target: DeletionTarget::Garden(garden()),
        prefixes: vec![ObjectPrefix::garden(garden())],
        keys: Vec::new(),
        notify: Vec::new(),
        garden_name: None,
    });
    finish(&tasks).await;

    assert_eq!(calls.load(Ordering::SeqCst), 6);
    assert_eq!(
        batches.load(Ordering::SeqCst),
        CascadeStep::ORDER.len(),
        "the records went once, after the objects"
    );
}
