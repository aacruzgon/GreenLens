//! The sync on the embedded `SurrealDB` 3.3.4 engine with a scripted [`FarmSource`] and a
//! counting embedder (docs/EON T-E4.1, T-E4.7, T-E4.9, T-E4.10). One datastore per test; time
//! moves only through the [`FixedClock`].

use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use secrecy::ExposeSecret as _;
use surrealdb::engine::any;
use tokio_util::sync::CancellationToken;

use domain::{
    authz::check_membership_of,
    connectors::{
        ConnectorAccount, ConnectorEndpoint, ConnectorKind, ConnectorSource, ConnectorStatus,
        FarmRecordKind, SyncErrorCode,
    },
    garden::gardens::TimeZone,
    id::{AuthId, ConnectorId, FarmRecordId, GardenId, UserId},
    ports::{
        Clock, DeletionRepo, EmbedMode, Embedding, EmbeddingModel, FarmItem, FarmPage,
        FarmPageRequest, FarmRecordStore, FarmSource, FarmSourceError, FarmVector, GardenRepo,
        IdGenExt, IdentityRepo, ModelError, NewConnector, NewFarmRecord, NewGarden,
        ResolvedLocation, SyncCursor, TextEmbedder, VendorToken,
    },
    records::weather::{Coordinates, LocationPrecision},
    rules::limits::{FARM_RECORD_KEEP, MAX_FARM_RECORDS_PER_GARDEN},
    testing::{FixedClock, SequentialIds},
    wire::Timestamp,
};
use store::{SurrealStore, apply_schema_with};

use super::{
    FarmSources, FarmSync, RunEnd, SYNC_EVERY, SyncConfig, chunk, delete_old_messages,
    seed::{self, EraSeed, Seeded},
};
use crate::sealing::{ConnectorKey, SealPurpose};

const KEY_HEX: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
const DIM: usize = 4;

pub fn key() -> ConnectorKey {
    ConnectorKey::from_hex(KEY_HEX).expect("a valid test key")
}

/// The pinned clock of the Era tests (docs/EON section 2).
pub fn pinned() -> Timestamp {
    "2026-10-15T12:00:00Z".parse().expect("a timestamp")
}

/// A fresh embedded store with the schema.
pub async fn store() -> Arc<SurrealStore> {
    let db = any::connect("mem://").await.expect("embedded engine");
    apply_schema_with(&db).await.expect("schema applies");
    Arc::new(SurrealStore::from_client(db).await.expect("pdm/main"))
}

/// A user who owns a new garden.
pub async fn garden(store: &SurrealStore, ids: &SequentialIds, name: &str) -> (UserId, GardenId) {
    let auth: AuthId = format!("user_{name}").parse().expect("auth id");
    let user = store
        .upsert_user(&auth, Some(name.into()))
        .await
        .expect("user");
    let garden: GardenId = ids.id();
    store
        .create_garden(
            user,
            NewGarden {
                id: garden,
                name: name.into(),
                location: ResolvedLocation {
                    coordinates: Coordinates {
                        lat: 43.55,
                        lon: -80.25,
                    },
                    precision: LocationPrecision::About1Km,
                    label: "Guelph, ON".into(),
                    time_zone: TimeZone("America/Toronto".into()),
                },
            },
        )
        .await
        .expect("garden");
    (user, garden)
}

/// An `active` connector with a sealed token and these sources, due at `now`.
pub async fn connector(
    store: &SurrealStore,
    ids: &SequentialIds,
    (user, garden): (UserId, GardenId),
    endpoint: ConnectorEndpoint,
    sources: &[&str],
    token: &str,
    now: Timestamp,
) -> ConnectorId {
    let id: ConnectorId = ids.id();
    store
        .seed_connector(
            garden,
            NewConnector {
                id,
                kind: ConnectorKind::Slack,
                status: ConnectorStatus::Active,
                endpoint,
                created_by: user,
                account: Some(ConnectorAccount::Slack {
                    team_id: "T1".into(),
                    team_name: "Farm".into(),
                    url: "https://farm.slack.com/".into(),
                }),
                secret: Some(key().seal(SealPurpose::Token, id, token.as_bytes())),
                oauth: None,
                sources: sources
                    .iter()
                    .map(|s| ConnectorSource {
                        id: (*s).to_owned(),
                        name: (*s).to_lowercase(),
                    })
                    .collect(),
                created_at: now,
            },
        )
        .await
        .expect("connector");
    id
}

/// Counts the texts it embeds; every vector is the same unit vector.
#[derive(Default)]
pub struct CountingEmbedder {
    pub calls: AtomicUsize,
    pub texts: AtomicUsize,
}

