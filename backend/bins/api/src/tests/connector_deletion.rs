//! T-E3.7 (docs/EON), the routes' and the runner's side: deleting a garden, demoting the owner
//! who created a connector, removing them, their leaving and deleting their account each revoke
//! that owner's connector tokens once, after the store's marking transaction, and then delete
//! the records and the row; a failed revoke still deletes them; and a restart between the
//! commit and the revoke finishes on `resume`. The store's side (which connectors the
//! transactions mark, and that the records go) runs on the embedded engine in
//! `crates/store/tests/connector_deletion.rs`.
//!
//! Every store call and every revoke writes to one [`Log`], so the tests assert the order.

use std::{
    collections::HashMap,
    sync::{Mutex, PoisonError},
};

use domain::{
    connectors::{
        Connector, ConnectorAccount, ConnectorEndpoint, ConnectorKind, ConnectorSource,
        ConnectorStatus,
    },
    garden::{account::GardenTransfer, members::Member},
    id::ConnectorId,
    ports::{
        AccountDeletionPlan, AuthAdmin, ConnectorRow, DeletionStarted, DeletionTarget,
        RemovedMember, RoleChange,
    },
};
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;

use super::*;
use crate::{
    connect::{ConnectVendors, Exchanged, VendorError},
    sealing::{ConnectorKey, SealPurpose, StoredTokens},
};

/// The demoted, removed or leaving owner's connectors (Slack, then a Google grant).
const MINE_SLACK: &str = "01JAX3K9T6R2B8V0QZ4M7D1EA1";
const MINE_DRIVE: &str = "01JAX3K9T6R2B8V0QZ4M7D1EA2";
/// The demo garden's `era` connector: no revoke.
const ERA: &str = "01JAX3K9T6R2B8V0QZ4M7D1EA3";
/// Another owner's connector, being disconnected by them at the same time.
const THEIRS: &str = "01JAX3K9T6R2B8V0QZ4M7D1EA4";
/// The other owner, and a member an owner removes.
const OTHER_OWNER: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5X";
const MEMBER: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5V";

/// Store calls and revokes, in the order they happened.
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
}

/// How the recorded revoke answers.
#[derive(Clone, Copy)]
enum Revoke {
    Ok,
    Fail,
    Panic,
}

/// The `ConnectVendors` seam as the deletion runner uses it: records each revoke by the token
/// Google's grant or Slack's bot token gives (the refresh token first), and answers as told.
struct Recorder {
    log: Arc<Log>,
    revoke: Revoke,
}

#[async_trait::async_trait]
impl ConnectVendors for Recorder {
    async fn exchange(
        &self,
        kind: ConnectorKind,
        _: &str,
        _: Option<SecretString>,
        _: &str,
    ) -> Result<Exchanged, VendorError> {
        Err(VendorError::NotConfigured(kind))
    }

    async fn refresh(
        &self,
        _: ConnectorKind,
        _: &StoredTokens,
        _: bool,
    ) -> Result<Option<StoredTokens>, VendorError> {
        Ok(None)
    }

    async fn available_sources(
        &self,
        kind: ConnectorKind,
        _: &StoredTokens,
    ) -> Result<Vec<ConnectorSource>, VendorError> {
        Err(VendorError::NotConfigured(kind))
    }

    async fn revoke(&self, kind: ConnectorKind, tokens: &StoredTokens) -> Result<(), VendorError> {
        let token = tokens
            .refresh_token
            .as_ref()
            .unwrap_or(&tokens.access_token)
            .expose_secret();
        self.log.push(format!("revoke:{kind:?}:{token}"));
        match self.revoke {
            Revoke::Ok => Ok(()),
            Revoke::Fail => Err(VendorError::Unavailable {
                vendor: kind,
                retry_after_s: None,
                source: None,
            }),
            Revoke::Panic => panic!("the vendor call panicked"),
        }
    }
}

fn recorder(log: &Arc<Log>) -> Recorder {
    Recorder {
        log: Arc::clone(log),
        revoke: Revoke::Ok,
    }
}

fn key() -> ConnectorKey {
    ConnectorKey::from_bytes([7; 32])
}

