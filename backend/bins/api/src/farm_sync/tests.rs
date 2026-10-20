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
use surrealdb::engine::any;
use tokio_util::sync::CancellationToken;

use domain::{
    connectors::{
        ConnectorAccount, ConnectorEndpoint, ConnectorKind, ConnectorSource, ConnectorStatus,
        FarmRecordKind, SyncErrorCode,
    },
    garden::gardens::TimeZone,
    id::{AuthId, ConnectorId, FarmRecordId, GardenId, UserId},
    ports::{
        Clock, EmbedMode, Embedding, EmbeddingModel, FarmItem, FarmPage, FarmPageRequest,
        FarmRecordStore, FarmSource, FarmSourceError, FarmVector, GardenRepo, IdGenExt,
        IdentityRepo, ModelError, NewConnector, NewFarmRecord, NewGarden, ResolvedLocation,
        SyncCursor, TextEmbedder, VendorToken,
    },
    records::weather::{Coordinates, LocationPrecision},
    rules::limits::{FARM_RECORD_KEEP, MAX_FARM_RECORDS_PER_GARDEN},
    testing::{FixedClock, SequentialIds},
    wire::Timestamp,
};
use store::{SurrealStore, apply_schema_with};

use super::{FarmSources, FarmSync, RunEnd, SYNC_EVERY, SyncConfig, chunk, delete_old_messages};
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
                retry_after_s: Some(30),
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
    assert_eq!(row.next_sync_at, pinned().plus(Duration::from_secs(30)));
    assert_eq!(row.connector.synced_at, None);
    assert_eq!(b.count(owner.1).await, 1);

    b.clock.advance(Duration::from_secs(30));
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