impl CountingEmbedder {
    pub fn texts(&self) -> usize {
        self.texts.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl TextEmbedder for CountingEmbedder {
    fn model(&self) -> EmbeddingModel {
        EmbeddingModel {
            name: "counting".into(),
            version: "1".into(),
            dimension: DIM,
        }
    }

    async fn embed(&self, mode: EmbedMode, texts: &[String]) -> Result<Vec<Embedding>, ModelError> {
        assert_eq!(mode, EmbedMode::Document, "farm records are documents");
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.texts.fetch_add(texts.len(), Ordering::SeqCst);
        Ok(texts
            .iter()
            .map(|_| Embedding(vec![1.0, 0.0, 0.0, 0.0]))
            .collect())
    }
}

type Script = Box<dyn FnMut(&FarmPageRequest) -> Result<FarmPage, FarmSourceError> + Send>;

/// A vendor whose pages a test writes, recording every request.
struct FakeSource {
    window: Option<Duration>,
    script: Mutex<Script>,
    requests: Mutex<Vec<FarmPageRequest>>,
    tokens: Mutex<Vec<String>>,
}

impl FakeSource {
    fn new(window: Option<Duration>, script: Script) -> Arc<Self> {
        Arc::new(Self {
            window,
            script: Mutex::new(script),
            requests: Mutex::new(Vec::new()),
            tokens: Mutex::new(Vec::new()),
        })
    }

    fn requests(&self) -> Vec<FarmPageRequest> {
        self.requests.lock().expect("requests").clone()
    }
}

#[async_trait]
impl FarmSource for FakeSource {
    fn kind(&self) -> ConnectorKind {
        ConnectorKind::Slack
    }

    fn reread_window(&self) -> Option<Duration> {
        self.window
    }

    async fn available_sources(
        &self,
        _token: &VendorToken,
    ) -> Result<Vec<ConnectorSource>, FarmSourceError> {
        Ok(Vec::new())
    }

    async fn read_page(
        &self,
        token: &VendorToken,
        request: FarmPageRequest,
    ) -> Result<FarmPage, FarmSourceError> {
        self.tokens
            .lock()
            .expect("tokens")
            .push(token.expose().to_owned());
        self.requests
            .lock()
            .expect("requests")
            .push(request.clone());
        (self.script.lock().expect("script"))(&request)
    }

    async fn revoke(&self, _token: &VendorToken) -> Result<(), FarmSourceError> {
        Ok(())
    }
}

fn item(source: &str, ts: &str, text: &str, at: Timestamp) -> FarmItem {
    FarmItem {
        vendor_id: format!("{source}/{ts}"),
        kind: FarmRecordKind::Message,
        title: format!("#{source}"),
        author: Some("Maria".into()),
        link: format!("https://farm.slack.com/archives/{source}/p{ts}"),
        text: text.into(),
        at,
    }
}

fn page(items: Vec<FarmItem>, more: bool, cursor: &str) -> FarmPage {
    FarmPage {
        items,
        removed: Vec::new(),
        kept: Vec::new(),
        cursor: SyncCursor(cursor.into()),
        more,
        probes: 0,
    }
}

struct Bench {
    store: Arc<SurrealStore>,
    clock: Arc<FixedClock>,
    embedder: Arc<CountingEmbedder>,
    sync: FarmSync,
    ids: SequentialIds,
}

fn bench(store: Arc<SurrealStore>, source: Arc<dyn FarmSource>, config: SyncConfig) -> Bench {
    let clock = Arc::new(FixedClock::at(pinned()));
    let embedder = Arc::new(CountingEmbedder::default());
    let sync = FarmSync::new(
        store.clone(),
        embedder.clone(),
        clock.clone(),
        key(),
        FarmSources::default().with(ConnectorEndpoint::Vendor, source),
        config,
    );
    Bench {
        store,
        clock,
        embedder,
        sync,
        ids: SequentialIds::default(),
    }
}

impl Bench {
    async fn run(&self) -> Vec<(ConnectorId, RunEnd)> {
        self.sync
            .run_due(&CancellationToken::new())
            .await
            .expect("due connectors listed")
    }

    async fn row(&self, id: ConnectorId) -> domain::ports::ConnectorRow {
        self.store
            .connector_row(id)
            .await
            .expect("read")
            .expect("exists")
    }

    async fn count(&self, garden: GardenId) -> u32 {
        self.store.garden_record_count(garden).await.expect("count")
    }
}

#[tokio::test]
async fn t_e4_1_a_run_takes_at_most_four_due_connectors() {
    let source = FakeSource::new(None, Box::new(|_| Ok(page(Vec::new(), false, "c"))));
    let b = bench(store().await, source.clone(), SyncConfig::default());
    let mut connectors = Vec::new();
    for name in ["a", "b", "c"] {
        let owner = garden(&b.store, &b.ids, name).await;
        for _ in 0..2 {
            let id = connector(
                &b.store,
                &b.ids,
                owner,
                ConnectorEndpoint::Vendor,
                &["C1"],
                "xoxb-1",
                b.clock.now(),
            )
            .await;
            connectors.push(id);
        }
    }
    let first = b.run().await;
    assert_eq!(first.len(), 4, "{first:?}");
    assert!(first.iter().all(|(_, end)| *end == RunEnd::Synced));
    for (id, _) in &first {
        let row = b.row(*id).await;
        assert_eq!(row.next_sync_at, pinned().plus(SYNC_EVERY));
        assert_eq!(row.connector.synced_at, Some(pinned()));
        assert_eq!(row.last_error, None);
    }
    // The other two are next; the four synced ones wait their 15 minutes.
    let second = b.run().await;
    assert_eq!(second.len(), 2);
    assert!(
        second
            .iter()
            .all(|(id, _)| !first.iter().any(|(f, _)| f == id))
    );
    assert_eq!(b.run().await.len(), 0);
    b.clock.advance(SYNC_EVERY);
    assert_eq!(b.run().await.len(), 4);
    // Each read got the unsealed token and the 180-day floor of its run.
    let requests = source.requests();
    assert_eq!(requests.len(), 10);
    assert!(
        requests[..6]
            .iter()
            .all(|r| r.oldest == pinned().minus(FARM_RECORD_KEEP))
    );
    assert!(
        source
            .tokens
            .lock()
            .expect("tokens")
            .iter()
            .all(|t| t == "xoxb-1")
    );
}

#[tokio::test]
async fn t_e4_7_unchanged_text_costs_no_embedding_and_a_changed_chunk_is_embedded() {
    let text = Arc::new(Mutex::new("Rings on bed 3, lower leaves.".to_owned()));
    let script_text = text.clone();
    let at = pinned().minus(Duration::from_hours(5));
    let source = FakeSource::new(
        None,
        Box::new(move |_| {
            let t = script_text.lock().expect("text").clone();
            Ok(page(
                vec![
                    item("C1", "1.000100", &t, at),
                    item("C1", "2.000100", "Aphids on bed 1.", at),
                ],
                false,
                "c",
            ))
        }),
    );
    let b = bench(store().await, source, SyncConfig::default());
    let owner = garden(&b.store, &b.ids, "a").await;
    let id = connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &["C1"],
        "xoxb-1",
        b.clock.now(),
    )
    .await;
    b.run().await;
    assert_eq!(b.embedder.texts(), 2);
    assert_eq!(b.count(owner.1).await, 2);

    b.clock.advance(SYNC_EVERY);
    b.run().await;
    assert_eq!(b.embedder.texts(), 2, "unchanged text keeps its vector");

    *text.lock().expect("text") = "Rings on bed 3, now upper leaves too.".into();
    b.clock.advance(SYNC_EVERY);
    b.run().await;
    assert_eq!(b.embedder.texts(), 3, "only the changed chunk");
    assert_eq!(b.count(owner.1).await, 2);
    let hashes: BTreeMap<String, String> = b
        .store
        .record_hashes(id, vec!["C1/1.000100".into()])
        .await
        .expect("hashes")
        .into_iter()
        .map(|(id, h)| (id.to_string(), h))
        .collect();
    assert_eq!(
        hashes.values().next(),
        Some(&chunk::text_hash("Rings on bed 3, now upper leaves too."))
    );
}

#[tokio::test]
async fn a_shorter_document_drops_its_extra_chunks() {
    let long = Arc::new(Mutex::new(
        ["a".repeat(1_500), "b".repeat(1_500)].join("\n\n"),
    ));
    let script_text = long.clone();
    let source = FakeSource::new(
        None,
        Box::new(move |_| {
            let mut doc = item("F1", "doc", &script_text.lock().expect("text"), pinned());
            doc.vendor_id = "D_SPRAY".into();
            doc.kind = FarmRecordKind::Document;
            Ok(page(vec![doc], false, "c"))
        }),
    );
    let b = bench(store().await, source, SyncConfig::default());
    let owner = garden(&b.store, &b.ids, "a").await;
    connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &["F1"],
        "t",
        b.clock.now(),
    )
    .await;
    b.run().await;
    assert_eq!(b.count(owner.1).await, 2);
    *long.lock().expect("text") = "a".repeat(1_500);
    b.clock.advance(SYNC_EVERY);
    b.run().await;
    assert_eq!(b.count(owner.1).await, 1);
    assert_eq!(b.embedder.texts(), 2, "chunk 0 is unchanged");
}

#[tokio::test]
async fn t_e4_9_a_refused_token_stops_the_sync_and_keeps_the_records() {
    let refuse = Arc::new(Mutex::new(false));
    let flag = refuse.clone();
    let source = FakeSource::new(
        None,
        Box::new(move |_| {
            if *flag.lock().expect("flag") {
                Err(FarmSourceError::AuthRefused {
                    vendor: ConnectorKind::Slack,
                })
            } else {
                Ok(page(
                    vec![item("C1", "1.0", "Spray log", pinned())],
                    false,
                    "c",
                ))
            }
        }),
    );
    let b = bench(store().await, source, SyncConfig::default());
    let owner = garden(&b.store, &b.ids, "a").await;
    let id = connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &["C1"],
        "t",
        b.clock.now(),
    )
    .await;
    b.run().await;
    assert_eq!(b.count(owner.1).await, 1);

    *refuse.lock().expect("flag") = true;
    b.clock.advance(SYNC_EVERY);
    let ends = b.run().await;
    assert_eq!(ends, [(id, RunEnd::Failed(SyncErrorCode::AuthRefused))]);
    let row = b.row(id).await;
    assert_eq!(row.connector.status, ConnectorStatus::ReauthNeeded);
    assert_eq!(row.last_error, Some(SyncErrorCode::AuthRefused));
    assert_eq!(b.count(owner.1).await, 1, "records stay searchable");
    b.clock.advance(SYNC_EVERY * 4);
    assert!(b.run().await.is_empty(), "nothing runs until a reconnect");
}