/// A connector marked `deleting`, its token sealed under [`key`] for its own id.
fn row(
    id: &str,
    created_by: &str,
    kind: ConnectorKind,
    endpoint: ConnectorEndpoint,
    token: &str,
) -> ConnectorRow {
    let connector: ConnectorId = id.parse().unwrap();
    ConnectorRow {
        connector: Connector {
            id: connector,
            garden_id: GARDEN.parse().unwrap(),
            kind,
            status: ConnectorStatus::Deleting,
            endpoint,
            account: None,
            sources: Vec::new(),
            synced_at: None,
            record_count: 2,
            created_at: NOW.parse().unwrap(),
        },
        created_by: created_by.parse().unwrap(),
        secret: Some(key().seal(SealPurpose::Token, connector, token.as_bytes())),
        oauth: None,
        cursors: BTreeMap::new(),
        next_sync_at: NOW.parse().unwrap(),
        last_error: None,
        sync_requested_at: None,
    }
}

fn mine() -> Vec<ConnectorRow> {
    vec![
        row(
            MINE_SLACK,
            USER,
            ConnectorKind::Slack,
            ConnectorEndpoint::Vendor,
            "xoxb-mine",
        ),
        row(
            MINE_DRIVE,
            USER,
            ConnectorKind::GoogleDrive,
            ConnectorEndpoint::Vendor,
            r#"{"access_token":"ya29.mine","refresh_token":"1//mine"}"#,
        ),
    ]
}

/// `deleting_connectors` answers `rows` not yet deleted (logged as `list:<garden or resume>`),
/// and each connector's batches delete 2 records, then the row, then nothing.
fn connector_store(store: &mut FakeStore, log: &Arc<Log>, rows: Vec<ConnectorRow>) {
    let rows_for_user = rows.clone();
    let l = Arc::clone(log);
    // Batches made per connector; a connector whose last batch ran is gone from the listing.
    let calls: Arc<Mutex<HashMap<ConnectorId, usize>>> = Arc::default();
    let listed = Arc::clone(&calls);
    store
        .farm_record_store
        .expect_deleting_connectors()
        .returning(move |garden| {
            l.push(format!(
                "list:{}",
                garden.map_or("resume".to_owned(), |g| g.to_string())
            ));
            let calls = listed.lock().unwrap_or_else(PoisonError::into_inner);
            Ok(rows
                .iter()
                .filter(|r| calls.get(&r.connector.id).is_none_or(|&n| n < 3))
                .cloned()
                .collect())
        });
    // The account deletion lists by creator, in any garden (logged as `list:user:<id>`).
    let l = Arc::clone(log);
    let listed = Arc::clone(&calls);
    let by_user = rows_for_user.clone();
    store
        .farm_record_store
        .expect_deleting_connectors_of_user()
        .returning(move |user| {
            l.push(format!("list:user:{user}"));
            let calls = listed.lock().unwrap_or_else(PoisonError::into_inner);
            Ok(by_user
                .iter()
                .filter(|r| r.created_by == user)
                .filter(|r| calls.get(&r.connector.id).is_none_or(|&n| n < 3))
                .cloned()
                .collect())
        });
    let l = Arc::clone(log);
    store
        .farm_record_store
        .expect_delete_connector_batch()
        .returning(move |connector| {
            let mut calls = calls.lock().unwrap_or_else(PoisonError::into_inner);
            let n = calls.entry(connector).or_default();
            let deleted = [2, 1, 0][(*n).min(2)];
            *n += 1;
            l.push(format!("delete:{connector}:{deleted}"));
            Ok(deleted)
        });
    // No other connector shares these accounts.
    store
        .farm_record_store
        .expect_shared_account_connectors()
        .returning(|_, _, _| Ok(0));
    // The cascades' own steps find nothing left, and an account's nothing of theirs elsewhere.
    store
        .deletion_repo
        .expect_delete_batch()
        .returning(|_, _| Ok(0));
    store
        .deletion_repo
        .expect_account_leftovers()
        .returning(|_| Ok(Vec::new()));
    store
        .deletion_repo
        .expect_delete_user_rows()
        .returning(|_| Ok(0));
}

/// `state` with a runner that revokes through `revoker`, as `wiring.rs` gives it the routes'
/// `ConnectVendors`, and deletes the Clerk account through `admin`.
fn revoking_state(
    store: FakeStore,
    auth: MockAuthVerifier,
    revoker: Recorder,
    admin: MockAuthAdmin,
) -> Arc<AppState> {
    revoking_state_with_blob(store, auth, revoker, admin, MockBlobStore::new())
}

