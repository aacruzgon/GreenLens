# PDM Backend: `store` crate

> **Status:** Draft v0.4.10, 2026-10-19. v0.4.10: the audit's fixes to the farm repository: creates bump `garden.connector_rev` so the limit holds under a race, Sync now refuses a connector stopped until new sources, a record stores its `source`, the 180-day cleanup takes Slack messages only (`farm_record_kind_at`), and `finish_sync` keeps a `next_sync_at` that moved during the run (`SyncOutcome::due_at`). v0.4.9: `FarmRecordStore` is implemented (roadmap E1.6): connectors with the 2-per-garden limit, the connect compare-and-set, the due query, **Sync now** as a compare-and-set on `sync_requested_at`, upserts that keep a vector only while its `text_hash` holds, deletes by item, chunk range and source, the exact search within one garden, record counts and the cleanup; plans and messages write and read `farm_records` and `farm_citations`, and rows from before them read as empty; 8 tests in `tests/farm.rs`. v0.4.8: the farm connectors' `connector` and `farm_record` tables are in the schema (with `sync_requested_at`, and `farm_records` and `farm_citations` on `action_plan` and `message`), the garden cascade deletes them before the garden, `garden_of` resolves a connector, `farm_record:[connector, vendor_id, n]` keys map to `FarmRecordId`, and `FarmRecordStore` is stubbed until roadmap E1.6. v0.4.7: `knowledge_meta` is in the schema, and an embedded-engine test loads the knowledge loader's output (work items M1, M6). v0.4.6: memories carry a `topic` and `belief_synced_at`, the `belief_forget` queue holds subjects to erase in Agent Memory (written by a shared memory's delete and by the plant and garden cascade starts), and `MemoryStore` gains the belief sweep's methods. v0.4.5: `label_set_version` follows `ml/models/FORMATS.md` (a SHA-256 over the BioCLIP revision, the 80 templates and the sorted labels) instead of an FNV-1a hash, and passes its test vector; open question 7 says so. v0.4.4: open question 4 is resolved: the vector search's bound `k` failed to parse on 3.3.4, so `search_chunks` writes it as a clamped literal, with a test on the embedded engine. v0.4.3: section 4 proposes the migration runner that applies the numbered migrations and the generated `knowledge.surql` after the schema, with its work items, tests and open questions. v0.4.2: the schema now lives at `backend/migrations/schema.surql` only, no longer a copy of a file in `docs/database`. v0.4.1: section 1 keeps only the `$now` difference from database 4, since the skills now follow this design; section 17 lists `TargetBinds`, which section 18 cites; research notes are no longer named by a path outside the repository. v0.4: the multi-line query listings are complete (a generator bug cut eight of them after their first line); the step-claim pair is described as conditional, as in database 4; the protected parameters are `auth`, `token`, `access` and `session`; a bound `$before` is overridden only in `RETURN`, and `Option` fails only from 2 ids; the closure-lifetime error has no code. v0.3: the account tombstone (`deleted_user`), SoilGrids retries, `live_lease`, and the store's `Notify` as the only claim wake-up (a requeue notifies too). v0.2: every repository method is real code (no `todo!()` left), reads decode straight into domain types through contract-shaped projections, and 12 more tests run on the embedded engine. v0.1: `SurrealStore`, which implements every repository of `domain::ports::store` and `MemoryStore` over SurrealDB 3.3.4, with every statement a named constant, one query per transaction with a conflict retry, one error classifier, and real code for each pattern that carries a guarantee, tested against the embedded 3.3.4 engine.