#[tokio::test]
async fn a_token_that_does_not_open_is_refused_too() {
    let source = FakeSource::new(None, Box::new(|_| panic!("never read")));
    let b = bench(store().await, source, SyncConfig::default());
    let owner = garden(&b.store, &b.ids, "a").await;
    let id = connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &["C1"],
        "t",
        b.clock.now(),
    )
    .await;
    let other = ConnectorKey::from_hex(&"ab".repeat(32)).expect("key");
    let sync = FarmSync::new(
        b.store.clone(),
        b.embedder.clone(),
        b.clock.clone(),
        other,
        FarmSources::default().with(
            ConnectorEndpoint::Vendor,
            FakeSource::new(None, Box::new(|_| panic!("never read"))),
        ),
        SyncConfig::default(),
    );
    let ends = sync
        .run_due(&CancellationToken::new())
        .await
        .expect("listed");
    assert_eq!(ends, [(id, RunEnd::Failed(SyncErrorCode::AuthRefused))]);
    assert_eq!(
        b.row(id).await.connector.status,
        ConnectorStatus::ReauthNeeded
    );
}

#[tokio::test]
async fn t_e4_9_the_10001st_record_stops_the_sync_with_too_large() {
    let source = FakeSource::new(
        None,
        Box::new(|_| {
            Ok(page(
                vec![item("C1", "new.1", "One more", pinned())],
                false,
                "c",
            ))
        }),
    );
    let b = bench(store().await, source, SyncConfig::default());
    let owner = garden(&b.store, &b.ids, "a").await;
    let id = connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &["C1"],
        "t",
        b.clock.now(),
    )
    .await;
    // 10,000 records already: written in one statement, not through the sync.
    let mut r = b
        .store
        .db()
        .query(
            "FOR $i IN 0..10000 {
  CREATE type::record('farm_record', [$c, string::concat('C1/', <string> $i), 0]) CONTENT {
    garden: $g, connector: $c, kind: 'message', vendor_id: string::concat('C1/', <string> $i),
    source: 'C1', title: '#c1', link: 'l', text: 't', text_hash: 'h', embedding: [1f, 0f, 0f, 0f],
    embedding_model: 'counting', at: $at };
};",
        )
        .bind(("c", store::ids::rid(&id)))
        .bind(("g", store::ids::rid(&owner.1)))
        .bind(("at", pinned().into_inner()))
        .await
        .expect("query sent");
    assert!(r.take_errors().is_empty());
    assert_eq!(b.count(owner.1).await, MAX_FARM_RECORDS_PER_GARDEN);

    let ends = b.run().await;
    assert_eq!(ends, [(id, RunEnd::Failed(SyncErrorCode::TooLarge))]);
    assert_eq!(b.count(owner.1).await, MAX_FARM_RECORDS_PER_GARDEN);
    assert_eq!(b.embedder.texts(), 0);
    let row = b.row(id).await;
    assert_eq!(row.connector.status, ConnectorStatus::Active);
    assert_eq!(row.last_error, Some(SyncErrorCode::TooLarge));
    b.clock.advance(SYNC_EVERY * 4);
    assert!(
        b.run().await.is_empty(),
        "nothing runs until the sources change"
    );
}

/// A vendor whose read of `C1` coincides with the owner dropping `C1` from the sources.
struct DroppingSource {
    store: Arc<SurrealStore>,
    connector: std::sync::OnceLock<ConnectorId>,
}

#[async_trait]
impl FarmSource for DroppingSource {
    fn kind(&self) -> ConnectorKind {
        ConnectorKind::Slack
    }

    fn reread_window(&self) -> Option<Duration> {
        None
    }

    async fn available_sources(
        &self,
        _token: &VendorToken,
    ) -> Result<Vec<ConnectorSource>, FarmSourceError> {
        Ok(Vec::new())
    }

    async fn read_page(
        &self,
        _token: &VendorToken,
        request: FarmPageRequest,
    ) -> Result<FarmPage, FarmSourceError> {
        let source = request.source.id;
        if source == "C1" {
            let id = self.connector.get().expect("the connector is set");
            self.store
                .db()
                .query("UPDATE $c SET sources = [{ id: 'C2', name: 'c2' }];")
                .bind(("c", store::ids::rid(id)))
                .await
                .expect("sources updated")
                .check()
                .expect("sources updated");
        }
        Ok(page(
            vec![item(&source, "1.0", "a message", pinned())],
            false,
            "c",
        ))
    }

    async fn revoke(&self, _token: &VendorToken) -> Result<(), FarmSourceError> {
        Ok(())
    }
}

#[tokio::test]
async fn a_page_of_a_source_dropped_meanwhile_does_not_count_toward_the_limit() {
    let store = store().await;
    let source = Arc::new(DroppingSource {
        store: store.clone(),
        connector: std::sync::OnceLock::new(),
    });
    let config = SyncConfig {
        max_records: 1,
        ..SyncConfig::default()
    };
    let b = bench(store, source.clone(), config);
    let owner = garden(&b.store, &b.ids, "a").await;
    let id = connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &["C1", "C2"],
        "t",
        b.clock.now(),
    )
    .await;
    source.connector.set(id).expect("set once");
    // C1's record is skipped by the store; counted anyway, C2's would pass the limit of 1.
    assert_eq!(b.run().await, [(id, RunEnd::Synced)]);
    assert_eq!(b.count(owner.1).await, 1);
}

/// A vendor whose second page of `C1` comes while another connector of the garden stores
/// records, as one syncing side by side in `run_due` would.
struct RacingSource {
    store: Arc<SurrealStore>,
    other: std::sync::OnceLock<ConnectorId>,
    calls: AtomicUsize,
}

#[async_trait]
impl FarmSource for RacingSource {
    fn kind(&self) -> ConnectorKind {
        ConnectorKind::Slack
    }

    fn reread_window(&self) -> Option<Duration> {
        None
    }

    async fn available_sources(
        &self,
        _token: &VendorToken,
    ) -> Result<Vec<ConnectorSource>, FarmSourceError> {
        Ok(Vec::new())
    }

    async fn read_page(
        &self,
        _token: &VendorToken,
        _request: FarmPageRequest,
    ) -> Result<FarmPage, FarmSourceError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            return Ok(page(
                vec![
                    item("C1", "1.0", "one", pinned()),
                    item("C1", "2.0", "two", pinned()),
                ],
                true,
                "p1",
            ));
        }
        let other = *self.other.get().expect("the other connector is set");
        let records = (0..3)
            .map(|i| NewFarmRecord {
                id: FarmRecordId::new(other, &format!("D1/{i}.0"), 0).expect("id"),
                kind: FarmRecordKind::Message,
                source: "D1".into(),
                title: "#d1".into(),
                author: None,
                link: "l".into(),
                text: "x".into(),
                text_hash: chunk::text_hash("x"),
                vector: Some(FarmVector {
                    embedding: Embedding(vec![1.0, 0.0, 0.0, 0.0]),
                    model: "counting".into(),
                }),
                at: pinned(),
            })
            .collect();
        self.store
            .upsert_records(other, records)
            .await
            .expect("the other connector's records");
        Ok(page(
            vec![item("C1", "3.0", "three", pinned())],
            false,
            "done",
        ))
    }

    async fn revoke(&self, _token: &VendorToken) -> Result<(), FarmSourceError> {
        Ok(())
    }
}

#[tokio::test]
async fn the_limit_counts_what_another_connector_stored_during_the_run() {
    let store = store().await;
    let source = Arc::new(RacingSource {
        store: store.clone(),
        other: std::sync::OnceLock::new(),
        calls: AtomicUsize::new(0),
    });
    let config = SyncConfig {
        max_records: 5,
        ..SyncConfig::default()
    };
    let b = bench(store, source.clone(), config);
    let owner = garden(&b.store, &b.ids, "a").await;
    let id = connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &["C1"],
        "t",
        b.clock.now(),
    )
    .await;
    let other = connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Era,
        &["D1"],
        "t",
        b.clock.now(),
    )
    .await;
    source.other.set(other).expect("set once");
    // Page 1 stores 2 (2 of 5); the other connector then stores 3 (5 of 5), so page 2's one
    // record would pass the limit. Counted once at the run's start, the garden would hold 6.
    let end = b
        .sync
        .sync_connector(b.row(id).await, &CancellationToken::new())
        .await;
    assert_eq!(end, RunEnd::Failed(SyncErrorCode::TooLarge));
    assert_eq!(b.count(owner.1).await, 5);
}