fn revoking_state_with_blob(
    store: FakeStore,
    auth: MockAuthVerifier,
    revoker: Recorder,
    admin: MockAuthAdmin,
    blob: MockBlobStore,
) -> Arc<AppState> {
    let mut app_state = state(store, auth, blob, no_images());
    let inner = Arc::get_mut(&mut app_state).expect("the test holds the only state");
    let admin: Arc<dyn AuthAdmin> = Arc::new(admin);
    inner.deletions = DeletionRunner::new(
        Arc::clone(&inner.ports.store),
        inner.deletes.clone(),
        admin,
        Pusher::new(
            Arc::clone(&inner.ports.store),
            Arc::clone(&inner.ports.notifier),
        ),
        Arc::clone(&inner.ports.clock),
        inner.tasks.clone(),
        inner.shutdown.clone(),
    )
    .with_revoke(key(), Arc::new(revoker) as Arc<dyn ConnectVendors>);
    app_state
}

/// Sends `request` and waits for every task it spawned.
async fn send(app_state: &Arc<AppState>, request: Request<Body>) -> StatusCode {
    let response = crate::router::app(Arc::clone(app_state))
        .oneshot(request)
        .await
        .unwrap();
    app_state.tasks.close();
    app_state.tasks.wait().await;
    response.status()
}

fn request(method: &str, uri: &str, body: Option<serde_json::Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, "Bearer session-token");
    match body {
        Some(body) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

fn position(events: &[String], event: &str) -> usize {
    events
        .iter()
        .position(|e| e == event)
        .unwrap_or_else(|| panic!("{event} missing from {events:?}"))
}

/// The connector's token was revoked exactly once, after `mark` committed, and its batches
/// all came after the revoke and ran until one deleted nothing.
fn revoked_once_then_deleted(events: &[String], mark: &str, connector: &str, revoke: &str) {
    assert_eq!(
        events.iter().filter(|e| *e == revoke).count(),
        1,
        "{revoke} once: {events:?}"
    );
    let revoked = position(events, revoke);
    assert!(
        position(events, mark) < revoked,
        "revoked after {mark}: {events:?}"
    );
    let batches: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, e)| e.starts_with(&format!("delete:{connector}:")))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        batches.len(),
        3,
        "records, the row, then nothing: {events:?}"
    );
    assert!(
        batches.iter().all(|&i| i > revoked),
        "deleted after the revoke: {events:?}"
    );
    assert_eq!(events[batches[2]], format!("delete:{connector}:0"));
}

fn revokes(events: &[String]) -> Vec<&str> {
    events
        .iter()
        .filter(|e| e.starts_with("revoke:"))
        .map(String::as_str)
        .collect()
}

#[tokio::test]
async fn deleting_a_garden_revokes_each_token_once_after_the_mark_then_deletes_the_connectors() {
    let log = Arc::new(Log::default());
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Owner));
    let l = Arc::clone(&log);
    store
        .deletion_repo
        .expect_start_garden_deletion()
        .times(1)
        .returning(move |owner, _| {
            l.push("mark:start_garden_deletion");
            Ok(DeletionStarted {
                target: DeletionTarget::Garden(owner.garden()),
                prefixes: Vec::new(),
                keys: Vec::new(),
                notify: Vec::new(),
                garden_name: Some("Back yard".to_owned()),
            })
        });
    let mut rows = mine();
    rows.push(row(
        ERA,
        USER,
        ConnectorKind::Slack,
        ConnectorEndpoint::Era,
        "xoxb-era-demo",
    ));
    connector_store(&mut store, &log, rows);
    let recorder = recorder(&log);
    let app_state = revoking_state(store, auth, recorder, MockAuthAdmin::new());

    let status = send(
        &app_state,
        request("DELETE", &format!("/gardens/{GARDEN}"), None),
    )
    .await;

    assert_eq!(status, StatusCode::ACCEPTED);
    let events = log.events();
    let mark = "mark:start_garden_deletion";
    revoked_once_then_deleted(&events, mark, MINE_SLACK, "revoke:Slack:xoxb-mine");
    revoked_once_then_deleted(&events, mark, MINE_DRIVE, "revoke:GoogleDrive:1//mine");
    assert_eq!(
        revokes(&events),
        ["revoke:Slack:xoxb-mine", "revoke:GoogleDrive:1//mine"],
        "the era connector's demo token isn't revoked"
    );
    assert!(
        events.contains(&format!("delete:{ERA}:0")),
        "but it is deleted"
    );
}