Companion to the [backend architecture](README.md), the [database notes](../database/README.md) (v1.4) and [`schema.surql`](../../backend/migrations/schema.surql). The ports it implements are in [domain 9](domain.md#9-boundary-traits). Every listing is copied from the compiled source of `crates/store` with the docgen tool: `cargo check -p store`, `cargo clippy -p store --all-targets` (clean) and `cargo test -p store` (7 unit tests and 25 tests against the embedded engine) pass on Rust 1.99.1 with `surrealdb = "=3.3.4"`.

## Contents

1. [Purpose and boundaries](#1-purpose-and-boundaries)
2. [Dependencies](#2-dependencies)
3. [Module layout](#3-module-layout)
4. [Connecting and the schema](#4-connecting-and-the-schema)
5. [Transactions, retries and the error mapping](#5-transactions-retries-and-the-error-mapping)
6. [IDs and rows](#6-ids-and-rows)
7. [Shared fragments](#7-shared-fragments)
8. [Access and identity](#8-access-and-identity)
9. [Gardens, plants and the map](#9-gardens-plants-and-the-map)
10. [Scans and events](#10-scans-and-events)
11. [Steps between jobs](#11-steps-between-jobs)
12. [Observations, questions and indicator answers](#12-observations-questions-and-indicator-answers)
13. [Plans and tasks](#13-plans-and-tasks)
14. [Jobs and workers](#14-jobs-and-workers)
15. [Idempotency, usage and the weather cache](#15-idempotency-usage-and-the-weather-cache)
16. [Chat, memory and knowledge](#16-chat-memory-and-knowledge)
17. [Deletion and cleanup](#17-deletion-and-cleanup)
18. [Concurrency and lifetimes](#18-concurrency-and-lifetimes)
19. [Errors](#19-errors)
20. [Testing](#20-testing)
21. [Decisions on open questions](#21-decisions-on-open-questions)
22. [Open questions](#22-open-questions)
23. [Sources](#23-sources)

---

## 1. Purpose and boundaries

**What it owns.** Everything that touches SurrealDB ([architecture 3, Rust crates](../architecture/README.md#rust-crates)):

- one `Surreal<Any>` client for the process, signed in as `api`, and the embedded schema applied as root on start ([database 1](../database/README.md#1-version-and-setup));
- `SurrealStore`, which implements all 17 repository traits (and so `Store` by its blanket impl, [domain 9](domain.md#9-boundary-traits)) and `MemoryStore`;
- every SurrealQL statement, as a `pub const &str` in its area's `sql` module, copied from [database 4](../database/README.md#4-write-patterns) and [5](../database/README.md#5-reads-indexes-and-vector-search) where they give one;
- the mapping between domain types and stored rows: `Id<T>`, `Slug<K>` and array keys ↔ `RecordId`, the literal-union enums, `FLEXIBLE` JSON;
- the conflict retry and the classification of every SDK error into `StoreError`.

**What it never does.** No business rule beyond what a statement checks in its transaction (the state machine, scoring and the role matrix live in `domain` and `pipeline`); no HTTP; no clock reads (every instant is passed in by the caller from the `Clock` port and bound as `$now`); no signed URLs (records come back as `X<Stored>`, [domain 5](domain.md#5-signed-url-typing)).

**Who calls it.** `api` holds `Arc<dyn Store>` and `Arc<dyn MemoryStore>`; `pipeline` and `agent` reach it only through those traits; `knowledge` depends on the crate for its `KnowledgeRepo` rows and statements.

**Everything is real code.** Every method of every repository runs its `sql::` constant; there is no `todo!()` left. The patterns that carry the system's guarantees (the gap-free event append and range replay, the plan compare-and-set, the job claim, the requeue, idempotency, the daily scan limit, the membership insert, alert dedupe, the step claim, commit and sweep, question steps, map versioning, the deletion batches and cleanup) and the other non-trivial transactions (`complete_job`, uploads, run starts, answers, guided checks, the snapshot, chat cards, account deletion) are tested on the embedded engine (section 20).

**The project skills and the database notes.** The Rust skills follow this document: store tests run on the embedded `kv-mem` engine, with one ignored test against the pinned image (`rust-testing`, `database.md`; `rust-surrealdb`), failures leave the crate as the domain's `StoreError` through the crate-private `DbFailure` (`rust-surrealdb`, `errors-and-retries.md`; `rust-errors`), and every statement goes through `run_tx` (section 5). The module layout matches [backend README 6](README.md#6-workspace-layout). Where a skill and this document disagree, this document wins. One deliberate difference from [database 4](../database/README.md#4-write-patterns) remains:

| Topic | Database notes | Here | Why |
|---|---|---|---|
| Clock | database 4 statements use `time::now()` | Bound `$now` everywhere (section 5) | Convention 10: time comes from the `Clock` port, so tests control lease and stamp times |

---

## 2. Dependencies

```toml
[package]
name = "store"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
async-trait.workspace = true
chrono.workspace = true
domain.workspace = true
rand.workspace = true
aws-lc-rs.workspace = true
serde.workspace = true
serde_json.workspace = true
surrealdb.workspace = true
thiserror.workspace = true
tokio.workspace = true
tracing.workspace = true
ulid.workspace = true

[dev-dependencies]
# kv-mem adds the embedded engine (about five times the clean build); tests only.
surrealdb = { workspace = true, features = ["kv-mem"] }
domain = { workspace = true, features = ["mock", "test-util"] }
tokio = { workspace = true, features = ["test-util"] }
```

| Crate | Version | Why |
|---|---|---|
| `surrealdb` | =3.3.4, default features off, `protocol-ws` + `rustls` | The SDK. `parse` (SurrealQL text parsing on the client) isn't needed. `kv-mem` only under `[dev-dependencies]`: it adds the embedded engine, roughly five times the clean build (research `surrealdb.md` §1), and dev-dependency features unify only when tests are built |
| `domain` | path | The ports, the ids, the rules |
| `aws-lc-rs` | 1.18.2 | SHA-256 for `label_set_version` ([`ml/models/FORMATS.md`](../../ml/models/FORMATS.md)); already in the tree through `api` |
| `async-trait` | 0.1.92 | The ports are `#[async_trait]` traits ([domain 10](domain.md#10-concurrency-and-lifetimes)) |
| `chrono` | 0.4.45 | `DateTime<Utc>` is what SurrealDB's `datetime` binds as; `surrealdb-types` implements `SurrealValue` for it with no feature |
| `serde`, `serde_json` | 1.0.229, 1.0.154 | `FLEXIBLE` objects are the contract shapes as JSON; `serde_json::Value` implements `SurrealValue` |
| `rand` | 0.10.3 | The retry's jitter, `rand::random_range` |
| `tokio` | 1.53.3 | The back-off sleep and the `Notify` the claim long-poll waits on |
| `thiserror`, `tracing` | 2.0.21, 0.1.44 | `RowError`; a warning when conflicts persist |
| `ulid` | 3.0.0 | `UlidIds`, the default `IdGen` for the only records the store creates on its own (a guided check and its job, from an upload); `SurrealStore::with_ids` swaps it, and tests pass `SequentialIds` |

---

## 3. Module layout

```
crates/store/
├─ Cargo.toml
├─ src/
│  ├─ lib.rs          crate docs, re-exports, the compile-time `Store + MemoryStore + Send + Sync + 'static` check
│  ├─ client.rs       StoreConfig, SurrealStore, connect / from_client / apply_schema, SCHEMA (include_str!)
│  ├─ tx.rs           run_tx: one query per transaction, conflict retry with jittered back-off
│  ├─ error.rs        first_failure, classify → DbFailure → StoreError, THROW codes, RowError
│  ├─ ids.rs          ToRecordId / FromRecordId for Id<T> and Slug<K>, array keys
│  ├─ rows.rs         stored_enum! and the literal-union row enums, JSON and time helpers
│  ├─ read.rs         json_of, take_one, take_all, page: reads straight into domain types
│  ├─ fragments.rs    SurrealQL fragments as literal macros, shared by several transactions
│  ├─ repo.rs         one module per repository area
│  └─ repo/           access, identity, gardens, plants, map, scans, events, steps, analysis,
│                     plans, jobs, idempotency (+ usage), weather, chat, memory, knowledge, deletion,
│                     farm (connectors and farm records), and load (the loaders and projections shared by
│                     several areas)
├─ tests/
│  ├─ embedded.rs     the guarantees against the embedded 3.3.4 engine (one datastore per test)
│  ├─ embedded/support.rs   fixtures built through the store's own methods
│  ├─ flows.rs        uploads, completion, guided checks, answers, reads, chat, memory, deletions, knowledge
│  ├─ farm.rs         the farm connectors' repository (EON T-E1.6)
│  └─ server.rs       ignored: the schema applied twice on the pinned image
└─ ../../migrations/schema.surql   the schema the crate embeds
```

---

## 4. Connecting and the schema

[Database 1](../database/README.md#1-version-and-setup): connect once, sign in as the database user `api` with `opt::auth::Database`, select `pdm`/`main`, and keep the client in `Arc<AppState>`. The schema is applied before the API serves traffic, over a short-lived root connection, because `api` (a database `EDITOR`) can't run the file's `DEFINE NAMESPACE` and `DEFINE DATABASE`.

```rust
/// The schema, embedded at build time. The file is `backend/migrations/schema.surql` ([database 1],
/// applying the schema). Idempotent: `OVERWRITE`
/// for tables, fields and functions, `IF NOT EXISTS` for indexes.
pub const SCHEMA: &str = include_str!("../../../migrations/schema.surql");

/// How the API reaches `SurrealDB`.
#[derive(Debug, Clone)]
pub struct StoreConfig {
    /// `ws://surrealdb:8000` on the Docker network.
    pub url: String,
    /// The database-level user, `api` ([database 1], users).
    pub username: String,
    /// Its password, from the environment.
    pub password: String,
}

/// The store: one `SurrealDB` client for the process, behind `Arc<dyn Store>` in `api`.
///
/// It is not `Clone`: in 3.x a clone of `Surreal<Any>` opens a new server session and replays
/// the sign-in ([database 6]; research `surrealdb.md` §2), so the store is shared through `Arc`
/// instead. `Surreal<Any>: Send + Sync + 'static` ✓, so `SurrealStore` is too.
pub struct SurrealStore {
    pub(crate) db: Surreal<Any>,
    /// Signalled after every job insert commits; the claim long-poll waits on it
    /// ([database 4], job claim: "an in-process `tokio::sync::Notify`").
    pub(crate) jobs_queued: Arc<Notify>,
    /// Ids for records the store creates on its own (a guided check and its job, from an
    /// upload). Every other id comes from the caller.
    pub(crate) ids: Arc<dyn IdGen>,
}
```

```rust
impl SurrealStore {
    /// Connects, signs in as the database user and selects `pdm`/`main`. Run
    /// [`SurrealStore::apply_schema`] first, as root.
    pub async fn connect(config: &StoreConfig) -> Result<Self, StoreError> {
        let db = any::connect(config.url.as_str())
            .await
            .map_err(|e| classify(e).into_store("connect"))?;
        db.signin(Database {
            namespace: NAMESPACE.to_owned(),
            database: DATABASE.to_owned(),
            username: config.username.clone(),
            password: config.password.clone(),
        })
        .await
        .map_err(|e| classify(e).into_store("sign in"))?;
        Self::from_client(db).await
    }

    /// Wraps a client that is already signed in (or the embedded `mem://` engine in tests,
    /// which has no auth) and selects `pdm`/`main`.
    pub async fn from_client(db: Surreal<Any>) -> Result<Self, StoreError> {
        db.use_ns(NAMESPACE)
            .use_db(DATABASE)
            .await
            .map_err(|e| classify(e).into_store("select database"))?;
        Ok(Self {
            db,
            jobs_queued: Arc::new(Notify::new()),
            ids: Arc::new(UlidIds),
        })
    }

    /// Applies [`SCHEMA`] over a short-lived root connection, before the API serves traffic.
    /// `api` can't run the file's first statements (`DEFINE NAMESPACE`, `DEFINE DATABASE`).
    pub async fn apply_schema(
        url: &str,
        root_user: &str,
        root_pass: &str,
    ) -> Result<(), StoreError> {
        let root = any::connect(url)
            .await
            .map_err(|e| classify(e).into_store("connect as root"))?;
        root.signin(Root {
            username: root_user.to_owned(),
            password: root_pass.to_owned(),
        })
        .await
        .map_err(|e| classify(e).into_store("sign in as root"))?;
        apply_schema_with(&root).await
    }

    /// Uses `ids` for the records the store creates on its own (tests pass `SequentialIds`).
    #[must_use]
    pub fn with_ids(mut self, ids: Arc<dyn IdGen>) -> Self {
        self.ids = ids;
        self
    }

    /// The client, for the knowledge loader's migration and for tests. Repository code is the
    /// only other user.
    pub fn db(&self) -> &Surreal<Any> {
        &self.db
    }

    /// The `Notify` the job broker's long poll waits on.
    pub fn jobs_queued(&self) -> Arc<Notify> {
        Arc::clone(&self.jobs_queued)
    }

    /// Readiness for `/status`: the health RPC within 2 seconds.
    pub async fn ready(&self) -> bool {
        tokio::time::timeout(Duration::from_secs(2), self.db.health())
            .await
            .is_ok_and(|r| r.is_ok())
    }
}
```

```rust
/// Applies [`SCHEMA`] on a client signed in as root (or the embedded engine). Every statement is
/// checked; one failure fails start-up.
pub async fn apply_schema_with(root: &Surreal<Any>) -> Result<(), StoreError> {
    run_tx(root, SCHEMA, |q| q)
        .await
        .map(drop)
        .map_err(|f| f.into_store("apply schema"))
}
```

- `SurrealStore` is **not `Clone`**: a clone of `Surreal<Any>` opens a new server session and replays the sign-in ([database 6](../database/README.md#6-rust-sdk)), so the store is shared through `Arc`. `lib.rs` asserts at compile time that it is `Store + MemoryStore + Send + Sync + 'static`, which is what `Arc<dyn Store>` across tokio's threads needs:

```rust

/// `SurrealStore` implements every repository (so `Store`, by its blanket impl) and
/// `MemoryStore`, and can live in `Arc<dyn Store>` across tokio's threads.
const _: () = {
    const fn assert_store<
        T: domain::ports::Store + domain::ports::MemoryStore + Send + Sync + 'static,
    >() {
    }
    assert_store::<SurrealStore>();
};
```

- The schema file is [`backend/migrations/schema.surql`](../../backend/migrations/schema.surql); `include_str!` embeds it, so the binary and its schema can't drift. ✓ The v1.4 file applies twice in a row on the embedded engine and, as root, on the pinned image by digest.

### Migrations after the schema (proposed)

Nothing applies the knowledge migration the loader generates yet ([roadmap 13](roadmap.md#13-gaps-found-while-writing-this-roadmap), gap 1). The design, its SurrealQL and the checks run on the pinned image are in [database 1](../database/README.md#1-version-and-setup) ("Applying the migrations"). In short: `schema.surql` on every start; then the hand-written `NNNN_<slug>.surql` files once each, in number order, hash-locked; then the generated `knowledge.surql`, which replaces the whole knowledge set and is applied again only when its hash changes. Each file runs in its own transaction with its row in the `migration` table.

The crate's side, proposed (it doesn't compile yet):

```text
// Proposed. store/build.rs lists backend/migrations/*.surql, refuses a bad name or a duplicate
// number, and writes MIGRATIONS (sorted: numbered files, then knowledge.surql) to OUT_DIR.
pub struct Migration { pub name: &'static str, pub sql: &'static str }
pub const MIGRATIONS: &[Migration];            // include!(concat!(env!("OUT_DIR"), "/migrations.rs"))

pub enum MigrationError {
    Changed { name: String, recorded: String, current: String },   // a recorded numbered file was edited
    Missing { name: String },                                      // recorded, but not in this build
    OutOfOrder { name: String, highest: String },                  // new, but sorts before an applied file
    Failed { name: String, source: StoreError },                   // a statement failed; nothing committed
}

pub struct Applied { pub numbered: Vec<&'static str>, pub knowledge: bool }

impl SurrealStore {
    // Replaces apply_schema: the schema, then the migrations, over one short-lived root connection.
    pub async fn migrate(url: &str, root_user: &str, root_pass: &str) -> Result<Applied, MigrationError>;
}
// The testable core: tests pass fixture files and the embedded engine.
pub async fn migrate_with(root: &Surreal<Any>, files: &[Migration]) -> Result<Applied, MigrationError>;
```

- `migrate_with` sends each wrapped file through `run_tx`. A transaction conflict means the file committed nothing, so `run_tx` re-sends it; if another process applied it meanwhile, the second try throws "migration raced", and the runner re-reads the record and moves on.
- `Failed` carries the file name and the statement index of the first real failure (`first_failure` skips `NotExecuted`), so the log names the line to fix.
- `Applied` is logged at `info`: `migrations applied numbered=[…] knowledge=true`.

**Work items**, for slice S2 of the [roadmap](roadmap.md#6-s2-perception-site-data-and-knowledge) (item 6, knowledge loading) and S0 for the live check:

| # | Slice | Work | Exit check |
|---|---|---|---|
| M1 | S2 | Add the `migration` table to `schema.surql`, and list it in database 3 (`knowledge_meta` is in, with the loader) | The schema applies twice in a row on the embedded engine and on the pinned image |
| M2 | S2 | `store/build.rs` and `MIGRATIONS`: names, numbering, and the forbidden statements (`USE`, `BEGIN`, `COMMIT`, `CANCEL`, `OPTION`, `DEFINE NAMESPACE`, `DATABASE`, `USER`, `ACCESS`) | A duplicate number or a bad name fails `cargo build`; `migration_files_follow_the_rules` passes on the committed files |
| M3 | S2 | `migrate_with` and `migrate`, replacing `apply_schema` and `apply_schema_with` | The embedded-engine tests below pass |
| M4 | S2 | `api` start-up: step 3 calls `migrate`; after step 6, the embedder check against `knowledge_meta` ([api 4](api.md#4-start-up-and-shutdown)) | `api` starts twice against the dev compose; the second start logs `numbered=[] knowledge=false` |
| M5 | S2 | `load_knowledge` reads `label_set_version` and `classifier_labels_version` from `knowledge_meta:current` instead of hashing the labels (section 22, item 7) | `knowledge_loads` asserts the stored version |
| M6 | S2 | The loader (`ml/knowledge/`, with the knowledge track) writes `knowledge.surql` as [contracts 10](../architecture/contracts.md#checks-and-generated-files) says | The same seed twice gives identical bytes; S2's exit check passes: a knowledge search returns ranked chunks from the loaded seed |
| M7 | S0 | The live CI job applies the committed migrations twice, as root, on the pinned image, and a generated 21 MB knowledge fixture over WebSocket | The live job is green |

**Tests on the embedded engine** (`kv-mem`, one datastore each):
- `committed_migrations_apply_twice`: `MIGRATIONS` apply, and a second run applies nothing.
- `a_changed_recorded_file_stops_the_start`, `a_missing_recorded_file_stops_the_start`, `an_out_of_order_file_stops_the_start`.
- `a_failing_file_commits_nothing`: no row and no record; the file before it stays applied.
- `knowledge_replaces_the_set`: a fixture v1, then a v2 without one species and two chunks. The species, its edges and the chunks are gone, the rest are upserted, and a search finds no removed chunk.
- `a_raced_file_is_skipped`: the record is written between the read and the apply; the runner re-reads and goes on.
- The embedded engine has no WebSocket, so only M7 checks the 64 MiB message cap.

**Open questions.**
1. **Removed knowledge records.** User records keep a plain link to a deleted species or problem. What should the app show for a plant whose species was removed: unknown, with a prompt to confirm it again? No reader handles a missing record yet.
2. **Memories after a text-model change.** The loader re-embeds chunks, but `memory.embedding` uses the same model and only `api` can re-embed it. A background job, or no model change once memories exist?
3. **Stale photos.** `seed-upload` deletes photos no longer in the seed before the new `api` starts, so the old `api` can sign URLs to deleted photos for a short time. Delete only photos in neither the old nor the new seed, or delete after `api` is healthy?
4. **Growth past 48 MiB.** Split the knowledge file by table (losing one transaction for the whole set), apply it over HTTP `/import` (limits *unverified*), or keep chunks out of Git?
5. **Rollback.** An older build refuses a database that recorded a file it doesn't know. Is restoring the pre-deploy backup an acceptable rollback during build week?

---

## 5. Transactions, retries and the error mapping

### `run_tx`

Every statement goes through one function. A transaction is one `BEGIN … COMMIT` sent as **one** query ([database 4](../database/README.md#4-write-patterns)), and a `TransactionConflict` re-sends it, up to 5 attempts in all, sleeping 10–50 ms × 2^attempt between them ([database 6](../database/README.md#6-rust-sdk)). A single statement is its own transaction and can conflict the same way (database v1.3.3), so plain reads go through `run_tx` too; a conflicted transaction committed nothing, so a retry is always safe, `+= 1` included.

```rust
/// Attempts per transaction, the first included.
pub const MAX_ATTEMPTS: u32 = 5;

/// The back-off before retry `attempt` (0-based): 10–50 ms of jitter, doubled per attempt, so
/// the four sleeps total 150–750 ms. The jitter is drawn from the thread-local generator and
/// dropped before the `.await`, so the caller's future stays `Send`.
pub fn backoff(attempt: u32) -> Duration {
    let jitter_ms: u64 = rand::random_range(10..=50);
    Duration::from_millis(jitter_ms << attempt)
}

/// Sends `sql` (one statement, or a whole `BEGIN … COMMIT` as **one** query) and returns its
/// results once no statement failed. `bind` adds the parameters to a fresh query on each
/// attempt.
///
/// - **Retry.** A `TransactionConflict` re-sends the whole query, up to [`MAX_ATTEMPTS`]. A
///   conflicted transaction committed nothing, so a retry is always safe, even for `+= 1`.
///   Any other failure returns at once: a lost compare-and-set is final.
/// - **`'c`** is the borrow of the client and of the statement text; the returned
///   [`Query<'c, Any>`] borrows both, and nothing outlives the call.
/// - **`F: Fn(Query<'c, Any>) -> Query<'c, Any> + Send`.** `Fn`, not `FnOnce`, because it runs
///   once per attempt; it borrows the caller's arguments and clones each value into a bind
///   (`.bind(("scan", rid(scan)))`), so the arguments are never moved out. `Send` because the
///   closure lives across the `.await`s of this future, which `#[async_trait]` boxes as
///   `dyn Future + Send`.
#[expect(
    clippy::result_large_err,
    reason = "the failure carries the SDK error by value and is mapped at once by the caller"
)]
pub async fn run_tx<'c, F>(
    db: &'c Surreal<Any>,
    sql: &'c str,
    bind: F,
) -> Result<IndexedResults, DbFailure>
where
    F: Fn(Query<'c, Any>) -> Query<'c, Any> + Send,
{
    let mut attempt = 0;
    loop {
        let failure = match bind(db.query(sql)).await {
            Ok(mut results) => match first_failure(results.take_errors()) {
                None => return Ok(results),
                Some(error) => classify(error),
            },
            // A parse error or a lost connection fails the whole request.
            Err(error) => classify(error),
        };
        match failure {
            DbFailure::TxConflict(_) if attempt + 1 < MAX_ATTEMPTS => {
                tokio::time::sleep(backoff(attempt)).await;
                attempt += 1;
            }
            failure => {
                if matches!(failure, DbFailure::TxConflict(_)) {
                    tracing::warn!(attempts = MAX_ATTEMPTS, "transaction conflicts persisted");
                }
                return Err(failure);
            }
        }
    }
}
```

**The closure's bounds**, each for a reason:

| Bound | Why |
|---|---|
| `'c` | The borrow of the client and of the statement text. `Surreal::query<'c>` returns a `Query<'c, Any>` that borrows both ([research `surrealdb.md` §8](#23-sources)); naming one lifetime for the argument and the return ties the closure's output to the query it was given |
| `Fn`, not `FnOnce` or `FnMut` | It runs once per attempt and mutates nothing: it borrows the caller's arguments and clones each value into a bind |
| `+ Send` | The closure is held across the `.await`s of `run_tx`'s future, which `#[async_trait]` boxes as `dyn Future + Send`; a generic's auto traits are only checked where the future is boxed, so the bound makes the error point here |
| `Fut` doesn't appear | The statement is a `const`, so only the binds vary; nothing else needs rebuilding |

**Borrowed binds.** `.bind(("auth_id", auth_id.as_str()))` passes a `&str`: `bind` takes `impl IntoVariables` and converts the value into an owned `surrealdb::types::Value` at once, so the query holds no borrow of the argument and the bind needs no lifetime ✓ (research `surrealdb.md` §8). Ids are converted with `rid(&id)` into owned `RecordId`s. Where a bind is a row (`JobWriteRow`, `PlanWriteRow`), the closure clones it per attempt.

**Results.** A statement's result is taken by index; transactions put their result in the last statement before `COMMIT` and take it at `num_statements() - 2`, so a fragment added earlier never shifts it (section 7). `take::<Option<T>>` is for a statement that returns one value, `take::<Vec<T>>` for one that returns an array ✓ (a `DELETE … RETURN VALUE $before.id` must be taken as `Vec`: as `Option` it gives `None` for 0 ids and `Some` for 1, but fails with "Tried to take only a single result from a query that contains multiple" once 2 or more come back).

### Classifying failures

`.query().await` is `Ok` even when statements failed; each statement's error is in `take_errors()`. Inside a transaction the other statements report `NotExecuted` (before the cause) or `Cancelled` (after it), so `check()` returns a skip, not the cause ✓ (research `surrealdb.md` §2; database v1.3.3). `first_failure` returns the lowest-index error that isn't a skip, and `classify` is the one place SDK errors are read:

```rust
/// How the server prefixes the message of a `THROW` ✓.
pub const THROWN_PREFIX: &str = "An error occurred: ";

/// The codes the store's statements `THROW`. Matched with `e.is_thrown()` and the text after
/// [`THROWN_PREFIX`], since a thrown error carries no typed details.
pub mod thrown {
    /// The plan compare-and-set lost ([database 4], plan writes).
    pub const STALE_PLAN: &str = "stale_plan";
    /// A step's claim, stamp or run no longer holds ([database 4], steps between jobs).
    pub const CLAIM_LOST: &str = "claim_lost";
    /// A photo answer's step no longer holds.
    pub const QUESTION_STEP_LOST: &str = "question_step_lost";
    /// The lease token, worker or status no longer matches.
    pub const LEASE_LOST: &str = "lease_lost";
    /// A record the statement needs is missing, being deleted or outside the garden.
    pub const NOT_FOUND: &str = "not_found";
    /// A map upload's `base_version` is stale ([database 4], garden map versions).
    pub const STALE_MAP: &str = "stale_map";
    /// The daily scan limit ([database 4], limits).
    pub const BUDGET_EXCEEDED: &str = "budget_exceeded";
    /// A state check refused the write (a scan that isn't `pending`, the last owner).
    pub const INVALID_STATE: &str = "invalid_state";
    /// A map upload pins plants that aren't the garden's.
    pub const UNKNOWN_PLANTS: &str = "unknown_plants";
}

/// What one failed statement means, before the caller adds context.
#[derive(Debug)]
pub enum DbFailure {
    /// `QueryError::TransactionConflict` ✓: [`crate::tx::run_tx`] retries the whole
    /// transaction; reaching a caller means the retries ran out.
    TxConflict(surrealdb::Error),
    /// `THROW '<code>'` ✓ (kind `Thrown`, message `"An error occurred: <code>"`).
    Thrown {
        /// The code after the prefix.
        code: String,
        /// The SDK error.
        source: surrealdb::Error,
    },
    /// `CREATE` of an existing id or `INSERT RELATION` of an existing edge ✓
    /// (`AlreadyExistsError::Record`).
    AlreadyExists(surrealdb::Error),
    /// A `UNIQUE` index refused the write ✓. On 3.3.4 this is kind `Internal` with no details,
    /// only the message ``Database index `<name>` already contains …``.
    UniqueIndex {
        /// The index name.
        index: String,
        /// The SDK error.
        source: surrealdb::Error,
    },
    /// The connection is gone (kind `Connection`).
    Unavailable(surrealdb::Error),
    /// Anything else: a bug, a schema mismatch, a type the schema refused.
    Other(surrealdb::Error),
}

/// The statement that caused a failure: the lowest-index error that isn't one a failed
/// transaction merely skipped (`NotExecuted` before the cause, `Cancelled` after it). When
/// every error is a skip (which shouldn't happen), the lowest-index one.
pub fn first_failure(
    errors: impl IntoIterator<Item = (usize, surrealdb::Error)>,
) -> Option<surrealdb::Error> {
    let mut errors: Vec<_> = errors.into_iter().collect();
    errors.sort_unstable_by_key(|(index, _)| *index);
    let mut first = None;
    for (_, error) in errors {
        let skipped = matches!(
            error.query_details(),
            Some(QueryError::NotExecuted | QueryError::Cancelled)
        );
        if !skipped {
            return Some(error);
        }
        if first.is_none() {
            first = Some(error);
        }
    }
    first
}

/// The one place SDK errors are classified.
pub fn classify(error: surrealdb::Error) -> DbFailure {
    if matches!(error.query_details(), Some(QueryError::TransactionConflict)) {
        return DbFailure::TxConflict(error);
    }
    if matches!(
        error.already_exists_details(),
        Some(AlreadyExistsError::Record { .. })
    ) {
        return DbFailure::AlreadyExists(error);
    }
    if error.is_connection() {
        return DbFailure::Unavailable(error);
    }
    if error.is_thrown() {
        let code = error
            .message()
            .strip_prefix(THROWN_PREFIX)
            .unwrap_or(error.message())
            .trim_matches(['\'', '"'])
            .to_owned();
        return DbFailure::Thrown {
            code,
            source: error,
        };
    }
    if let Some(index) = unique_index(&error) {
        return DbFailure::UniqueIndex {
            index,
            source: error,
        };
    }
    DbFailure::Other(error)
}
```

| Failure on 3.3.4 | How it is recognised | `DbFailure` |
|---|---|---|
| Write conflict at `COMMIT`, or on a single statement | `query_details() == Some(QueryError::TransactionConflict)` ✓ | `TxConflict` (retried) |
| `THROW 'stale_plan'` | `is_thrown()`, message `"An error occurred: stale_plan"` ✓; the code is the text after the prefix | `Thrown { code }` |
| `CREATE` of an existing id, `INSERT RELATION` of an existing edge | `already_exists_details() == Some(Record { .. })` ✓ | `AlreadyExists` |
| A `UNIQUE` index (`task_alert`, `user_auth`, `action_plan_version`) | kind `Internal`, message ``Database index `<name>` already contains …`` ✓ (no typed details) | `UniqueIndex { index }` |
| Lost connection | `is_connection()` | `Unavailable` |
| Anything else (a coercion, a rejected `DELETE`, a parse error) | — | `Other` |

```rust
impl DbFailure {
    /// The `THROW` code, if this is one.
    pub fn thrown(&self) -> Option<&str> {
        match self {
            Self::Thrown { code, .. } => Some(code),
            _ => None,
        }
    }

    /// The default mapping. `what` names the operation for the log (`"append event"`).
    pub fn into_store(self, what: &'static str) -> StoreError {
        match self {
            Self::TxConflict(_) => StoreError::Conflict(Conflict::Retries),
            Self::AlreadyExists(_) | Self::UniqueIndex { .. } => {
                StoreError::Conflict(Conflict::AlreadyExists)
            }
            Self::Unavailable(source) => StoreError::Unavailable(Box::new(source)),
            Self::Thrown { code, source } => match code.as_str() {
                thrown::STALE_PLAN => StoreError::Conflict(Conflict::StalePlan { current: None }),
                thrown::CLAIM_LOST => StoreError::Conflict(Conflict::ClaimLost),
                thrown::QUESTION_STEP_LOST => StoreError::Conflict(Conflict::QuestionStepLost),
                thrown::NOT_FOUND => StoreError::NotFound,
                _ => StoreError::Query {
                    what,
                    source: Box::new(source),
                },
            },
            Self::Other(source) => StoreError::Query {
                what,
                source: Box::new(source),
            },
        }
    }
}
```

A caller that knows more maps first: the plan write turns `stale_plan` and the `action_plan_version` unique index into `StalePlan { current }` by reading the plan that won (section 13); the map upload turns `stale_map` into `StaleMap { current_version }` and `unknown_plants` into `Rejected(UnknownPlants { plant_ids })` (section 9); `create_scan` turns `budget_exceeded` into `Rejected(BudgetExceeded)` with the next UTC midnight (section 10); alert dedupe and `upsert_user` treat their unique index as success (sections 13, 8).

A row that doesn't convert into its domain type is a `RowError`, reported as `StoreError::Query`: a bug or a schema drift, never a user error.

```rust
/// A stored row that doesn't convert into its domain type: a bug or a schema drift, never a
/// user error. Reported as [`StoreError::Query`].
#[derive(Debug, thiserror::Error)]
pub enum RowError {
    /// A record id of the wrong table or key shape.
    #[error("record id {id} is not a {expected} id")]
    BadId {
        /// The id as stored.
        id: String,
        /// The table expected.
        expected: &'static str,
    },
    /// A field whose value the domain type refuses.
    #[error("field {field}: {problem}")]
    BadField {
        /// The field.
        field: &'static str,
        /// Why.
        problem: String,
    },
    /// A FLEXIBLE object that doesn't parse as its contract shape.
    #[error("field {field} does not match its contract shape")]
    Json {
        /// The field.
        field: &'static str,
        /// The serde error.
        #[source]
        source: serde_json::Error,
    },
}
```

---

## 6. IDs and rows

### `Id<T>`, `Slug<K>` and array keys

`domain` never depends on `surrealdb` (convention 5), so the mapping is two traits here, implemented once for every table through the domain's markers:

```rust
/// A domain id that names one record.
pub trait ToRecordId {
    /// The record id.
    fn record_id(&self) -> RecordId;
}

/// A domain id read back from a record id. Checks the table and the key's shape.
pub trait FromRecordId: Sized {
    /// Parses the record id.
    fn from_record_id(id: &RecordId) -> Result<Self, RowError>;
}
```

```rust
impl<T: Table> ToRecordId for Id<T> {
    fn record_id(&self) -> RecordId {
        RecordId::new(T::NAME, self.to_string())
    }
}
```

```rust
impl<T: Table> FromRecordId for Id<T> {
    fn from_record_id(id: &RecordId) -> Result<Self, RowError> {
        string_key(id, T::NAME)?
            .parse()
            .map_err(|_| bad_id(id, T::NAME))
    }
}
```

```rust
/// `rid(id)`: the shortest spelling at a bind site.
pub fn rid<I: ToRecordId + ?Sized>(id: &I) -> RecordId {
    id.record_id()
}

/// `rid` over an `Option`, for `option<record<…>>` fields (`None` binds as `NONE`).
pub fn opt_rid<I: ToRecordId>(id: Option<&I>) -> Option<RecordId> {
    id.map(ToRecordId::record_id)
}

/// The domain id of a record id.
pub fn from_rid<I: FromRecordId>(id: &RecordId) -> Result<I, RowError> {
    I::from_record_id(id)
}

/// The domain id of an optional record id.
pub fn from_opt_rid<I: FromRecordId>(id: Option<&RecordId>) -> Result<Option<I>, RowError> {
    id.map(I::from_record_id).transpose()
}
```

- `T: Table` gives the table name as `T::NAME`, so `from_record_id` checks that a `scan:…` id isn't read as a `GardenId`. `rid<I: ToRecordId + ?Sized>` takes a reference so it works on `Slug<K>` (which holds an `Arc<str>`) without cloning.
- ULID keys are the 26-character string: `RecordIdKey` has no `From<Ulid>` (`surrealdb-types` uses `ulid` 1.x) ✓.
- **Array keys** are built with `Array::from(vec![Value::RecordId(..), Value::Number(..)])` ✓; `RecordIdKey` has no `From<Vec<_>>`, and `RecordId::parse_simple` always yields a string key (research `surrealdb.md` §4):

```rust
/// `scan_event:[scan, seq]`.
pub fn scan_event_rid(scan: ScanId, seq: EventSeq) -> RecordId {
    array_key(
        "scan_event",
        vec![Value::RecordId(rid(&scan)), int(seq_i64(seq))],
    )
}

/// The `seq` of a `scan_event:[scan, seq]` key.
pub fn seq_of(id: &RecordId) -> Result<EventSeq, RowError> {
    if let RecordIdKey::Array(parts) = &id.key
        && let Some(Value::Number(Number::Int(seq))) = parts.get(1)
        && let Ok(seq) = u64::try_from(*seq)
    {
        return Ok(EventSeq(seq));
    }
    Err(bad_id(id, "scan_event"))
}

/// `member_of:[user, garden]`.
pub fn member_rid(user: UserId, garden: domain::id::GardenId) -> RecordId {
    array_key(
        "member_of",
        vec![Value::RecordId(rid(&user)), Value::RecordId(rid(&garden))],
    )
}

/// `idempotency_key:[user, "METHOD /path", key]`.
pub fn idempotency_rid(user: UserId, route: &str, key: IdempotencyKey) -> RecordId {
    array_key(
        "idempotency_key",
        vec![
            Value::RecordId(rid(&user)),
            Value::String(route.to_owned()),
            Value::String(key.to_string()),
        ],
    )
}

/// `usage_daily:[user, "YYYY-MM-DD"]` (UTC day).
pub fn usage_rid(user: UserId, day: NaiveDate) -> RecordId {
    array_key(
        "usage_daily",
        vec![
            Value::RecordId(rid(&user)),
            Value::String(day.format("%Y-%m-%d").to_string()),
        ],
    )
}

/// `indicator_answer:[scan, run, indicator_id]` (schema v1.4).
pub fn indicator_answer_rid(scan: ScanId, run: Run, indicator: &IndicatorId) -> RecordId {
    array_key(
        "indicator_answer",
        vec![
            Value::RecordId(rid(&scan)),
            int(i64::from(run.0)),
            Value::String(indicator.to_string()),
        ],
    )
}

/// `weather_cache:[hour, cell]`: the hour first, so cleanup is one key-range delete.
pub fn weather_rid(hour: DateTime<Utc>, cell: &str) -> RecordId {
    array_key(
        "weather_cache",
        vec![hour.into_value(), Value::String(cell.to_owned())],
    )
}

/// `farm_record:[connector, vendor_id, n]` ([connectors 4]): an array key, because a Slack
/// `ts` holds a dot.
///
/// [connectors 4]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#4-data
pub fn farm_record_rid(id: &FarmRecordId) -> RecordId {
    array_key(
        "farm_record",
        vec![
            Value::RecordId(rid(&id.connector)),
            Value::String(id.vendor_id.to_string()),
            int(i64::from(id.n)),
        ],
    )
}

/// The [`FarmRecordId`] of a `farm_record:[connector, vendor_id, n]` key.
pub fn farm_record_id_of(id: &RecordId) -> Result<FarmRecordId, RowError> {
    if id.table.as_str() == "farm_record"
        && let RecordIdKey::Array(parts) = &id.key
        && let [
            Value::RecordId(connector),
            Value::String(vendor_id),
            Value::Number(Number::Int(n)),
        ] = parts.as_slice()
        && let Ok(n) = u32::try_from(*n)
        && let Ok(connector) = from_rid::<ConnectorId>(connector)
        && let Ok(record) = FarmRecordId::new(connector, vendor_id, n)
    {
        return Ok(record);
    }
    Err(bad_id(id, "farm_record"))
}
```

### Rows

**Why rows are owned.** A row is decoded by `SurrealValue::from_value(Value)`, which takes the SDK's value by value and drops it after `take(n)`, so a row can't borrow from it: every field is owned (`String`, `RecordId`, `Vec<_>`). Rows also never cross a task; each repository method converts them into domain types before returning. Stored literal unions get a row mirror with `From` both ways, made by one macro:

```rust
/// Declares a row enum mirroring a domain enum variant for variant, with `From` both ways. A
/// variant may carry the stored literal when `snake_case` wouldn't produce it
/// (`About1Km = "about_1_km"`).
macro_rules! stored_enum {
    (
        $(#[$doc:meta])*
        $row:ident <=> $domain:ty { $($variant:ident $(= $rename:literal)?),+ $(,)? }
    ) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, ::surrealdb::types::SurrealValue)]
        #[surreal(untagged, rename_all = "snake_case")]
        pub enum $row {
            $(
                #[allow(missing_docs, reason = "mirrors the domain variant of the same name")]
                $(#[surreal(rename = $rename)])?
                $variant,
            )+
        }

        impl From<$domain> for $row {
            fn from(value: $domain) -> Self {
                type D = $domain;
                match value {
                    $(D::$variant => Self::$variant,)+
                }
            }
        }

        impl From<$row> for $domain {
            fn from(value: $row) -> Self {
                match value {
                    $($row::$variant => Self::$variant,)+
                }
            }
        }
    };
}
```

- `#[surreal(untagged, rename_all = "snake_case")]` is required: without `untagged` a derived unit enum is written as `{ "open": {} }` and fails to decode the stored string `'open'` ✓ (research `surrealdb.md` §3). A variant may carry its stored literal (`About1Km = "about_1_km"`), because `snake_case` would give `about1_km`.
- Inside `From<$domain>`, `type D = $domain;` lets the match name variants through a type alias (enum variants are reachable through a type alias since Rust 1.37, [Reference, paths](https://doc.rust-lang.org/reference/paths.html#paths-in-expressions)), which a `$domain:ty` fragment can't do directly.
- Less central literal unions (observation stages, sources, roles) are strings from their serde form (`enum_str`), so the store doesn't mirror every contract enum.
- Contract shapes that the schema stores as `object FLEXIBLE` are `serde_json::Value`, from and into the domain types with `to_json` / `from_json`. Timestamps inside them become RFC 3339 strings; queries never filter on them. Every queried instant is a top-level `datetime` bound as `DateTime<Utc>`, never a formatted string ([database 2](../database/README.md#2-conventions)).

```rust
/// A contract shape as the JSON a `FLEXIBLE` object stores. Timestamps inside become RFC 3339
/// strings, which is right for these blobs: queries never filter on them.
pub fn to_json<T: Serialize>(
    field: &'static str,
    value: &T,
) -> Result<serde_json::Value, RowError> {
    serde_json::to_value(value).map_err(|source| RowError::Json { field, source })
}

/// A `FLEXIBLE` object read back as its contract shape.
pub fn from_json<T: DeserializeOwned>(
    field: &'static str,
    value: serde_json::Value,
) -> Result<T, RowError> {
    serde_json::from_value(value).map_err(|source| RowError::Json { field, source })
}

/// A `Timestamp` as the `datetime` `SurrealDB` stores: bind `DateTime<Utc>`, never a formatted
/// string, which would be stored as a string ([database 2]).
pub fn dt(t: Timestamp) -> DateTime<Utc> {
    t.into_inner()
}

/// A stored `datetime` as a `Timestamp`.
pub fn ts(t: DateTime<Utc>) -> Timestamp {
    Timestamp::from(t)
}
```

### Reads into domain types

Most reads don't need a row struct. Each statement projects the stored record into its contract shape in SurrealQL (`scan AS scan_id`, `{ table: record::tb(ref), id: record::id(ref) } AS ref`, `[] AS images`), the result is taken as a `surrealdb::types::Value`, and `read::json_of` converts it with the wire rules: a record id becomes its bare key (the ULID or the slug, [contracts 1](../architecture/contracts.md#wire-format)), a `datetime` an RFC 3339 string with its full precision, `NONE` an absent field. The domain type's own `Deserialize` then builds it, so a projection that misses a field fails loudly as a `RowError`.

```rust
/// A `SurrealDB` value as the JSON the contracts define.
pub fn json_of(value: Value) -> serde_json::Value {
    use serde_json::Value as J;
    match value {
        Value::None | Value::Null => J::Null,
        Value::Bool(b) => J::Bool(b),
        Value::Number(Number::Int(i)) => J::from(i),
        Value::Number(Number::Float(f)) => {
            serde_json::Number::from_f64(f).map_or(J::Null, J::Number)
        }
        Value::Number(Number::Decimal(d)) => d
            .to_string()
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map_or(J::Null, J::Number),
        Value::String(s) => J::String(s),
        Value::Datetime(d) => J::String(
            d.into_inner()
                .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true),
        ),
        Value::RecordId(id) => match id.key {
            RecordIdKey::String(s) => J::String(s),
            RecordIdKey::Number(n) => J::from(n),
            RecordIdKey::Array(parts) => J::Array(parts.into_iter().map(json_of).collect()),
            other => J::String(format!("{other:?}")),
        },
        Value::Duration(d) => J::from(d.into_inner().as_secs()),
        Value::Array(items) => J::Array(items.into_iter().map(json_of).collect()),
        Value::Object(fields) => J::Object(
            fields
                .into_iter()
                .filter(|(_, v)| !matches!(v, Value::None | Value::Null))
                .map(|(k, v)| (k, json_of(v)))
                .collect(),
        ),
        Value::Uuid(u) => J::String(u.to_string()),
        other => J::String(format!("{other:?}")),
    }
}

/// Statement `index`'s single record (`SELECT … FROM ONLY`), or `None`.
pub fn take_one<T: DeserializeOwned>(
    results: &mut IndexedResults,
    index: usize,
    what: &'static str,
) -> Result<Option<T>, StoreError> {
    let value: Value = results.take(index).map_err(|e| take_failed(what, e))?;
    match value {
        Value::None | Value::Null => Ok(None),
        Value::Array(items) => match items.into_iter().next() {
            Some(first) => decode(what, first)
                .map(Some)
                .map_err(|e| e.into_store(what)),
            None => Ok(None),
        },
        other => decode(what, other)
            .map(Some)
            .map_err(|e| e.into_store(what)),
    }
}

/// Statement `index`'s records.
pub fn take_all<T: DeserializeOwned>(
    results: &mut IndexedResults,
    index: usize,
    what: &'static str,
) -> Result<Vec<T>, StoreError> {
    let value: Value = results.take(index).map_err(|e| take_failed(what, e))?;
    match value {
        Value::None | Value::Null => Ok(Vec::new()),
        Value::Array(items) => items
            .into_iter()
            .map(|v| decode(what, v).map_err(|e| e.into_store(what)))
            .collect(),
        other => decode(what, other)
            .map(|t| vec![t])
            .map_err(|e| e.into_store(what)),
    }
}

/// A page from rows fetched with `LIMIT limit + 1`: the extra row only says there is more,
/// and the cursor is the last returned row's key ([database 5]: page by cursor, never `START`).
pub fn page<T>(
    mut rows: Vec<T>,
    limit: u32,
    cursor_of: impl Fn(&T) -> String,
) -> domain::wire::Page<T> {
    let limit = limit as usize;
    let more = rows.len() > limit;
    rows.truncate(limit);
    let next_cursor = if more {
        rows.last().and_then(|r| cursor_of(r).parse().ok())
    } else {
        None
    };
    domain::wire::Page {
        items: rows,
        next_cursor,
    }
}
```

`repo::load` holds the loaders shared by several repositories (a scan with its images, guided checks and open question; a plant with its latest scan; a diagnosis with its soil observations; a plan with its open question and task ids; a garden's context), and the projections are literal macros (`scan_fields!`, `plant_fields!`, `question_fields!`, `observation_fields!`, `task_fields!` …) reused by every statement that reads that record. Lists page by a cursor on their sort key and the id (`[name, id] > $after`), fetching `limit + 1` rows to know whether there is a next page ([database 5](../database/README.md#5-reads-indexes-and-vector-search): never `START`).

---

## 7. Shared fragments

Several transactions share pieces: the step claim's re-check, the hand-over, the observation write, the indicator answers, the job enqueue. Each is a `macro_rules!` that expands to a **string literal**, so each whole transaction is still one named `const &str` built at compile time with `concat!` (a `const` can't be passed to `concat!`, a literal macro can). No value is ever formatted in; every value is a bind, and each fragment documents the binds it reads. Fragments that end a transaction put their result last, so the caller takes it at `num_statements() - 2` whatever came before.

```rust
/// The step-claim re-check ([database 4], steps between jobs: "commit: re-check the claim").
/// Binds: `$scan`, `$step`, `$stamp`, `$run`. Leaves `$s` (the scan's claim fields, `status`
/// and the deleting flags) for later fragments.
macro_rules! claim_check {
    () => {
        "LET $s = (SELECT pipeline_step, pipeline_step_run, pipeline_step_at, run, status, rescore_pending,
                 plant.deleting_at AS plant_deleting, garden.deleting_at AS garden_deleting
          FROM ONLY $scan);
IF $s = NONE OR $s.pipeline_step != $step OR $s.pipeline_step_at != $stamp OR $s.pipeline_step_run != $s.run
   OR $s.run != $run OR $s.plant_deleting != NONE OR $s.garden_deleting != NONE { THROW 'claim_lost'; };
"
    };
}

/// The hand-over ([database 4], steps between jobs). Binds: `$scan`, `$handover` (the step's own
/// next step, or `NONE` to release), `$now`. Reads `$s` from [`claim_check!`]. Returns the next
/// step, or `NONE` when released.
macro_rules! handover {
    () => {
        "LET $next = IF $s.rescore_pending { 'score' } ELSE { $handover };
UPDATE $scan SET pipeline_step = $next,
                 pipeline_step_run = IF $next = NONE { NONE } ELSE { run },
                 pipeline_step_at = IF $next = NONE { NONE } ELSE { $now },
                 rescore_pending = false;
RETURN $next;
"
    };
}

/// An entry point's claim ([database 4], steps between jobs ✓: "claim, by an entry point"),
/// with the database's two statements made conditional on each other: the flag is set only
/// when the claim came back `NONE` because another step holds it. A scan that is being deleted
/// or whose status claims nothing (`pending`, `queued`, `failed`) is `not_needed`; an optional
/// `$only_statuses` narrows the statuses that claim (a guided check claims only on a `diagnosed`
/// or `planned` scan; left unbound it is `NONE` and allows all). Binds: `$scan`, `$step`, `$now`.
/// Returns `{ outcome, run, stamp, status }`.
macro_rules! claim_entry {
    () => {
        "LET $c = (SELECT status, deleting_at, plant.deleting_at AS plant_deleting, garden.deleting_at AS garden_deleting FROM ONLY $scan);
LET $eligible = $c != NONE AND $c.deleting_at = NONE AND $c.plant_deleting = NONE AND $c.garden_deleting = NONE
  AND !($c.status IN ['pending', 'queued', 'failed']) AND ($only_statuses = NONE OR $c.status IN $only_statuses);
LET $claimed = IF $eligible {
  (UPDATE ONLY $scan SET pipeline_step = $step, pipeline_step_run = run, pipeline_step_at = $now,
                         rescore_pending = IF $step = 'score' { false } ELSE { rescore_pending }
     WHERE pipeline_step = NONE AND plant.deleting_at = NONE AND garden.deleting_at = NONE RETURN AFTER)
} ELSE { NONE };
IF $eligible AND $claimed = NONE { UPDATE ONLY $scan SET rescore_pending = true WHERE pipeline_step != NONE; };
RETURN { outcome: IF !$eligible { 'not_needed' } ELSE IF $claimed = NONE { 'busy' } ELSE { 'claimed' },
         run: $claimed.pipeline_step_run, stamp: $claimed.pipeline_step_at, status: $c.status };
"
    };
}
```

The other fragments (`observation_write!`, `evidence_write!`, `job_enqueue!`, `resolve_question!`, `plan_cas!`, `question_step_check!`) are listed with the areas that use them.

---

## 8. Access and identity

| Method | Statement | Database |
|---|---|---|
| `AccessRepo::role_in` | `ROLE_IN`: a direct read of `member_of:[user, garden]`, skipping a garden being deleted | [4, membership](../database/README.md#membership-) |
| `garden_of` | one statement per resource kind; a record, its plant or its garden being deleted reads as missing | [4, deletion](../database/README.md#deletion-) step 1 |
| `conversation_owner`, `pending_action_conversation` | `CONVERSATION_OWNER`, `PENDING_ACTION_CONVERSATION` | — |
| `IdentityRepo::upsert_user` | `USER_BY_AUTH` first (one indexed read per request, with the tombstone check), `CREATE_USER` only on a miss (the tombstone re-checked in its transaction); a racing webhook loses on `user_auth` and reads the winner; a tombstoned auth id is `NotFound` on both paths, which `authenticate` maps to `401` | [3](../database/README.md#3-tables), [4, deletion](../database/README.md#deletion-) |
| `register_push_token` | `UPSERT push_token:[token]`; `created_at` keeps its first value | — |
| `me`, `update_me`, push tokens | `ME`, `UPDATE_ME` (withdrawing consent rounds every owned garden to about 10 km in the same transaction), `REMOVE_PUSH_TOKEN`, `PUSH_TARGETS` (iOS and Android only), `FORGET_PUSH_TOKENS` | — |

```rust
/// The statements. Each lookup skips a record being deleted, its plant's and its garden's
/// deletion included, so a record being deleted reads as missing (`404`).
pub mod sql {
    /// The membership check: a direct read of `member_of:[$user, $garden]`.
    pub const ROLE_IN: &str = "SELECT VALUE role FROM $member WHERE out.deleting_at = NONE;";
    /// A plant's garden.
    pub const GARDEN_OF_PLANT: &str =
        "SELECT VALUE garden FROM $id WHERE deleting_at = NONE AND garden.deleting_at = NONE;";
    /// A scan's garden.
    pub const GARDEN_OF_SCAN: &str = "SELECT VALUE garden FROM $id WHERE deleting_at = NONE AND plant.deleting_at = NONE AND garden.deleting_at = NONE;";
    /// A plan's, task's or memory's garden (their plant, when they have one, must be live).
    pub const GARDEN_OF_PLANT_CHILD: &str = "SELECT VALUE garden FROM $id WHERE plant.deleting_at = NONE AND garden.deleting_at = NONE;";
    /// An invite's garden.
    pub const GARDEN_OF_INVITE: &str =
        "SELECT VALUE garden FROM $id WHERE garden.deleting_at = NONE;";
    /// A farm connector's garden; a connector being deleted reads as missing.
    pub const GARDEN_OF_CONNECTOR: &str =
        "SELECT VALUE garden FROM $id WHERE status != 'deleting' AND garden.deleting_at = NONE;";
    /// A conversation's owner and garden.
    pub const CONVERSATION_OWNER: &str =
        "SELECT user, garden FROM $id WHERE garden.deleting_at = NONE;";
    /// A pending action's conversation.
    pub const PENDING_ACTION_CONVERSATION: &str = "SELECT VALUE conversation FROM $id;";
}
```

`upsert_user` is called by `authz::authenticate` on every request, so it reads first and creates only on a miss; the new user's key is `rand::ulid()` generated by SurrealDB, because the port passes no id. `display_name` defaults to "Gardener" when the token has none.

```rust
#[async_trait::async_trait]
impl IdentityRepo for SurrealStore {
    async fn upsert_user(
        &self,
        auth_id: &AuthId,
        display_name: Option<String>,
    ) -> Result<UserId, StoreError> {
        const WHAT: &str = "upsert user";
        // A deleted account (its tombstone exists) is `NotFound`, which `authenticate` maps to
        // `401`, on both paths.
        match self.find_user(auth_id).await? {
            (_, true) => return Err(StoreError::NotFound),
            (Some(user), false) => return Ok(user),
            (None, false) => {}
        }
        let created = run_tx(&self.db, sql::CREATE_USER, |q| {
            q.bind(("auth_id", auth_id.as_str()))
                .bind(("name", display_name.clone()))
                .bind(("tomb", deleted_user_rid(auth_id)))
        })
        .await;
        match created {
            Ok(mut results) => {
                let ids: Vec<RecordId> = results.take(2).map_err(|e| take_failed(WHAT, e))?;
                ids.first()
                    .ok_or(StoreError::NotFound)
                    .and_then(|id| from_rid(id).map_err(|e| e.into_store(WHAT)))
            }
            // The webhook and a first request raced: the other one created the user.
            Err(DbFailure::UniqueIndex { index, .. }) if index == "user_auth" => {
                self.find_user(auth_id).await?.0.ok_or(StoreError::NotFound)
            }
            Err(failure) => Err(failure.into_store(WHAT)),
        }
    }

    async fn user_by_auth(&self, auth_id: &AuthId) -> Result<Option<UserId>, StoreError> {
        Ok(self.find_user(auth_id).await?.0)
    }

    async fn me(&self, user: UserId) -> Result<Me, StoreError> {
        const WHAT: &str = "me";
        let mut r = run_tx(&self.db, sql::ME, |q| q.bind(("user", rid(&user))))
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let row: Option<UserRow> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        Me::try_from(row.ok_or(StoreError::NotFound)?).map_err(|e| e.into_store(WHAT))
    }

    async fn update_me(&self, user: UserId, patch: UpdateMeRequest) -> Result<Me, StoreError> {
        const WHAT: &str = "update me";
        let mut r = run_tx(&self.db, sql::UPDATE_ME, |q| {
            q.bind(("user", rid(&user)))
                .bind(("name", patch.display_name.clone()))
                .bind(("consent", patch.location_consent))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let last = r.num_statements().saturating_sub(2);
        let row: Option<UserRow> = r.take(last).map_err(|e| take_failed(WHAT, e))?;
        Me::try_from(row.ok_or(StoreError::NotFound)?).map_err(|e| e.into_store(WHAT))
    }

    async fn register_push_token(
        &self,
        user: UserId,
        request: PushTokenRequest,
    ) -> Result<(), StoreError> {
        const WHAT: &str = "register push token";
        let platform =
            crate::rows::to_json("platform", &request.platform).map_err(|e| e.into_store(WHAT))?;
        run_tx(&self.db, sql::REGISTER_PUSH_TOKEN, |q| {
            q.bind(("push_token", crate::ids::push_token_rid(&request.token)))
                .bind(("user", rid(&user)))
                .bind(("platform", platform.clone()))
        })
        .await
        .map(drop)
        .map_err(|f| f.into_store(WHAT))
    }

    async fn remove_push_token(&self, user: UserId, token: &PushToken) -> Result<(), StoreError> {
        const WHAT: &str = "remove push token";
        run_tx(&self.db, sql::REMOVE_PUSH_TOKEN, |q| {
            q.bind(("push_token", crate::ids::push_token_rid(token)))
                .bind(("user", rid(&user)))
        })
        .await
        .map(drop)
        .map_err(|f| f.into_store(WHAT))
    }

    async fn push_targets(&self, users: &[UserId]) -> Result<Vec<PushTarget>, StoreError> {
        const WHAT: &str = "push targets";
        let users: Vec<RecordId> = users.iter().map(rid).collect();
        let mut r = run_tx(&self.db, sql::PUSH_TARGETS, |q| {
            q.bind(("users", users.clone()))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<PushTargetRow> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        rows.into_iter()
            .map(|row| {
                Ok(PushTarget {
                    user: from_rid(&row.user)?,
                    token: row.token.parse().map_err(|_| RowError::BadField {
                        field: "token",
                        problem: "not a push token".to_owned(),
                    })?,
                    platform: crate::rows::from_json(
                        "platform",
                        serde_json::Value::String(row.platform),
                    )?,
                })
            })
            .collect::<Result<_, RowError>>()
            .map_err(|e| e.into_store(WHAT))
    }

    async fn forget_push_tokens(&self, tokens: &[PushToken]) -> Result<(), StoreError> {
        const WHAT: &str = "forget push tokens";
        if tokens.is_empty() {
            return Ok(());
        }
        let ids: Vec<RecordId> = tokens.iter().map(crate::ids::push_token_rid).collect();
        run_tx(&self.db, sql::FORGET_PUSH_TOKENS, |q| {
            q.bind(("tokens", ids.clone()))
        })
        .await
        .map(drop)
        .map_err(|f| f.into_store(WHAT))
    }
}
```

---

## 9. Gardens, plants and the map

| Method | Statement | Database |
|---|---|---|
| `create_garden` | `CREATE_GARDEN`: the garden and the owner's `INSERT RELATION` in one transaction | [4, membership](../database/README.md#membership-) ✓ |
| `accept_invite` | `ACCEPT_INVITE`: a live invite of a live garden, then `INSERT RELATION` unless already a member (whose role never changes) | [4, membership](../database/README.md#membership-) |
| `garden` | `GARDEN`, with `my_role` and `member_count` | — |
| `set_soil_profile`, `soil_profile_failed`, `gardens_due_soil_profile` | `SET_SOIL_PROFILE` (`ready` clears `soil_retry_at`), `SOIL_PROFILE_FAILED` (`unavailable` until `$retry_at`, the profile kept), `DUE_SOIL_PROFILE` (`pending`, or `unavailable` whose retry time has come; the oldest retry first) | schema v1.4 `garden.soil_retry_at` |
| `put_map`, `map`, `current_map` | `PUT_MAP`, `CURRENT_MAP` | [4, garden map versions](../database/README.md#garden-map-versions-) ✓ |
| lists and edits | `GARDENS` (by name), `UPDATE_GARDEN`, `CHANGE_ROLE` and `REMOVE_MEMBER` (the last owner stays, `invalid_state`), `INVITES` (unused and unexpired, newest first), `MEMBERS` (by `since`), `PLANTS` (by name, with `latest_scan` and its thumbnail), `TIMELINE` (newest first, with the diagnosis and the highest-confidence height and width), `GROWTH_SERIES` (one point per scan and quantity) | [5](../database/README.md#5-reads-indexes-and-vector-search) |

```rust
/// `POST /gardens`: the garden and the caller's `owner` membership in one transaction, the
/// membership with [database 4]'s `INSERT RELATION` ✓ (it fails on an existing edge;
/// `RELATE` with a fixed id would overwrite it). The centroid is `type::point([lon, lat])`
/// ✓: a `(lon, lat)` literal doesn't take binds (a parse error on 3.3.4). Statements:
/// 0 `BEGIN`, 1 `CREATE`, 2 `INSERT RELATION`, 3 the garden read, 4 `COMMIT`.
pub const CREATE_GARDEN: &str = "BEGIN;
CREATE $garden SET name = $name, centroid = type::point([$lon, $lat]), location_precision = $precision,
  location_label = $label, time_zone = $time_zone;
INSERT RELATION INTO member_of { id: type::record('member_of', [$user, $garden]), in: $user, out: $garden, role: 'owner' };
SELECT *, 'owner' AS my_role, 1 AS member_count FROM ONLY $garden;
COMMIT;";

/// `POST /invites/{code}/accept` ([database 4], membership ✓): a live invite of a live
/// garden, then `INSERT RELATION` unless the caller is a member already, whose role is
/// never changed. Statements: 0 `BEGIN`, 1 `LET $inv`, 2 `IF` (unknown → `not_found`),
/// 3 `LET $member`, 4 `LET $joined`, 5 the result, 6 `COMMIT`.
pub const ACCEPT_INVITE: &str = "BEGIN;
LET $inv = (SELECT id, garden, role FROM invite WHERE code = $code AND used_at = NONE AND revoked_at = NONE
            AND expires_at > $now AND garden.deleting_at = NONE LIMIT 1)[0];
IF $inv = NONE { THROW 'not_found' };
LET $member = type::record('member_of', [$user, $inv.garden]);
LET $joined = IF array::len(SELECT VALUE id FROM $member) > 0 { false } ELSE {
  INSERT RELATION INTO member_of { id: $member, in: $user, out: $inv.garden, role: $inv.role };
  UPDATE $inv.id SET used_at = $now, used_by = $user;
  true
};
RETURN { garden: $inv.garden, joined: $joined };
COMMIT;";
```

The centroid is `type::point([$lon, $lat])`: ✓ a `($lon, $lat)` literal doesn't take parameters (a parse error on 3.3.4), `type::point($lon, $lat)` wants one argument, and a GeoJSON-shaped object isn't coerced ([database 8, caveat 17](../database/README.md#8-caveats)).

`Invite.created_by` is optional (contracts v0.9): `InviteRow.created_by` is `Option<RecordId>` because the field is `ON DELETE UNSET`, and the domain field is now `Option<UserId>` (section 21).

**The map upload** is one transaction: the version check, unknown plants refused, pins of plants being deleted dropped, new plants created, the version incremented, the map written, older maps deleted, and every plant's `position` set or unset. Two uploads from the same base conflict on the garden row; the loser's retry then fails the version check.

```rust
/// `PUT /gardens/{id}/map` in one transaction ([database 4] ✓): the `base_version`
/// check (`THROW` and nothing changes), unknown plants refused, pins of plants being
/// deleted dropped, new plants created, the version incremented, the map written, older
/// maps deleted, and every plant's `position` set or unset. Two concurrent uploads conflict
/// on the garden row; the retry then fails the version check.
pub const PUT_MAP: &str = "BEGIN;
LET $g = (SELECT map_version FROM ONLY $garden WHERE deleting_at = NONE);
IF $g = NONE { THROW 'not_found' };
IF $g.map_version != $base { THROW 'stale_map' };
IF array::len(array::filter($existing, |$p| array::len(SELECT VALUE id FROM $p WHERE garden = $garden) = 0)) > 0 { THROW 'unknown_plants' };
LET $dropped = (SELECT VALUE id FROM plant WHERE id IN $existing AND deleting_at != NONE);
FOR $n IN $new_plants {
  CREATE $n.id SET garden = $garden, name = $n.name, species = $n.species,
    species_status = IF $n.species != NONE { 'confirmed' } ELSE { 'unknown' }, created_at = $now;
};
LET $version = $g.map_version + 1;
LET $pins = array::filter($pins_in, |$p| !($p.plant IN $dropped));
UPDATE $garden SET map_version = $version;
CREATE garden_map SET garden = $garden, version = $version, beds = $beds, pins = $pins,
  north_deg = $north, created_by = $user, created_at = $now;
DELETE garden_map WHERE garden = $garden AND version < $version;
UPDATE plant SET position = NONE WHERE garden = $garden AND position != NONE AND !(id IN $pins.plant);
FOR $p IN $pins { UPDATE $p.plant SET position = { map_version: $version, x_m: $p.x_m, y_m: $p.y_m }; };
COMMIT;";
```

```rust
#[async_trait::async_trait]
impl MapRepo for SurrealStore {
    async fn map(&self, member: Membership<Viewer>) -> Result<Option<GardenMap>, StoreError> {
        self.current_map(member.garden()).await
    }

    async fn put_map(
        &self,
        editor: Membership<Editor>,
        write: MapWrite,
    ) -> Result<GardenMap, StoreError> {
        const WHAT: &str = "put map";
        let garden = rid(&editor.garden());
        let binds = MapBinds::new(&write).map_err(|e| e.into_store(WHAT))?;
        let result = run_tx(&self.db, sql::PUT_MAP, |q| {
            q.bind(("garden", garden.clone()))
                .bind(("user", rid(&editor.user())))
                .bind(("base", i64::from(write.upload.base_version)))
                .bind(("existing", binds.existing.clone()))
                .bind(("new_plants", binds.new_plants.clone()))
                .bind(("pins_in", binds.pins.clone()))
                .bind(("beds", binds.beds.clone()))
                .bind(("north", write.upload.north_deg))
                .bind(("now", dt(write.created_at)))
        })
        .await;
        match result {
            Ok(_) => {}
            Err(failure) if failure.thrown() == Some(thrown::STALE_MAP) => {
                let mut r = run_tx(&self.db, sql::MAP_VERSION, |q| {
                    q.bind(("garden", garden.clone()))
                })
                .await
                .map_err(|f| f.into_store(WHAT))?;
                let version: Option<i64> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
                return Err(StoreError::Conflict(Conflict::StaleMap {
                    current_version: u32_of("map_version", version.unwrap_or(0))
                        .map_err(|e| e.into_store(WHAT))?,
                }));
            }
            Err(failure) if failure.thrown() == Some(thrown::UNKNOWN_PLANTS) => {
                let mut r = run_tx(&self.db, sql::UNKNOWN_PLANTS, |q| {
                    q.bind(("garden", garden.clone()))
                        .bind(("existing", binds.existing.clone()))
                })
                .await
                .map_err(|f| f.into_store(WHAT))?;
                let unknown: Vec<RecordId> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
                return Err(StoreError::Rejected(DomainError::UnknownPlants {
                    plant_ids: unknown
                        .iter()
                        .map(from_rid)
                        .collect::<Result<_, _>>()
                        .map_err(|e: RowError| e.into_store(WHAT))?,
                }));
            }
            Err(failure) => return Err(failure.into_store(WHAT)),
        }
        self.current_map(editor.garden())
            .await?
            .ok_or(StoreError::NotFound)
    }

    async fn current_map(&self, garden: GardenId) -> Result<Option<GardenMap>, StoreError> {
        const WHAT: &str = "current map";
        let mut results = run_tx(&self.db, sql::CURRENT_MAP, |q| {
            q.bind(("garden", rid(&garden)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<MapRow> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        rows.into_iter()
            .next()
            .map(GardenMap::try_from)
            .transpose()
            .map_err(|e| e.into_store(WHAT))
    }
}
```

---

## 10. Scans and events

| Method | Statement | Database |
|---|---|---|
| `create_scan` | `CREATE_SCAN`: the scan (and a new plant) with `UPSERT usage_daily … scans += 1` and the limit `THROW` in one transaction | [4, limits](../database/README.md#limits-and-counters-) ✓ |
| `append_event` | `APPEND_EVENT`: the counter on the scan, the event at `scan_event:[scan, seq]` | [4, scan events](../database/README.md#scan-events-a-gap-free-seq-) ✓ |
| `events_after` | `EVENTS_AFTER`: a key-range read, scoped to the proof's garden | [4, scan events](../database/README.md#scan-events-a-gap-free-seq-) ✓ |
| `stream_state` | `STREAM_STATE`, with `step_claimed` from `pipeline_step != NONE` | [contracts 4](../architecture/contracts.md#when-the-stream-closes) |
| `add_image` | `ADD_IMAGE`: the status, `deleting_at`, kind limit and question re-checked; a guided check and its `guided` job created; a question photo's first step and job | [contracts 3](../architecture/contracts.md#post-scansidimages) |
| `remove_image`, `start_run`, `fail_scan`, `record_feedback` | `REMOVE_IMAGE`, `START_RUN`, `FAIL_SCAN`, `RECORD_FEEDBACK` | [contracts 3](../architecture/contracts.md#3-scan-rest-api) |
| `scan`, `scan_detail`, `snapshot` | the loaders of section 6 and `SNAPSHOT_PARTS` | [contracts 3](../architecture/contracts.md#3-scan-rest-api) |

```rust
/// The statements.
pub mod sql {
    /// The append ([database 4] ✓), with a guard for a missing scan. Statements: 0 `BEGIN`,
    /// 1 `LET`, 2 `IF`, 3 `CREATE` (the event), 4 `COMMIT`. `ts` is bound from the event, so
    /// the clock port controls it.
    pub const APPEND_EVENT: &str = "BEGIN;
LET $seq = (UPDATE ONLY $scan SET last_event_seq += 1 RETURN VALUE last_event_seq);
IF $seq = NONE { THROW 'not_found' };
CREATE type::record('scan_event', [$scan, $seq]) SET run = $run, type = $type, payload = $payload, ts = $ts;
COMMIT;";

    /// The index of the created event in [`APPEND_EVENT`].
    pub const APPEND_EVENT_RESULT: usize = 3;

    /// Replay after `Last-Event-ID` ([database 4] ✓): a key-range read, no index, scoped to the
    /// proof's garden and skipping a scan, plant or garden being deleted. Statements: 0 `IF`,
    /// 1 `SELECT`.
    pub const EVENTS_AFTER: &str =
        "IF array::len(SELECT VALUE id FROM $scan WHERE garden = $garden AND deleting_at = NONE
      AND plant.deleting_at = NONE AND garden.deleting_at = NONE) = 0 { THROW 'not_found' };
SELECT * FROM scan_event:[$scan, $after + 1]..=[$scan, ..];";

    /// What the closing rule needs ([contracts 4], when the stream closes).
    pub const STREAM_STATE: &str = "SELECT status, open_question.status AS question_status, pipeline_step != NONE AS step_claimed,
       array::len(SELECT VALUE id FROM guided_check WHERE scan = $parent.id AND status = 'analysing') AS checks_analysing
FROM ONLY $scan WHERE deleting_at = NONE;";
}
```

- **Gap-free `seq`.** The scan row is the counter; the increment and the `CREATE` commit together or not at all, so a conflict leaves no gap. ✓ 40 concurrent appends on a 4-thread runtime commit exactly 1…40 after retries (section 20). `DEFINE SEQUENCE` would leave gaps ([database 8, caveat 8](../database/README.md#8-caveats)).
- **Stored body.** `type` beside a `payload` object holding the rest of `ScanEventBody<Stored>` (image and mask ids, never URLs, [contracts 4](../architecture/contracts.md#transport)); the replay puts `type` back and decodes the tagged union.

```rust
/// A `scan_event` row. The body is split as stored: `type` beside a `payload` object holding
/// the rest of [`ScanEventBody<Stored>`]'s fields (image and mask ids, never URLs).
#[derive(Debug, Clone, SurrealValue)]
pub struct EventRow {
    /// `scan_event:[scan, seq]`.
    pub id: RecordId,
    /// The run.
    pub run: i64,
    /// The event type.
    #[surreal(rename = "type")]
    pub kind: String,
    /// The body without `type`.
    pub payload: serde_json::Value,
    /// When it was stored.
    pub ts: DateTime<Utc>,
}
```

```rust
#[async_trait::async_trait]
impl EventRepo for SurrealStore {
    async fn append_event(
        &self,
        scan: ScanId,
        event: NewEvent,
    ) -> Result<ScanEvent<Stored>, StoreError> {
        const WHAT: &str = "append event";
        let parts = EventParts::try_from(&event.body).map_err(|e| e.into_store(WHAT))?;
        let mut results = run_tx(&self.db, sql::APPEND_EVENT, |q| {
            q.bind(("scan", rid(&scan)))
                .bind(("run", i64::from(event.run.0)))
                .bind(("type", parts.kind))
                .bind(("payload", parts.payload.clone()))
                .bind(("ts", dt(event.ts)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let row: Option<EventRow> = results
            .take(sql::APPEND_EVENT_RESULT)
            .map_err(|e| take_failed(WHAT, e))?;
        row.ok_or(StoreError::NotFound)?
            .try_into()
            .map_err(|e: RowError| e.into_store(WHAT))
    }

    async fn events_after(
        &self,
        member: Membership<Viewer>,
        scan: ScanId,
        after: EventSeq,
    ) -> Result<Vec<ScanEvent<Stored>>, StoreError> {
        const WHAT: &str = "replay events";
        let mut results = run_tx(&self.db, sql::EVENTS_AFTER, |q| {
            q.bind(("scan", rid(&scan)))
                .bind(("garden", rid(&member.garden())))
                .bind(("after", seq_i64(after)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<EventRow> = results.take(1).map_err(|e| take_failed(WHAT, e))?;
        rows.into_iter()
            .map(|row| ScanEvent::try_from(row).map_err(|e| e.into_store(WHAT)))
            .collect()
    }

    async fn stream_state(&self, scan: ScanId) -> Result<StreamState, StoreError> {
        const WHAT: &str = "stream state";
        let mut results = run_tx(&self.db, sql::STREAM_STATE, |q| {
            q.bind(("scan", rid(&scan)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let row: Option<StreamStateRow> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        let row = row.ok_or(StoreError::NotFound)?;
        Ok(StreamState {
            status: row.status.into(),
            question_unresolved: matches!(
                row.question_status.as_deref(),
                Some("open" | "answering")
            ),
            guided_check_analysing: row.checks_analysing > 0,
            step_claimed: row.step_claimed,
        })
    }
}
```

```rust
/// `POST /scans`: the scan (and the new plant), counted in `usage_daily` in the same
/// transaction ([database 4], limits ✓: `+=` on a missing record starts from the default,
/// and the `THROW` over 30 rolls the scan back). Statements: 0 `BEGIN`, 1 the plant `IF`,
/// 2 `CREATE` scan, 3 `LET $count`, 4 the limit `IF`, 5 the scan read, 6 `COMMIT`.
pub const CREATE_SCAN: &str = "BEGIN;
IF $new_plant != NONE {
  CREATE $plant SET garden = $garden, name = $new_plant.name, species = $new_plant.species,
    species_status = IF $new_plant.species != NONE { 'confirmed' } ELSE { 'unknown' }, created_at = $now;
} ELSE IF array::len(SELECT VALUE id FROM $plant WHERE garden = $garden AND deleting_at = NONE) = 0 {
  THROW 'not_found';
};
CREATE $scan SET garden = $garden, plant = $plant, created_by = $user, created_at = $now;
LET $count = (UPSERT ONLY $usage SET scans += 1 RETURN VALUE scans);
IF $count > $limit { THROW 'budget_exceeded' };
SELECT *, created_by.display_name AS creator_name FROM ONLY $scan;
COMMIT;";
```

The daily limit is counted in the scan's transaction, so a `THROW` over 30 rolls the scan and the count back together ✓; the error is built with `domain::rules::limits::check_daily_scans`, so its `resets_at` follows the rule.

---

## 11. Steps between jobs

[Contracts 3](../architecture/contracts.md#scan-lifecycle), steps between jobs; [database 4, steps between jobs](../database/README.md#steps-between-jobs-) ✓. A scan holds at most one claim (`pipeline_step`, its run, its stamp). Every entry point claims before it works; a step commits its output in one transaction that first re-checks the claim and then hands over; the sweep re-claims a stale step by its old stamp.

| Method | Statement |
|---|---|
| `claim_step` | `CLAIM_STEP` = [`claim_entry!`](#7-shared-fragments): the compare-and-set on `pipeline_step = NONE`, or `rescore_pending` when another step holds it, or `not_needed` for a scan being deleted or in `pending`, `queued` or `failed` |
| `commit_step` | `COMMIT_RETRIEVE`, `COMMIT_SCORE`, `COMMIT_ASK`, `COMMIT_VERIFY`, `PlanRepo`'s `COMMIT_PLAN`, `COMMIT_RELEASE`: each `claim_check!`, the output, `handover!` |
| `stale_steps` | `STALE_STEPS`: three indexed reads (`scan_stalled`), 2 minutes for `retrieve` and `score`, 5 for `plan` |
| `reclaim_step` | `RECLAIM_STEP`, database 4's re-claim with `$now` |

```rust
/// The statements. Each commit is one transaction: [`claim_check!`] first (a lost claim throws
/// `claim_lost` and nothing is written), the output, then [`handover!`], whose `RETURN` is the
/// last statement before `COMMIT`.
pub mod sql {
    use crate::fragments::{
        claim_check, claim_entry, evidence_write, handover, job_enqueue, observation_write,
        resolve_question,
    };

    /// An entry point's claim, alone in its transaction.
    pub const CLAIM_STEP: &str = concat!("BEGIN;\n", claim_entry!(), "COMMIT;");

    /// `retrieve`: the measurement observations, the measured answers, the candidate set and
    /// the weather snapshot, and the `vlm` job (checked against an existing one of the run).
    /// Hands over by releasing when a job was queued, else to `score`.
    pub const COMMIT_RETRIEVE: &str = concat!(
        "BEGIN;\n",
        claim_check!(),
        observation_write!(),
        evidence_write!(),
        "UPDATE $scan SET candidates = $candidates, weather = $weather;\n",
        job_enqueue!(),
        handover!(),
        "COMMIT;"
    );

    /// `score`: the diagnosis with a compare-and-set on `scan.diagnosis` (its `supersedes`, or
    /// `NONE` for the run's first), the status move, a resolved question, and the hand-over
    /// (`plan`, or release).
    pub const COMMIT_SCORE: &str = concat!(
        "BEGIN;\n",
        claim_check!(),
        "CREATE $diagnosis.id CONTENT $diagnosis.row;
LET $dwon = (UPDATE $scan SET diagnosis = $diagnosis.id WHERE diagnosis = $expected_diagnosis RETURN VALUE diagnosis);
IF array::len($dwon) = 0 { THROW 'claim_lost' };
IF $status != NONE { UPDATE $scan SET status = $status; };
",
        resolve_question!(),
        handover!(),
        "COMMIT;"
    );

    /// `score` that asks: the question, `awaiting_answer`, `questions_asked += 1`, release.
    pub const COMMIT_ASK: &str = concat!(
        "BEGIN;\n",
        claim_check!(),
        "CREATE $question.id CONTENT $question.row;
UPDATE $scan SET status = 'awaiting_answer', open_question = $question.id, questions_asked += 1;
",
        resolve_question!(),
        handover!(),
        "COMMIT;"
    );

    /// `score` that goes outside the candidate set: measured answers for the added
    /// candidates, the `verify` job (or none when the measurements settled everything), the
    /// candidates appended, then release (or `score` when no job was queued).
    pub const COMMIT_VERIFY: &str = concat!(
        "BEGIN;\n",
        claim_check!(),
        evidence_write!(),
        job_enqueue!(),
        "UPDATE $scan SET candidates = array::concat(candidates, $added_candidates);\n",
        handover!(),
        "COMMIT;"
    );

    /// Nothing to write: only the hand-over by the rescore rule.
    pub const COMMIT_RELEASE: &str = concat!("BEGIN;\n", claim_check!(), handover!(), "COMMIT;");

    /// Claims older than their limit (index `scan_stalled` ✓): 2 minutes for `retrieve` and
    /// `score`, 5 for `plan`, on scans that aren't `pending`, `failed` or being deleted.
    pub const STALE_STEPS: &str = "SELECT id, pipeline_step, pipeline_step_run, pipeline_step_at FROM scan
  WHERE pipeline_step = 'retrieve' AND pipeline_step_at < $short AND !(status IN ['pending', 'failed'])
    AND deleting_at = NONE AND plant.deleting_at = NONE AND garden.deleting_at = NONE;
SELECT id, pipeline_step, pipeline_step_run, pipeline_step_at FROM scan
  WHERE pipeline_step = 'score' AND pipeline_step_at < $short AND !(status IN ['pending', 'failed'])
    AND deleting_at = NONE AND plant.deleting_at = NONE AND garden.deleting_at = NONE;
SELECT id, pipeline_step, pipeline_step_run, pipeline_step_at FROM scan
  WHERE pipeline_step = 'plan' AND pipeline_step_at < $long AND !(status IN ['pending', 'failed'])
    AND deleting_at = NONE AND plant.deleting_at = NONE AND garden.deleting_at = NONE;";

    /// The sweep's re-claim by the old stamp and run ([database 4] ✓); `NONE` back means
    /// someone else has it. Its new stamp makes a slow original's commit throw `claim_lost`.
    pub const RECLAIM_STEP: &str = "UPDATE ONLY $scan SET pipeline_step_at = $now
  WHERE pipeline_step = $step AND pipeline_step_at = $seen AND pipeline_step_run = run AND pipeline_step_run = $run
    AND plant.deleting_at = NONE AND garden.deleting_at = NONE
  RETURN AFTER;";
}
```

**Hand-over per output.** The step's own next step is computed in Rust and bound as `$handover`; [`handover!`](#7-shared-fragments) replaces it with `score` when `rescore_pending` was set while the step ran:

| Output | Own hand-over |
|---|---|
| `Retrieve` | release when a job was queued, else `score` |
| `Score` | `plan` when `needs_plan`, else release |
| `Ask` | release (`awaiting_answer`) |
| `Verify` | release when a job was queued, else `score` |
| `Plan` | release (and `diagnosed → planned`) |
| `Release` | release |

```rust
#[async_trait::async_trait]
impl StepRepo for SurrealStore {
    async fn claim_step(
        &self,
        scan: ScanId,
        step: PipelineStep,
        now: Timestamp,
    ) -> Result<ClaimAttempt, StoreError> {
        const WHAT: &str = "claim step";
        let mut results = run_tx(&self.db, sql::CLAIM_STEP, |q| {
            q.bind(("scan", rid(&scan)))
                .bind(("step", PipelineStepRow::from(step)))
                .bind(("now", dt(now)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let last = results.num_statements().saturating_sub(2);
        let row: Option<ClaimRow> = results.take(last).map_err(|e| take_failed(WHAT, e))?;
        claim_attempt(&row.ok_or(StoreError::NotFound)?, scan, step, now)
            .map_err(|e| e.into_store(WHAT))
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one arm per StepOutput variant, each a bind list"
    )]
    async fn commit_step(
        &self,
        claim: StepClaim,
        output: StepOutput,
        now: Timestamp,
    ) -> Result<Option<StepClaim>, StoreError> {
        const WHAT: &str = "commit step";
        let row_err = |e: RowError| e.into_store(WHAT);
        let next = match output {
            StepOutput::Retrieve {
                observations,
                evidence,
                candidates,
                weather,
                job,
            } => {
                let obs = observations
                    .as_ref()
                    .map(|w| ObservationWriteRow::new(claim.scan, w))
                    .transpose()
                    .map_err(row_err)?;
                let measured = ObservationWriteRow::measured_images(observations.as_ref());
                let answers =
                    answer_rows(claim.scan, claim.run, &evidence, None, now).map_err(row_err)?;
                let candidates = to_json("candidates", &candidates).map_err(row_err)?;
                let weather = weather
                    .as_ref()
                    .map(|w| to_json("weather", w))
                    .transpose()
                    .map_err(row_err)?;
                let queued = job.is_some();
                let job = job
                    .as_ref()
                    .map(JobWriteRow::new)
                    .transpose()
                    .map_err(row_err)?;
                let handover = (!queued).then_some(PipelineStep::Score);
                let next = self
                    .send_commit(sql::COMMIT_RETRIEVE, &claim, handover, now, |q| {
                        q.bind(("obs", obs.clone()))
                            .bind(("measured_images", measured.clone()))
                            .bind(("answers", answers.clone()))
                            .bind(("candidates", candidates.clone()))
                            .bind(("weather", weather.clone()))
                            .bind(("job", job.clone()))
                    })
                    .await?;
                if queued {
                    self.jobs_queued.notify_waiters();
                }
                next
            }
            StepOutput::Score {
                diagnosis,
                status,
                resolved_question,
                needs_plan,
            } => {
                let row = DiagnosisWriteRow::try_from(&diagnosis).map_err(row_err)?;
                let expected = opt_rid(diagnosis.supersedes.as_ref());
                let status = status.map(ScanStatusRow::from);
                let resolved = opt_rid(resolved_question.as_ref());
                let handover = needs_plan.then_some(PipelineStep::Plan);
                self.send_commit(sql::COMMIT_SCORE, &claim, handover, now, |q| {
                    q.bind(("diagnosis", row.clone()))
                        .bind(("expected_diagnosis", expected.clone()))
                        .bind(("status", status))
                        .bind(("resolved", resolved.clone()))
                })
                .await?
            }
            StepOutput::Ask {
                question,
                resolved_question,
            } => {
                let row = QuestionWriteRow::new(claim.scan, &question).map_err(row_err)?;
                let resolved = opt_rid(resolved_question.as_ref());
                self.send_commit(sql::COMMIT_ASK, &claim, None, now, |q| {
                    q.bind(("question", row.clone()))
                        .bind(("resolved", resolved.clone()))
                })
                .await?
            }
            StepOutput::Verify {
                job,
                evidence,
                added_candidates,
            } => {
                let answers =
                    answer_rows(claim.scan, claim.run, &evidence, None, now).map_err(row_err)?;
                let added = to_json("candidates", &added_candidates).map_err(row_err)?;
                let queued = job.is_some();
                let job = job
                    .as_ref()
                    .map(JobWriteRow::new)
                    .transpose()
                    .map_err(row_err)?;
                let handover = (!queued).then_some(PipelineStep::Score);
                let next = self
                    .send_commit(sql::COMMIT_VERIFY, &claim, handover, now, |q| {
                        q.bind(("measured_images", Vec::<RecordId>::new()))
                            .bind(("answers", answers.clone()))
                            .bind(("job", job.clone()))
                            .bind(("added_candidates", added.clone()))
                    })
                    .await?;
                if queued {
                    self.jobs_queued.notify_waiters();
                }
                next
            }
            StepOutput::Plan(write) => {
                self.commit_plan(&claim, &write, now)
                    .await?
                    .map(|step| StepClaim {
                        scan: claim.scan,
                        step: step.into(),
                        run: claim.run,
                        stamp: now,
                    })
            }
            StepOutput::Release => {
                self.send_commit(sql::COMMIT_RELEASE, &claim, None, now, |q| q)
                    .await?
            }
        };
        Ok(next)
    }

    async fn stale_steps(&self, now: Timestamp) -> Result<Vec<StaleStep>, StoreError> {
        const WHAT: &str = "stale steps";
        let short = now.minus(PipelineStep::Score.stale_after());
        let long = now.minus(PipelineStep::Plan.stale_after());
        let mut results = run_tx(&self.db, sql::STALE_STEPS, |q| {
            q.bind(("short", dt(short))).bind(("long", dt(long)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let mut stale = Vec::new();
        for index in 0..3 {
            let rows: Vec<ClaimedScanRow> =
                results.take(index).map_err(|e| take_failed(WHAT, e))?;
            for row in rows {
                stale.push(StaleStep::try_from(row).map_err(|e| e.into_store(WHAT))?);
            }
        }
        Ok(stale)
    }

    async fn reclaim_step(
        &self,
        stale: StaleStep,
        now: Timestamp,
    ) -> Result<Option<StepClaim>, StoreError> {
        const WHAT: &str = "reclaim step";
        let mut results = run_tx(&self.db, sql::RECLAIM_STEP, |q| {
            q.bind(("scan", rid(&stale.scan)))
                .bind(("step", PipelineStepRow::from(stale.step)))
                .bind(("seen", dt(stale.seen)))
                .bind(("run", i64::from(stale.run.0)))
                .bind(("now", dt(now)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let row: Option<ClaimedScanRow> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        Ok(row.map(|_| StepClaim {
            scan: stale.scan,
            step: stale.step,
            run: stale.run,
            stamp: now,
        }))
    }
}
```

- The two claim statements are conditional on each other, as in [database 4](../database/README.md#steps-between-jobs-): run unconditionally, the second (`SET rescore_pending = true WHERE pipeline_step != NONE`) also runs after a successful claim and flags the scan the caller just claimed (✓ on the pinned image, a fresh claim then reads `rescore_pending = true`).
- **Stamps are compared exactly.** The stamp is the bound `$now`, stored with full precision, and the commit compares `pipeline_step_at != $stamp`. The domain's `Timestamp` keeps nanoseconds in memory and writes whole seconds only on the wire (contracts 12, question 12; section 21).
- The diagnosis is written with a compare-and-set on `scan.diagnosis` (its `supersedes`, or `NONE`); under the claim nothing else writes it, so a loss throws `claim_lost`.
- A job queued by a commit signals `jobs_queued` after the commit returns.

`send_commit` takes the per-output binds as a second closure. Its bound is `Fn(Query<'a, Any>) -> Query<'a, Any> + Send + Sync`: `run_tx`'s closure captures it by reference, and a reference is `Send` only when its referent is `Sync` ([std, `Sync`](https://doc.rust-lang.org/std/marker/trait.Sync.html)).

---

## 12. Observations, questions and indicator answers

**Indicator answers** close [domain 12](domain.md#12-open-questions) item 7: nothing stored the measured and user answers between `retrieve` and `score`. The new table `indicator_answer:[scan, run, indicator_id]` ([database 4, indicator answers](../database/README.md#indicator-answers-), schema v1.4) holds every answer of a run, the VLM's included, and its write keeps the higher-ranked one (user 3 > measurement 2 > vlm 1, [contracts 7](../architecture/contracts.md#measurement)). ✓ A measured answer replaces a VLM one, a user answer survives a later measured one, and an answer of the same rank replaces the stored one.

```rust
/// The rank of an evidence source (contracts 7: user, then measurement, then vlm).
pub const fn rank(source: EvidenceSource) -> i64 {
    match source {
        EvidenceSource::User => 3,
        EvidenceSource::Measurement => 2,
        EvidenceSource::Vlm => 1,
    }
}

/// `$answers` for one run's evidence.
pub fn answer_rows(
    scan: ScanId,
    run: Run,
    evidence: &[IndicatorEvidence],
    question: Option<QuestionId>,
    now: Timestamp,
) -> Result<Vec<AnswerWriteRow>, RowError> {
    evidence
        .iter()
        .map(|e| {
            Ok(AnswerWriteRow {
                id: indicator_answer_rid(scan, run, &e.indicator_id),
                rank: rank(e.source),
                row: AnswerRow {
                    scan: rid(&scan),
                    run: i64::from(run.0),
                    source: e.source.into(),
                    evidence: to_json("evidence", e)?,
                    image: e.measured.as_ref().map(|m| rid(&m.image_id)),
                    question: question
                        .filter(|_| e.source == EvidenceSource::User)
                        .map(|q| rid(&q)),
                    answered_at: dt(now),
                },
            })
        })
        .collect()
}
```

The fragments that write observations (replacement per image, [database 5](../database/README.md#5-reads-indexes-and-vector-search)) and answers:

```rust
/// A stage's observations, replacing that stage's earlier ones for the images it covers, and
/// those tied to no image when `replace_imageless` ([database 5]: replacement per image).
/// Binds: `$scan`, `$obs` (an [`crate::repo::analysis::ObservationWriteRow`] or `NONE`).
/// Measurement observations get the top-level `plant` and `taken_at` (the scan's
/// `created_at`) for the growth series.
macro_rules! observation_write {
    () => {
        "IF $obs != NONE {
  LET $sc = (SELECT plant, created_at FROM ONLY $scan);
  DELETE observation WHERE scan = $scan AND pipeline_stage = $obs.stage
    AND (image IN $obs.images OR ($obs.replace_imageless AND image = NONE));
  FOR $o IN $obs.rows {
    CREATE $o.id CONTENT $o.row;
    IF $o.row.quantity != NONE { UPDATE $o.id SET plant = $sc.plant, taken_at = $sc.created_at; };
  };
  FOR $m IN $obs.masks { CREATE $m.id CONTENT $m.row; };
};
"
    };
}

/// Indicator answers with the ranking user > measurement > vlm (contracts 7; schema v1.4
/// `indicator_answer`): re-measured photos first lose their measured answers, then each answer
/// is written only when its rank is at least the stored one. Binds: `$scan`, `$run`,
/// `$measured_images`, `$answers` ([`crate::repo::analysis::AnswerWriteRow`]s).
macro_rules! evidence_write {
    () => {
        "IF array::len($measured_images) > 0 {
  DELETE indicator_answer WHERE scan = $scan AND run = $run AND source = 'measurement' AND image IN $measured_images;
};
FOR $a IN $answers {
  IF ((SELECT VALUE rank FROM ONLY $a.id) ?? 0) <= $a.rank { UPSERT $a.id CONTENT $a.row; };
};
"
    };
}
```

**Question steps** ([contracts 3](../architecture/contracts.md#post-scansidanswers), a photo answer that stalls): `question.step`, `step_at` and the question's run, claimed and re-claimed like the scan's steps.

| Method | Statement |
|---|---|
| `advance_question_step` | `QUESTION_TO_MEASURING`, `QUESTION_TO_VERIFYING`, `QUESTION_TO_SCORE` (each `question_step_check!` first) |
| `stalled_question_steps` | `STALLED_QUESTION_STEPS` (index `question_stalled`) |
| `reclaim_question_step` | `RECLAIM_QUESTION_STEP` |
| `similar_cases` | `SIMILAR_CASES`: exact cosine over `scan_image.embedding` within the garden ([database 5](../database/README.md#5-reads-indexes-and-vector-search)) |
| `store_partial`, `store_observations` | `STORE_PARTIAL` (under the lease), `STORE_OBSERVATIONS` (under the claim); the diff is the write's ids against the ids it replaces, read first (`REPLACED_IDS`) |
| `answer_question` | `ANSWER_QUESTION`: the user's evidence (rank 3), the question answered, the `score` claim |
| `drop_photo_answer`, `questions_due`, `continue_without_answer` | `DROP_PHOTO_ANSWER`, `QUESTIONS_DUE`, `CONTINUE_WITHOUT_ANSWER` |
| `finish_guided_check` | `FINISH_GUIDED_CHECK`: the check found from the job's payload image; the `score` claim only on a `diagnosed` or `planned` scan (`$only_statuses`) |

`QuestionStepOutput::Score` stores the measurements, clears the question's step (it stays `answering` until `score` resolves it) and claims the scan's `score` step **in one transaction**, so a crash between them can't strand the question; `advance_question_step` returns `QuestionAdvance::Scored(ClaimAttempt)` for it (a domain change, section 21).

```rust
/// Measured and nothing left to ask: store them, clear the step (the question stays
/// `answering` until `score` resolves it) and claim the scan's `score` step, all in one
/// transaction. Returns the claim outcome last.
pub const QUESTION_TO_SCORE: &str = concat!(
    "BEGIN;\n",
    question_step_check!(),
    "LET $scan = $q.scan;\n",
    observation_write!(),
    evidence_write!(),
    "UPDATE $question SET step = NONE, step_at = NONE;\n",
    claim_entry!(),
    "COMMIT;"
);
```

```rust
/// A photo answer's step re-check ([contracts 3], a photo answer that stalls). Binds:
/// `$question`, `$step`, `$stamp`, `$run`. Leaves `$q` with the question's scan.
macro_rules! question_step_check {
    () => {
        "LET $q = (SELECT scan, status, step, step_at, run, scan.run AS scan_run,
                 scan.plant.deleting_at AS plant_deleting, scan.garden.deleting_at AS garden_deleting
          FROM ONLY $question);
IF $q = NONE OR $q.status != 'answering' OR $q.step != $step OR $q.step_at != $stamp OR $q.run != $q.scan_run
   OR $q.run != $run OR $q.plant_deleting != NONE OR $q.garden_deleting != NONE { THROW 'question_step_lost'; };
"
    };
}
```

---

## 13. Plans and tasks

[Database 4, plan writes](../database/README.md#plan-writes-compare-and-set-) ✓ and [alert dedupe](../database/README.md#alert-dedupe-) ✓; [contracts 7](../architecture/contracts.md#actionplan), re-planning.

| Method | Statement |
|---|---|
| `write_plan` | `COMMIT_PLAN`: `claim_check!`, `plan_cas!`, `diagnosed → planned`, `handover!` (the same transaction as `commit_step(Plan)`) |
| `write_revision` | `WRITE_REVISION`: `plan_cas!` with `$expected = base_plan_id`, no claim, never retried on a lost compare-and-set |
| `create_alert_task` | `CREATE_ALERT_TASK`: the live-plant check and the `CREATE`; the `task_alert` index makes a duplicate `Ok(None)` |
| reads | `CURRENT_PLAN`, `PLAN` (the plan with its open question and task ids), `TASKS` (by due date, undated last, cursor on that key), `TASK`, `ACTIVE_TRIGGERS` (with the step's title and detail from its plan) |
| task writes | `CREATE_TASK` (plant and scan checked live in the insert's transaction ✓), `UPDATE_TASK` (only `user` and `chat` tasks change their text; a repeating task's next occurrence in the same transaction), `DELETE_TASK` |

```rust
/// The plan compare-and-set ([database 4], plan writes ✓), with what a supersede does in the
/// same transaction ([contracts 7]: the old plan's open tasks are dismissed with
/// `plan_superseded` and its trigger rules removed) and the new tasks and rules, each skipped
/// when its plant isn't live in the garden ([contracts 7], tasks only on live plants).
/// Binds: `$scan`, `$plan` ([`crate::repo::plans::PlanWriteRow`]), `$expected`, `$tasks`,
/// `$triggers`, `$now`.
macro_rules! plan_cas {
    () => {
        "CREATE $plan.id CONTENT $plan.row;
LET $won = (UPDATE $scan SET plan = $plan.id WHERE plan = $expected RETURN VALUE plan);
IF array::len($won) = 0 { THROW 'stale_plan' };
IF $expected != NONE {
  UPDATE task SET dismissed_at = $now, dismissed_reason = 'plan_superseded'
    WHERE plan = $expected AND done_at = NONE AND dismissed_at = NONE;
  UPDATE plan_trigger SET active = false WHERE plan = $expected;
};
FOR $t IN $tasks {
  LET $p = $t.row.plant;
  IF $p = NONE OR array::len(SELECT VALUE id FROM $p WHERE garden = $plan.row.garden AND deleting_at = NONE) > 0 {
    CREATE $t.id CONTENT $t.row;
  };
};
FOR $r IN $triggers {
  LET $p = $r.row.plant;
  IF array::len(SELECT VALUE id FROM $p WHERE garden = $plan.row.garden AND deleting_at = NONE) > 0 {
    CREATE $r.id CONTENT $r.row;
  };
};
"
    };
}
```

- ✓ A losing write leaves nothing: the `THROW` (or, when two writers create the same version, the `action_plan_version` unique index, which fails before the `THROW` runs) rolls back the new plan, and the store reads the plan that won into `StalePlan { current }`.
- The supersede dismisses the old plan's open tasks with `plan_superseded` and deactivates its trigger rules in the same transaction ✓; each new task and rule is skipped when its plant isn't live in the garden ([contracts 7](../architecture/contracts.md#task), tasks only on live plants).
- `task_ids` aren't stored on the plan: they are read back after the write (`PLAN_TASK_IDS`).

```rust
/// `$plan` of [`plan_cas!`](crate::fragments).
#[derive(Debug, Clone, SurrealValue)]
pub struct PlanWriteRow {
    /// `action_plan:<ulid>`.
    pub id: RecordId,
    /// The row.
    pub row: PlanRow,
}

/// A `task` row ([database 3]); `state` is computed by the schema.
#[derive(Debug, Clone, SurrealValue)]
pub struct TaskRow {
    /// Its garden.
    pub garden: RecordId,
    /// Its plant.
    pub plant: Option<RecordId>,
    /// Its plan version.
    pub plan: Option<RecordId>,
    /// The plan step.
    pub plan_step_id: Option<String>,
    /// A guided-check task's scan.
    pub scan: Option<RecordId>,
    /// Title.
    pub title: String,
    /// Detail.
    pub detail: Option<String>,
    /// Due date, `YYYY-MM-DD`.
    pub due: Option<String>,
    /// Repeat.
    pub repeat: Option<RepeatRow>,
    /// Who made it.
    pub source: TaskSourceRow,
    /// The rule that fired.
    pub trigger: Option<TaskTriggerRow>,
    /// The dedupe key, server-side only.
    pub alert_key: Option<String>,
    /// The user who made it; `NONE` once deleted.
    pub created_by: Option<RecordId>,
    /// When done.
    pub done_at: Option<DateTime<Utc>>,
    /// When dismissed.
    pub dismissed_at: Option<DateTime<Utc>>,
    /// Why dismissed.
    pub dismissed_reason: Option<DismissedReasonRow>,
    /// When created.
    pub created_at: DateTime<Utc>,
}
```

```rust
impl SurrealStore {
    /// The scan's garden (it never changes once the scan exists).
    pub(crate) async fn scan_garden(&self, scan: ScanId) -> Result<GardenId, StoreError> {
        const WHAT: &str = "scan's garden";
        let mut results = run_tx(&self.db, sql::SCAN_GARDEN, |q| q.bind(("scan", rid(&scan))))
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let garden: Option<RecordId> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        from_rid(&garden.ok_or(StoreError::NotFound)?).map_err(|e| e.into_store(WHAT))
    }

    /// The rows of a plan write.
    pub(crate) fn plan_binds(write: &PlanWrite, garden: GardenId) -> Result<PlanBinds, RowError> {
        let garden_rid = rid(&garden);
        Ok(PlanBinds {
            plan: PlanWriteRow::new(write, garden_rid.clone())?,
            expected: opt_rid(write.expected.as_ref()),
            tasks: write
                .tasks
                .iter()
                .map(|t| TaskWriteRow::new(t, garden))
                .collect(),
            triggers: write
                .triggers
                .iter()
                .map(|r| TriggerWriteRow::new(r, write.plan.id, garden_rid.clone()))
                .collect(),
        })
    }

    /// Maps a failed plan write: `stale_plan` becomes `StalePlan` with the plan that won.
    pub(crate) async fn plan_failure(
        &self,
        scan: ScanId,
        failure: DbFailure,
        what: &'static str,
    ) -> StoreError {
        // The THROW, or the unique `action_plan (scan, version)` backstop when two writers both
        // create the same version ([database 4] ✓): the CREATE fails before the THROW runs.
        let lost = failure.thrown() == Some(thrown::STALE_PLAN)
            || matches!(&failure, DbFailure::UniqueIndex { index, .. } if index == "action_plan_version");
        if !lost {
            return failure.into_store(what);
        }
        let current = async {
            let mut results = run_tx(&self.db, sql::CURRENT_PLAN_ID, |q| {
                q.bind(("scan", rid(&scan)))
            })
            .await
            .ok()?;
            let plan: Option<RecordId> = results.take(0).ok()?;
            plan.and_then(|p| from_rid::<PlanId>(&p).ok())
        }
        .await;
        StoreError::Conflict(Conflict::StalePlan { current })
    }

    /// The plan as written, with the task ids it created.
    pub(crate) async fn written_plan(
        &self,
        mut plan: ActionPlan,
    ) -> Result<ActionPlan, StoreError> {
        const WHAT: &str = "plan task ids";
        let mut results = run_tx(&self.db, sql::PLAN_TASK_IDS, |q| {
            q.bind(("plan", rid(&plan.id)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let ids: Vec<RecordId> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        plan.task_ids = ids
            .iter()
            .map(from_rid::<TaskId>)
            .collect::<Result<_, _>>()
            .map_err(|e| e.into_store(WHAT))?;
        Ok(plan)
    }

    /// [`sql::COMMIT_PLAN`]: returns the next step's name (`NONE` when released).
    pub(crate) async fn commit_plan(
        &self,
        claim: &StepClaim,
        write: &PlanWrite,
        now: Timestamp,
    ) -> Result<Option<PipelineStepRow>, StoreError> {
        const WHAT: &str = "write plan";
        let garden = self.scan_garden(claim.scan).await?;
        let binds = Self::plan_binds(write, garden).map_err(|e| e.into_store(WHAT))?;
        let result = run_tx(&self.db, sql::COMMIT_PLAN, |q| {
            crate::repo::steps::bind_claim(q, claim)
                .bind(("plan", binds.plan.clone()))
                .bind(("expected", binds.expected.clone()))
                .bind(("tasks", binds.tasks.clone()))
                .bind(("triggers", binds.triggers.clone()))
                .bind(("handover", None::<PipelineStepRow>))
                .bind(("now", dt(now)))
        })
        .await;
        let mut results = match result {
            Ok(results) => results,
            Err(failure) => return Err(self.plan_failure(claim.scan, failure, WHAT).await),
        };
        let last = results.num_statements().saturating_sub(2);
        results.take(last).map_err(|e| take_failed(WHAT, e))
    }
}
```

---

## 14. Jobs and workers

[Contracts 5](../architecture/contracts.md#5-worker-api-and-jobs); [database 4, job claim](../database/README.md#job-claim-with-slots-) ✓.

| Method | Statement |
|---|---|
| `enqueue_job` | `ENQUEUE_JOB`, then `jobs_queued.notify_waiters()` after the commit |
| `live_lease` | `CHECK_LEASE`: `Some` for a live lease, `None` for a job this lease already finished (a late event, dropped with `204`), `LeaseLost` otherwise |
| `claim_job` | `CLAIM_JOB` |
| `heartbeat` | `HEARTBEAT`: the worker row with `versions` verbatim, lease extension within `claimed_at + max_runtime`, the jobs to cancel |
| `check_lease` | `CHECK_LEASE`; none back is `Conflict::LeaseLost { job_id }` |
| `requeue_stale` | `REQUEUE_STALE`, then `notify_waiters()` when any job went back to `queued` |
| `perception_alive`, `live_workers`, `delete_finished_jobs` | `PERCEPTION_ALIVE`, `LIVE_WORKERS`, `DELETE_FINISHED_JOBS` |
| `complete_job` | `LEASE_STATE`, then `COMPLETE_DONE_CLAIM` / `COMPLETE_DONE` or `COMPLETE_FAILED` (below) |
| `job_image`, `status` | `JOB_IMAGE` (only an image the leased job's payload names), `STATUS` |

```rust
/// The claim ([database 4] ✓): counts the worker's leased jobs against `slots`, then
/// leases the oldest eligible queued job of its kinds, re-checking `status = 'queued'`. The
/// fallback's filter holds a photo answer's segmentation job while a perception worker is
/// alive. Added to the documented statement: the fallback's `max_runtime` is doubled at
/// claim ([contracts 5]), and a run's first job moves the scan from `queued` to
/// `analysing` in the same transaction. Statements: 0 `BEGIN`, 1 `LET $held`,
/// 2 `LET $perception_alive`, 3 `LET $claimed`, 4 `LET $job`, 5 `LET $started`,
/// 6 the result, 7 `COMMIT`.
pub const CLAIM_JOB: &str = "BEGIN;
LET $held = (SELECT count() AS n FROM job WHERE worker = $worker AND status = 'leased' GROUP ALL)[0].n ?? 0;
LET $perception_alive = ((SELECT count() AS n FROM worker WHERE caps CONTAINS 'perception' AND last_heartbeat > $now - 30s GROUP ALL)[0].n ?? 0) > 0;
LET $claimed = IF $held < $slots {
  (UPDATE (SELECT VALUE id FROM job
          WHERE status = 'queued' AND kind IN $kinds AND queued_at <= $eligible_before
            AND (!$hold_question OR question = NONE OR !$perception_alive)
          ORDER BY queued_at LIMIT 1)
  SET status = 'leased', worker = $worker, lease_token = rand::ulid(),
      claimed_at = $now, lease_until = $now + 2m,
      max_runtime = IF $on_fallback { max_runtime * 2 } ELSE { max_runtime }
  WHERE status = 'queued'
  RETURN AFTER)
} ELSE { [] };
LET $job = $claimed[0];
LET $started = IF $job != NONE AND $job.run != NONE {
  array::len(UPDATE $job.scan SET status = 'analysing' WHERE status = 'queued' AND run = $job.run RETURN VALUE id) > 0
} ELSE { false };
RETURN { job: $job, started_analysing: $started };
COMMIT;";

/// Requeues expired leases (index `job_lease` ✓) and the jobs of workers silent for 30
/// seconds, resetting `queued_at`, invalidating the token and counting attempts
/// ([contracts 5]: every requeue after the 2nd counts); a job out of attempts fails for
/// good instead, and a `guided` job out of attempts marks its check `failed` in the same
/// transaction (contracts 5). Statements: 0 `BEGIN`, 1–3 `LET`, 4 `FOR`, 5 the requeued jobs, 6 `COMMIT`.
pub const REQUEUE_STALE: &str = "BEGIN;
LET $silent = (SELECT VALUE record::id(id)[0] FROM worker WHERE last_heartbeat < $now - 30s);
LET $expired = (SELECT VALUE id FROM job WHERE status = 'leased' AND lease_until < $now);
LET $orphaned = (SELECT VALUE id FROM job WHERE status = 'leased' AND worker IN $silent);
FOR $j IN array::union($expired, $orphaned) {
  LET $r = (SELECT requeues, attempts FROM ONLY $j);
  LET $att = IF $r.requeues >= $free_requeues { $r.attempts + 1 } ELSE { $r.attempts };
  UPDATE $j SET requeues = $r.requeues + 1, attempts = $att,
    status = IF $att >= $max_attempts { 'failed' } ELSE { 'queued' },
    finished_at = IF $att >= $max_attempts { $now } ELSE { NONE },
    worker = NONE, lease_token = NONE, lease_until = NONE, claimed_at = NONE, queued_at = $now;
  IF $att >= $max_attempts AND $j.kind = 'guided' {
    UPDATE guided_check SET status = 'failed'
      WHERE scan = $j.scan AND status = 'analysing' AND image = type::record('scan_image', $j.payload.image.image_id);
  };
};
SELECT id, kind, scan, run, question, status FROM job WHERE id IN array::union($expired, $orphaned);
COMMIT;";

/// A heartbeat: upserts `worker:[worker_id]` with its `versions` verbatim, extends each
/// listed lease whose token matches (never past `claimed_at + max_runtime`), and returns
/// the listed jobs the worker must stop: cancelled, requeued or leased elsewhere.
pub const HEARTBEAT: &str = "BEGIN;
UPSERT $worker SET caps = $caps, slots = $slots, gpu = $gpu, versions = $versions, last_heartbeat = $now;
FOR $h IN $held {
  UPDATE $h.job SET lease_until = IF $now + 2m < claimed_at + max_runtime { $now + 2m } ELSE { claimed_at + max_runtime }
    WHERE status = 'leased' AND lease_token = $h.token AND worker = $worker_id;
};
RETURN array::filter($held, |$h| array::len(SELECT VALUE id FROM $h.job
    WHERE status = 'leased' AND lease_token = $h.token AND worker = $worker_id) = 0).map(|$h| $h.job);
COMMIT;";
```

- **The claim** is database 4's statement with `$now` for `time::now()`, the fallback's filter `(!$hold_question OR question = NONE OR !$perception_alive)`, and two additions: the fallback's `max_runtime` doubled at claim ([contracts 5](../architecture/contracts.md#leases-runs-and-attempts)), and a run's first job moving the scan `queued → analysing` in the same transaction (`ClaimedJob.started_analysing`). ✓ Two workers that pick the same job conflict, and the loser's retry gets the next one.
- **The requeue** clears the lease token, resets `queued_at`, counts every requeue after the 2nd as an attempt (`rules::jobs::FREE_REQUEUES`), and fails a job out of attempts instead; a `guided` job that runs out marks its check `failed` in the same transaction ([contracts 5](../architecture/contracts.md#leases-runs-and-attempts)). `record::id(id)[0]` reads `worker:[worker_id]`'s key.
- `job.payload` stores the `JobSpec`'s payload; the claim rebuilds the adjacently tagged `{kind, payload}` and decodes it. `Job.attempt` is `attempts + 1`.

**Completing a job.** `complete_job` reads the job first (`LEASE_STATE`): a repeat by the same lease after `done` or `failed` is `AlreadyCompleted` (contracts 5: `204`), and anything but a live lease is `LeaseLost`. `done` stores each stage's observations (replacing per image), the VLM's answers (ranked), the job `done`, a question photo's segmentation moving its question to `measuring`, a `verify` result clearing its question's step, and the claim the result hands to, in one transaction. `failed` with attempts left requeues the job (the token cleared, so the old lease can't complete it); otherwise it fails for good, and a guided job's check is marked `failed` in the same transaction. `check_lease` accepts a job this same lease already finished, so the repeat reaches `complete_job`.

```rust
/// `complete` with `status: done`, under the lease: each stage's observations (replacing
/// per image, [database 5]), the VLM's indicator answers (ranked, schema v1.4), the job
/// `done`, a question photo's segmentation moving its question to `measuring` and a `verify`
/// result clearing the question's step (it stays `answering` until `score` resolves it),
/// then the step claim the result hands to ([contracts 3], claiming).
pub const COMPLETE_DONE_CLAIM: &str = concat!(
    complete_done_head!(),
    "LET $step = $claim_step;\n",
    claim_entry!(),
    "COMMIT;"
);

/// `complete` with `status: failed`: a retryable failure with attempts left is requeued
/// (token cleared, `queued_at` reset); otherwise the job fails for good, and a `guided`
/// job's check is marked `failed` in the same transaction (contracts 5). Returns whether it
/// failed for good.
pub const COMPLETE_FAILED: &str = "BEGIN;
LET $j = (SELECT status, lease_token, worker, lease_until, attempts, kind, scan, payload FROM ONLY $job);
IF $j = NONE OR $j.status != 'leased' OR $j.lease_token != $lease_token OR $j.worker != $worker OR $j.lease_until < $now { THROW 'lease_lost' };
LET $att = IF $retryable { $j.attempts + 1 } ELSE { $j.attempts };
LET $final = !$retryable OR $att >= $max_attempts;
UPDATE $job SET attempts = $att, error = $error,
  status = IF $final { 'failed' } ELSE { 'queued' },
  finished_at = IF $final { $now } ELSE { NONE },
  worker = IF $final { worker } ELSE { NONE },
  lease_token = IF $final { lease_token } ELSE { NONE },
  lease_until = NONE,
  claimed_at = IF $final { claimed_at } ELSE { NONE },
  queued_at = IF $final { queued_at } ELSE { $now };
IF $final AND $j.kind = 'guided' {
  UPDATE guided_check SET status = 'failed'
    WHERE scan = $j.scan AND status = 'analysing' AND image = type::record('scan_image', $j.payload.image.image_id);
};
RETURN $final;
COMMIT;";
```

```rust
/// A job with its record id, as [`job_enqueue!`](crate::fragments) and
/// [`sql::ENQUEUE_JOB`] create it.
#[derive(Debug, Clone, SurrealValue)]
pub struct JobWriteRow {
    /// `job:<ulid>`.
    pub id: RecordId,
    /// The row.
    pub row: NewJobRow,
}

/// A leased `job` row, as the claim returns it.
#[derive(Debug, Clone, SurrealValue)]
pub struct LeasedJobRow {
    /// `job:<ulid>`.
    pub id: RecordId,
    /// Its kind.
    pub kind: JobKindRow,
    /// Its scan.
    pub scan: RecordId,
    /// Its run.
    pub run: Option<i64>,
    /// The payload.
    pub payload: serde_json::Value,
    /// The new token (`rand::ulid()`).
    pub lease_token: String,
    /// When the lease ends.
    pub lease_until: DateTime<Utc>,
    /// The runtime limit, doubled on the fallback.
    pub max_runtime: surrealdb::types::Duration,
    /// Failed attempts counted so far.
    pub attempts: i64,
}
```

```rust
impl TryFrom<LeasedJobRow> for Job {
    type Error = RowError;

    fn try_from(row: LeasedJobRow) -> Result<Self, RowError> {
        let kind = JobKind::from(row.kind);
        let spec = from_json(
            "payload",
            serde_json::json!({ "kind": kind.as_str(), "payload": row.payload }),
        )?;
        Ok(Self {
            v: V1,
            job_id: from_rid(&row.id)?,
            scan_id: from_rid(&row.scan)?,
            run: row.run.map(|r| u32_of("run", r).map(Run)).transpose()?,
            attempt: u32_of("attempts", row.attempts)? + 1,
            lease_token: row
                .lease_token
                .parse()
                .map_err(|e: domain::id::IdParseError| RowError::BadField {
                    field: "lease_token",
                    problem: e.to_string(),
                })?,
            lease_until: ts(row.lease_until),
            max_runtime_s: u32::try_from(row.max_runtime.into_inner().as_secs())
                .unwrap_or(u32::MAX),
            spec,
        })
    }
}
```

---

## 15. Idempotency, usage and the weather cache

[Contracts 1, idempotency](../architecture/contracts.md#idempotency); [database 4, idempotency keys](../database/README.md#idempotency-keys-) ✓, [limits](../database/README.md#limits-and-counters-) ✓, [cleanup](../database/README.md#cleanup-no-ttl).

```rust
/// The statements.
pub mod sql {
    /// The race-free "first request wins" ([database 4] ✓): a second `CREATE` of the same
    /// `idempotency_key:[user, route, key]` fails with "already exists".
    pub const BEGIN_KEY: &str = "CREATE $key SET fingerprint = $fp, expires_at = $expires_at;";
    /// The stored key after "already exists".
    pub const READ_KEY: &str = "SELECT * FROM ONLY $key;";
    /// An expired key the cleanup hasn't reached yet: deleted, then `BEGIN_KEY` again.
    pub const DELETE_EXPIRED_KEY: &str = "DELETE $key WHERE expires_at <= $now;";
    /// The finished response ([contracts 1]: `2xx` and the deterministic `4xx` only).
    pub const FINISH_KEY: &str =
        "UPDATE $key SET state = 'done', response_status = $status, response_body = $body;";
    /// A `409`, `429` or `5xx`, or an interrupted chat message: the key is released.
    pub const RELEASE_KEY: &str = "DELETE $key;";
    /// Cleanup, one batch of 500 ([database 4], cleanup). `RETURN VALUE $before.id`, not
    /// `RETURN NONE`, so the caller sees when a batch came back empty.
    pub const DELETE_EXPIRED: &str = "DELETE (SELECT VALUE id FROM idempotency_key WHERE expires_at < $now LIMIT 500) RETURN VALUE $before.id;";

    /// A user's day.
    pub const USAGE: &str = "SELECT * FROM ONLY $usage;";
    /// Chat tokens after a turn ([database 4], limits ✓: `+=` on a missing record starts from
    /// the default).
    pub const ADD_CHAT_USAGE: &str =
        "UPSERT ONLY $usage SET tokens_in += $tokens_in, tokens_out += $tokens_out RETURN AFTER;";
}
```

```rust
/// What a stored key means for a repeat with `fingerprint` ([contracts 1], the rules table).
pub fn decide(row: KeyRow, fingerprint: &str) -> Result<IdempotencyBegin, RowError> {
    if row.fingerprint != fingerprint {
        return Ok(IdempotencyBegin::Conflict);
    }
    match row.state {
        KeyStateRow::Running => Ok(IdempotencyBegin::InProgress),
        KeyStateRow::Done => {
            let status = row.response_status.ok_or(RowError::BadField {
                field: "response_status",
                problem: "missing on a done key".to_owned(),
            })?;
            Ok(IdempotencyBegin::Replay(StoredResponse {
                status: u16::try_from(status).map_err(|_| RowError::BadField {
                    field: "response_status",
                    problem: format!("{status} is not an HTTP status"),
                })?,
                body: row.response_body.unwrap_or(serde_json::Value::Null),
            }))
        }
    }
}
```

```rust
#[async_trait::async_trait]
impl IdempotencyRepo for SurrealStore {
    async fn begin(
        &self,
        scope: &IdempotencyScope,
        fingerprint: &str,
        expires_at: Timestamp,
    ) -> Result<IdempotencyBegin, StoreError> {
        const WHAT: &str = "begin idempotent request";
        let key = idempotency_rid(scope.user, &scope.route, scope.key);
        // The port passes `expires_at`; the request's own time is 24 hours before it.
        let now = expires_at.minus(IDEMPOTENCY_TTL);
        for _ in 0..2 {
            match self.create_key(&key, fingerprint, expires_at).await {
                Ok(()) => return Ok(IdempotencyBegin::Started),
                Err(DbFailure::AlreadyExists(_)) => {}
                Err(failure) => return Err(failure.into_store(WHAT)),
            }
            match self.read_key(&key).await? {
                Some(row) if row.expires_at > dt(now) => {
                    return decide(row, fingerprint).map_err(|e| e.into_store(WHAT));
                }
                // Expired but not yet cleaned up (reads hide expired keys, database 4).
                Some(_) => {
                    run_tx(&self.db, sql::DELETE_EXPIRED_KEY, |q| {
                        q.bind(("key", key.clone())).bind(("now", dt(now)))
                    })
                    .await
                    .map_err(|f| f.into_store(WHAT))?;
                }
                // The cleanup deleted it in between: create again.
                None => {}
            }
        }
        Err(StoreError::Conflict(domain::ports::Conflict::Retries))
    }

    async fn finish(
        &self,
        scope: &IdempotencyScope,
        response: StoredResponse,
    ) -> Result<(), StoreError> {
        const WHAT: &str = "finish idempotent request";
        let body = match response.body {
            serde_json::Value::Null => None,
            body @ serde_json::Value::Object(_) => Some(body),
            other => Some(serde_json::json!({ "value": other })),
        };
        let key = idempotency_rid(scope.user, &scope.route, scope.key);
        run_tx(&self.db, sql::FINISH_KEY, |q| {
            q.bind(("key", key.clone()))
                .bind(("status", i64::from(response.status)))
                .bind(("body", body.clone()))
        })
        .await
        .map(drop)
        .map_err(|f| f.into_store(WHAT))
    }

    async fn release(&self, scope: &IdempotencyScope) -> Result<(), StoreError> {
        const WHAT: &str = "release idempotency key";
        let key = idempotency_rid(scope.user, &scope.route, scope.key);
        run_tx(&self.db, sql::RELEASE_KEY, |q| q.bind(("key", key.clone())))
            .await
            .map(drop)
            .map_err(|f| f.into_store(WHAT))
    }

    async fn delete_expired(&self, now: Timestamp) -> Result<u32, StoreError> {
        const WHAT: &str = "delete expired idempotency keys";
        let mut results = run_tx(&self.db, sql::DELETE_EXPIRED, |q| q.bind(("now", dt(now))))
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let ids: Vec<RecordId> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        Ok(u32::try_from(ids.len()).unwrap_or(u32::MAX))
    }
}
```

- `begin` derives the request's time as `expires_at − 24 h`, so an expired key the cleanup hasn't reached is deleted and created again; reads hide expired keys ([database 4](../database/README.md#cleanup-no-ttl)).
- A concurrent `CREATE` of the same key can conflict instead of failing with "already exists" (database v1.3.3); `run_tx`'s retry then gets "already exists".
- A stored body that isn't an object is wrapped as `{ "value": … }` (`response_body` is `option<object>`); a `204` stores none.
- `UsageRepo` reads `usage_daily:[user, day]` and adds chat tokens with `UPSERT … +=`; the scan count is incremented inside `create_scan`.
- `WeatherCacheRepo`: `put` is a `CREATE` whose "already exists" is success (the first write of the hour wins); `cached` reads the key; `gardens_by_cell` groups every live garden by `weather_cell` (also the orphan sweep's list of gardens); `delete_before` is database 4's key-range delete ✓. The cache holds `CellWeather`, without the garden's `location_precision` (contracts v0.9).

---

## 16. Chat, memory and knowledge

| Method | Statement |
|---|---|
| `ChatRepo::expire_pending_actions` | `EXPIRE_PENDING_ACTIONS` ([database 4, cleanup](../database/README.md#cleanup-no-ttl)) |
| `ChatRepo::message_count` | `MESSAGE_COUNT`, for the summary trigger (a domain addition, section 21) |
| other chat methods | `CREATE_CONVERSATION` (a live anchor of the garden), `CONVERSATIONS`, `CONVERSATION` + `MESSAGES` (newest first, with their cards), `STORE_MESSAGE` (attachments must be the conversation's; text, tool-call and tool-result parts only), `CONTEXT` (oldest first, with tool calls), `CREATE_PENDING_ACTION`, `MARK_CONFIRMED` (a retry keeps the result), `REJECT_PENDING_ACTION`, `RATE_MESSAGE` |
| other memory methods | `MEMORIES`, `SHARED_FOR_PLAN`, `SHARE`, `DELETE` (the author of a chat memory; an owner for shared ones), `MEMORY_OWNER` |
| `MemoryStore::remember`, `add_note`, `add_pipeline_memory` | `CREATE_MEMORY` with `topic` from the `NewMemory`, and `source`, `shared` and `author` set by the method |
| `MemoryStore::unsynced_shared` | `UNSYNCED_SHARED`: `shared = true AND belief_synced_at IS NONE`, leaving out a plant or garden with `deleting_at` set, by `created_at` then id, limited; the index `memory_unsynced` serves it. A private chat memory is never listed, and `SHARE` leaves `belief_synced_at` unset, so a shared one is listed afterwards |
| `MemoryStore::mark_synced` | `MARK_SYNCED`: sets `belief_synced_at`; a missing id is a no-op |
| `MemoryStore::belief_forgets` | `BELIEF_FORGETS`: the queued subjects, oldest `requested_at` first |
| `MemoryStore::finish_belief_forget` | `FINISH_BELIEF_FORGET`, one transaction: `belief_synced_at` to `NONE` on the subject's shared memories (a garden's take its plants'), and the subject's `belief_forget` rows deleted |
| the queue's writers | `DELETE` of a shared memory, `START_PLANT` and `START_GARDEN` each create the subject's `belief_forget` row unless one is queued, in their own transaction. The row's `garden` and `plant` are plain links, so it outlives the records it names |
| `MemoryStore::search` | `SEARCH`: exact cosine within one garden, the shared memories and the caller's own ([database 5](../database/README.md#5-reads-indexes-and-vector-search)) ✓ never another member's private ones |
| `KnowledgeRepo::search_chunks` | `SEARCH_CHUNKS`: HNSW `<|k, 40|>` with indexed filters applied first; `k` is a literal, clamped to 1–40 |
| `KnowledgeRepo::species_photo_object` | `SPECIES_PHOTO`: the key only if the species lists it |
| `KnowledgeRepo::load_knowledge` | `LOAD_KNOWLEDGE`: one statement per table, projected into the seed shapes, with `affects`, `caused_by`, `looks_like`, `treatments` and `preys_on` from the graph; the species photos from `reference_images`; the newest classifier label set; `label_set_version` derived (section 22) |

```rust
/// `search_memory` ([database 5] ✓): exact cosine within one garden, the shared memories
/// and the caller's own, never another member's private ones; with a plant, that plant's
/// and the garden-level facts. A garden has at most a few thousand memories, so an exact
/// search is cheap and always correct, and it avoids the filtered-KNN bug after deletes
/// (#7423).
pub const SEARCH: &str = "SELECT *, vector::similarity::cosine(embedding, $query) AS similarity FROM memory
WHERE garden = $garden AND (shared = true OR author = $user) AND ($plant = NONE OR plant = $plant OR plant = NONE)
  AND plant.deleting_at = NONE
ORDER BY similarity DESC LIMIT $k;";
```

### Farm connectors and records

`repo::farm` implements `FarmRecordStore` ([connectors 4](../connectors/README.md#4-data), [6](../connectors/README.md#6-sync); EON E1.6). Plans write `farm_records` and messages `farm_citations` (`PlanRow`, `MessageRow`, both `#[surreal(default)]`), and `PLAN` and the message projection read them; `DEFAULT []` applies only on write, so a row stored before the fields reads as `NONE`, which the projection leaves out and the domain types' `#[serde(default)]` read as empty.

| Method | Statement |
|---|---|
| `create_connector` | `CREATE_CONNECTOR`: bumps `garden.connector_rev` first, so creates racing on a garden conflict and the retry meets `connector_limit` (`Rejected(InvalidState)`) when the garden holds 2 connectors besides those being deleted |
| `seed_connector` | `SEED_CONNECTOR`: creates the fixed id only when it is free, so every start may run it |
| `connectors`, `connector_row` | `CONNECTORS` (oldest first, without `deleting`), `CONNECTOR_ROW`; `record_count` is counted per connector on `farm_record_connector`. The sealed `secret` and `oauth` are read by `SurrealValue` and removed before the JSON decode of `Connector` |
| `complete_connect` | `COMPLETE_CONNECT`: a compare-and-set on `status = 'pending'`, the nonce and `expires_at`; `Conflict(AlreadyExists)` when it lost |
| `set_sources` | `SET_SOURCES`: the chosen sources' cursors kept (`object::entries` filtered with a closure), `too_large` and `source_missing` cleared, due now |
| `request_sync` | `REQUEST_SYNC` (EON decision 12): `invalid_state` for a connector stopped with `too_large` or `source_missing`, else the compare-and-set below; `SyncRequest::TooSoon { allowed_at }` from the last accepted request |
| `due_connectors` | `DUE_CONNECTORS` on `connector_due`: `active`, `next_sync_at <= $now`, not waiting on new sources, earliest first |
| `finish_sync`, `set_status` | `FINISH_SYNC`, `SET_STATUS`: never on a connector being deleted, never creating one; `FINISH_SYNC` is a transaction that keeps `next_sync_at` (and does not store a stopping failure) when it moved past `SyncOutcome::due_at` during the run |
| `start_connector_deletion`, `delete_connector_batch` | `START_DELETION`; `DELETE_CONNECTOR_RECORDS` 500 at a time (only for a `deleting` connector), then `DELETE_CONNECTOR` |
| `connectors_created_by`, `delete_expired_pending` | `CREATED_BY`; `DELETE_EXPIRED_PENDING`, 500 at a time |
| `record_hashes`, `upsert_records` | `RECORD_HASHES`; `UPSERT_RECORDS`, one transaction: a chunk without a vector keeps the stored one only when its `text_hash` is unchanged (`missing_vector` otherwise), and nothing is written for a connector that isn't `active` or `reauth_needed` (`NotFound`) |
| `delete_records`, `delete_chunks_from`, `delete_source_records` | `DELETE_RECORDS` (`vendor_id IN $vendor_ids` on `farm_record_connector`); `DELETE_CHUNKS_FROM`, a key range `farm_record:[$connector, $vendor_id, $from]..=[$connector, $vendor_id, ..]`; `DELETE_SOURCE_RECORDS`, by the `source` stored on each record (index `farm_record_source`), 500 at a time |
| `source_vendor_ids_since` | `SOURCE_VENDOR_IDS_SINCE`, the same source filter with `at >= $since` |
| `garden_record_count` | `GARDEN_RECORD_COUNT` on `farm_record_garden` |
| `search_records` | `SEARCH` below (EON gap G7) |
| `delete_records_before` | `DELETE_RECORDS_BEFORE`, Slack messages only, on `farm_record_kind_at`, 500 at a time |

```rust
/// `POST /connectors/{id}/sync` (EON decision 12): a connector stopped until new sources
/// (`too_large`, `source_missing`) is `invalid_state`, because the due query skips it. The
/// conditional `UPDATE` is the compare-and-set. It returns the connector's id when it was accepted and nothing within
/// 5 minutes of the last accepted request; the last statement returns that request's time.
pub const REQUEST_SYNC: &str = "BEGIN;
LET $c = (SELECT status, sync_requested_at, last_error FROM ONLY $connector WHERE garden = $garden);
IF $c = NONE OR $c.status = 'deleting' { THROW 'not_found' };
IF $c.status != 'active' OR $c.last_error IN ['too_large', 'source_missing'] { THROW 'invalid_state' };
UPDATE $connector SET sync_requested_at = $now, next_sync_at = $now
  WHERE sync_requested_at = NONE OR sync_requested_at <= $cutoff RETURN VALUE id;
RETURN $c.sync_requested_at;
COMMIT;";

/// `search_farm_records` (EON gap G7): `memory`'s exact cosine search without its sharing
/// filter, within one garden, leaving out a connector being deleted.
pub const SEARCH: &str = "SELECT id, connector.kind AS system, title, author, link, text, at,
  vector::similarity::cosine(embedding, $query) AS similarity FROM farm_record
WHERE garden = $garden AND ($kind = NONE OR kind = $kind) AND ($since = NONE OR at >= $since)
  AND connector.status != 'deleting'
ORDER BY similarity DESC LIMIT $k;";
```

`KnowledgeStore` belongs to the `knowledge` crate ([knowledge.md](knowledge.md)); `repo::knowledge` exposes its rows (`ChunkHitRow`, `ReferenceImageRow`, `LookalikeRow`) and statements so the loader and the migration can share them.

---

## 17. Deletion and cleanup

[Database 4, deletion](../database/README.md#deletion-) ✓; [contracts 9, deletion cascades](../architecture/contracts.md#deletion-cascades); [storage 4](../storage/README.md#4-writes-and-deletes). Bottom-up in batches of 500, never one large transaction:

| Method | What it does |
|---|---|
| `start_garden_deletion` | `START_GARDEN`: `deleting_at`, jobs cancelled, step claims and question steps cleared, the garden queued in `belief_forget`; returns the garden prefix, the name and the other members |
| `start_plant_deletion` | `START_PLANT`: the same for the plant's scans, its pin removed from the current map in place, and the plant queued in `belief_forget`; returns each scan's and anchored conversation's prefix |
| `start_scan_deletion` | `START_SCAN`: only a `pending` scan (else `Rejected(InvalidState { status })`); its guided jobs cancelled |
| `delete_batch` | one batch of one `CascadeStep`, below |
| `unfinished_deletions` | `UNFINISHED`: gardens, plants and scans with `deleting_at`, to resume after a restart |
| `unreferenced_keys` | `UNREFERENCED_KEYS`: the orphan sweep's database side; every key field counts ([storage 4](../storage/README.md#4-writes-and-deletes)) |
| account tombstone | `START_ACCOUNT_DELETION` (`Ok(false)` when the tombstone exists: one cascade), `UNFINISHED_ACCOUNT_DELETIONS` (with `user_deleted`), `FINISH_ACCOUNT_DELETION`, `DELETE_FINISHED_TOMBSTONES` (24 hours after the auth provider's deletion) |
| account deletion steps | `ACCOUNT_GARDENS` (the only member deletes; the only owner hands to the longest-standing editor and leaves, or deletes with no editor; anyone else leaves), `TRANSFER_OWNERSHIP`, `MEMBER_CONVERSATIONS`, `USER_FEEDBACK`, `DELETE_USER` |

**A step is several tables in a fixed order.** Each table holds a `REFERENCE ON DELETE REJECT` to something a later one deletes. `delete_batch` reads the target's scans and conversations once, runs the step's statements in order and returns at the first that deleted anything; the caller repeats the step until it returns 0 ([domain 9](domain.md#9-boundary-traits), `CascadeStep::steps_for`). Each batch returns `$before.id` (database v1.3.3): `RETURN NONE` can't tell an empty batch from a full one.

```rust
/// The statements of one cascade step for one target, in their fixed order.
pub fn step_statements(target: DeletionTarget, step: CascadeStep) -> &'static [&'static str] {
    use CascadeStep as S;
    use DeletionTarget as T;
    match (target, step) {
        (_, S::Conversations) => &[
            sql::PENDING_ACTIONS,
            sql::MESSAGE_FEEDBACK,
            sql::MESSAGES,
            sql::CHAT_IMAGES,
            sql::CONVERSATIONS,
        ],
        (T::Member { .. }, S::MemoriesPlantsMap) => &[sql::MEMORIES_OF_MEMBER],
        (T::Member { .. }, _) => &[],
        (_, S::EventsAndJobs) => &[sql::EVENTS, sql::JOBS],
        (T::Garden(_), S::TasksAndTriggers) => &[sql::TASKS_OF_GARDEN, sql::TRIGGERS_OF_GARDEN],
        (T::Plant(_), S::TasksAndTriggers) => &[
            sql::TASKS_OF_PLANT,
            sql::TASKS_OF_PLANS,
            sql::TRIGGERS_OF_PLANT,
            sql::TRIGGERS_OF_PLANS,
        ],
        (T::Garden(_), S::PlansAndDiagnoses) => {
            &[sql::UNLINK_SCANS, sql::PLANS_OF_GARDEN, sql::DIAGNOSES]
        }
        (T::Plant(_), S::PlansAndDiagnoses) => {
            &[sql::UNLINK_SCANS, sql::PLANS_OF_PLANT, sql::DIAGNOSES]
        }
        (_, S::MasksAndGuidedChecks) => &[sql::MASKS, sql::GUIDED_CHECKS, sql::INDICATOR_ANSWERS],
        (_, S::ObservationsQuestionsFeedback) => {
            &[sql::OBSERVATIONS, sql::QUESTIONS, sql::FEEDBACK]
        }
        (_, S::ImagesAndScans) => &[sql::IMAGES, sql::SCANS],
        (T::Garden(_), S::MemoriesPlantsMap) => {
            &[sql::MEMORIES_OF_GARDEN, sql::PLANTS_OF_GARDEN, sql::MAPS]
        }
        (T::Plant(_), S::MemoriesPlantsMap) => &[sql::MEMORIES_OF_PLANT, sql::PLANT],
        (T::Garden(_), S::FarmRecordsAndConnectors) => {
            &[sql::FARM_RECORDS_OF_GARDEN, sql::CONNECTORS_OF_GARDEN]
        }
        (T::Garden(_), S::Garden) => &[sql::INVITES, sql::MEMBERSHIPS, sql::GARDEN],
        // A pending scan has no tasks, plans, diagnoses, memories or garden step, and a plant no
        // connectors (CascadeStep::steps_for never asks for them).
        (T::PendingScan(_), _) | (T::Plant(_), S::FarmRecordsAndConnectors | S::Garden) => &[],
    }
}
```

| Step | Statements, in order |
|---|---|
| 4.1 events and jobs | one scan's events as a key-range delete (its counter zeroed so the next batch takes the next scan), then the scans' jobs |
| 4.2 tasks and trigger rules | a garden's; or a plant's own, those its plans made (`task.plan`, `plan_trigger.plan`) and those others made for it (`task.plant`, `plan_trigger.plant`) |
| 4.3 plans and diagnoses | unset `scan.plan`, `diagnosis`, `open_question`; plans **by equality** (`garden = $garden` or `plant = $plant`, never `scan IN`, #7593 ✓); diagnoses |
| 4.4 | masks, guided checks and **indicator answers** (schema v1.4), then observations, questions, scan feedback |
| 4.5 | scan images, then scans |
| 4.6 conversations | pending actions, message feedback, messages, chat images, conversations |
| 4.7 | memories, plants (a plant lost its pin at start), the garden map |
| farm records and connectors | a garden's farm records, then its connectors ([connectors 8](../connectors/README.md#8-privacy-and-security)); revoking their tokens is the runner's |
| 4.7 (cont.) | invites, memberships, the garden |

**The target's binds.** `TargetBinds::of` picks the target's scope statement (`SCOPE_GARDEN`, `SCOPE_PLANT`, `SCOPE_SCAN` or `SCOPE_MEMBER`) and the record ids it binds; what the target lacks binds as `NONE`. `delete_batch` binds the same four values to the scope query and to every batch, adding `$scans` and `$conversations` from the scope's last two statements. `bind` is a function rather than a closure for the lifetime reason in section 18.

```rust
/// The target's binds for the scope query and every batch (`NONE` for what the target lacks).
#[derive(Debug, Clone, Default)]
pub struct TargetBinds {
    garden: Option<RecordId>,
    plant: Option<RecordId>,
    scan: Option<RecordId>,
    user: Option<RecordId>,
}
```

```rust
impl TargetBinds {
    /// The scope statement and binds of a target.
    pub fn of(target: DeletionTarget) -> (&'static str, Self) {
        match target {
            DeletionTarget::Garden(g) => (
                sql::SCOPE_GARDEN,
                Self {
                    garden: Some(rid(&g)),
                    ..Self::default()
                },
            ),
            DeletionTarget::Plant(p) => (
                sql::SCOPE_PLANT,
                Self {
                    plant: Some(rid(&p)),
                    ..Self::default()
                },
            ),
            DeletionTarget::PendingScan(s) => (
                sql::SCOPE_SCAN,
                Self {
                    scan: Some(rid(&s)),
                    ..Self::default()
                },
            ),
            DeletionTarget::Member { garden, user } => (
                sql::SCOPE_MEMBER,
                Self {
                    garden: Some(rid(&garden)),
                    user: Some(rid(&user)),
                    ..Self::default()
                },
            ),
        }
    }

    /// Adds the binds. A function, not a closure: a closure's signature gets no lifetime
    /// elision tying its returned `Query<'c, _>` to its argument's `'c` (the compiler infers
    /// two unrelated lifetimes), while a function's `'c` names the one borrow both share.
    pub fn bind<'c>(&self, q: Query<'c, Any>) -> Query<'c, Any> {
        q.bind(("garden", self.garden.clone()))
            .bind(("plant", self.plant.clone()))
            .bind(("scan", self.scan.clone()))
            .bind(("user", self.user.clone()))
    }
}
```

✓ A garden with a scan, an event, a job, a plan with a task, an indicator answer and a map deletes cleanly in this order (section 20), and so does one with a connector, a farm record, a plan citing it and a message citing it. A `Member` target runs the conversations step and the member's unshared chat memories.

**Cleanup** (database 4, every 10 minutes, batches of 500): `IdempotencyRepo::delete_expired`, `WeatherCacheRepo::delete_before`, `ChatRepo::expire_pending_actions`, `JobRepo::delete_finished_jobs`, each a real method returning how many rows it touched.

---

## 18. Concurrency and lifetimes

- **One client, shared.** `SurrealStore` holds one `Surreal<Any>` (`Send + Sync + 'static` ✓) and lives in `Arc`; every method takes `&self`. Cloning the client would open a new server session per clone (section 4).
- **Futures are `Send`.** Every port method is `#[async_trait]`, which boxes its future as `dyn Future + Send`; `Query` and the futures that hold `IndexedResults` across `.await` are `Send` ✓ (research `surrealdb.md` §8). `run_tx`'s closure is `Send` for that reason, and `send_commit`'s inner closure is `Sync` because it is captured by reference (section 11).
- **The per-scan writer** in `api` serialises event appends inside one process ([contracts 4](../architecture/contracts.md#transport)); the store's retry covers appends from other processes or races with other writes to the scan row, so `append_event` is safe from any number of writers.
- **`jobs_queued: Arc<Notify>` is the only wake-up.** Every write that makes a job claimable calls `notify_waiters()` after its commit: `enqueue_job`, a step commit or a run start that queues one, an upload's guided or segmentation job, a retryable failure's requeue in `complete_job`, and `requeue_stale` when any job went back to `queued`. The broker's long poll waits on it with a 1-second fallback poll ([database 4](../database/README.md#job-claim-with-slots-)); no other component signals the broker. `notify_waiters` wakes every waiting claim, and they race for the job in the claim's transaction.
- **Owned rows, borrowed inputs.** Rows are owned (section 6); inputs are borrowed only for the duration of a call: `&ObservationWrite` into `ObservationWriteRow::new`, `&[IndicatorEvidence]` into `answer_rows`, `&str` binds converted at once (section 5).

**Named lifetimes and generics.**

| Item | Parameter | Why |
|---|---|---|
| `run_tx<'c, F>` | `'c`, `F` | Section 5 |
| `bind_claim<'c>(q: Query<'c, Any>, claim: &StepClaim) -> Query<'c, Any>` | `'c` | The query's borrow of the client passes through; the claim is read, not held |
| `TargetBinds::bind<'c>` | `'c` | A function, not a closure: a closure's signature gets no lifetime elision tying its returned `Query<'c, _>` to its argument's `'c`, so `|q| q.bind(..)` with an annotated argument fails with "lifetime may not live long enough … returning this value requires that `'1` must outlive `'2`" (✓ rustc 1.99.1, which gives this error no code; fixed this way); a function names the one borrow both share |
| `send_commit<'a, F>` | `'a` | `&'a self` and the queries built from its client |
| `ToRecordId for Id<T: Table>`, `Slug<K: SlugKind>` | `T`, `K` | One impl per marker family, so every table gets the mapping for free (section 6) |
| `rid<I: ToRecordId + ?Sized>` | `?Sized` | Accepts `&Slug<K>` and any unsized implementor without a copy |
| `stored_enum!` | `$row`, `$domain:ty` | A declarative macro instead of a trait: the derive must see the variants |

---

## 19. Errors

The store returns the domain's `StoreError` ([domain 6](domain.md#6-errors)); `api` maps it to HTTP, and the handler maps each `Conflict` to its contract code before `?` ([rust-errors](#23-sources)).

| `StoreError` | Raised for |
|---|---|
| `NotFound` | no row, a record being deleted, a record outside the proof's garden, `THROW 'not_found'` |
| `Conflict(StalePlan { current })` | a lost plan compare-and-set, by `THROW` or the `action_plan_version` index |
| `Conflict(ClaimLost)` | a step's claim, stamp or run no longer holds, or the plant or garden is being deleted |
| `Conflict(QuestionStepLost)` | a photo answer's step no longer holds |
| `Conflict(LeaseLost { job_id })` | a lease check that found nothing |
| `Conflict(StaleMap { current_version })` | a map upload from an old base |
| `Conflict(AlreadyExists)` | an existing key or unique value where the caller didn't expect one |
| `Conflict(Retries)` | transaction conflicts after 5 attempts |
| `Rejected(DomainError)` | `BudgetExceeded` (daily scans), `InvalidState { status }` (a non-pending scan's deletion), `UnknownPlants { plant_ids }` (map pins) |
| `Unavailable` | a lost connection |
| `Query { what, source }` | anything else, with the operation's name and the SDK or row error as `source` |

---

## 20. Testing

`cargo test -p store` runs 7 unit tests (error classification, the back-off range, id round trips, array keys, the JSON conversion, the growth series' one point per scan) and 25 tests, plus the 14 farm connector tests of `tests/farm.rs`, against the **embedded SurrealDB 3.3.4 engine** (`kv-mem`, a dev-dependency only). Each test calls `support::store()`, which connects to a new `mem://` datastore and applies the schema, so tests never share rows. Fixtures go through the store's own methods (`upsert_user`, `create_garden`, the membership check, `create_scan`), so those paths run in every test. `SequentialIds` from `domain`'s `test-util` gives predictable ids; raw fixture SQL uses `type::record('scan', '…')` because sequential test ULIDs are all digits, which a bare `scan:000…1` literal would read as a number.

| Test | What it proves |
|---|---|
| `schema_applies_twice` | The v1.4 schema is idempotent |
| `concurrent_appends_are_gap_free_and_replay_by_range` | 40 appends on a **multi-thread** runtime (conflicts don't happen on a current-thread one, research `surrealdb.md` §3) commit exactly `seq` 1…40; the replay after 35 returns 36…40 with the body decoded |
| `plan_compare_and_set` | A first plan; a second "first plan" gets `StalePlan { current: v1 }` and nothing of it is stored; v2 over v1 dismisses v1's open task with `plan_superseded` |
| `job_claim_slots_requeue_and_fallback_hold` | The claim, `started_analysing`, a full slot, the heartbeat, the requeue of an expired lease (a free requeue), the fallback held back from a photo answer's segmentation job while the perception worker is alive and taking it 31 s after its last heartbeat, with `max_runtime` doubled |
| `step_claim_commit_and_sweep` | The claim and `Busy`; `retrieve` queues its job and hands over to `score` because of `rescore_pending`; the same claim can't commit twice; the sweep re-claims once; the slow original's commit loses; a claim of an older run never commits |
| `question_steps_move_and_recover` | `segmenting → measuring`, the stalled step found and re-claimed, the slow original's advance losing |
| `idempotency_begin_finish_release` | `Started`, `InProgress`, `Conflict`, `Replay` with the stored status and body, `release`, cleanup |
| `daily_scan_limit` | The 31st scan of a UTC day is `BudgetExceeded`, and 30 scans exist |
| `garden_deletion_runs_in_order` | Every cascade step of a populated garden runs to 0 and every table is empty |
| `engine_facts_behind_the_v1_4_notes` | Protected parameter names, `type::point`, and the indicator-answer ranking |
| `flows.rs`: `uploads_run_start_and_snapshot` | A run shot, the kind limit (`TooManyImages`), a guided check with its job, the submit and a refused second submit, the snapshot, the stream state, the detail |
| `complete_job_paths` | A partial result's diff, `complete` storing observations and claiming `retrieve`, a repeat (`AlreadyCompleted`), a wrong token (`LeaseLost`), a guided job requeued twice then failed for good with its check `failed` |
| `guided_check_finishes` | A guided result on a `pending` scan: the check `done` with its texture, no claim |
| `ask_and_answer` | `score` asks; the question is due after 10 minutes; the answer stores the user's evidence, claims `score`, and refuses a second answer |
| `pending_scan_deletion` | A pending scan with a shot and a jar test deletes in order; a `queued` one is `InvalidState { status: "queued" }` |
| `gardens_members_and_account_plan` | An invite created, listed and accepted once; members and gardens pages; the last owner can't be demoted; the account plan hands the garden to the editor, and the transfer applies |
| `plants_tasks_and_growth` | The plants page with `latest_scan`, the timeline, task create/list/complete/delete, the growth series and the timeline's height |
| `chat_flow` | A conversation, a message with an attachment and one with a tool call, a card confirmed twice and then refused a reject, the context oldest first, the detail, feedback, the list |
| `memory_visibility` | A private memory and a shared note, search, the page, the plan writer's shared read, share, an owner's delete |
| `memory_belief_sync` | `topic` round-trips; `unsynced_shared` skips private memories and lists notes and pipeline memories oldest first; `mark_synced`; `share` makes a chat memory appear; a private delete queues nothing; shared deletes queue the plant once; finishing brings back the plant's remaining memories and empties the queue; a garden-level delete queues the garden, and finishing it brings back every shared memory |
| `knowledge_loads` | Seeded rows load into seed shapes with photos, `affects` and `preys_on` |
| `account_tombstone` | The tombstone starts one cascade, refuses a sign-in before and after the user is deleted, reports `user_deleted`, finishes, and is cleaned up after 24 hours |
| `soil_profile_retries` | A failure waits until its retry time, then the garden is due again |
| `live_lease_and_requeue_wake` | `live_lease` live, lost after a requeue, `None` after `complete`; the requeue wakes a waiting long poll |
| `remove_fail_feedback_and_plant_deletion` | A shot removed with its key, feedback refused while `pending`, a failed run, feedback after, a plant's cascade leaving the garden; the cascade queues the plant's erase, and the queued row outlives its records |
| `farm.rs`: `connectors_and_the_due_query` | Create, the connect compare-and-set (a wrong nonce, a reuse and an expired state lose), the 2-connector limit, idempotent seeding, the list; the due query skips `pending`, `reauth_needed`, `deleting` and a connector waiting on new sources; `finish_sync` keeps only chosen sources' cursors; `set_sources` clears the wait; a deleting connector stays deleting |
| `sync_now_is_one_request_in_five_minutes` | Accepted at t, refused at t + 4:59 with `allowed_at` t + 5:00, refused from a second store over the same datastore, accepted at t + 5:00; a pending connector is `InvalidState`, another garden's owner `NotFound` |
| `upserts_keep_the_vector_while_the_text_is_unchanged` | An unchanged hash without a vector keeps the vector and updates the other fields; a changed hash without one, or a new record without one, is refused; a changed one with its vector replaces it; a deleting connector takes no records |
| `deletes_by_item_chunk_and_source` | `source_vendor_ids_since` by stored source and time (`C10/` isn't `C1`'s); a re-chunked document keeps chunk 0; delete by item; delete by source leaves the other channels' records |
| `a_drive_source_is_all_its_records` | A Drive connector's folder is all its records |
| `the_search_stays_in_its_garden_and_ranks_by_similarity` | Hits ranked by cosine, `k`, `kind` and `since`; another garden's record with the exact query vector never comes back; a deleting connector's records leave at once |
| `counts_cleanup_and_deletion` | `garden_record_count` and `record_count` (501); the 180-day cleanup in batches; the pending cleanup; a connector's deletion in batches of 500, records before the connector, only once it is `deleting` |
| `plans_and_messages_carry_farm_citations` | A plan's `farm_records` and a step's `farm_citations`, a message's `farm_citations`, through the store; a plan and messages whose fields were removed and defined again (no backfill) read as empty |
| `server.rs` (ignored) | The schema applied twice, as root, on the pinned image by digest |

Run against the pinned image as well with `PDM_TEST_SURREALDB_URL=ws://127.0.0.1:18000 cargo test -p store --test server -- --ignored` (the container command is in the file).

---

## 21. Decisions on open questions

Answers this crate settles, for [contracts 12](../architecture/contracts.md#12-open-questions) and [domain 12](domain.md#12-open-questions):

| Question | Answer here |
|---|---|
| Contracts 12, timestamp precision | The database keeps full precision: every instant is bound as `DateTime<Utc>` with its nanoseconds, and step and question-step stamps are compared exactly, so the store must never round. Whole seconds are a wire matter (`Timestamp`'s serde) |
| Contracts 20, lease hand-back at shutdown | No hand-back call: a stopping worker's jobs are requeued by `requeue_stale` when its heartbeat is 30 s old or its lease expires (`REQUEUE_STALE`), and that requeue is free for the first two |
| Contracts 22, the per-scan writer's lifecycle | Not the store's to decide; `append_event` is correct from any number of writers in any number of processes (the counter and the retry), so the writer's lifecycle affects ordering latency, not correctness |
| Domain 12, item 7 (where indicator answers live) | The `indicator_answer` table (schema v1.4, section 12) |
| Domain 12, item 8 (`Invite.created_by`) | Optional, as contracts v0.9 now says; the domain field became `Option<UserId>` with `skip_serializing_if` |

**Domain changes this crate made** (all additive or agreed with the callers): `Invite.created_by: Option<UserId>`; `AnalysisRepo::similar_cases` (for the agent's `find_similar_cases`); `ChatRepo::message_count` (the summary trigger); `AnalysisRepo::advance_question_step` returns `QuestionAdvance { Next, Scored }`; `GuidedOutcome` lost its `check` field (the store finds the check from the leased job); `CompletionOutcome::Stored` gained `diff: ObservationDiff`.

**Other decisions made while implementing every method:**

| Topic | Decision |
|---|---|
| Contracts 5, a repeated `complete` | `check_lease` accepts a job this same lease already finished (`done` or `failed`), so the repeat reaches `complete_job`, which answers `AlreadyCompleted`; a requeue clears the token, so a requeued lease never matches |
| Contracts 5, a final failure | Non-retryable failures fail for good at once; retryable ones count an attempt; the failed job keeps its token, so a repeat is `AlreadyCompleted` too |
| Account deletion (architecture 5) | A garden handed to an heir is also in `leave`: the leaver drops to editor in `transfer_ownership`, then the `Member` cascade removes them |
| `GET /tasks` without `state` | Every state |
| Ports with no time (open question 5) | `create_plant` and `register_push_token` take the `created_at` default; `finish_guided_check` takes `now` like the other job completions |
| `ScanSnapshot.questions` | Resolved questions read as `open` (open question 6) |

**Schema changes (v1.4):** `indicator_answer`, `scan.candidates`, `question.indicator`, `mask.rle`.

---

## 22. Open questions

1. **The cascade's `Member` target leaves no marker.** `unfinished_deletions` can resume gardens, plants and pending scans after a restart, but not a member's removal. A `member_of` row is already gone when the cascade runs; a small `pending_removal` record (garden, user) would make it resumable. Belongs in [database 9](../database/README.md#9-open-questions).
2. **`PlanRepo::write_plan` and `StepRepo::commit_step(Plan)` run the same transaction.** Both exist in the ports; one could go ([domain 9](domain.md#9-boundary-traits)).
3. **The orphan sweep's key check scans tables.** `UNREFERENCED_KEYS` filters `storage_key`, `depth_key`, `depth_confidence_key` and the mask and chat image keys by `IN $keys` with no index on those fields. Once a day per garden this is cheap at hackathon scale; an index on each key field, or a garden filter in the port, would bound it.
4. **Resolved: a bound `k` in `<|k, 40|>`** is a parse error on 3.3.4 ("expected an unsigned integer", ✓ [database 1](../database/README.md#1-version-and-setup)). `search_chunks` now writes `k` into the statement as a literal, clamped to 1–40 (`MAX_CHUNK_HITS`, the search's `ef`), and `knowledge_search_returns_the_nearest_chunks_first` runs the search on the embedded engine with 384-dimension vectors.
5. **Three port methods pass no time.** `store_partial`, `create_plant` and `register_push_token` have no `now`. The store checks a partial result's lease by token, worker and status only (a requeue clears the token; `complete` checks expiry), and the other two take the `created_at` default. Adding `now: Timestamp` to those three methods ([domain 9](domain.md#9-boundary-traits)) would put them back under the `Clock` port. `finish_guided_check` took this step on 2026-10-19: it read the database clock, so a test whose fixed clock fell behind real time lost its lease.
6. **Resolved questions have no wire status.** `Question.status` is `open | answering`, so `ScanSnapshot.questions`, which holds every question of the run, reads an answered, superseded or dropped question as `open`; the live state is `scan.open_question`. A server-side status on the snapshot's questions would remove the ambiguity.
7. **`label_set_version` isn't stored.** The schema keeps no generated files (contracts 10), so `load_knowledge` computes it from every species' and organism's `bioclip_label` with the recipe of [`ml/models/FORMATS.md`](../../ml/models/FORMATS.md): the first 16 hex digits of a SHA-256 over `BIOCLIP_MODEL`, the 80 templates (`bioclip_templates.json`, a copy of `ml/`'s, kept equal by a test) and the sorted `"{table}:{id}\t{label}"` lines. It passes `FORMATS.md`'s test vector (`129f0395e8d89bd5`), so the knowledge loader, `ml/training` and the API agree. Database 1 proposes storing the loader's value in `knowledge_meta` once the migration runner exists.
8. **The snapshot is several reads.** `snapshot` runs `SNAPSHOT_PARTS` and the loaders as separate queries. Under the step claim nothing else writes the run's outputs, and new input only sets `rescore_pending`, so the reads agree in practice; one read-only `BEGIN … COMMIT` would make it a single snapshot.

---

## 23. Sources

**Rust language**
- The Book: [16.4 `Send` and `Sync`](https://doc.rust-lang.org/book/ch16-04-extensible-concurrency-sync-and-send.html) · [10.3 lifetimes](https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html) · [20.5 macros](https://doc.rust-lang.org/book/ch20-05-macros.html) · [13.1 closures](https://doc.rust-lang.org/book/ch13-01-closures.html)
- The Reference: [lifetime elision](https://doc.rust-lang.org/reference/lifetime-elision.html) · [paths in expressions](https://doc.rust-lang.org/reference/paths.html#paths-in-expressions) · [macros by example](https://doc.rust-lang.org/reference/macros-by-example.html)
- std: [`Sync`](https://doc.rust-lang.org/std/marker/trait.Sync.html) · [`concat!`](https://doc.rust-lang.org/std/macro.concat.html) · Tokio: [`Notify`](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Notify.html) · [`time::sleep`](https://docs.rs/tokio/1.53.3/tokio/time/fn.sleep.html)

**Crates at the pinned versions**
- `surrealdb` 3.3.4 (docs.rs failed to build it, so its source in `~/.cargo/registry/src/*/surrealdb-3.3.4/`: `src/method/query.rs` for `Query`, `bind`, `take`, `take_errors`, `num_statements`) · [`surrealdb-types` 3.3.4 `Error`](https://docs.rs/surrealdb-types/3.3.4/surrealdb_types/struct.Error.html) · [`QueryError`](https://docs.rs/surrealdb-types/3.3.4/surrealdb_types/enum.QueryError.html) · [`RecordId`](https://docs.rs/surrealdb-types/3.3.4/surrealdb_types/struct.RecordId.html) · [`SurrealValue`](https://docs.rs/surrealdb-types/3.3.4/surrealdb_types/trait.SurrealValue.html)
- [SurrealQL `type::point`](https://surrealdb.com/docs/surrealql/functions/database/type#typepoint) · [record ids and ranges](https://surrealdb.com/docs/reference/query-language/language-primitives/data-types/record-ids) · [transactions](https://surrealdb.com/docs/learn/querying/concepts-and-guides/transactions)
- [rand 0.10.3 `random_range`](https://docs.rs/rand/0.10.3/rand/fn.random_range.html) · [async-trait 0.1.92](https://docs.rs/async-trait/0.1.92/async_trait/)

**Project documents, notes and skills**
- [Database](../database/README.md) v1.4 and [schema](../../backend/migrations/schema.surql) · [contracts](../architecture/contracts.md) · [storage](../storage/README.md) · [domain](domain.md) · [knowledge](knowledge.md)
- Research notes and a SurrealDB probe from writing this design (not kept in the repository): the `untagged` enums, `take_errors()` minus `NotExecuted`, `is_thrown()` with the message, unique indexes as `Internal`, array keys, multi-thread retry tests
- Skills in the main checkout's `.claude/skills/`: `rust-surrealdb` (with `errors-and-retries.md` and `write-patterns.md`), `rust-errors`, `rust-testing`, `rust-async`, `rust/versions.md`

The crate and SurrealQL links were checked against the pinned sources on 2026-10-16; the Rust language pages were opened on 2026-10-15 for the domain document.