#[tokio::test]
async fn t_e4_9_cleanup_deletes_old_slack_messages_and_keeps_old_documents() {
    let b = bench(
        store().await,
        FakeSource::new(None, Box::new(|_| Ok(page(Vec::new(), false, "c")))),
        SyncConfig::default(),
    );
    let owner = garden(&b.store, &b.ids, "a").await;
    let id = connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &["C1"],
        "t",
        b.clock.now(),
    )
    .await;
    let old = pinned().minus(FARM_RECORD_KEEP + Duration::from_hours(24));
    let record = |vendor: &str, kind: FarmRecordKind, at: Timestamp| NewFarmRecord {
        id: FarmRecordId::new(id, vendor, 0).expect("id"),
        kind,
        source: "C1".into(),
        title: "t".into(),
        author: None,
        link: "l".into(),
        text: "x".into(),
        text_hash: chunk::text_hash("x"),
        vector: Some(FarmVector {
            embedding: Embedding(vec![1.0, 0.0, 0.0, 0.0]),
            model: "counting".into(),
        }),
        at,
    };
    b.store
        .upsert_records(
            id,
            vec![
                record("C1/old", FarmRecordKind::Message, old),
                record("C1/new", FarmRecordKind::Message, pinned()),
                record("F_OLD_DOC", FarmRecordKind::Document, old),
            ],
        )
        .await
        .expect("records");
    assert_eq!(
        delete_old_messages(b.store.as_ref(), pinned())
            .await
            .expect("cleanup"),
        1
    );
    let left: Vec<String> = b
        .store
        .record_hashes(
            id,
            vec!["C1/old".into(), "C1/new".into(), "F_OLD_DOC".into()],
        )
        .await
        .expect("hashes")
        .into_iter()
        .map(|(r, _)| r.vendor_id.to_string())
        .collect();
    assert_eq!(left.len(), 2);
    assert!(left.contains(&"C1/new".to_owned()) && left.contains(&"F_OLD_DOC".to_owned()));
}

#[tokio::test]
async fn a_throttled_pass_saves_its_cursor_and_resumes_on_the_next_run() {
    let calls = Arc::new(AtomicUsize::new(0));
    let n = calls.clone();
    let source = FakeSource::new(
        Some(Duration::from_hours(7 * 24)),
        Box::new(move |r| match n.fetch_add(1, Ordering::SeqCst) {
            0 => {
                assert_eq!(r.cursor, None);
                Ok(page(vec![item("C1", "3.0", "three", pinned())], true, "p1"))
            }
            1 => Err(FarmSourceError::RateLimited {
                vendor: ConnectorKind::Slack,
                retry_after_s: Some(90),
            }),
            _ => {
                assert_eq!(r.cursor, Some(SyncCursor("p1".into())), "resumes at page 2");
                assert_eq!(r.reread_from, None, "a first pass is not a re-read");
                Ok(page(
                    vec![item("C1", "2.0", "two", pinned())],
                    false,
                    "done",
                ))
            }
        }),
    );
    let b = bench(store().await, source, SyncConfig::default());
    let owner = garden(&b.store, &b.ids, "a").await;
    let id = connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &["C1"],
        "t",
        b.clock.now(),
    )
    .await;
    let ends = b.run().await;
    assert_eq!(ends, [(id, RunEnd::Failed(SyncErrorCode::RateLimited))]);
    let row = b.row(id).await;
    assert_eq!(row.next_sync_at, pinned().plus(Duration::from_secs(90)));
    assert_eq!(row.connector.synced_at, None);
    assert_eq!(b.count(owner.1).await, 1);

    b.clock.advance(Duration::from_secs(90));
    assert_eq!(b.run().await, [(id, RunEnd::Synced)]);
    assert_eq!(b.count(owner.1).await, 2);
    assert_eq!(b.row(id).await.last_error, None);
}

#[tokio::test]
async fn the_daily_reread_deletes_what_the_vendor_no_longer_has() {
    let gone = Arc::new(Mutex::new(false));
    let flag = gone.clone();
    let recent = pinned().minus(Duration::from_hours(48));
    let source = FakeSource::new(
        Some(Duration::from_hours(7 * 24)),
        Box::new(move |r| {
            let all = [
                item("C1", "1.0", "kept", recent),
                item("C1", "2.0", "deleted at the vendor", recent),
                item("C1", "3.0", "a thread", recent),
            ];
            if r.reread_from.is_none() {
                // A later pass with nothing new.
                let items = if r.cursor.is_none() {
                    all.to_vec()
                } else {
                    Vec::new()
                };
                return Ok(page(items, false, "c"));
            }
            let mut p = page(vec![all[0].clone()], false, "c");
            if !*flag.lock().expect("flag") {
                p.items.push(all[1].clone());
            }
            // The thread is unchanged: kept, not sent.
            p.kept.push(all[2].vendor_id.clone());
            Ok(p)
        }),
    );
    let b = bench(store().await, source.clone(), SyncConfig::default());
    let owner = garden(&b.store, &b.ids, "a").await;
    connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &["C1"],
        "t",
        b.clock.now(),
    )
    .await;
    b.run().await;
    assert_eq!(b.count(owner.1).await, 3);

    // Within the day: a plain later pass.
    b.clock.advance(SYNC_EVERY);
    b.run().await;
    assert!(source.requests().iter().all(|r| r.reread_from.is_none()));

    *gone.lock().expect("flag") = true;
    b.clock.advance(Duration::from_hours(24));
    b.run().await;
    let reread = source.requests().pop().expect("a request");
    assert_eq!(
        reread.reread_from,
        Some(b.clock.now().minus(Duration::from_hours(7 * 24)))
    );
    assert_eq!(
        b.count(owner.1).await,
        2,
        "the gone message only; the kept thread stays"
    );

    // The next re-read is a day later, not on the next run.
    b.clock.advance(SYNC_EVERY);
    b.run().await;
    assert_eq!(
        source.requests().pop().expect("a request").reread_from,
        None
    );
}

#[tokio::test]
async fn a_vendor_without_a_reread_never_deletes_by_absence() {
    let first = Arc::new(AtomicUsize::new(0));
    let n = first.clone();
    let source = FakeSource::new(
        None,
        Box::new(move |r| {
            assert_eq!(r.reread_from, None);
            let items = if n.fetch_add(1, Ordering::SeqCst) == 0 {
                vec![item("F1", "doc", "Spray log", pinned())]
            } else {
                Vec::new()
            };
            Ok(page(items, false, "token"))
        }),
    );
    let b = bench(store().await, source, SyncConfig::default());
    let owner = garden(&b.store, &b.ids, "a").await;
    connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &["F1"],
        "t",
        b.clock.now(),
    )
    .await;
    b.run().await;
    b.clock.advance(Duration::from_hours(48));
    b.run().await;
    assert_eq!(b.count(owner.1).await, 1);
}

#[tokio::test]
async fn a_spent_budget_leaves_the_connector_due() {
    let source = FakeSource::new(None, Box::new(|_| panic!("never read")));
    let config = SyncConfig {
        run_budget: Duration::ZERO,
        ..SyncConfig::default()
    };
    let b = bench(store().await, source, config);
    let owner = garden(&b.store, &b.ids, "a").await;
    let id = connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &["C1"],
        "t",
        b.clock.now(),
    )
    .await;
    assert_eq!(b.run().await, [(id, RunEnd::PagesLeft)]);
    let row = b.row(id).await;
    assert_eq!(row.next_sync_at, pinned(), "the next run continues");
    assert_eq!(row.connector.synced_at, None);
}

