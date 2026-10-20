//! T-E1.6 (docs/EON): the farm connectors' repository on the embedded `SurrealDB` 3.3.4 engine,
//! one datastore per test. Time is driven through each method's `now` or cutoff argument,
//! never by sleeping.

#[path = "embedded/support.rs"]
#[expect(
    dead_code,
    reason = "the shared fixtures; these tests use only some of them"
)]
mod support;

use std::{collections::BTreeMap, time::Duration};

use domain::{
    authz::{Editor, Membership, Owner, Viewer, check_conversation, check_membership_of},
    chat::{Anchor, MessageRole},
    connectors::{
        ConnectorAccount, ConnectorEndpoint, ConnectorKind, ConnectorSource, ConnectorStatus,
        FarmCitation, FarmRecordKind, SyncErrorCode,
    },
    error::DomainError,
    garden::gardens::TimeZone,
    id::{ConnectorId, FarmRecordId, GardenId, PlanId},
    ports::{
        ChatRepo, CompletedConnect, Conflict, Embedding, FarmRecordFilter, FarmRecordStore,
        FarmVector, GardenRepo, IdGenExt, NewConnector, NewConversation, NewFarmRecord, NewGarden,
        NewMessage, PendingConnect, PlanRepo, PlanWrite, ResolvedLocation, Sealed, StoreError,
        SyncCursor, SyncOutcome, SyncRequest,
    },
    records::{
        plan::{ActionPlan, PlanReason, PlanStep, PlanWhen, StepCategory},
        weather::{Coordinates, LocationPrecision},
    },
    rules::limits::{FARM_RECORD_KEEP, PENDING_CONNECT_TTL, SYNC_NOW_INTERVAL},
    wire::Timestamp,
};
use store::{SurrealStore, apply_schema_with, ids::rid};
use support::{Fixture, exec, fixture, t0};

const DIM: usize = 4;

fn at(minutes: u64) -> Timestamp {
    t0().plus(Duration::from_secs(minutes * 60))
}

async fn owner_of(store: &SurrealStore, f: &Fixture) -> Membership<Owner> {
    check_membership_of(store, f.user, f.garden)
        .await
        .expect("the creator owns the garden")
}

fn new_connector(f: &Fixture, kind: ConnectorKind) -> NewConnector {
    NewConnector {
        id: f.ids.id(),
        kind,
        status: ConnectorStatus::Pending,
        endpoint: ConnectorEndpoint::Vendor,
        created_by: f.user,
        account: None,
        secret: None,
        oauth: Some(PendingConnect {
            nonce: "nonce-1".into(),
            device_hash: "h".repeat(43).parse().expect("a device hash"),
            verifier: Some(sealed(b"sealed-verifier")),
            expires_at: at(10),
        }),
        sources: Vec::new(),
        created_at: t0(),
    }
}

fn sealed(b: &[u8]) -> Sealed {
    Sealed(b.to_vec().into())
}

fn slack_account() -> ConnectorAccount {
    ConnectorAccount::Slack {
        team_id: "T0123".into(),
        team_name: "Maple Row Farm".into(),
        url: "https://maplerow.slack.com/".into(),
    }
}

fn source(id: &str, name: &str) -> ConnectorSource {
    ConnectorSource {
        id: id.into(),
        name: name.into(),
    }
}

/// An `active` Slack connector with channels `C1` and `C2`, seeded (no consent needed).
async fn active_slack(store: &SurrealStore, f: &Fixture, garden: GardenId) -> ConnectorId {
    let mut c = new_connector(f, ConnectorKind::Slack);
    c.status = ConnectorStatus::Active;
    c.oauth = None;
    c.account = Some(slack_account());
    c.secret = Some(sealed(b"sealed-token"));
    c.sources = vec![source("C1", "field-reports"), source("C2", "spray-log")];
    let id = c.id;
    store.seed_connector(garden, c).await.expect("seeded");
    id
}

fn one_hot(i: usize) -> Vec<f32> {
    let mut v = vec![0.0; DIM];
    v[i % DIM] = 1.0;
    v
}

fn record(
    connector: ConnectorId,
    vendor_id: &str,
    n: u32,
    text: &str,
    vector: Option<Vec<f32>>,
    when: Timestamp,
) -> NewFarmRecord {
    NewFarmRecord {
        id: FarmRecordId::new(connector, vendor_id, n).expect("a farm record id"),
        kind: if vendor_id.contains('/') {
            FarmRecordKind::Message
        } else {
            FarmRecordKind::Document
        },
        source: vendor_id
            .split_once('/')
            .map_or("folder-1", |(channel, _)| channel)
            .to_owned(),
        title: "#field-reports".into(),
        author: Some("Sam".into()),
        link: format!("https://maplerow.slack.com/archives/{vendor_id}"),
        text: text.into(),
        text_hash: format!("hash:{text}"),
        vector: vector.map(|v| FarmVector {
            embedding: Embedding(v),
            model: "test-embedder".into(),
        }),
        at: when,
    }
}

/// The stored vector of one record.
async fn stored_vector(store: &SurrealStore, id: &FarmRecordId) -> Vec<f64> {
    let mut r = store
        .db()
        .query("SELECT VALUE embedding FROM ONLY $id;")
        .bind(("id", store::ids::farm_record_rid(id)))
        .await
        .expect("query sent");
    assert!(r.take_errors().is_empty());
    r.take::<Vec<f64>>(0).expect("a vector")
}

