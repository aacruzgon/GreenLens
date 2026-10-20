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
    // Ten channels, each page 100 ms (a paced vendor), and a budget for about three.
    let source = FakeSource::new(
        None,
        Box::new(|_| {
            std::thread::sleep(Duration::from_millis(100));
            Ok(page(Vec::new(), false, "c"))
        }),
    );
    let config = SyncConfig {
        run_budget: Duration::from_millis(250),
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
    let read = |from: usize| -> Vec<String> {
        source.requests()[from..]
            .iter()
            .map(|r| r.source.id.clone())
            .collect()
    };

    // Three a run takes four runs; a loaded machine may fit fewer, so up to ten are allowed:
    // what matters is that no channel is read twice before every channel is read once.
    let mut ends = Vec::new();
    for _ in 0..10 {
        let end = b.run().await;
        assert_eq!(end.len(), 1, "due again at once until the round ends");
        ends.push(end[0].1);
        if end[0].1 == RunEnd::Synced {
            break;
        }
        assert_eq!(b.row(id).await.connector.synced_at, None);
    }
    assert_eq!(ends.last(), Some(&RunEnd::Synced), "{ends:?}");
    assert!(ends.len() >= 3, "a run read more than the budget: {ends:?}");
    assert_eq!(
        read(0),
        channels,
        "each channel once, in order, none starved"
    );
    let row = b.row(id).await;
    assert_eq!(row.connector.synced_at, Some(pinned()));
    assert_eq!(row.next_sync_at, pinned().plus(SYNC_EVERY));

    // The next round starts one channel further on.
    let before = source.requests().len();
    b.clock.advance(SYNC_EVERY);
    b.run().await;
    assert_eq!(read(before).first().map(String::as_str), Some("C1"));
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
        run_budget: Duration::from_secs(60),
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

#[tokio::test]
async fn a_source_added_later_is_read_alone_and_ends_the_round() {
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
    // A third channel joins, as `PUT sources` would leave it: due now, with no cursor.
    b.store
        .db()
        .query("UPDATE $c SET sources += { id: 'C3', name: 'c3' }, next_sync_at = $now;")
        .bind(("c", store::ids::rid(&id)))
        .bind(("now", b.clock.now().into_inner()))
        .await
        .expect("sources updated")
        .check()
        .expect("sources updated");
    let before = source.requests().len();
    assert_eq!(b.run().await, [(id, RunEnd::Synced)]);
    let read: Vec<String> = source.requests()[before..]
        .iter()
        .map(|r| r.source.id.clone())
        .collect();
    assert_eq!(read, ["C3"], "only the new channel");
    b.clock.advance(SYNC_EVERY);
    let before = source.requests().len();
    assert_eq!(b.run().await, [(id, RunEnd::Synced)]);
    assert_eq!(source.requests().len() - before, 3, "then all three again");
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
        Seeded::NoGarden
    );
}