#[tokio::test]
async fn demoting_the_creating_owner_deletes_only_their_connectors() {
    let log = Arc::new(Log::default());
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Owner));
    let l = Arc::clone(&log);
    store
        .garden_repo
        .expect_change_role()
        .times(1)
        .returning(move |_, member, role| {
            l.push("mark:change_role");
            Ok(RoleChange {
                member: Member {
                    user_id: member,
                    display_name: "Ada".to_owned(),
                    role,
                    since: NOW.parse().unwrap(),
                },
                demoted_owner: true,
            })
        });
    // Another owner's connector is being deleted too (their own disconnect): not this task's.
    let mut rows = mine();
    rows.push(row(
        THEIRS,
        OTHER_OWNER,
        ConnectorKind::Slack,
        ConnectorEndpoint::Vendor,
        "xoxb-theirs",
    ));
    connector_store(&mut store, &log, rows);
    let recorder = recorder(&log);
    let app_state = revoking_state(store, auth, recorder, MockAuthAdmin::new());

    let status = send(
        &app_state,
        request(
            "PATCH",
            &format!("/gardens/{GARDEN}/members/{USER}"),
            Some(json!({ "role": "editor" })),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let events = log.events();
    revoked_once_then_deleted(
        &events,
        "mark:change_role",
        MINE_SLACK,
        "revoke:Slack:xoxb-mine",
    );
    revoked_once_then_deleted(
        &events,
        "mark:change_role",
        MINE_DRIVE,
        "revoke:GoogleDrive:1//mine",
    );
    assert!(
        !events
            .iter()
            .any(|e| e.contains(THEIRS) || e.contains("theirs")),
        "the other owner's connector is left alone: {events:?}"
    );
}

/// A role change that demotes no owner (making someone an owner, or an editor a viewer)
/// marks nothing, so the route starts nothing (B's finding 4).
#[tokio::test]
async fn a_role_change_that_demotes_no_owner_starts_nothing() {
    for role in ["owner", "viewer"] {
        let mut store = FakeStore::default();
        let auth = signed_in(&mut store, Some(Role::Owner));
        store
            .garden_repo
            .expect_change_role()
            .times(1)
            .returning(|_, member, role| {
                Ok(RoleChange {
                    member: Member {
                        user_id: member,
                        display_name: "Bo".to_owned(),
                        role,
                        since: NOW.parse().unwrap(),
                    },
                    demoted_owner: false,
                })
            });
        store.farm_record_store.expect_deleting_connectors().never();
        let log = Arc::new(Log::default());
        let app_state = revoking_state(store, auth, recorder(&log), MockAuthAdmin::new());
        let status = send(
            &app_state,
            request(
                "PATCH",
                &format!("/gardens/{GARDEN}/members/{MEMBER}"),
                Some(json!({ "role": role })),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(log.events(), [] as [String; 0], "{role}");
    }
}

/// `DELETE /gardens/{id}/members/{target}` by `USER`, an owner: a removal, or leaving when
/// `target` is `USER`. The connectors listed are the target's.
async fn remove(target: &str) -> Vec<String> {
    let log = Arc::new(Log::default());
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Owner));
    let l = Arc::clone(&log);
    store
        .garden_repo
        .expect_remove_member()
        .times(1)
        .returning(move |member, user| {
            l.push("mark:remove_member");
            Ok(RemovedMember {
                garden: member.garden(),
                user,
                conversations: Vec::new(),
                chat_prefixes: Vec::new(),
            })
        });
    let rows = mine()
        .into_iter()
        .map(|mut r| {
            r.created_by = target.parse().unwrap();
            r
        })
        .collect();
    connector_store(&mut store, &log, rows);
    let recorder = recorder(&log);
    let app_state = revoking_state(store, auth, recorder, MockAuthAdmin::new());
    let status = send(
        &app_state,
        request(
            "DELETE",
            &format!("/gardens/{GARDEN}/members/{target}"),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    log.events()
}

#[tokio::test]
async fn removing_the_creating_owner_deletes_their_connectors() {
    let events = remove(MEMBER).await;
    revoked_once_then_deleted(
        &events,
        "mark:remove_member",
        MINE_SLACK,
        "revoke:Slack:xoxb-mine",
    );
    revoked_once_then_deleted(
        &events,
        "mark:remove_member",
        MINE_DRIVE,
        "revoke:GoogleDrive:1//mine",
    );
}

#[tokio::test]
async fn leaving_deletes_the_leavers_connectors() {
    let events = remove(USER).await;
    revoked_once_then_deleted(
        &events,
        "mark:remove_member",
        MINE_SLACK,
        "revoke:Slack:xoxb-mine",
    );
    revoked_once_then_deleted(
        &events,
        "mark:remove_member",
        MINE_DRIVE,
        "revoke:GoogleDrive:1//mine",
    );
}

#[tokio::test]
async fn deleting_the_account_deletes_their_connectors_before_the_user() {
    let log = Arc::new(Log::default());
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Owner));
    let garden: GardenId = GARDEN.parse().unwrap();
    // The only owner hands the garden to an editor, then leaves it.
    store
        .deletion_repo
        .expect_plan_account_deletion()
        .returning(move |_| {
            Ok(AccountDeletionPlan {
                delete: Vec::new(),
                transfer: vec![GardenTransfer {
                    garden_id: garden,
                    new_owner_user_id: MEMBER.parse().unwrap(),
                }],
                leave: vec![garden],
            })
        });
    store
        .deletion_repo
        .expect_start_account_deletion()
        .times(1)
        .returning(|_, _, _| Ok(true));
    let l = Arc::clone(&log);
    store
        .deletion_repo
        .expect_transfer_ownership()
        .times(1)
        .returning(move |_, _| {
            l.push("mark:transfer_ownership");
            Ok(())
        });
    let l = Arc::clone(&log);
    store
        .garden_repo
        .expect_remove_member()
        .times(1)
        .returning(move |member, user| {
            l.push("mark:remove_member");
            Ok(RemovedMember {
                garden: member.garden(),
                user,
                conversations: Vec::new(),
                chat_prefixes: Vec::new(),
            })
        });
    connector_store(&mut store, &log, mine());
    store
        .deletion_repo
        .expect_delete_user_feedback()
        .returning(|_| Ok(0));
    let l = Arc::clone(&log);
    store
        .deletion_repo
        .expect_delete_user()
        .times(1)
        .returning(move |_| {
            l.push("delete_user");
            Ok(())
        });
    store
        .deletion_repo
        .expect_finish_account_deletion()
        .times(1)
        .returning(|_, _| Ok(()));
    let mut admin = MockAuthAdmin::new();
    admin.expect_delete_account().times(1).returning(|_| Ok(()));
    let recorder = recorder(&log);
    let app_state = revoking_state(store, auth, recorder, admin);

    let status = send(&app_state, request("DELETE", "/me", None)).await;

    assert_eq!(status, StatusCode::ACCEPTED);
    let events = log.events();
    let mark = "mark:transfer_ownership";
    revoked_once_then_deleted(&events, mark, MINE_SLACK, "revoke:Slack:xoxb-mine");
    revoked_once_then_deleted(&events, mark, MINE_DRIVE, "revoke:GoogleDrive:1//mine");
    let last_batch = position(&events, &format!("delete:{MINE_DRIVE}:0"));
    assert!(
        last_batch < position(&events, "delete_user"),
        "the user goes after the connectors that reference it: {events:?}"
    );
}

#[tokio::test]
async fn a_failed_revoke_still_deletes_the_records() {
    let log = Arc::new(Log::default());
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Owner));
    let l = Arc::clone(&log);
    store
        .garden_repo
        .expect_change_role()
        .times(1)
        .returning(move |_, member, role| {
            l.push("mark:change_role");
            Ok(RoleChange {
                member: Member {
                    user_id: member,
                    display_name: "Ada".to_owned(),
                    role,
                    since: NOW.parse().unwrap(),
                },
                demoted_owner: true,
            })
        });
    connector_store(&mut store, &log, mine());
    let recorder = Recorder {
        log: Arc::clone(&log),
        revoke: Revoke::Fail,
    };
    let app_state = revoking_state(store, auth, recorder, MockAuthAdmin::new());
    let status = send(
        &app_state,
        request(
            "PATCH",
            &format!("/gardens/{GARDEN}/members/{USER}"),
            Some(json!({ "role": "viewer" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let events = log.events();
    revoked_once_then_deleted(
        &events,
        "mark:change_role",
        MINE_SLACK,
        "revoke:Slack:xoxb-mine",
    );
    revoked_once_then_deleted(
        &events,
        "mark:change_role",
        MINE_DRIVE,
        "revoke:GoogleDrive:1//mine",
    );
}

#[tokio::test]
async fn a_restart_between_the_commit_and_the_revoke_finishes_on_resume() {
    // The demotion committed and the process stopped before the revoke: the connectors are
    // still `deleting`, with their tokens, and no task is left. `resume` finds them.
    let log = Arc::new(Log::default());
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Owner));
    store
        .deletion_repo
        .expect_unfinished_deletions()
        .returning(|| Ok(Vec::new()));
    store
        .deletion_repo
        .expect_unfinished_account_deletions()
        .returning(|| Ok(Vec::new()));
    log.push("mark:committed before the restart");
    connector_store(&mut store, &log, mine());
    let recorder = recorder(&log);
    let app_state = revoking_state(store, auth, recorder, MockAuthAdmin::new());

    assert_eq!(app_state.deletions.resume().await.unwrap(), 2);
    app_state.tasks.close();
    app_state.tasks.wait().await;

    let events = log.events();
    assert_eq!(events[1], "list:resume", "every garden not being deleted");
    let mark = "mark:committed before the restart";
    revoked_once_then_deleted(&events, mark, MINE_SLACK, "revoke:Slack:xoxb-mine");
    revoked_once_then_deleted(&events, mark, MINE_DRIVE, "revoke:GoogleDrive:1//mine");
}

/// Without `CONNECTOR_KEY` no token can be opened: a former owner's connectors stay
/// `deleting`, with their tokens, until the key is configured (B's finding 6).
#[tokio::test]
async fn with_connectors_off_a_former_owners_connectors_wait_for_the_key() {
    let log = Arc::new(Log::default());
    let mut store = FakeStore::default();
    store
        .deletion_repo
        .expect_unfinished_deletions()
        .returning(|| Ok(Vec::new()));
    store
        .deletion_repo
        .expect_unfinished_account_deletions()
        .returning(|| Ok(Vec::new()));
    connector_store(&mut store, &log, mine());
    store.farm_record_store.checkpoint();
    let l = Arc::clone(&log);
    store
        .farm_record_store
        .expect_deleting_connectors()
        .returning(move |_| {
            l.push("list:resume");
            Ok(mine())
        });
    store
        .farm_record_store
        .expect_delete_connector_batch()
        .never();
    let app_state = state(
        store,
        MockAuthVerifier::new(),
        MockBlobStore::new(),
        no_images(),
    );
    assert_eq!(app_state.deletions.resume().await.unwrap(), 0);
    app_state.tasks.close();
    app_state.tasks.wait().await;
    assert_eq!(log.events(), ["list:resume"]);
}

/// `state` with connectors off: a runner with no key to open tokens, deleting the Clerk
/// account through `admin`.
fn keyless_state(store: FakeStore, auth: MockAuthVerifier, admin: MockAuthAdmin) -> Arc<AppState> {
    let mut app_state = state(store, auth, MockBlobStore::new(), no_images());
    let inner = Arc::get_mut(&mut app_state).expect("the test holds the only state");
    let admin: Arc<dyn AuthAdmin> = Arc::new(admin);
    inner.deletions = DeletionRunner::new(
        Arc::clone(&inner.ports.store),
        inner.deletes.clone(),
        admin,
        Pusher::new(
            Arc::clone(&inner.ports.store),
            Arc::clone(&inner.ports.notifier),
        ),
        Arc::clone(&inner.ports.clock),
        inner.tasks.clone(),
        inner.shutdown.clone(),
    );
    app_state
}

/// Audit of PR 72, finding 4: an account deletion can't wait for the key either. The user
/// record can't go while a connector names it (`connector.created_by`, `ON DELETE REJECT`), so
/// the former owner's connectors and records are deleted unrevoked, and the account at Clerk
/// after them.
#[tokio::test]
async fn with_connectors_off_an_account_deletion_still_deletes_its_connectors() {
    let log = Arc::new(Log::default());
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Owner));
    let garden: GardenId = GARDEN.parse().unwrap();
    store
        .deletion_repo
        .expect_plan_account_deletion()
        .returning(move |_| {
            Ok(AccountDeletionPlan {
                delete: Vec::new(),
                transfer: vec![GardenTransfer {
                    garden_id: garden,
                    new_owner_user_id: MEMBER.parse().unwrap(),
                }],
                leave: vec![garden],
            })
        });
    store
        .deletion_repo
        .expect_start_account_deletion()
        .times(1)
        .returning(|_, _, _| Ok(true));
    store
        .deletion_repo
        .expect_transfer_ownership()
        .times(1)
        .returning(|_, _| Ok(()));
    store
        .garden_repo
        .expect_remove_member()
        .times(1)
        .returning(|member, user| {
            Ok(RemovedMember {
                garden: member.garden(),
                user,
                conversations: Vec::new(),
                chat_prefixes: Vec::new(),
            })
        });
    let mut rows = mine();
    rows.push(row(
        THEIRS,
        OTHER_OWNER,
        ConnectorKind::Slack,
        ConnectorEndpoint::Vendor,
        "xoxb-theirs",
    ));
    connector_store(&mut store, &log, rows);
    store
        .deletion_repo
        .expect_delete_user_feedback()
        .returning(|_| Ok(0));
    let l = Arc::clone(&log);
    store
        .deletion_repo
        .expect_delete_user()
        .times(1)
        .returning(move |_| {
            l.push("delete_user");
            Ok(())
        });
    let l = Arc::clone(&log);
    store
        .deletion_repo
        .expect_finish_account_deletion()
        .times(1)
        .returning(move |_, _| {
            l.push("clerk_deleted");
            Ok(())
        });
    let mut admin = MockAuthAdmin::new();
    admin.expect_delete_account().times(1).returning(|_| Ok(()));
    let app_state = keyless_state(store, auth, admin);

    let status = send(&app_state, request("DELETE", "/me", None)).await;

    assert_eq!(status, StatusCode::ACCEPTED);
    let events = log.events();
    assert_eq!(revokes(&events), [] as [&str; 0]);
    for mine in [MINE_SLACK, MINE_DRIVE] {
        assert!(
            position(&events, &format!("delete:{mine}:0")) < position(&events, "delete_user"),
            "{mine} goes before the user: {events:?}"
        );
    }
    assert!(
        !events
            .iter()
            .any(|e| e.starts_with(&format!("delete:{THEIRS}"))),
        "another owner's connector stays: {events:?}"
    );
    assert_eq!(events.last().map(String::as_str), Some("clerk_deleted"));
}