/// Create, complete, list, read back, the 2-connector limit, and the due query.
#[tokio::test]
#[expect(clippy::too_many_lines, reason = "one connector's life, step by step")]
async fn connectors_and_the_due_query() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let owner = owner_of(&store, &f).await;
    let viewer: Membership<Viewer> = owner.at_least();

    // A pending connect stores its oauth state, never returned in `Connector`.
    let pending = new_connector(&f, ConnectorKind::Slack);
    let pending_id = pending.id;
    let created = store.create_connector(owner, pending).await.unwrap();
    assert_eq!(created.status, ConnectorStatus::Pending);
    assert_eq!(created.garden_id, f.garden);
    assert_eq!(created.record_count, 0);
    assert_eq!(created.created_at, t0());
    let row = store.connector_row(pending_id).await.unwrap().unwrap();
    assert_eq!(row.created_by, f.user);
    let oauth = row.oauth.expect("the pending connect's state");
    assert_eq!(oauth.nonce, "nonce-1");
    assert_eq!(oauth.verifier, Some(sealed(b"sealed-verifier")));
    assert_eq!(oauth.expires_at, at(10));
    assert!(row.secret.is_none() && row.cursors.is_empty());

    // The exchange: a wrong nonce loses, the right one activates, and a second use loses.
    let done = |nonce: &str, now| CompletedConnect {
        nonce: nonce.into(),
        account: slack_account(),
        secret: sealed(b"sealed-token"),
        now,
    };
    assert!(matches!(
        store
            .complete_connect(owner, pending_id, done("other", at(1)))
            .await,
        Err(StoreError::Conflict(Conflict::AlreadyExists))
    ));
    let active = store
        .complete_connect(owner, pending_id, done("nonce-1", at(1)))
        .await
        .unwrap();
    assert_eq!(active.status, ConnectorStatus::Active);
    assert_eq!(active.account, Some(slack_account()));
    assert!(matches!(
        store
            .complete_connect(owner, pending_id, done("nonce-1", at(2)))
            .await,
        Err(StoreError::Conflict(Conflict::AlreadyExists))
    ));
    let row = store.connector_row(pending_id).await.unwrap().unwrap();
    assert!(row.oauth.is_none(), "the exchange clears the state");
    assert_eq!(row.secret, Some(sealed(b"sealed-token")));
    assert_eq!(row.next_sync_at, at(1), "due at once");

    // An expired state loses too.
    let late = new_connector(&f, ConnectorKind::GoogleDrive);
    let late_id = late.id;
    store.create_connector(owner, late).await.unwrap();
    assert!(matches!(
        store
            .complete_connect(owner, late_id, done("nonce-1", at(10)))
            .await,
        Err(StoreError::Conflict(Conflict::AlreadyExists))
    ));

    // A garden holds at most 2 connectors besides those being deleted.
    assert!(matches!(
        store
            .create_connector(owner, new_connector(&f, ConnectorKind::Slack))
            .await,
        Err(StoreError::Rejected(DomainError::InvalidState(_)))
    ));

    // Seeding is idempotent by the fixed id and bypasses the limit.
    let seeded = active_slack(&store, &f, f.garden).await;
    let mut again = new_connector(&f, ConnectorKind::Slack);
    again.id = seeded;
    again.status = ConnectorStatus::Active;
    let first = store
        .connectors(viewer)
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.id == seeded)
        .unwrap();
    assert_eq!(store.seed_connector(f.garden, again).await.unwrap(), first);

    // Listed oldest first.
    let listed: Vec<ConnectorId> = store
        .connectors(viewer)
        .await
        .unwrap()
        .iter()
        .map(|c| c.id)
        .collect();
    let mut expected = vec![pending_id, late_id, seeded];
    expected.sort_unstable();
    assert_eq!(listed, expected);

    // The due query: `active` and past `next_sync_at` only. `late` is pending; `reauth`
    // and `deleting` are left out; `too_large` waits for new sources.
    let due = |now| {
        let store = store.clone();
        async move {
            let mut ids: Vec<ConnectorId> = store
                .due_connectors(now, 10)
                .await
                .unwrap()
                .iter()
                .map(|c| c.connector.id)
                .collect();
            ids.sort_unstable();
            ids
        }
    };
    let mut both = vec![pending_id, seeded];
    both.sort_unstable();
    assert_eq!(due(at(1)).await, both);
    assert_eq!(due(t0()).await, vec![seeded], "not yet due");
    assert_eq!(store.due_connectors(at(1), 1).await.unwrap().len(), 1);

    store
        .set_status(
            pending_id,
            ConnectorStatus::ReauthNeeded,
            Some(SyncErrorCode::AuthRefused),
        )
        .await
        .unwrap();
    assert_eq!(due(at(1)).await, vec![seeded]);
    let row = store.connector_row(pending_id).await.unwrap().unwrap();
    assert_eq!(row.connector.status, ConnectorStatus::ReauthNeeded);
    assert_eq!(row.last_error, Some(SyncErrorCode::AuthRefused));

    store
        .finish_sync(
            seeded,
            SyncOutcome {
                cursors: BTreeMap::from([
                    ("C1".to_owned(), SyncCursor("1729000000.000100".into())),
                    ("C9".to_owned(), SyncCursor("dropped meanwhile".into())),
                ]),
                due_at: t0(),
                synced_at: Some(at(2)),
                next_sync_at: at(17),
                last_error: Some(SyncErrorCode::TooLarge),
            },
        )
        .await
        .unwrap();
    let row = store.connector_row(seeded).await.unwrap().unwrap();
    assert_eq!(row.connector.synced_at, Some(at(2)));
    assert_eq!(row.next_sync_at, at(17));
    assert_eq!(
        row.cursors,
        BTreeMap::from([("C1".to_owned(), SyncCursor("1729000000.000100".into()))]),
        "only chosen sources keep a cursor"
    );
    assert!(
        due(at(20)).await.is_empty(),
        "too_large waits for new sources"
    );

    // New sources clear the wait, keep the kept sources' cursors and make it due now.
    let changed = store
        .set_sources(owner, seeded, vec![source("C2", "spray-log")], at(21))
        .await
        .unwrap();
    assert_eq!(changed.sources, vec![source("C2", "spray-log")]);
    let row = store.connector_row(seeded).await.unwrap().unwrap();
    assert!(row.cursors.is_empty(), "C1 was dropped");
    assert_eq!(row.last_error, None);
    assert_eq!(due(at(21)).await, vec![seeded]);

    // Deleting: left out of the list and the due query, and a status write can't revive it.
    let deleting = store.start_connector_deletion(owner, seeded).await.unwrap();
    assert_eq!(deleting.connector.status, ConnectorStatus::Deleting);
    assert_eq!(deleting.secret, Some(sealed(b"sealed-token")));
    store
        .set_status(seeded, ConnectorStatus::Active, None)
        .await
        .unwrap();
    assert_eq!(due(at(30)).await, Vec::<ConnectorId>::new());
    assert!(
        !store
            .connectors(viewer)
            .await
            .unwrap()
            .iter()
            .any(|c| c.id == seeded)
    );
    assert!(matches!(
        store.set_sources(owner, seeded, Vec::new(), at(31)).await,
        Err(StoreError::NotFound)
    ));
    assert_eq!(
        store
            .connectors_created_by(f.garden, f.user)
            .await
            .unwrap()
            .len(),
        3
    );
}