#[tokio::test]
async fn a_spent_budget_moves_on_to_the_next_sources_on_the_next_run() {
    // Ten quiet channels, each page 100 ms (a paced vendor), and a budget for three a run.
    let source = FakeSource::new(
        None,
        Box::new(|_| {
            std::thread::sleep(Duration::from_millis(100));
            Ok(page(Vec::new(), false, "c"))
        }),
    );
    let config = SyncConfig {
        run_budget: Duration::from_millis(280),
        ..SyncConfig::default()
    };
    let b = bench(store().await, source.clone(), config);
    let owner = garden(&b.store, &b.ids, "a").await;
    let channels: Vec<String> = (0..10).map(|i| format!("C{i}")).collect();
    let names: Vec<&str> = channels.iter().map(String::as_str).collect();
    let id = connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &names,
        "t",
        b.clock.now(),
    )
    .await;

    // Every channel has its turn within four runs, each once and in order; the round then
    // ends synced.
    let runs = b.until_synced(id, 4).await;
    assert!(runs >= 3, "a run read more than the budget: {runs} runs");
    assert_eq!(
        reads(&source, 0),
        channels,
        "each channel once, in order, none starved"
    );
    let row = b.row(id).await;
    assert_eq!(row.connector.synced_at, Some(pinned()));
    assert_eq!(row.next_sync_at, pinned().plus(SYNC_EVERY));

    // The next round starts one channel further on than where this one stopped (after C9),
    // and again reads each channel once.
    let before = source.requests().len();
    b.clock.advance(SYNC_EVERY);
    b.until_synced(id, 4).await;
    let next = reads(&source, before);
    assert_eq!(next.first().map(String::as_str), Some("C1"));
    let mut sorted = next.clone();
    sorted.sort();
    assert_eq!(sorted, channels, "each channel once: {next:?}");
}

/// A vendor that uses `uses` of each page's older-item checks, as far as the limit allows.
fn probing(uses: Arc<Mutex<Vec<u32>>>) -> Arc<FakeSource> {
    FakeSource::new(
        None,
        Box::new(move |r| {
            let want = uses.lock().expect("uses").pop().unwrap_or(0);
            let mut p = page(Vec::new(), false, "c");
            p.probes = want.min(r.probe_limit);
            Ok(p)
        }),
    )
}

#[tokio::test]
async fn older_item_checks_are_two_a_run_over_every_source_and_none_on_a_first_pass() {
    let uses = Arc::new(Mutex::new(Vec::new()));
    let source = probing(uses.clone());
    // A budget that covers a page and two checks on every page.
    let config = SyncConfig {
        run_budget: Duration::from_mins(10),
        ..SyncConfig::default()
    };
    let b = bench(store().await, source.clone(), config);
    let owner = garden(&b.store, &b.ids, "a").await;
    let id = connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &["C1", "C2", "C3"],
        "t",
        b.clock.now(),
    )
    .await;
    let limits = |from: usize| -> Vec<(String, u32)> {
        source.requests()[from..]
            .iter()
            .map(|r| (r.source.id.clone(), r.probe_limit))
            .collect()
    };
    let pair = |s: &str, n: u32| (s.to_owned(), n);

    // The first pass reads every thread in full: no checks.
    assert_eq!(b.run().await, [(id, RunEnd::Synced)]);
    assert_eq!(limits(0), [pair("C1", 0), pair("C2", 0), pair("C3", 0)]);

    // Round 2 starts at C2, which uses both checks: none left for the others.
    *uses.lock().expect("uses") = vec![2, 2, 2];
    b.clock.advance(SYNC_EVERY);
    let before = source.requests().len();
    b.run().await;
    assert_eq!(
        limits(before),
        [pair("C2", 2), pair("C3", 0), pair("C1", 0)]
    );

    // Round 3 starts at C3, which uses one; C1 gets the other.
    *uses.lock().expect("uses") = vec![2, 2, 1];
    b.clock.advance(SYNC_EVERY);
    let before = source.requests().len();
    b.run().await;
    assert_eq!(
        limits(before),
        [pair("C3", 2), pair("C1", 1), pair("C2", 0)]
    );
}

#[tokio::test]
async fn no_older_item_check_is_offered_at_the_end_of_the_budget() {
    let source = probing(Arc::default());
    let config = SyncConfig {
        run_budget: Duration::from_secs(130),
        ..SyncConfig::default()
    };
    let b = bench(store().await, source.clone(), config);
    let owner = garden(&b.store, &b.ids, "a").await;
    connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &["C1"],
        "t",
        b.clock.now(),
    )
    .await;
    b.run().await;
    b.clock.advance(SYNC_EVERY);
    b.run().await;
    let requests = source.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|r| r.probe_limit == 0), "{requests:?}");
}

impl Bench {
    /// The connector's sources become `sources`, due now, as `PUT sources` leaves them.
    async fn choose(&self, id: ConnectorId, sources: &[&str]) {
        let sources: Vec<ConnectorSource> = sources
            .iter()
            .map(|s| ConnectorSource {
                id: (*s).to_owned(),
                name: s.to_lowercase(),
            })
            .collect();
        self.store
            .db()
            .query("UPDATE $c SET sources = $sources, next_sync_at = $now;")
            .bind(("c", store::ids::rid(&id)))
            .bind((
                "sources",
                serde_json::to_value(&sources).expect("sources serialise"),
            ))
            .bind(("now", self.clock.now().into_inner()))
            .await
            .expect("sources updated")
            .check()
            .expect("sources updated");
    }

    /// Runs until the round ends, at most `runs` times; every run before it has pages left.
    async fn until_synced(&self, id: ConnectorId, runs: usize) -> usize {
        for n in 1..=runs {
            let end = self.run().await;
            assert_eq!(end.len(), 1, "due again at once until the round ends");
            if end[0].1 == RunEnd::Synced {
                return n;
            }
            assert_eq!(end[0], (id, RunEnd::PagesLeft));
        }
        panic!("the round didn't end in {runs} runs");
    }
}

/// The sources of the requests from `from` on.
fn reads(source: &FakeSource, from: usize) -> Vec<String> {
    source.requests()[from..]
        .iter()
        .map(|r| r.source.id.clone())
        .collect()
}

/// A vendor whose every page takes 100 ms (a paced vendor; with a 250 ms budget a run reads at
/// most three pages) and whose sources have the passes `pages` gives, by source id and pass
/// number: the pages of that pass. Each page's cursor is `<source>:<pass>:<page>`, and each
/// request must carry the cursor of the source's last page.
fn paced(pages: impl Fn(&str, u32) -> u32 + Send + 'static) -> Arc<FakeSource> {
    let mut at: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    FakeSource::new(
        None,
        Box::new(move |r| {
            std::thread::sleep(Duration::from_millis(100));
            let id = r.source.id.clone();
            let (pass, page_no) = at.entry(id.clone()).or_insert((0, 0));
            let expected = (*pass > 0 || *page_no > 0).then(|| {
                let (p, n) = if *page_no == 0 {
                    (*pass - 1, pages(&id, *pass - 1))
                } else {
                    (*pass, *page_no)
                };
                SyncCursor(format!("{id}:{p}:{n}"))
            });
            assert_eq!(r.cursor, expected, "{id} resumes from its last page");
            *page_no += 1;
            let cursor = format!("{id}:{pass}:{page_no}");
            let more = *page_no < pages(&id, *pass);
            if !more {
                *pass += 1;
                *page_no = 0;
            }
            Ok(page(Vec::new(), more, &cursor))
        }),
    )
}