/// A garden's cascade can't wait for the key: the garden goes, so its connectors are deleted,
/// unrevoked.
#[tokio::test]
async fn with_connectors_off_a_garden_cascade_still_deletes_its_connectors() {
    let log = Arc::new(Log::default());
    let mut store = FakeStore::default();
    connector_store(&mut store, &log, mine());
    let app_state = state(
        store,
        MockAuthVerifier::new(),
        MockBlobStore::new(),
        no_images(),
    );
    app_state
        .deletions
        .delete_records(DeletionTarget::Garden(GARDEN.parse().unwrap()))
        .await
        .unwrap();
    let events = log.events();
    assert_eq!(revokes(&events), [] as [&str; 0]);
    assert!(events.contains(&format!("delete:{MINE_SLACK}:0")));
    assert!(events.contains(&format!("delete:{MINE_DRIVE}:0")));
}

/// A revoke that panics is one failed revoke: it runs in a task of its own, so the records
/// are still deleted and the cascade goes on (B's finding 3).
#[tokio::test]
async fn a_panicking_revoke_still_deletes_the_records() {
    let log = Arc::new(Log::default());
    let mut store = FakeStore::default();
    connector_store(&mut store, &log, mine());
    let recorder = Recorder {
        log: Arc::clone(&log),
        revoke: Revoke::Panic,
    };
    let app_state = revoking_state(
        store,
        MockAuthVerifier::new(),
        recorder,
        MockAuthAdmin::new(),
    );
    app_state
        .deletions
        .delete_records(DeletionTarget::Garden(GARDEN.parse().unwrap()))
        .await
        .unwrap();
    let events = log.events();
    assert_eq!(
        revokes(&events),
        ["revoke:Slack:xoxb-mine", "revoke:GoogleDrive:1//mine"]
    );
    assert!(events.contains(&format!("delete:{MINE_SLACK}:0")));
    assert!(events.contains(&format!("delete:{MINE_DRIVE}:0")));
}