/// Sync now: one accepted request per connector in 5 minutes, held in the store.
#[tokio::test]
async fn sync_now_is_one_request_in_five_minutes() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let owner = owner_of(&store, &f).await;
    let c = active_slack(&store, &f, f.garden).await;

    assert_eq!(
        store.request_sync(owner, c, at(100)).await.unwrap(),
        SyncRequest::Accepted
    );
    let row = store.connector_row(c).await.unwrap().unwrap();
    assert_eq!(row.sync_requested_at, Some(at(100)));
    assert_eq!(row.next_sync_at, at(100), "due now");

    // Within 5 minutes: refused, with when the next is allowed; nothing changes.
    let just_before = at(105).minus(Duration::from_secs(1));
    assert_eq!(
        store.request_sync(owner, c, just_before).await.unwrap(),
        SyncRequest::TooSoon {
            allowed_at: at(100).plus(SYNC_NOW_INTERVAL)
        }
    );
    assert_eq!(
        store
            .connector_row(c)
            .await
            .unwrap()
            .unwrap()
            .sync_requested_at,
        Some(at(100))
    );

    // The limit is in the store: a second store over the same datastore (a restarted
    // process) sees it.
    let restarted = SurrealStore::from_client(store.db().clone()).await.unwrap();
    assert!(matches!(
        restarted.request_sync(owner, c, at(101)).await.unwrap(),
        SyncRequest::TooSoon { .. }
    ));

    // At 5 minutes: accepted again.
    assert_eq!(
        store.request_sync(owner, c, at(105)).await.unwrap(),
        SyncRequest::Accepted
    );

    // Another garden's owner can't reach it, and a pending connector doesn't sync.
    let pending = new_connector(&f, ConnectorKind::GoogleDrive);
    let pending_id = pending.id;
    store.create_connector(owner, pending).await.unwrap();
    assert!(matches!(
        store.request_sync(owner, pending_id, at(200)).await,
        Err(StoreError::Rejected(DomainError::InvalidState(_)))
    ));
    let other = other_garden(&store, &f).await;
    let other_owner: Membership<Owner> = check_membership_of(&*store, f.user, other).await.unwrap();
    assert!(matches!(
        store.request_sync(other_owner, c, at(300)).await,
        Err(StoreError::NotFound)
    ));
}

/// A second garden owned by the fixture's user.
async fn other_garden(store: &SurrealStore, f: &Fixture) -> GardenId {
    let garden: GardenId = f.ids.id();
    store
        .create_garden(
            f.user,
            NewGarden {
                id: garden,
                name: "Allotment".into(),
                location: ResolvedLocation {
                    coordinates: Coordinates {
                        lat: 45.42,
                        lon: -75.69,
                    },
                    precision: LocationPrecision::About1Km,
                    label: "Ottawa, ON".into(),
                    time_zone: TimeZone("America/Toronto".into()),
                },
            },
        )
        .await
        .expect("second garden");
    garden
}

/// Upserts by `(connector, vendor_id, n)`: an unchanged `text_hash` keeps the vector, a
/// changed one replaces it, and a vector never stays with other text.
#[tokio::test]
async fn upserts_keep_the_vector_while_the_text_is_unchanged() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let c = active_slack(&store, &f, f.garden).await;
    let ts = "C1/1729000000.000100";
    let first = record(c, ts, 0, "Rings on bed 3.", Some(one_hot(0)), t0());
    let id = first.id.clone();
    assert_eq!(store.upsert_records(c, vec![first]).await.unwrap(), 1);
    assert_eq!(stored_vector(&store, &id).await, vec![1.0, 0.0, 0.0, 0.0]);
    assert_eq!(
        store.record_hashes(c, vec![ts.into()]).await.unwrap(),
        vec![(id.clone(), "hash:Rings on bed 3.".to_owned())]
    );

    // Same text, no vector: the stored vector stays and the other fields follow.
    let mut same = record(c, ts, 0, "Rings on bed 3.", None, t0());
    same.author = Some("Samira".into());
    store.upsert_records(c, vec![same]).await.unwrap();
    assert_eq!(stored_vector(&store, &id).await, vec![1.0, 0.0, 0.0, 0.0]);
    let viewer = owner_of(&store, &f).await.at_least();
    let hit = &store
        .search_records(
            viewer,
            &Embedding(one_hot(0)),
            FarmRecordFilter::default(),
            5,
        )
        .await
        .unwrap()[0];
    assert_eq!(hit.hit.citation.author.as_deref(), Some("Samira"));

    // Changed text without a vector is refused, and nothing changes.
    let stale = record(c, ts, 0, "Rings on bed 4.", None, t0());
    assert!(matches!(
        store.upsert_records(c, vec![stale]).await,
        Err(StoreError::Query { .. })
    ));
    // A new record without a vector is refused too.
    let orphan = record(c, "C1/1729000001.000100", 0, "New.", None, t0());
    assert!(store.upsert_records(c, vec![orphan]).await.is_err());

    // Changed text with its vector replaces both.
    let changed = record(c, ts, 0, "Rings on bed 4.", Some(one_hot(1)), t0());
    store.upsert_records(c, vec![changed]).await.unwrap();
    assert_eq!(stored_vector(&store, &id).await, vec![0.0, 1.0, 0.0, 0.0]);
    assert_eq!(
        store.record_hashes(c, vec![ts.into()]).await.unwrap()[0].1,
        "hash:Rings on bed 4."
    );

    // A record of another connector, and any record of a deleting connector, are refused.
    let other: ConnectorId = f.ids.id();
    assert!(
        store
            .upsert_records(c, vec![record(other, ts, 0, "x", Some(one_hot(0)), t0())])
            .await
            .is_err()
    );
    let owner = owner_of(&store, &f).await;
    store.start_connector_deletion(owner, c).await.unwrap();
    assert!(matches!(
        store
            .upsert_records(c, vec![record(c, ts, 1, "y", Some(one_hot(2)), t0())])
            .await,
        Err(StoreError::NotFound)
    ));
}

