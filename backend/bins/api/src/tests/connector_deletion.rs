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
    connectors::{Connector, ConnectorEndpoint, ConnectorKind, ConnectorStatus},
    garden::{account::GardenTransfer, members::Member},
    id::ConnectorId,
    ports::{
        AccountDeletionPlan, AuthAdmin, ConnectorRow, DeletionStarted, DeletionTarget,
        RemovedMember, VendorToken,
    },
};
use integrations::farm::FarmClientError;
use serde_json::json;

use super::*;
use crate::{
    revoke::TokenRevoker,
    sealing::{ConnectorKey, SealPurpose},
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

/// Records each revoke by the token it was given; fails every one when `fail`.
struct Recorder {
    log: Arc<Log>,
    fail: bool,
}

#[async_trait::async_trait]
impl TokenRevoker for Recorder {
    async fn revoke(
        &self,
        kind: ConnectorKind,
        token: &VendorToken,
    ) -> Result<(), FarmClientError> {
        self.log.push(format!("revoke:{kind:?}:{}", token.expose()));
        if self.fail {
            return Err(FarmClientError::Unavailable {
                vendor: "slack",
                source: None,
            });
        }
        Ok(())
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

/// `deleting_connectors` answers `rows` (logged as `list:<garden or resume>`), and each
/// connector's batches delete 2 records, then the row, then nothing.
fn connector_store(store: &mut FakeStore, log: &Arc<Log>, rows: Vec<ConnectorRow>) {
    let l = Arc::clone(log);
    store
        .farm_record_store
        .expect_deleting_connectors()
        .returning(move |garden| {
            l.push(format!(
                "list:{}",
                garden.map_or("resume".to_owned(), |g| g.to_string())
            ));
            Ok(rows.clone())
        });
    let l = Arc::clone(log);
    let calls: Mutex<HashMap<ConnectorId, usize>> = Mutex::default();
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
    // The cascades' own steps find nothing left.
    store
        .deletion_repo
        .expect_delete_batch()
        .returning(|_, _| Ok(0));
}

/// `state` with a runner that revokes through `revoker` and deletes the Clerk account through
/// `admin`.
fn revoking_state(
    store: FakeStore,
    auth: MockAuthVerifier,
    revoker: Recorder,
    admin: MockAuthAdmin,
) -> Arc<AppState> {
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
    )
    .with_revoke(key(), Arc::new(revoker));
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
    let recorder = Recorder {
        log: Arc::clone(&log),
        fail: false,
    };
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
            Ok(Member {
                user_id: member,
                display_name: "Ada".to_owned(),
                role,
                since: NOW.parse().unwrap(),
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
    let recorder = Recorder {
        log: Arc::clone(&log),
        fail: false,
    };
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

#[tokio::test]
async fn making_someone_an_owner_deletes_nothing() {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Owner));
    store
        .garden_repo
        .expect_change_role()
        .times(1)
        .returning(|_, member, role| {
            Ok(Member {
                user_id: member,
                display_name: "Bo".to_owned(),
                role,
                since: NOW.parse().unwrap(),
            })
        });
    store.farm_record_store.expect_deleting_connectors().never();
    let log = Arc::new(Log::default());
    let recorder = Recorder {
        log: Arc::clone(&log),
        fail: false,
    };
    let app_state = revoking_state(store, auth, recorder, MockAuthAdmin::new());
    let status = send(
        &app_state,
        request(
            "PATCH",
            &format!("/gardens/{GARDEN}/members/{MEMBER}"),
            Some(json!({ "role": "owner" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(log.events(), [] as [String; 0]);
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
    let recorder = Recorder {
        log: Arc::clone(&log),
        fail: false,
    };
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
    let recorder = Recorder {
        log: Arc::clone(&log),
        fail: false,
    };
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
            Ok(Member {
                user_id: member,
                display_name: "Ada".to_owned(),
                role,
                since: NOW.parse().unwrap(),
            })
        });
    connector_store(&mut store, &log, mine());
    let recorder = Recorder {
        log: Arc::clone(&log),
        fail: true,
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
    let recorder = Recorder {
        log: Arc::clone(&log),
        fail: false,
    };
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

#[tokio::test]
async fn with_connectors_off_the_records_are_still_deleted() {
    // No `CONNECTOR_KEY`: the token can't be opened, so nothing is revoked, but the connector
    // and its records still go.
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
    let app_state = state(
        store,
        MockAuthVerifier::new(),
        MockBlobStore::new(),
        no_images(),
    );
    assert_eq!(app_state.deletions.resume().await.unwrap(), 2);
    app_state.tasks.close();
    app_state.tasks.wait().await;
    let events = log.events();
    assert_eq!(revokes(&events), [] as [&str; 0]);
    assert!(events.contains(&format!("delete:{MINE_SLACK}:0")));
    assert!(events.contains(&format!("delete:{MINE_DRIVE}:0")));
}