fn paced_budget() -> SyncConfig {
    SyncConfig {
        run_budget: Duration::from_millis(250),
        ..SyncConfig::default()
    }
}

#[tokio::test]
async fn a_source_added_later_joins_the_others_in_the_next_round() {
    let source = FakeSource::new(None, Box::new(|_| Ok(page(Vec::new(), false, "c"))));
    let b = bench(store().await, source.clone(), SyncConfig::default());
    let owner = garden(&b.store, &b.ids, "a").await;
    let id = connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &["C1", "C2"],
        "t",
        b.clock.now(),
    )
    .await;
    for _ in 0..3 {
        assert_eq!(b.run().await, [(id, RunEnd::Synced)]);
        b.clock.advance(SYNC_EVERY);
    }
    // A third channel joins: due now, with no cursor.
    b.choose(id, &["C1", "C2", "C3"]).await;
    let before = source.requests().len();
    assert_eq!(b.run().await, [(id, RunEnd::Synced)]);
    let mut read = reads(&source, before);
    read.sort();
    assert_eq!(read, ["C1", "C2", "C3"], "read with the others, not alone");
}

impl Bench {
    /// The round in progress, as the sync stored it (the first before the first run).
    async fn round(&self, id: ConnectorId) -> u64 {
        self.row(id)
            .await
            .cursors
            .get("#sync")
            .and_then(|c| serde_json::from_str::<serde_json::Value>(&c.0).ok())
            .and_then(|v| v["round"].as_u64())
            .unwrap_or(1)
    }

    /// One run, 15 minutes after the last (due whatever the last run's end), and the sources
    /// it read; `rounds` gets them, and a new entry when the run ended the round.
    async fn run_into(
        &self,
        id: ConnectorId,
        source: &FakeSource,
        rounds: &mut Vec<Vec<String>>,
    ) -> RunEnd {
        self.clock.advance(SYNC_EVERY);
        let round = self.round(id).await;
        let before = source.requests().len();
        let end = self.run().await;
        assert_eq!(end.len(), 1, "due");
        let read = reads(source, before);
        rounds.last_mut().expect("a round").extend(read);
        if self.round(id).await > round {
            rounds.push(Vec::new());
        }
        end[0].1
    }
}

/// A finished, then the budget ended with B mid-pass, then C was added: no run reports the
/// connector synced while B has pages left.
#[tokio::test]
async fn a_round_never_ends_synced_with_a_source_mid_pass() {
    // B's second pass is ten pages; every other pass is one.
    let source = paced(|id, pass| if id == "B" && pass == 1 { 10 } else { 1 });
    let b = bench(store().await, source.clone(), paced_budget());
    let owner = garden(&b.store, &b.ids, "a").await;
    let id = connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &["A", "B"],
        "t",
        b.clock.now(),
    )
    .await;
    b.until_synced(id, 3).await;
    let first = b.row(id).await.connector.synced_at;
    assert_eq!(first, Some(pinned()));
    let b_reads = |source: &FakeSource| reads(source, 0).iter().filter(|s| *s == "B").count();

    // Until A has been read in round 2 and B is mid-pass.
    let before = source.requests().len();
    let mut rounds = vec![Vec::new()];
    let mut runs = 0;
    while !reads(&source, before).contains(&"A".to_owned()) {
        runs += 1;
        assert!(runs < 5, "A isn't read: {rounds:?}");
        let end = b.run_into(id, &source, &mut rounds).await;
        assert_ne!(end, RunEnd::Synced, "B is mid-pass");
    }
    assert!(b_reads(&source) < 11, "B is mid-pass: {rounds:?}");

    b.choose(id, &["A", "B", "C"]).await;
    let before = source.requests().len();
    loop {
        runs += 1;
        assert!(runs < 20, "never synced: {rounds:?}");
        let end = b.run_into(id, &source, &mut rounds).await;
        let synced_at = b.row(id).await.connector.synced_at;
        if b_reads(&source) < 11 {
            assert_ne!(end, RunEnd::Synced, "B is mid-pass: {rounds:?}");
            assert_eq!(synced_at, first, "B is mid-pass: {rounds:?}");
        } else if end == RunEnd::Synced {
            assert_eq!(synced_at, Some(b.clock.now()));
            break;
        }
    }
    assert!(reads(&source, before).contains(&"C".to_owned()));
}

#[tokio::test]
async fn a_new_source_with_a_long_first_pass_does_not_hold_up_the_others() {
    // N's first pass is 1000 pages, far longer than the test, until the test shortens it.
    let length = Arc::new(AtomicUsize::new(1000));
    let n_length = length.clone();
    let source = paced(move |id, pass| {
        if id == "N" && pass == 0 {
            u32::try_from(n_length.load(Ordering::SeqCst)).expect("a page count")
        } else {
            1
        }
    });
    let b = bench(store().await, source.clone(), paced_budget());
    let owner = garden(&b.store, &b.ids, "a").await;
    let channels: Vec<String> = (0..9).map(|i| format!("C{i}")).collect();
    let names: Vec<&str> = channels.iter().map(String::as_str).collect();
    let id = connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &names,
        "t",
        b.clock.now(),
    )
    .await;
    b.until_synced(id, 9).await;
    let first = b.row(id).await.connector.synced_at;

    // N joins first in the order. Each round reads every other channel once, and some of N.
    let mut chosen = vec!["N"];
    chosen.extend(&names);
    b.choose(id, &chosen).await;
    let n_reads = |source: &FakeSource| reads(source, 0).iter().filter(|s| *s == "N").count();
    let mut rounds = vec![Vec::new()];
    let mut runs = 0;
    while rounds.len() <= 3 {
        runs += 1;
        assert!(runs < 30, "the rounds stall: {rounds:?}");
        let end = b.run_into(id, &source, &mut rounds).await;
        assert_ne!(end, RunEnd::Synced, "N has pages left");
        assert_eq!(b.row(id).await.connector.synced_at, first);
    }
    rounds.pop();
    for round in &rounds {
        for c in &channels {
            assert_eq!(
                round.iter().filter(|s| *s == c).count(),
                1,
                "{c} once a round: {round:?}"
            );
        }
        assert!(round.contains(&"N".to_owned()), "N advances: {round:?}");
    }

    // N's pass ends at its next page; the round it ends in is synced.
    length.store(n_reads(&source) + 1, Ordering::SeqCst);
    loop {
        runs += 1;
        assert!(runs < 45, "never synced: {rounds:?}");
        if b.run_into(id, &source, &mut rounds).await == RunEnd::Synced {
            break;
        }
    }
    assert_eq!(b.row(id).await.connector.synced_at, Some(b.clock.now()));
}

#[tokio::test]
async fn a_source_that_keeps_failing_does_not_stop_the_others() {
    let broken = Arc::new(Mutex::new(true));
    let fails = broken.clone();
    let source = FakeSource::new(
        None,
        Box::new(move |r| {
            if r.source.id == "C2" && *fails.lock().expect("flag") {
                return Err(FarmSourceError::Unavailable {
                    vendor: ConnectorKind::Slack,
                    source: "a channel the vendor can't serve".into(),
                });
            }
            Ok(page(Vec::new(), false, "c"))
        }),
    );
    let b = bench(store().await, source.clone(), SyncConfig::default());
    let owner = garden(&b.store, &b.ids, "a").await;
    let id = connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &["C1", "C2", "C3"],
        "t",
        b.clock.now(),
    )
    .await;

    // Three rounds: every one reads C1 and C3, and C2 fails in every one.
    let mut rounds = vec![Vec::new()];
    let mut runs = 0;
    while rounds.len() <= 3 {
        runs += 1;
        assert!(runs < 12, "the rounds stall: {rounds:?}");
        let end = b.run_into(id, &source, &mut rounds).await;
        assert_ne!(end, RunEnd::Synced);
        let row = b.row(id).await;
        assert_eq!(
            row.last_error,
            Some(SyncErrorCode::VendorUnavailable),
            "reported after run {runs}: {rounds:?}"
        );
        assert_eq!(row.connector.synced_at, None, "C2 never synced");
    }
    rounds.pop();
    for round in &rounds {
        for c in ["C1", "C2", "C3"] {
            assert!(round.contains(&c.to_owned()), "{c} read: {round:?}");
        }
    }

    // Once C2 answers, a round ends synced and the error clears.
    *broken.lock().expect("flag") = false;
    for n in 0.. {
        assert!(n < 4, "never synced: {rounds:?}");
        if b.run_into(id, &source, &mut rounds).await == RunEnd::Synced {
            break;
        }
    }
    let row = b.row(id).await;
    assert_eq!(row.last_error, None);
    assert_eq!(row.connector.synced_at, Some(b.clock.now()));
}