/// Deletes by item, by chunk range and by source; the 7-day re-read's list.
#[tokio::test]
async fn deletes_by_item_chunk_and_source() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let c = active_slack(&store, &f, f.garden).await;
    let records = vec![
        record(c, "C1/1.000100", 0, "a", Some(one_hot(0)), t0()),
        record(c, "C1/2.000100", 0, "b", Some(one_hot(1)), at(60)),
        record(c, "C10/1.000100", 0, "c", Some(one_hot(2)), at(60)),
        record(c, "C2/1.000100", 0, "d", Some(one_hot(3)), at(60)),
        record(c, "1aBcD", 0, "doc 0", Some(one_hot(0)), t0()),
        record(c, "1aBcD", 1, "doc 1", Some(one_hot(1)), t0()),
        record(c, "1aBcD", 2, "doc 2", Some(one_hot(2)), t0()),
    ];
    assert_eq!(store.upsert_records(c, records).await.unwrap(), 7);

    // The re-read's list: C1's items from `since` on; `C10/` isn't C1's.
    assert_eq!(
        store
            .source_vendor_ids_since(c, "C1".into(), at(30))
            .await
            .unwrap(),
        vec!["C1/2.000100".to_owned()]
    );
    let mut all_c1 = store
        .source_vendor_ids_since(c, "C1".into(), t0())
        .await
        .unwrap();
    all_c1.sort();
    assert_eq!(all_c1, vec!["C1/1.000100", "C1/2.000100"]);

    // A document re-chunked into 1 chunk: chunks 1 and up go, chunk 0 keeps its vector.
    assert_eq!(
        store
            .delete_chunks_from(c, "1aBcD".into(), 1)
            .await
            .unwrap(),
        2
    );
    let doc = store.record_hashes(c, vec!["1aBcD".into()]).await.unwrap();
    assert_eq!(doc.len(), 1);
    assert_eq!(doc[0].0.n, 0);

    // By item.
    assert_eq!(
        store
            .delete_records(c, vec!["C1/1.000100".into(), "1aBcD".into(), "gone".into()])
            .await
            .unwrap(),
        2
    );

    // By source: C1's go, C10's and C2's stay.
    assert_eq!(
        store.delete_source_records(c, "C1".into()).await.unwrap(),
        1
    );
    assert_eq!(
        store.delete_source_records(c, "C1".into()).await.unwrap(),
        0
    );
    let mut left: Vec<String> = store
        .record_hashes(
            c,
            vec![
                "C10/1.000100".into(),
                "C2/1.000100".into(),
                "C1/2.000100".into(),
            ],
        )
        .await
        .unwrap()
        .into_iter()
        .map(|(id, _)| id.vendor_id.to_string())
        .collect();
    left.sort();
    assert_eq!(left, vec!["C10/1.000100", "C2/1.000100"]);
}

/// A Drive connector's source is its one folder: dropping it drops all its records.
#[tokio::test]
async fn a_drive_source_is_all_its_records() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let mut d = new_connector(&f, ConnectorKind::GoogleDrive);
    d.status = ConnectorStatus::Active;
    d.oauth = None;
    d.sources = vec![source("folder-1", "Farm records")];
    let c = d.id;
    store.seed_connector(f.garden, d).await.unwrap();
    store
        .upsert_records(
            c,
            vec![
                record(c, "1aBcD", 0, "x", Some(one_hot(0)), t0()),
                record(c, "2eFgH", 0, "y", Some(one_hot(1)), t0()),
            ],
        )
        .await
        .unwrap();
    assert_eq!(
        store
            .source_vendor_ids_since(c, "folder-1".into(), t0())
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        store
            .delete_source_records(c, "folder-1".into())
            .await
            .unwrap(),
        2
    );
}

/// The exact search: only the proof's garden, ranked by cosine similarity, with its filters;
/// a deleting connector's records are left out.
#[tokio::test]
#[expect(clippy::too_many_lines, reason = "two gardens and three filters")]
async fn the_search_stays_in_its_garden_and_ranks_by_similarity() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let viewer: Membership<Viewer> = owner_of(&store, &f).await.at_least();
    let c = active_slack(&store, &f, f.garden).await;
    let near = vec![0.9, 0.1, 0.0, 0.0];
    let middle = vec![0.5, 0.5, 0.0, 0.0];
    let far = vec![0.0, 0.0, 1.0, 0.0];
    store
        .upsert_records(
            c,
            vec![
                record(c, "C1/1.000100", 0, "far", Some(far.clone()), t0()),
                record(c, "C1/2.000100", 0, "near", Some(near.clone()), at(60)),
                record(c, "1aBcD", 0, "middle doc", Some(middle), at(120)),
            ],
        )
        .await
        .unwrap();

    // The other garden holds the exact query vector: it must never come back.
    let other = other_garden(&store, &f).await;
    let o = active_slack(&store, &f, other).await;
    store
        .upsert_records(
            o,
            vec![record(
                o,
                "C1/1.000100",
                0,
                "other garden",
                Some(one_hot(0)),
                t0(),
            )],
        )
        .await
        .unwrap();

    let query = Embedding(one_hot(0));
    let hits = store
        .search_records(viewer, &query, FarmRecordFilter::default(), 10)
        .await
        .unwrap();
    let texts: Vec<&str> = hits.iter().map(|h| h.hit.text.as_str()).collect();
    assert_eq!(texts, vec!["near", "middle doc", "far"]);
    assert!(hits.windows(2).all(|w| w[0].similarity >= w[1].similarity));
    assert!(hits[2].similarity.abs() < 1e-6);
    let top = &hits[0].hit.citation;
    assert_eq!(top.record_id.connector, c);
    assert_eq!(top.record_id.vendor_id.as_ref(), "C1/2.000100");
    assert_eq!(top.system, ConnectorKind::Slack);
    assert_eq!(top.at, at(60));
    assert_eq!(top.author.as_deref(), Some("Sam"));

    // k, kind and since.
    assert_eq!(
        store
            .search_records(viewer, &query, FarmRecordFilter::default(), 1)
            .await
            .unwrap()
            .len(),
        1
    );
    let docs = store
        .search_records(
            viewer,
            &query,
            FarmRecordFilter {
                kind: Some(FarmRecordKind::Document),
                since: None,
            },
            10,
        )
        .await
        .unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].hit.text, "middle doc");
    let recent = store
        .search_records(
            viewer,
            &query,
            FarmRecordFilter {
                kind: None,
                since: Some(at(60)),
            },
            10,
        )
        .await
        .unwrap();
    assert_eq!(recent.len(), 2);

    // The other garden's member finds only theirs.
    let other_viewer: Membership<Viewer> =
        check_membership_of(&*store, f.user, other).await.unwrap();
    let theirs = store
        .search_records(other_viewer, &query, FarmRecordFilter::default(), 10)
        .await
        .unwrap();
    assert_eq!(theirs.len(), 1);
    assert_eq!(theirs[0].hit.text, "other garden");

    // A disconnected connector's records leave the search at once.
    let owner = owner_of(&store, &f).await;
    store.start_connector_deletion(owner, c).await.unwrap();
    assert_eq!(
        store
            .search_records(viewer, &query, FarmRecordFilter::default(), 10)
            .await
            .unwrap(),
        Vec::new()
    );
}

