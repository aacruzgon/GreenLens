//! T-E3.7 (docs/EON), the store's side, on the embedded `SurrealDB` 3.3.4 engine: the
//! transactions that delete a garden, demote an owner, remove a member (or let one leave) and
//! hand a garden on at account deletion mark the connectors that owner created `deleting`, and
//! only those; `deleting_connectors` lists them with their sealed tokens for `api` to revoke;
//! and their records and rows then go, after which the user can be deleted. The revoke itself,
//! its order and its failures are `api`'s, tested in `bins/api/src/tests/connector_deletion.rs`.

#[path = "embedded/support.rs"]
#[expect(
    dead_code,
    reason = "the shared fixtures; these tests use only some of them"
)]
mod support;

use domain::{
    authz::{Membership, Owner, Viewer, check_membership_of},
    connectors::{ConnectorEndpoint, ConnectorKind, ConnectorStatus, FarmRecordKind},
    error::DomainError,
    garden::account::GardenTransfer,
    id::{AuthId, ConnectorId, FarmRecordId, GardenId, UserId},
    ports::{
        CascadeStep, DeletionRepo, DeletionTarget, Embedding, FarmRecordStore, FarmVector,
        GardenRepo, IdGenExt, IdentityRepo, NewConnector, NewFarmRecord, Sealed, StoreError,
    },
    wire::Role,
};
use store::SurrealStore;
use support::{Fixture, exec, fixture, rec, t0};

/// Adds a member with `role` to the fixture's garden.
async fn member(store: &SurrealStore, f: &Fixture, auth: &str, role: &str) -> UserId {
    let user = store
        .upsert_user(&auth.parse::<AuthId>().expect("auth id"), Some(auth.into()))
        .await
        .expect("user");
    exec(
        store,
        &format!(
            "LET $u = {}; LET $g = {};
INSERT RELATION INTO member_of {{ id: type::record('member_of', [$u, $g]), in: $u, out: $g, role: '{role}' }};",
            rec("user", user),
            rec("garden", f.garden)
        ),
    )
    .await;
    user
}

async fn owner(store: &SurrealStore, user: UserId, garden: GardenId) -> Membership<Owner> {
    check_membership_of(store, user, garden)
        .await
        .expect("an owner")
}

/// An `active` Slack connector `created_by` made, with a sealed token and `records` records.
async fn connector(
    store: &SurrealStore,
    f: &Fixture,
    created_by: UserId,
    records: u32,
) -> ConnectorId {
    let id: ConnectorId = f.ids.id();
    store
        .seed_connector(
            f.garden,
            NewConnector {
                id,
                kind: ConnectorKind::Slack,
                status: ConnectorStatus::Active,
                endpoint: ConnectorEndpoint::Vendor,
                created_by,
                account: None,
                secret: Some(Sealed(format!("sealed-{id}").into_bytes().into())),
                oauth: None,
                sources: Vec::new(),
                created_at: t0(),
            },
        )
        .await
        .expect("seeded");
    let records = (0..records)
        .map(|i| NewFarmRecord {
            id: FarmRecordId::new(id, &format!("C1/{i}.000100"), 0).expect("a record id"),
            kind: FarmRecordKind::Message,
            source: "C1".into(),
            title: "#field-reports".into(),
            author: None,
            link: "https://maplerow.slack.com/archives/C1".into(),
            text: format!("m{i}"),
            text_hash: format!("h{i}"),
            vector: Some(FarmVector {
                embedding: Embedding(vec![1.0, 0.0]),
                model: "test-embedder".into(),
            }),
            at: t0(),
        })
        .collect();
    store.upsert_records(id, records).await.expect("records");
    id
}

async fn status(store: &SurrealStore, c: ConnectorId) -> Option<ConnectorStatus> {
    store
        .connector_row(c)
        .await
        .expect("read")
        .map(|row| row.connector.status)
}

/// What `api` does after the mark: each listed connector's batches until one deletes nothing.
async fn finish(store: &SurrealStore, garden: GardenId) -> Vec<ConnectorId> {
    let mut done = Vec::new();
    for row in store
        .deleting_connectors(Some(garden))
        .await
        .expect("listed")
    {
        while store
            .delete_connector_batch(row.connector.id)
            .await
            .expect("a batch")
            > 0
        {}
        done.push(row.connector.id);
    }
    done
}

#[tokio::test]
async fn a_garden_deletion_marks_every_connector_and_its_cascade_deletes_them() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let bob = member(&store, &f, "user_bob", "owner").await;
    let mine = connector(&store, &f, f.user, 3).await;
    let bobs = connector(&store, &f, bob, 2).await;

    let started = store
        .start_garden_deletion(owner(&store, f.user, f.garden).await, t0())
        .await
        .unwrap();
    assert_eq!(status(&store, mine).await, Some(ConnectorStatus::Deleting));
    assert_eq!(status(&store, bobs).await, Some(ConnectorStatus::Deleting));
    let listed = store.deleting_connectors(Some(f.garden)).await.unwrap();
    assert_eq!(
        listed.iter().map(|r| r.connector.id).collect::<Vec<_>>(),
        [mine, bobs]
    );
    assert!(
        listed.iter().all(|r| r.secret.is_some()),
        "the tokens stay for the revoke"
    );
    assert_eq!(
        store.deleting_connectors(None).await.unwrap(),
        [],
        "a garden being deleted resumes through its own cascade"
    );

    // The runner's order: every step, the connectors finished just before theirs.
    let target = started.target;
    for &step in CascadeStep::steps_for(target) {
        if step == CascadeStep::FarmRecordsAndConnectors {
            assert_eq!(finish(&store, f.garden).await, [mine, bobs]);
        }
        while store.delete_batch(target, step).await.unwrap() > 0 {}
    }
    assert_eq!(status(&store, mine).await, None);
    assert_eq!(status(&store, bobs).await, None);
    assert_eq!(store.unfinished_deletions().await.unwrap(), []);
    let left = exec(&store, "SELECT VALUE id FROM farm_record;")
        .await
        .take::<Vec<surrealdb::types::RecordId>>(0)
        .unwrap();
    assert_eq!(left, [], "no record outlives its connector");
}