/// Revoking Google's refresh token ends the whole grant of that account, and Slack's bot token
/// is one per workspace: neither is revoked while another live connector holds the same
/// account (A's finding 1, nit 7).
#[tokio::test]
async fn a_token_another_connector_holds_is_not_revoked() {
    let log = Arc::new(Log::default());
    let mut store = FakeStore::default();
    let mut rows = mine();
    rows[0].connector.account = Some(ConnectorAccount::Slack {
        team_id: "T0123".into(),
        team_name: "Maple Row Farm".into(),
        url: "https://maplerow.slack.com/".into(),
    });
    rows[1].connector.account = Some(ConnectorAccount::GoogleDrive {
        email: "farm@example.org".into(),
    });
    store
        .farm_record_store
        .expect_deleting_connectors()
        .returning(move |_| Ok(rows.clone()));
    store
        .farm_record_store
        .expect_delete_connector_batch()
        .returning(|_| Ok(0));
    store
        .deletion_repo
        .expect_delete_batch()
        .returning(|_, _| Ok(0));
    // The Slack workspace is held by no one else; the Google account by one other connector.
    store
        .farm_record_store
        .expect_shared_account_connectors()
        .withf(|kind, account, except| {
            *kind == ConnectorKind::Slack
                && account == "T0123"
                && *except == MINE_SLACK.parse::<ConnectorId>().unwrap()
        })
        .returning(|_, _, _| Ok(0));
    store
        .farm_record_store
        .expect_shared_account_connectors()
        .withf(|kind, account, except| {
            *kind == ConnectorKind::GoogleDrive
                && account == "farm@example.org"
                && *except == MINE_DRIVE.parse::<ConnectorId>().unwrap()
        })
        .returning(|_, _, _| Ok(1));
    let app_state = revoking_state(
        store,
        MockAuthVerifier::new(),
        recorder(&log),
        MockAuthAdmin::new(),
    );
    app_state
        .deletions
        .delete_records(DeletionTarget::Garden(GARDEN.parse().unwrap()))
        .await
        .unwrap();
    assert_eq!(revokes(&log.events()), ["revoke:Slack:xoxb-mine"]);
}