/// Counts for the 10,000 limit and the connector's `record_count`; the cleanup of old records
/// and of pending connectors; a connector's deletion, records first.
#[tokio::test]
async fn counts_cleanup_and_deletion() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let owner = owner_of(&store, &f).await;
    let viewer: Membership<Viewer> = owner.at_least();
    let c = active_slack(&store, &f, f.garden).await;
    let other = other_garden(&store, &f).await;
    let o = active_slack(&store, &f, other).await;

    // 501 records: more than one deletion batch.
    let old = t0();
    let recent = old.plus(FARM_RECORD_KEEP).plus(Duration::from_secs(3600));
    let many: Vec<NewFarmRecord> = (0..501)
        .map(|i| {
            let when = if i < 3 { old } else { recent };
            record(
                c,
                &format!("C1/{i}.000100"),
                0,
                &format!("m{i}"),
                Some(one_hot(i)),
                when,
            )
        })
        .collect();
    store.upsert_records(c, many).await.unwrap();
    store
        .upsert_records(o, vec![record(o, "C1/1.0", 0, "x", Some(one_hot(0)), old)])
        .await
        .unwrap();
    assert_eq!(store.garden_record_count(f.garden).await.unwrap(), 501);
    assert_eq!(store.garden_record_count(other).await.unwrap(), 1);
    let listed = store.connectors(viewer).await.unwrap();
    assert_eq!(listed[0].record_count, 501);

    // The 180-day cleanup: records older than the cutoff, in every garden.
    let now = recent.plus(Duration::from_secs(60));
    let cutoff = now.minus(FARM_RECORD_KEEP);
    assert_eq!(store.delete_records_before(cutoff).await.unwrap(), 4);
    assert_eq!(store.delete_records_before(cutoff).await.unwrap(), 0);
    assert_eq!(store.garden_record_count(f.garden).await.unwrap(), 498);
    assert_eq!(store.garden_record_count(other).await.unwrap(), 0);

    // The pending cleanup: only `pending` connectors started before the cutoff.
    let pending = new_connector(&f, ConnectorKind::GoogleDrive);
    let pending_id = pending.id;
    store.create_connector(owner, pending).await.unwrap();
    assert_eq!(store.delete_expired_pending(t0()).await.unwrap(), 0);
    let ten_minutes_on = t0().plus(PENDING_CONNECT_TTL).plus(Duration::from_secs(1));
    assert_eq!(
        store
            .delete_expired_pending(ten_minutes_on.minus(PENDING_CONNECT_TTL))
            .await
            .unwrap(),
        1
    );
    assert!(store.connector_row(pending_id).await.unwrap().is_none());
    assert!(store.connector_row(c).await.unwrap().is_some());

    // A connector that isn't being deleted can't be deleted in batches.
    assert!(matches!(
        store.delete_connector_batch(c).await,
        Err(StoreError::Rejected(DomainError::InvalidState(_)))
    ));
    store.start_connector_deletion(owner, c).await.unwrap();
    let mut batches = Vec::new();
    loop {
        let n = store.delete_connector_batch(c).await.unwrap();
        if n == 0 {
            break;
        }
        batches.push(n);
        assert!(batches.len() < 10, "never finished");
    }
    assert_eq!(batches, vec![498, 1], "records, then the connector");
    assert!(store.connector_row(c).await.unwrap().is_none());
    assert_eq!(store.garden_record_count(f.garden).await.unwrap(), 0);
    assert!(store.connector_row(o).await.unwrap().is_some());
}