#[tokio::test]
async fn a_connector_without_sources_reads_nothing_and_waits_15_minutes() {
    let source = FakeSource::new(None, Box::new(|_| panic!("never read")));
    let b = bench(store().await, source.clone(), SyncConfig::default());
    let owner = garden(&b.store, &b.ids, "a").await;
    let id = connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &[],
        "t",
        b.clock.now(),
    )
    .await;
    assert_eq!(b.run().await, [(id, RunEnd::NoSources)]);
    let row = b.row(id).await;
    assert_eq!(row.connector.synced_at, None);
    assert_eq!(row.next_sync_at, pinned().plus(SYNC_EVERY));
    assert_eq!(source.requests(), Vec::<FarmPageRequest>::new());
}

#[tokio::test]
async fn a_source_left_mid_pass_resumes_from_its_cursor() {
    // A's first pass is five pages; `paced` checks each request carries the last cursor.
    let source = paced(|id, pass| if id == "A" && pass == 0 { 5 } else { 1 });
    let b = bench(store().await, source.clone(), paced_budget());
    let owner = garden(&b.store, &b.ids, "a").await;
    let id = connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &["A", "B"],
        "t",
        b.clock.now(),
    )
    .await;
    let runs = b.until_synced(id, 6).await;
    assert!(runs >= 2, "five pages take more than one run");
    let cursors: Vec<Option<String>> = source
        .requests()
        .iter()
        .filter(|r| r.source.id == "A")
        .map(|r| r.cursor.clone().map(|c| c.0))
        .collect();
    assert_eq!(
        cursors,
        [
            None,
            Some("A:0:1".into()),
            Some("A:0:2".into()),
            Some("A:0:3".into()),
            Some("A:0:4".into()),
        ]
    );
    assert_eq!(reads(&source, 0).iter().filter(|s| *s == "B").count(), 1);
}

#[tokio::test]
async fn an_outage_retries_15_minutes_later() {
    let source = FakeSource::new(
        None,
        Box::new(|_| {
            Err(FarmSourceError::Unavailable {
                vendor: ConnectorKind::Slack,
                source: "down".into(),
            })
        }),
    );
    let b = bench(store().await, source, SyncConfig::default());
    let owner = garden(&b.store, &b.ids, "a").await;
    let id = connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &["C1"],
        "t",
        b.clock.now(),
    )
    .await;
    let ends = b.run().await;
    assert_eq!(
        ends,
        [(id, RunEnd::Failed(SyncErrorCode::VendorUnavailable))]
    );
    let row = b.row(id).await;
    assert_eq!(row.next_sync_at, pinned().plus(SYNC_EVERY));
    assert_eq!(row.last_error, Some(SyncErrorCode::VendorUnavailable));
    assert_eq!(row.connector.status, ConnectorStatus::Active);
    assert_eq!(b.run().await.len(), 0);
    b.clock.advance(SYNC_EVERY);
    assert_eq!(b.run().await.len(), 1);
}

#[tokio::test]
async fn a_connector_without_an_adapter_waits_15_minutes() {
    let b = bench(
        store().await,
        FakeSource::new(None, Box::new(|_| panic!("never read"))),
        SyncConfig::default(),
    );
    let owner = garden(&b.store, &b.ids, "a").await;
    let id = connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Era,
        &["C1"],
        "t",
        b.clock.now(),
    )
    .await;
    assert_eq!(b.run().await, [(id, RunEnd::NoAdapter)]);
    assert_eq!(b.row(id).await.next_sync_at, pinned().plus(SYNC_EVERY));
}

#[tokio::test]
async fn t_e4_10_seeding_twice_gives_one_pair_of_sealed_era_connectors() {
    let store = store().await;
    let ids = SequentialIds::default();
    let (user, garden) = garden(&store, &ids, "maple").await;
    let clock = FixedClock::at(pinned());
    let seed = EraSeed {
        slack_base: "http://era-slack:8080".into(),
        garden,
    };
    for _ in 0..2 {
        assert_eq!(
            seed::seed_era_connectors(store.as_ref(), &key(), &clock, &seed)
                .await
                .expect("seeded"),
            Seeded::Done
        );
    }
    let viewer = check_membership_of(store.as_ref(), user, garden)
        .await
        .expect("owner");
    let listed = store.connectors(viewer).await.expect("list");
    assert_eq!(listed.len(), 2, "{listed:?}");
    for (id, kind, token) in [
        (
            seed::ERA_SLACK_CONNECTOR,
            ConnectorKind::Slack,
            seed::ERA_SLACK_TOKEN,
        ),
        (
            seed::ERA_DRIVE_CONNECTOR,
            ConnectorKind::GoogleDrive,
            seed::ERA_DRIVE_TOKEN,
        ),
    ] {
        let id: ConnectorId = id.parse().expect("id");
        let row = store
            .connector_row(id)
            .await
            .expect("read")
            .expect("seeded");
        assert_eq!(row.connector.kind, kind);
        assert_eq!(row.connector.endpoint, ConnectorEndpoint::Era);
        assert_eq!(row.connector.status, ConnectorStatus::Active);
        assert_eq!(row.created_by, user);
        assert_eq!(row.next_sync_at, pinned(), "due at once");
        let sealed = row.secret.expect("sealed");
        assert!(!sealed.0.windows(token.len()).any(|w| w == token.as_bytes()));
        let opened = key()
            .unseal(SealPurpose::Token, id, &sealed)
            .expect("opens under the key");
        assert_eq!(opened.expose_secret(), token.as_bytes());
    }
    let slack = store
        .connector_row(seed::ERA_SLACK_CONNECTOR.parse().expect("id"))
        .await
        .expect("read")
        .expect("seeded");
    assert_eq!(
        slack.connector.account,
        Some(ConnectorAccount::Slack {
            team_id: "T_MAPLE".into(),
            team_name: "Maple Row Farm".into(),
            url: "http://era-slack:8080/".into(),
        })
    );
    assert_eq!(
        slack
            .connector
            .sources
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        ["C_FIELD", "C_CREW"]
    );
}

#[tokio::test]
async fn seeding_skips_a_demo_garden_that_does_not_exist_yet() {
    let store = store().await;
    let ids = SequentialIds::default();
    let seed = EraSeed {
        slack_base: "http://era-slack:8080".into(),
        garden: ids.id(),
    };
    let clock = FixedClock::at(pinned());
    assert_eq!(
        seed::seed_era_connectors(store.as_ref(), &key(), &clock, &seed)
            .await
            .expect("no error"),
        Seeded::NoGarden
    );
    assert!(
        store
            .connector_row(seed::ERA_SLACK_CONNECTOR.parse().expect("id"))
            .await
            .expect("read")
            .is_none()
    );
}