#[tokio::test]
async fn demoting_the_creating_owner_marks_only_their_connectors() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let bob = member(&store, &f, "user_bob", "owner").await;
    let mine = connector(&store, &f, f.user, 3).await;
    let bobs = connector(&store, &f, bob, 2).await;
    let by_bob = owner(&store, bob, f.garden).await;

    // Making an owner an owner again marks nothing.
    store
        .change_role(by_bob, f.user, Role::Owner)
        .await
        .unwrap();
    assert_eq!(store.deleting_connectors(None).await.unwrap(), []);

    let demoted = store
        .change_role(by_bob, f.user, Role::Editor)
        .await
        .unwrap();
    assert_eq!(demoted.role, Role::Editor);
    assert_eq!(status(&store, mine).await, Some(ConnectorStatus::Deleting));
    assert_eq!(
        status(&store, bobs).await,
        Some(ConnectorStatus::Active),
        "another owner's connector survives the demotion"
    );
    let resumable: Vec<_> = store
        .deleting_connectors(None)
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.connector.id, r.created_by))
        .collect();
    assert_eq!(resumable, [(mine, f.user)], "a restart finds it");

    assert_eq!(finish(&store, f.garden).await, [mine]);
    assert_eq!(status(&store, mine).await, None);
    assert_eq!(
        store.garden_record_count(f.garden).await.unwrap(),
        2,
        "bob's"
    );
    assert_eq!(status(&store, bobs).await, Some(ConnectorStatus::Active));

    // Demoting the last owner is refused, and marks nothing.
    assert!(matches!(
        store.change_role(by_bob, bob, Role::Viewer).await,
        Err(StoreError::Rejected(DomainError::InvalidState(_)))
    ));
    assert_eq!(status(&store, bobs).await, Some(ConnectorStatus::Active));
}

/// `user` (an owner with a connector) is removed by bob, or leaves.
async fn removal(leaving: bool) {
    let store = support::store().await;
    let f = fixture(&store).await;
    let bob = member(&store, &f, "user_bob", "owner").await;
    let mine = connector(&store, &f, f.user, 3).await;
    let bobs = connector(&store, &f, bob, 2).await;
    let by: Membership<Viewer> = if leaving {
        owner(&store, f.user, f.garden).await.at_least()
    } else {
        owner(&store, bob, f.garden).await.at_least()
    };

    let removed = store.remove_member(by, f.user).await.expect("removed");
    assert_eq!(removed.user, f.user);
    assert_eq!(status(&store, mine).await, Some(ConnectorStatus::Deleting));
    assert_eq!(status(&store, bobs).await, Some(ConnectorStatus::Active));
    let target = DeletionTarget::Member {
        garden: f.garden,
        user: f.user,
    };
    assert_eq!(finish(&store, f.garden).await, [mine]);
    for &step in CascadeStep::steps_for(target) {
        while store.delete_batch(target, step).await.expect("a batch") > 0 {}
    }
    assert_eq!(status(&store, mine).await, None);
    assert_eq!(
        store.garden_record_count(f.garden).await.expect("counted"),
        2
    );
}

#[tokio::test]
async fn removing_the_creating_owner_marks_their_connectors() {
    removal(false).await;
}

#[tokio::test]
async fn leaving_marks_the_leavers_connectors() {
    removal(true).await;
}

#[tokio::test]
async fn an_account_deletion_hands_the_garden_on_and_deletes_the_leavers_connectors() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let heir = member(&store, &f, "user_bob", "editor").await;
    let mine = connector(&store, &f, f.user, 3).await;

    let plan = store.plan_account_deletion(f.user).await.unwrap();
    assert_eq!(plan.leave, [f.garden]);
    for transfer in plan.transfer {
        assert_eq!(
            transfer,
            GardenTransfer {
                garden_id: f.garden,
                new_owner_user_id: heir
            }
        );
        store.transfer_ownership(transfer, f.user).await.unwrap();
    }
    assert_eq!(
        status(&store, mine).await,
        Some(ConnectorStatus::Deleting),
        "handing the garden on demotes the leaver"
    );
    // The user can't go while a connector still names them (`REFERENCE ON DELETE REJECT`).
    assert!(store.delete_user(f.user).await.is_err());

    let leaver: Membership<Viewer> = check_membership_of(&*store, f.user, f.garden)
        .await
        .unwrap();
    store.remove_member(leaver, f.user).await.unwrap();
    assert_eq!(finish(&store, f.garden).await, [mine]);
    store.delete_user_feedback(f.user).await.unwrap();
    store.delete_user(f.user).await.unwrap();
    assert_eq!(status(&store, mine).await, None);
    assert_eq!(store.garden_record_count(f.garden).await.unwrap(), 0);
}