/// A plan's `farm_records` (and a step's `farm_citations`) and a message's `farm_citations`
/// round-trip through the store; rows written before the fields existed read as empty.
#[tokio::test]
#[expect(clippy::too_many_lines, reason = "a plan and a message, new and old")]
async fn plans_and_messages_carry_farm_citations() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let c = active_slack(&store, &f, f.garden).await;
    let record_id = FarmRecordId::new(c, "C1/1729000000.000100", 0).unwrap();
    let citation = FarmCitation {
        record_id: record_id.clone(),
        system: ConnectorKind::Slack,
        title: "#field-reports".into(),
        author: Some("Sam".into()),
        at: t0(),
        link: "https://maplerow.slack.com/archives/C1/p1729000000000100".into(),
    };

    // The plan.
    let plan_id: PlanId = f.ids.id();
    let plan = ActionPlan {
        id: plan_id,
        scan_id: f.scan,
        plant_id: f.plant,
        version: 1,
        supersedes: None,
        provisional: false,
        diagnosis_id: f.ids.id(),
        open_question: None,
        constraints: Vec::new(),
        summary: "Check bed 3".into(),
        steps: vec![PlanStep {
            id: "s1".parse().unwrap(),
            order: 1,
            title: "Check bed 3 for new rings".into(),
            detail: "The crew saw rings twice.".into(),
            treatment_id: None,
            category: StepCategory::Monitoring,
            when: PlanWhen::Now,
            targets: Vec::new(),
            plant_id: None,
            citations: vec!["example_extension_tomato_diseases".parse().unwrap()],
            farm_citations: vec![record_id.clone()],
            safety_note: None,
        }],
        watch_for: Vec::new(),
        citations: Vec::new(),
        farm_records: vec![citation.clone()],
        task_ids: Vec::new(),
        llm_model: "test".into(),
        prompt_version: "v1".into(),
        created_at: t0(),
    };
    let editor: Membership<Editor> = f.editor;
    let written = store
        .write_revision(
            editor,
            PlanWrite {
                plan: plan.clone(),
                reason: PlanReason::Initial,
                expected: None,
                tasks: Vec::new(),
                triggers: Vec::new(),
            },
        )
        .await
        .unwrap();
    assert_eq!(written.farm_records, vec![citation.clone()]);
    let read = store.current_plan(f.scan).await.unwrap().unwrap();
    assert_eq!(read.farm_records, vec![citation.clone()]);
    assert_eq!(read.steps[0].farm_citations, vec![record_id.clone()]);

    // The message.
    let viewer: Membership<Viewer> = f.editor.at_least();
    let conv = store
        .create_conversation(
            viewer,
            NewConversation {
                id: f.ids.id(),
                user: f.user,
                anchor: Anchor::Garden(f.garden),
                created_at: t0(),
            },
        )
        .await
        .unwrap();
    let mut verifier = domain::ports::MockAuthVerifier::new();
    verifier.expect_verify().returning(|_, now| {
        Ok(domain::ports::VerifiedToken {
            auth_id: "user_2abc".parse().unwrap(),
            display_name: None,
            expires_at: now,
        })
    });
    let user = domain::authz::authenticate(&verifier, &*store, "token", t0())
        .await
        .unwrap();
    let owner = check_conversation::<Viewer>(&*store, &user, conv.id)
        .await
        .unwrap();
    let message = |text: &str, farm_citations: Vec<FarmCitation>| NewMessage {
        id: f.ids.id(),
        role: MessageRole::Assistant,
        text: text.into(),
        attachments: Vec::new(),
        citations: Vec::new(),
        farm_citations,
        tool_calls: Vec::new(),
        route: None,
        interrupted: false,
        model: None,
        prompt_version: None,
        tokens_in: 0,
        tokens_out: 0,
        created_at: t0(),
    };
    let stored = store
        .store_message(
            owner,
            message("Bed 3 was sprayed on Oct 12.", vec![citation.clone()]),
        )
        .await
        .unwrap();
    assert_eq!(stored.farm_citations, vec![citation.clone()]);
    let old = store
        .store_message(owner, message("Hello.", Vec::new()))
        .await
        .unwrap();
    let (_, context) = store.context(owner, 10).await.unwrap();
    let cited = context.iter().find(|m| m.message.id == stored.id).unwrap();
    assert_eq!(cited.message.farm_citations, vec![citation]);

    // Rows from before the fields existed: drop the fields, unset them on a plan and a
    // message, define the fields again (`DEFAULT []` doesn't backfill), and read.
    let old_plan: PlanId = plan_id;
    exec(
        &store,
        "REMOVE FIELD farm_records ON action_plan; REMOVE FIELD farm_citations ON message;",
    )
    .await;
    let mut r = store
        .db()
        .query(
            "UPDATE $plan UNSET farm_records; UPDATE $message UNSET farm_citations;
             UPDATE $old UNSET farm_citations;",
        )
        .bind(("plan", rid(&old_plan)))
        .bind(("message", rid(&stored.id)))
        .bind(("old", rid(&old.id)))
        .await
        .unwrap();
    assert!(r.take_errors().is_empty());
    apply_schema_with(store.db()).await.unwrap();
    let mut r = store
        .db()
        .query(
            "SELECT VALUE farm_records FROM ONLY $plan; SELECT VALUE farm_citations FROM ONLY $message;",
        )
        .bind(("plan", rid(&old_plan)))
        .bind(("message", rid(&old.id)))
        .await
        .unwrap();
    assert!(r.take_errors().is_empty());
    let none: Option<surrealdb::types::Value> = r.take(0).unwrap();
    assert!(
        matches!(none, None | Some(surrealdb::types::Value::None)),
        "an old plan has no farm_records: {none:?}"
    );
    let read = store.current_plan(f.scan).await.unwrap().unwrap();
    assert_eq!(read.farm_records, Vec::new());
    let (_, context) = store.context(owner, 10).await.unwrap();
    assert_eq!(context.len(), 2);
    assert!(context.iter().all(|m| m.message.farm_citations.is_empty()));
}

/// Two or three creates racing on a garden that holds one connector: the limit is 2, so the
/// losers conflict, retry and meet it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn racing_creates_respect_the_connector_limit() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let owner = owner_of(&store, &f).await;
    let existing = new_connector(&f, ConnectorKind::Slack);
    store.create_connector(owner, existing).await.unwrap();

    let (a, b, c) = (
        new_connector(&f, ConnectorKind::GoogleDrive),
        new_connector(&f, ConnectorKind::GoogleDrive),
        new_connector(&f, ConnectorKind::GoogleDrive),
    );
    let (ra, rb, rc) = tokio::join!(
        store.create_connector(owner, a),
        store.create_connector(owner, b),
        store.create_connector(owner, c),
    );
    let accepted = [&ra, &rb, &rc].iter().filter(|r| r.is_ok()).count();
    assert_eq!(accepted, 1, "{ra:?} {rb:?} {rc:?}");
    for r in [ra, rb, rc].into_iter().filter(Result::is_err) {
        assert!(matches!(
            r,
            Err(StoreError::Rejected(DomainError::InvalidState(_)))
        ));
    }
    let viewer: Membership<Viewer> = owner.at_least();
    assert_eq!(store.connectors(viewer).await.unwrap().len(), 2);
}