#[test]
fn retry_after_is_kept_between_a_minute_and_an_hour() {
    use super::retry_wait;
    let secs = |s| retry_wait(s).as_secs();
    assert_eq!(secs(None), 60, "a missing value waits a minute");
    assert_eq!(secs(Some(0)), 60);
    assert_eq!(secs(Some(1)), 60);
    assert_eq!(secs(Some(60)), 60);
    assert_eq!(secs(Some(600)), 600);
    assert_eq!(secs(Some(3600)), 3600);
    assert_eq!(secs(Some(86_400)), 3600, "a day is cut to an hour");
    assert_eq!(secs(Some(u32::MAX)), 3600);
}

#[test]
fn a_page_is_offered_only_the_checks_its_budget_covers() {
    use super::probes_offered;
    let offered = |secs: u64, left: u32| probes_offered(Duration::from_secs(secs), left);
    // 70 s for the page's own call, then 70 s for each check.
    assert_eq!(offered(0, 2), 0);
    assert_eq!(offered(139, 2), 0, "the page and one check would overrun");
    assert_eq!(offered(140, 2), 1);
    assert_eq!(offered(180, 2), 1, "a 3-minute run's first page");
    assert_eq!(offered(209, 2), 1);
    assert_eq!(offered(210, 2), 2);
    assert_eq!(offered(600, 2), 2, "never more than the run has left");
    assert_eq!(offered(600, 1), 1);
    assert_eq!(offered(600, 0), 0);
}

/// A vendor whose page never comes, counting the calls.
struct HangingSource(AtomicUsize);

#[async_trait]
impl FarmSource for HangingSource {
    fn kind(&self) -> ConnectorKind {
        ConnectorKind::Slack
    }

    fn reread_window(&self) -> Option<Duration> {
        None
    }

    async fn available_sources(
        &self,
        _token: &VendorToken,
    ) -> Result<Vec<ConnectorSource>, FarmSourceError> {
        Ok(Vec::new())
    }

    async fn read_page(
        &self,
        _token: &VendorToken,
        _request: FarmPageRequest,
    ) -> Result<FarmPage, FarmSourceError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        std::future::pending().await
    }

    async fn revoke(&self, _token: &VendorToken) -> Result<(), FarmSourceError> {
        Ok(())
    }
}

#[tokio::test]
async fn shutdown_interrupts_a_page_in_flight_and_leaves_the_connector_due() {
    let source = Arc::new(HangingSource(AtomicUsize::new(0)));
    let b = bench(store().await, source.clone(), SyncConfig::default());
    let owner = garden(&b.store, &b.ids, "a").await;
    let id = connector(
        &b.store,
        &b.ids,
        owner,
        ConnectorEndpoint::Vendor,
        &["C1"],
        "t",
        b.clock.now(),
    )
    .await;
    let shutdown = CancellationToken::new();
    let stopper = shutdown.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        stopper.cancel();
    });
    let ends = tokio::time::timeout(Duration::from_secs(10), b.sync.run_due(&shutdown))
        .await
        .expect("the run ends with shutdown, not with the page")
        .expect("due connectors listed");
    assert_eq!(ends, [(id, RunEnd::PagesLeft)]);
    assert_eq!(source.0.load(Ordering::SeqCst), 1);
    let row = b.row(id).await;
    assert_eq!(row.next_sync_at, pinned(), "still due on the next start");
    assert_eq!(b.count(owner.1).await, 0);
}

#[tokio::test]
async fn seeding_repairs_the_connectors_under_the_current_key() {
    let store = store().await;
    let ids = SequentialIds::default();
    let (_, garden) = garden(&store, &ids, "maple").await;
    let clock = FixedClock::at(pinned());
    let seed = EraSeed {
        slack_base: "http://era-slack:8080".into(),
        garden,
    };
    let id: ConnectorId = seed::ERA_SLACK_CONNECTOR.parse().expect("id");
    seed::seed_era_connectors(store.as_ref(), &key(), &clock, &seed)
        .await
        .expect("seeded");
    store
        .set_status(
            id,
            ConnectorStatus::ReauthNeeded,
            Some(SyncErrorCode::AuthRefused),
        )
        .await
        .expect("stopped");

    // The next start has another key.
    let rotated = ConnectorKey::from_bytes([7; 32]);
    assert_eq!(
        seed::seed_era_connectors(store.as_ref(), &rotated, &clock, &seed)
            .await
            .expect("seeded"),
        Seeded::Done
    );
    let row = store
        .connector_row(id)
        .await
        .expect("read")
        .expect("exists");
    assert_eq!(row.connector.status, ConnectorStatus::Active);
    assert_eq!(row.last_error, None);
    let opened = rotated
        .unseal(SealPurpose::Token, id, &row.secret.expect("sealed"))
        .expect("opens under the new key");
    assert_eq!(opened.expose_secret(), seed::ERA_SLACK_TOKEN.as_bytes());
}

#[tokio::test]
async fn seeding_reports_a_connector_in_another_garden_and_leaves_it() {
    let store = store().await;
    let ids = SequentialIds::default();
    let (_, first) = garden(&store, &ids, "first").await;
    let (_, second) = garden(&store, &ids, "second").await;
    let clock = FixedClock::at(pinned());
    let seed = |garden| EraSeed {
        slack_base: "http://era-slack:8080".into(),
        garden,
    };
    seed::seed_era_connectors(store.as_ref(), &key(), &clock, &seed(first))
        .await
        .expect("seeded");
    assert_eq!(
        seed::seed_era_connectors(store.as_ref(), &key(), &clock, &seed(second))
            .await
            .expect("no error"),
        Seeded::Elsewhere(first)
    );
    let id: ConnectorId = seed::ERA_SLACK_CONNECTOR.parse().expect("id");
    let row = store
        .connector_row(id)
        .await
        .expect("read")
        .expect("exists");
    assert_eq!(row.connector.garden_id, first);
}

#[tokio::test]
async fn seeding_skips_a_garden_whose_deletion_is_under_way() {
    let store = store().await;
    let ids = SequentialIds::default();
    let (user, garden) = garden(&store, &ids, "maple").await;
    let owner = check_membership_of(store.as_ref(), user, garden)
        .await
        .expect("owner");
    store
        .start_garden_deletion(owner, pinned())
        .await
        .expect("deletion started");
    let clock = FixedClock::at(pinned());
    let seed = EraSeed {
        slack_base: "http://era-slack:8080".into(),
        garden,
    };
    assert_eq!(
        seed::seed_era_connectors(store.as_ref(), &key(), &clock, &seed)
            .await
            .expect("no error"),
        Seeded::GardenDeleting
    );
    assert!(
        store
            .connector_row(seed::ERA_SLACK_CONNECTOR.parse().expect("id"))
            .await
            .expect("read")
            .is_none()
    );
}

#[tokio::test]
async fn seeding_leaves_a_connector_being_deleted_to_its_deletion() {
    let store = store().await;
    let ids = SequentialIds::default();
    let (user, garden) = garden(&store, &ids, "maple").await;
    let clock = FixedClock::at(pinned());
    let seed = EraSeed {
        slack_base: "http://era-slack:8080".into(),
        garden,
    };
    seed::seed_era_connectors(store.as_ref(), &key(), &clock, &seed)
        .await
        .expect("seeded");
    let id: ConnectorId = seed::ERA_SLACK_CONNECTOR.parse().expect("id");
    let owner = check_membership_of(store.as_ref(), user, garden)
        .await
        .expect("owner");
    store
        .start_connector_deletion(owner, id)
        .await
        .expect("deletion started");
    let before = store
        .connector_row(id)
        .await
        .expect("read")
        .expect("exists");

    assert_eq!(
        seed::seed_era_connectors(store.as_ref(), &key(), &clock, &seed)
            .await
            .expect("no error"),
        Seeded::ConnectorDeleting
    );
    let after = store
        .connector_row(id)
        .await
        .expect("read")
        .expect("exists");
    assert_eq!(
        after.connector.status,
        ConnectorStatus::Deleting,
        "not revived"
    );
    assert_eq!(after.secret, before.secret, "its token isn't sealed again");
}