/// A removed member's connectors go before their chat objects, and a store error listing them
/// skips none of the chat steps: it is logged, and `resume` finishes the connectors (B's
/// finding 5).
#[tokio::test]
async fn a_removal_finishes_connectors_before_the_objects_and_survives_a_listing_error() {
    for listing_fails in [false, true] {
        let log = Arc::new(Log::default());
        let mut store = FakeStore::default();
        let auth = signed_in(&mut store, Some(Role::Owner));
        let l = Arc::clone(&log);
        store
            .garden_repo
            .expect_remove_member()
            .times(1)
            .returning(move |member, user| {
                l.push("mark:remove_member");
                Ok(RemovedMember {
                    garden: member.garden(),
                    user,
                    conversations: Vec::new(),
                    chat_prefixes: vec![domain::ports::ObjectPrefix::garden(member.garden())],
                })
            });
        let rows: Vec<ConnectorRow> = mine()
            .into_iter()
            .map(|mut r| {
                r.created_by = MEMBER.parse().unwrap();
                r
            })
            .collect();
        connector_store(&mut store, &log, rows);
        store.deletion_repo.checkpoint();
        let l = Arc::clone(&log);
        store
            .deletion_repo
            .expect_delete_batch()
            .returning(move |_, _| {
                l.push("records");
                Ok(0)
            });
        if listing_fails {
            store.farm_record_store.checkpoint();
            let l = Arc::clone(&log);
            store
                .farm_record_store
                .expect_deleting_connectors()
                .returning(move |_| {
                    l.push("list:failed");
                    Err(domain::ports::StoreError::Unavailable(
                        "connection reset".into(),
                    ))
                });
        }
        let mut blob = MockBlobStore::new();
        let l = Arc::clone(&log);
        blob.expect_delete_prefix().returning(move |_| {
            l.push("objects");
            Ok(domain::ports::DeleteReport {
                deleted: 1,
                failed: Vec::new(),
            })
        });
        let app_state =
            revoking_state_with_blob(store, auth, recorder(&log), MockAuthAdmin::new(), blob);
        let status = send(
            &app_state,
            request(
                "DELETE",
                &format!("/gardens/{GARDEN}/members/{MEMBER}"),
                None,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let events = log.events();
        let objects = position(&events, "objects");
        assert!(objects < position(&events, "records"), "{events:?}");
        if listing_fails {
            assert_eq!(revokes(&events), [] as [&str; 0]);
        } else {
            assert!(
                position(&events, &format!("delete:{MINE_DRIVE}:0")) < objects,
                "connectors before objects: {events:?}"
            );
        }
    }
}