/// Sync now on a connector stopped until new sources is refused, because the due query would
/// skip it; new sources lift the wait.
#[tokio::test]
async fn sync_now_is_refused_on_a_stopped_connector() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let owner = owner_of(&store, &f).await;
    let c = active_slack(&store, &f, f.garden).await;
    for code in [SyncErrorCode::TooLarge, SyncErrorCode::SourceMissing] {
        store
            .set_status(c, ConnectorStatus::Active, Some(code))
            .await
            .unwrap();
        assert!(matches!(
            store.request_sync(owner, c, at(100)).await,
            Err(StoreError::Rejected(DomainError::InvalidState(_)))
        ));
        assert_eq!(
            store
                .connector_row(c)
                .await
                .unwrap()
                .unwrap()
                .sync_requested_at,
            None
        );
    }
    store
        .set_sources(owner, c, vec![source("C2", "spray-log")], at(101))
        .await
        .unwrap();
    assert_eq!(
        store.request_sync(owner, c, at(102)).await.unwrap(),
        SyncRequest::Accepted
    );
    // A transient failure doesn't stop the sync.
    store
        .set_status(c, ConnectorStatus::Active, Some(SyncErrorCode::RateLimited))
        .await
        .unwrap();
    assert_eq!(
        store.request_sync(owner, c, at(200)).await.unwrap(),
        SyncRequest::Accepted
    );
}

/// Two Sync now requests at once: one is accepted, the other is too soon.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn racing_sync_requests_accept_exactly_one() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let owner = owner_of(&store, &f).await;
    let c = active_slack(&store, &f, f.garden).await;
    let (a, b) = tokio::join!(
        store.request_sync(owner, c, at(100)),
        store.request_sync(owner, c, at(100)),
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    let accepted = [a, b]
        .iter()
        .filter(|r| **r == SyncRequest::Accepted)
        .count();
    assert_eq!(accepted, 1, "{a:?} {b:?}");
    assert!(
        [a, b]
            .iter()
            .any(|r| matches!(r, SyncRequest::TooSoon { .. }))
    );
}

fn outcome(due_at: Timestamp, next: Timestamp, last_error: Option<SyncErrorCode>) -> SyncOutcome {
    SyncOutcome {
        due_at,
        cursors: BTreeMap::new(),
        synced_at: Some(next),
        next_sync_at: next,
        last_error,
    }
}

/// A run that ends after a Sync now or a `set_sources` landed keeps their `next_sync_at`, and
/// a stopping failure of the old sources isn't stored over the new ones.
#[tokio::test]
async fn finish_sync_keeps_what_landed_during_the_run() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let owner = owner_of(&store, &f).await;
    let c = active_slack(&store, &f, f.garden).await;

    // Nothing landed: the run's times and failure are stored.
    let due = store.due_connectors(at(1), 10).await.unwrap();
    let due_at = due[0].next_sync_at;
    store
        .finish_sync(c, outcome(due_at, at(16), Some(SyncErrorCode::RateLimited)))
        .await
        .unwrap();
    let row = store.connector_row(c).await.unwrap().unwrap();
    assert_eq!(row.next_sync_at, at(16));
    assert_eq!(row.last_error, Some(SyncErrorCode::RateLimited));

    // A Sync now lands during the next run: its next_sync_at (now) stays.
    let due_at = at(16);
    store.request_sync(owner, c, at(20)).await.unwrap();
    store
        .finish_sync(c, outcome(due_at, at(36), None))
        .await
        .unwrap();
    let row = store.connector_row(c).await.unwrap().unwrap();
    assert_eq!(row.next_sync_at, at(20), "the Sync now is not pushed out");
    assert_eq!(row.last_error, None);
    assert_eq!(row.connector.synced_at, Some(at(36)));

    // New sources land during a run that stops on `too_large`: the wait isn't stored.
    let due_at = row.next_sync_at;
    store
        .set_sources(owner, c, vec![source("C2", "spray-log")], at(40))
        .await
        .unwrap();
    store
        .finish_sync(c, outcome(due_at, at(55), Some(SyncErrorCode::TooLarge)))
        .await
        .unwrap();
    let row = store.connector_row(c).await.unwrap().unwrap();
    assert_eq!(row.next_sync_at, at(40));
    assert_eq!(row.last_error, None, "the new sources run first");
    assert_eq!(
        store.due_connectors(at(41), 10).await.unwrap().len(),
        1,
        "still due"
    );

    // Without anything landing, too_large is stored and stops the connector.
    store
        .finish_sync(c, outcome(at(40), at(60), Some(SyncErrorCode::TooLarge)))
        .await
        .unwrap();
    let row = store.connector_row(c).await.unwrap().unwrap();
    assert_eq!(row.last_error, Some(SyncErrorCode::TooLarge));
    assert_eq!(store.due_connectors(at(61), 10).await.unwrap().len(), 0);
}

/// Dropping one Drive folder keeps another folder's records, as when the owner swaps folders
/// and the new folder's first sync runs before the delete.
#[tokio::test]
async fn dropping_a_drive_folder_keeps_the_new_folders_records() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let mut d = new_connector(&f, ConnectorKind::GoogleDrive);
    d.status = ConnectorStatus::Active;
    d.oauth = None;
    d.sources = vec![source("folder-2", "New records")];
    let c = d.id;
    store.seed_connector(f.garden, d).await.unwrap();
    let mut old = record(c, "1aBcD", 0, "old", Some(one_hot(0)), t0());
    old.source = "folder-1".into();
    let mut new = record(c, "2eFgH", 0, "new", Some(one_hot(1)), t0());
    new.source = "folder-2".into();
    store.upsert_records(c, vec![old, new]).await.unwrap();
    assert_eq!(
        store
            .delete_source_records(c, "folder-1".into())
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        store
            .source_vendor_ids_since(c, "folder-2".into(), t0())
            .await
            .unwrap(),
        vec!["2eFgH".to_owned()]
    );
    assert_eq!(store.garden_record_count(f.garden).await.unwrap(), 1);
}

/// The 180-day cleanup deletes old Slack messages only; an old Drive document stays.
#[tokio::test]
async fn the_cleanup_leaves_old_drive_documents() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let c = active_slack(&store, &f, f.garden).await;
    store
        .upsert_records(
            c,
            vec![
                record(c, "C1/1.000100", 0, "old message", Some(one_hot(0)), t0()),
                record(c, "1aBcD", 0, "old spray log", Some(one_hot(1)), t0()),
            ],
        )
        .await
        .unwrap();
    let cutoff = t0().plus(FARM_RECORD_KEEP);
    assert_eq!(store.delete_records_before(cutoff).await.unwrap(), 1);
    assert_eq!(store.delete_records_before(cutoff).await.unwrap(), 0);
    let left = store
        .record_hashes(c, vec!["C1/1.000100".into(), "1aBcD".into()])
        .await
        .unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].0.vendor_id.to_string(), "1aBcD");
}

/// A deletion revokes an account's token only when no other connector holds it: live
/// (`active` or `reauth_needed`) `vendor` connectors of the same Slack team or Google account
/// count, in any garden; the one being deleted, pending ones, those being deleted, `era`
/// connectors and the other kind don't.
#[tokio::test]
async fn other_holders_of_an_accounts_token_are_counted() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let owner = owner_of(&store, &f).await;
    let other = other_garden(&store, &f).await;
    let other_owner = check_membership_of::<Owner>(&*store, f.user, other)
        .await
        .expect("the creator owns the second garden");
    let here = active_slack(&store, &f, f.garden).await;
    let there = active_slack(&store, &f, other).await;
    let slack =
        |except| store.shared_account_connectors(ConnectorKind::Slack, "T0123".to_owned(), except);
    assert_eq!(slack(here).await.unwrap(), 1);
    assert_eq!(
        store
            .shared_account_connectors(ConnectorKind::Slack, "T9999".into(), here)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        store
            .shared_account_connectors(ConnectorKind::GoogleDrive, "T0123".into(), here)
            .await
            .unwrap(),
        0,
        "a Slack team is never a Google account"
    );
    store
        .set_status(
            there,
            ConnectorStatus::ReauthNeeded,
            Some(SyncErrorCode::AuthRefused),
        )
        .await
        .unwrap();
    assert_eq!(slack(here).await.unwrap(), 1);
    store
        .create_connector(owner, new_connector(&f, ConnectorKind::Slack))
        .await
        .unwrap();
    store
        .start_connector_deletion(other_owner, there)
        .await
        .unwrap();
    assert_eq!(slack(here).await.unwrap(), 0);
    assert_eq!(slack(there).await.unwrap(), 1);

    // Google: keyed on the account's email; an `era` connector of the same account doesn't
    // count, since its token is the simulator's.
    let google = |id: ConnectorId, endpoint| {
        let mut c = new_connector(&f, ConnectorKind::GoogleDrive);
        c.id = id;
        c.status = ConnectorStatus::Active;
        c.endpoint = endpoint;
        c.oauth = None;
        c.account = Some(ConnectorAccount::GoogleDrive {
            email: "farm@example.org".into(),
        });
        c.secret = Some(sealed(b"sealed-grant"));
        c
    };
    let drive = f.ids.id();
    let demo = f.ids.id();
    store
        .seed_connector(other, google(drive, ConnectorEndpoint::Vendor))
        .await
        .unwrap();
    store
        .seed_connector(other, google(demo, ConnectorEndpoint::Era))
        .await
        .unwrap();
    let email = |except| {
        store.shared_account_connectors(
            ConnectorKind::GoogleDrive,
            "farm@example.org".to_owned(),
            except,
        )
    };
    assert_eq!(email(drive).await.unwrap(), 0, "the era one doesn't count");
    assert_eq!(email(demo).await.unwrap(), 1);
}

/// T-E3.6 (docs/EON): the cleanup's cutoff is 10 minutes before now. A pending connector
/// started 11 minutes ago is gone after it; one started 9 minutes ago and an active one stay.
#[tokio::test]
async fn pending_connectors_older_than_ten_minutes_are_cleaned_up() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let owner = owner_of(&store, &f).await;
    let active = active_slack(&store, &f, f.garden).await;
    let old = new_connector(&f, ConnectorKind::Slack);
    let old_id = old.id;
    store.create_connector(owner, old).await.unwrap();
    let mut fresh = new_connector(&f, ConnectorKind::GoogleDrive);
    fresh.created_at = at(2);
    let fresh_id = fresh.id;
    // A garden holds 2 connectors; the third goes to a second garden of the same owner.
    let other = other_garden(&store, &f).await;
    let other_owner = check_membership_of::<Owner>(&*store, f.user, other)
        .await
        .expect("the creator owns the second garden");
    store.create_connector(other_owner, fresh).await.unwrap();

    let now = at(11);
    assert_eq!(
        store
            .delete_expired_pending(now.minus(PENDING_CONNECT_TTL))
            .await
            .unwrap(),
        1
    );
    assert!(store.connector_row(old_id).await.unwrap().is_none());
    assert!(store.connector_row(fresh_id).await.unwrap().is_some());
    assert!(store.connector_row(active).await.unwrap().is_some());
}

/// T-E3.8 (docs/EON), on the embedded engine: what `PUT /connectors/{id}/sources` does when an
/// owner drops a channel. The new sources are stored, then the dropped channel's records are
/// deleted in batches; the kept channel's records stay.
#[tokio::test]
async fn dropping_a_channel_deletes_its_records_and_keeps_the_others() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let owner = owner_of(&store, &f).await;
    let c = active_slack(&store, &f, f.garden).await;
    store
        .upsert_records(
            c,
            vec![
                record(
                    c,
                    "C1/1.000100",
                    0,
                    "rings on tomatoes",
                    Some(one_hot(0)),
                    t0(),
                ),
                record(c, "C1/2.000100", 0, "bed 3 sprayed", Some(one_hot(1)), t0()),
                record(c, "C2/1.000100", 0, "crew notes", Some(one_hot(2)), t0()),
            ],
        )
        .await
        .unwrap();
    let kept = store
        .set_sources(owner, c, vec![source("C2", "spray-log")], at(1))
        .await
        .unwrap();
    assert_eq!(kept.sources, vec![source("C2", "spray-log")]);
    let mut deleted = 0;
    loop {
        let n = store.delete_source_records(c, "C1".into()).await.unwrap();
        if n == 0 {
            break;
        }
        deleted += n;
    }
    assert_eq!(deleted, 2);
    assert_eq!(
        store
            .source_vendor_ids_since(c, "C2".into(), t0())
            .await
            .unwrap(),
        vec!["C2/1.000100".to_owned()]
    );
    assert_eq!(
        store
            .source_vendor_ids_since(c, "C1".into(), t0())
            .await
            .unwrap(),
        Vec::<String>::new()
    );
    assert_eq!(store.garden_record_count(f.garden).await.unwrap(), 1);
}
