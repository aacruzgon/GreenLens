# PDM Backend: `store` crate

> **Status:** Draft v0.4.27, 2026-10-24. v0.4.27: `existing_gardens` (`EXISTING_GARDENS`) says which gardens still have a record, for the orphan sweep's pass over prefixes whose garden is gone; `tests/orphans.rs` runs it and `UNREFERENCED_KEYS` (all five key fields kept, a stray answered) on the embedded engine. `create_alert_task` takes the spell's last fired day and creates one task per spell (`task.spell`, `task.spell_until`, index `task_spell`), refusing a garden being deleted. v0.4.26: `usage_daily.chat_budget` is `DEFAULT ALWAYS 0`, so a row from before it takes its next write; its test is `a_usage_row_from_before_chat_budget_takes_the_next_scan`. v0.4.25: `usage_daily.chat_budget`, the daily chat budget's weighted count, is added up beside the raw `tokens_in` and `tokens_out`. v0.4.24: `plant_summaries` gives a plant's species only when it is confirmed, and its test covers a plant being deleted, another garden's plant and a newer canopy width. v0.4.23: answer blocks: `message.blocks` (`array<object> FLEXIBLE DEFAULT []`) is written from `NewMessage` and projected with the message, read as empty from a message stored before it; `plant_summaries` (`PLANT_SUMMARIES`) reads every live plant of a garden with its status, species and the newest scan's height in one statement. v0.4.22: `search_records_for_plan`, the plan writer's farm-record search by the scan's garden (EON E5.4), shares `SEARCH` with `search_records`; `garden_has_records` reads at most one searchable record first. v0.4.21: the lease checks and the stale requeue read the job's `payload.stages` for `LeasedJob::segmentation`, with a test on the embedded engine. v0.4.20: the mask test also shows a segmentation write keeps the masks of a photo it doesn't cover. v0.4.19: a segmentation write also deletes the masks of the photos it covers, so masks follow their observations when a photo is segmented again; the orphan sweep removes their objects (section 12). v0.4.18: the runner refuses a top-level `RETURN` and reads each record back (`MigrationError::Unrecorded`), and a numbered file deletes the knowledge record in its transaction (section 4). v0.4.17: open question 3 (stale photos) is settled: `seed-upload` never deletes a photo. v0.4.16: the migration runner is built (section 4): `build.rs` embeds `backend/migrations/*.surql` as `MIGRATIONS`, `migrate_with` applies the schema, the numbered files and `knowledge.surql` in their own transactions with a `migration` record and a hash compare-and-set, `SurrealStore::migrate` replaces `apply_schema`, and `tests/migrations.rs` covers it on the embedded engine. v0.4.15: the user writes that set a species (`create_plant`, `confirm_species`, a new plant of `create_scan` and of `put_map`) `THROW 'unknown_species'` in their transaction for a species the knowledge base doesn't hold, answered as `Rejected(InvalidRequest)` at the request's field (`unknown_species`), with a test on the embedded engine; the plant projection reads a dangling species link as no species (open question 1). v0.4.14: `run_stage_events` reads a run's `run_started` and stage events in `seq` order (`RUN_STAGE_EVENTS`, a key-range read filtered by run and type), for the stage events a job's `complete` stands in for (backend roadmap gap 9), with a test on the embedded engine. v0.4.13: the farm search filters by `until` (before the instant) and `author` (part of the display name, lowercased, never a record without one) beside `kind` and `since` (EON gap G12). v0.4.12: `begin` takes over a `running` idempotency key older than 10 minutes with a compare-and-set, and `deleting_connectors_of_user` lists a user's `deleting` connectors with no garden filter; `delete_expired_pending` takes a second cutoff and leaves a pending connector claimed since then. v0.4.11: `UPSERT_RECORDS` skips a record whose source is no longer among the connector's sources, in the same transaction; `SEED_CONNECTOR` repairs an existing connector of the garden (token, `status`, `last_error`). v0.4.10: the audit's fixes to the farm repository: creates bump `garden.connector_rev` so the limit holds under a race, Sync now refuses a connector stopped until new sources, a record stores its `source`, the 180-day cleanup takes Slack messages only (`farm_record_kind_at`), and `finish_sync` keeps a `next_sync_at` that moved during the run (`SyncOutcome::due_at`). v0.4.9: `FarmRecordStore` is implemented (roadmap E1.6): connectors with the 2-per-garden limit, the connect compare-and-set, the due query, **Sync now** as a compare-and-set on `sync_requested_at`, upserts that keep a vector only while its `text_hash` holds, deletes by item, chunk range and source, the exact search within one garden, record counts and the cleanup; plans and messages write and read `farm_records` and `farm_citations`, and rows from before them read as empty; 8 tests in `tests/farm.rs`. v0.4.8: the farm connectors' `connector` and `farm_record` tables are in the schema (with `sync_requested_at`, and `farm_records` and `farm_citations` on `action_plan` and `message`), the garden cascade deletes them before the garden, `garden_of` resolves a connector, `farm_record:[connector, vendor_id, n]` keys map to `FarmRecordId`, and `FarmRecordStore` is stubbed until roadmap E1.6. v0.4.7: `knowledge_meta` is in the schema, and an embedded-engine test loads the knowledge loader's output (work items M1, M6). v0.4.6: memories carry a `topic` and `belief_synced_at`, the `belief_forget` queue holds subjects to erase in Agent Memory (written by a shared memory's delete and by the plant and garden cascade starts), and `MemoryStore` gains the belief sweep's methods. v0.4.5: `label_set_version` follows `ml/models/FORMATS.md` (a SHA-256 over the BioCLIP revision, the 80 templates and the sorted labels) instead of an FNV-1a hash, and passes its test vector; open question 7 says so. v0.4.4: open question 4 is resolved: the vector search's bound `k` failed to parse on 3.3.4, so `search_chunks` writes it as a clamped literal, with a test on the embedded engine. v0.4.3: section 4 proposes the migration runner that applies the numbered migrations and the generated `knowledge.surql` after the schema, with its work items, tests and open questions. v0.4.2: the schema now lives at `backend/migrations/schema.surql` only, no longer a copy of a file in `docs/database`. v0.4.1: section 1 keeps only the `$now` difference from database 4, since the skills now follow this design; section 17 lists `TargetBinds`, which section 18 cites; research notes are no longer named by a path outside the repository. v0.4: the multi-line query listings are complete (a generator bug cut eight of them after their first line); the step-claim pair is described as conditional, as in database 4; the protected parameters are `auth`, `token`, `access` and `session`; a bound `$before` is overridden only in `RETURN`, and `Option` fails only from 2 ids; the closure-lifetime error has no code. v0.3: the account tombstone (`deleted_user`), SoilGrids retries, `live_lease`, and the store's `Notify` as the only claim wake-up (a requeue notifies too). v0.2: every repository method is real code (no `todo!()` left), reads decode straight into domain types through contract-shaped projections, and 12 more tests run on the embedded engine. v0.1: `SurrealStore`, which implements every repository of `domain::ports::store` and `MemoryStore` over SurrealDB 3.3.4, with every statement a named constant, one query per transaction with a conflict retry, one error classifier, and real code for each pattern that carries a guarantee, tested against the embedded 3.3.4 engine.

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

@@file ../Cargo.toml@@

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
├─ build.rs          embeds backend/migrations/*.surql (but the schema) as MIGRATIONS; refuses a bad name or a duplicate number
├─ src/
│  ├─ lib.rs          crate docs, re-exports, the compile-time `Store + MemoryStore + Send + Sync + 'static` check
│  ├─ client.rs       StoreConfig, SurrealStore, connect / from_client / migrate, SCHEMA (include_str!)
│  ├─ migrate.rs      the migration runner: migrate_with, apply_migrations, MigrationError, check_contents, knowledge_meta
│  ├─ migrate/naming.rs   the file names a migration may have; build.rs includes it too
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
│  ├─ migrations.rs   the migration runner, with the knowledge loader's fixture
│  └─ server.rs       ignored: the migrations applied twice on the pinned image
└─ ../../migrations/   schema.surql, the numbered files and knowledge.surql, all embedded
```

---

## 4. Connecting and the schema

[Database 1](../database/README.md#1-version-and-setup): connect once, sign in as the database user `api` with `opt::auth::Database`, select `pdm`/`main`, and keep the client in `Arc<AppState>`. The schema is applied before the API serves traffic, over a short-lived root connection, because `api` (a database `EDITOR`) can't run the file's `DEFINE NAMESPACE` and `DEFINE DATABASE`.

@@items client.rs SCHEMA StoreConfig SurrealStore@@

@@impl client.rs impl SurrealStore@@

@@items client.rs apply_schema_with@@

- `SurrealStore` is **not `Clone`**: a clone of `Surreal<Any>` opens a new server session and replays the sign-in ([database 6](../database/README.md#6-rust-sdk)), so the store is shared through `Arc`. `lib.rs` asserts at compile time that it is `Store + MemoryStore + Send + Sync + 'static`, which is what `Arc<dyn Store>` across tokio's threads needs:

@@lines lib.rs 23-34@@

- The schema file is [`backend/migrations/schema.surql`](../../backend/migrations/schema.surql); `include_str!` embeds it, so the binary and its schema can't drift. ✓ The v1.4 file applies twice in a row on the embedded engine and, as root, on the pinned image by digest.

### The migrations

Built for slice S2 ([roadmap 13](roadmap.md#13-gaps-found-while-writing-this-roadmap), gap 1). The design, its SurrealQL and the checks run on the pinned image are in [database 1](../database/README.md#1-version-and-setup) ("Applying the migrations"). In short: `schema.surql` on every start; then the hand-written `NNNN_<slug>.surql` files once each, in number order, hash-locked; then the generated `knowledge.surql`, which replaces the whole knowledge set and is applied again only when its hash changes or a numbered file ran in the same start. Each file runs in its own transaction with its row in the `migration` table.

`build.rs` lists `backend/migrations/*.surql`, refuses a bad name or a duplicate number (the build fails naming the file), and writes `MIGRATIONS` to `OUT_DIR`, sorted: the numbered files, then `knowledge.surql`. The naming rules live in `migrate/naming.rs`, which `build.rs` includes with `#[path]`, so the build and the runner read names the same way.

@@items migrate.rs Migration Applied MigrationError@@

@@items migrate.rs migrate_with apply_migrations@@

The wrapper each file runs in. The file goes on the wrapper's first line, so a parse error's line number is the file's own; the binds carry a `migration_` prefix, so a file's own `LET $name` can't clash with them:

@@items migrate.rs WRAP_HEAD WRAP_TAIL@@

- `apply_migrations` sends each wrapped file through `run_tx_at`, `run_tx` with the failed statement's index. A transaction conflict means the file committed nothing, so it is re-sent. If another process applied the file meanwhile, the `THROW` rolls this attempt back; the runner re-reads the record and skips the file when the hash matches, or stops with `Changed` when a numbered file's doesn't. A knowledge file is re-applied over the newer record, up to 3 attempts.
- `Failed` names the file and the statement, counting the file's statements from 1 (`first_failure_at` skips `NotExecuted` and `Cancelled`), so the log names the line to fix. A parse error has no index; its message gives the line.
- `api` logs the result at `info`: `migrations applied numbered=[…] knowledge=true`.
- `apply_schema_with` stays for tests that need the schema alone; `migrate_with` runs it first.
- `check_contents` is the rule on what a file may hold, checked line by line: no `USE`, `BEGIN`, `COMMIT`, `CANCEL`, `OPTION`, or `DEFINE NAMESPACE`, `DATABASE`, `USER` or `ACCESS`; in `knowledge.surql`, every statement writes a knowledge table or `knowledge_meta` and ends with `RETURN NONE;`. The unit test `migration_files_follow_the_rules` runs it on every embedded file, and `the_knowledge_meta_and_the_fixture_follow_the_rules` on the loader's fixture.
- `knowledge_meta` reads `knowledge_meta:current` (`SurrealStore::knowledge_meta` as `api`). `api` compares it with its text embedder at start ([api 4](api.md#4-start-up-and-shutdown)):

@@items migrate.rs KnowledgeMeta@@

**Work items**, for slice S2 of the [roadmap](roadmap.md#6-s2-perception-site-data-and-knowledge) (item 6, knowledge loading) and S0 for the live check:

| # | Slice | Work | State |
|---|---|---|---|
| M1 | S2 | The `migration` table in `schema.surql`, listed in database 3 | Done: the schema applies twice on the embedded engine and, as root, on the pinned image |
| M2 | S2 | `store/build.rs` and `MIGRATIONS`: names, numbering, and the forbidden statements | Done: a duplicate number or a bad name fails `cargo build`; `migration_files_follow_the_rules` passes on the committed files |
| M3 | S2 | `migrate_with` and `migrate`, replacing `apply_schema` | Done: the tests below |
| M4 | S2 | `api` start-up: step 3 calls `migrate`; after step 6, the embedder check against `knowledge_meta` | Done in code (`the_embedder_must_match_the_knowledge_set`); not yet run against the dev compose |
| M5 | S2 | `load_knowledge` reads `label_set_version` and `classifier_labels_version` from `knowledge_meta:current` instead of hashing the labels (section 22, item 7) | Open |
| M6 | S2 | The loader (`ml/knowledge/`) writes `knowledge.surql` as [contracts 10](../architecture/contracts.md#checks-and-generated-files) says | Done in PR #48; the first real seed is open |
| M7 | S0 | The live CI job applies the committed migrations twice, as root, on the pinned image, and a generated 21 MB knowledge fixture over WebSocket | Open. `tests/server.rs` already applies the committed files twice and the loader's fixture once, then skips it, on the pinned image |

**Tests on the embedded engine** (`tests/migrations.rs`, `kv-mem`, one datastore each):
- `committed_migrations_apply_twice`: `MIGRATIONS` apply, and a second run applies nothing.
- `numbered_files_apply_once_in_order_with_their_records`: files given out of order apply in number order with their records; a new file, ending in a comment with no `;`, applies alone on the next start.
- `a_changed_recorded_file_stops_the_start` (and a `\r\n` copy of the same file is no change), `a_missing_recorded_file_stops_the_start`, `an_out_of_order_file_stops_the_start`.
- `a_failing_file_commits_nothing`: no row and no record, `Failed` at statement 2; the file before it stays applied. A parse error has no statement index.
- `knowledge_replaces_the_set`: the loader's fixture, then a v2 without one species, two chunks and an edge. The species, the edge and the chunks are gone, the rest are kept, and the removed chunks' vectors find only the chunk that stays.
- `a_numbered_file_reapplies_the_knowledge`: a numbered file that deletes the chunks, and the unchanged knowledge file runs again in the same start.
- `a_start_that_dies_after_a_numbered_file_still_reapplies_the_knowledge`: the numbered file commits, the knowledge file fails; the next start, with the knowledge file whose hash was recorded before, applies it again, since the numbered file deleted its record in its own transaction.
- `a_return_that_skips_the_record_stops_the_start`: a top-level `RETURN` commits the file's write without its record; the runner reads the record back and stops with `Unrecorded`, and `check_contents` refuses the file.
- `a_raced_file_is_skipped`: another process applies both files between this one's read and its transactions; this one throws "migration raced", re-reads and applies nothing. `a_raced_numbered_file_with_another_hash_is_a_change`.
- `the_knowledge_meta_and_the_fixture_follow_the_rules`: no `knowledge_meta` before a knowledge file, the fixture's after.
- The embedded engine has no WebSocket, so only M7 checks the 64 MiB message cap.

**Open questions.**
1. **Removed knowledge records.** User records keep a plain link to a deleted species or problem. The plant projection (`plant_fields!`) reads a species link to a missing record as no species, and the user writes that set a species (`create_plant`, `confirm_species`, a new plant of `create_scan` and of `put_map`) refuse one the knowledge base doesn't hold with `invalid_request` (`thrown::UNKNOWN_SPECIES`), so only the pipeline's suggestion or a later knowledge load can leave a dangling link. What should the app show for a plant whose species was removed: unknown, with a prompt to confirm it again? Links to a missing problem are still read as they are.
2. **Memories after a text-model change.** The loader re-embeds chunks, but `memory.embedding` uses the same model and only `api` can re-embed it. A background job, or no model change once memories exist?
3. **Stale photos.** *Settled on 2026-10-20:* `seed-upload` only adds photos and never deletes one, so the old `api` keeps signing URLs to photos that exist, during a deploy and after a rollback ([contracts 10](../architecture/contracts.md#10-knowledge-seed-format)). Photos that left the seed stay in `knowledge/`, a few hundred KB each.
4. **Growth past 48 MiB.** Split the knowledge file by table (losing one transaction for the whole set), apply it over HTTP `/import` (limits *unverified*), or keep chunks out of Git?
5. **Rollback.** An older build refuses a database that recorded a file it doesn't know. Is restoring the pre-deploy backup an acceptable rollback during build week?

---

## 5. Transactions, retries and the error mapping

### `run_tx`

Every statement goes through one function. A transaction is one `BEGIN … COMMIT` sent as **one** query ([database 4](../database/README.md#4-write-patterns)), and a `TransactionConflict` re-sends it, up to 5 attempts in all, sleeping 10–50 ms × 2^attempt between them ([database 6](../database/README.md#6-rust-sdk)). A single statement is its own transaction and can conflict the same way (database v1.3.3), so plain reads go through `run_tx` too; a conflicted transaction committed nothing, so a retry is always safe, `+= 1` included.

@@items tx.rs MAX_ATTEMPTS backoff run_tx@@

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

@@items error.rs THROWN_PREFIX mod:thrown unknown_species DbFailure first_failure classify@@

| Failure on 3.3.4 | How it is recognised | `DbFailure` |
|---|---|---|
| Write conflict at `COMMIT`, or on a single statement | `query_details() == Some(QueryError::TransactionConflict)` ✓ | `TxConflict` (retried) |
| `THROW 'stale_plan'` | `is_thrown()`, message `"An error occurred: stale_plan"` ✓; the code is the text after the prefix | `Thrown { code }` |
| `CREATE` of an existing id, `INSERT RELATION` of an existing edge | `already_exists_details() == Some(Record { .. })` ✓ | `AlreadyExists` |
| A `UNIQUE` index (`task_alert`, `user_auth`, `action_plan_version`) | kind `Internal`, message ``Database index `<name>` already contains …`` ✓ (no typed details) | `UniqueIndex { index }` |
| Lost connection | `is_connection()` | `Unavailable` |
| Anything else (a coercion, a rejected `DELETE`, a parse error) | — | `Other` |

@@impl error.rs impl DbFailure@@

A caller that knows more maps first: the plan write turns `stale_plan` and the `action_plan_version` unique index into `StalePlan { current }` by reading the plan that won (section 13); the map upload turns `stale_map` into `StaleMap { current_version }` and `unknown_plants` into `Rejected(UnknownPlants { plant_ids })` (section 9); `create_scan` turns `budget_exceeded` into `Rejected(BudgetExceeded)` with the next UTC midnight (section 10); `create_plant`, `confirm_species`, `create_scan` and `put_map` turn `unknown_species` into `Rejected(InvalidRequest)` at the request's field (`species_id`, `new_plant.species_id` or `pins`) through `unknown_species`; alert dedupe and `upsert_user` treat their unique index as success (sections 13, 8).

A row that doesn't convert into its domain type is a `RowError`, reported as `StoreError::Query`: a bug or a schema drift, never a user error.

@@items error.rs RowError@@

---

## 6. IDs and rows

### `Id<T>`, `Slug<K>` and array keys

`domain` never depends on `surrealdb` (convention 5), so the mapping is two traits here, implemented once for every table through the domain's markers:

@@items ids.rs ToRecordId FromRecordId@@

@@impl ids.rs impl<T: Table> ToRecordId for Id<T>@@

@@impl ids.rs impl<T: Table> FromRecordId for Id<T>@@

@@items ids.rs rid opt_rid from_rid from_opt_rid@@

- `T: Table` gives the table name as `T::NAME`, so `from_record_id` checks that a `scan:…` id isn't read as a `GardenId`. `rid<I: ToRecordId + ?Sized>` takes a reference so it works on `Slug<K>` (which holds an `Arc<str>`) without cloning.
- ULID keys are the 26-character string: `RecordIdKey` has no `From<Ulid>` (`surrealdb-types` uses `ulid` 1.x) ✓.
- **Array keys** are built with `Array::from(vec![Value::RecordId(..), Value::Number(..)])` ✓; `RecordIdKey` has no `From<Vec<_>>`, and `RecordId::parse_simple` always yields a string key (research `surrealdb.md` §4):

@@items ids.rs scan_event_rid seq_of member_rid idempotency_rid usage_rid indicator_answer_rid weather_rid farm_record_rid farm_record_id_of@@

### Rows

**Why rows are owned.** A row is decoded by `SurrealValue::from_value(Value)`, which takes the SDK's value by value and drops it after `take(n)`, so a row can't borrow from it: every field is owned (`String`, `RecordId`, `Vec<_>`). Rows also never cross a task; each repository method converts them into domain types before returning. Stored literal unions get a row mirror with `From` both ways, made by one macro:

@@items rows.rs macro_rules!:stored_enum@@

- `#[surreal(untagged, rename_all = "snake_case")]` is required: without `untagged` a derived unit enum is written as `{ "open": {} }` and fails to decode the stored string `'open'` ✓ (research `surrealdb.md` §3). A variant may carry its stored literal (`About1Km = "about_1_km"`), because `snake_case` would give `about1_km`.
- Inside `From<$domain>`, `type D = $domain;` lets the match name variants through a type alias (enum variants are reachable through a type alias since Rust 1.37, [Reference, paths](https://doc.rust-lang.org/reference/paths.html#paths-in-expressions)), which a `$domain:ty` fragment can't do directly.
- Less central literal unions (observation stages, sources, roles) are strings from their serde form (`enum_str`), so the store doesn't mirror every contract enum.
- Contract shapes that the schema stores as `object FLEXIBLE` are `serde_json::Value`, from and into the domain types with `to_json` / `from_json`. Timestamps inside them become RFC 3339 strings; queries never filter on them. Every queried instant is a top-level `datetime` bound as `DateTime<Utc>`, never a formatted string ([database 2](../database/README.md#2-conventions)).

@@items rows.rs to_json from_json dt ts@@

### Reads into domain types

Most reads don't need a row struct. Each statement projects the stored record into its contract shape in SurrealQL (`scan AS scan_id`, `{ table: record::tb(ref), id: record::id(ref) } AS ref`, `[] AS images`), the result is taken as a `surrealdb::types::Value`, and `read::json_of` converts it with the wire rules: a record id becomes its bare key (the ULID or the slug, [contracts 1](../architecture/contracts.md#wire-format)), a `datetime` an RFC 3339 string with its full precision, `NONE` an absent field. The domain type's own `Deserialize` then builds it, so a projection that misses a field fails loudly as a `RowError`.

@@items read.rs json_of take_one take_all page@@

`repo::load` holds the loaders shared by several repositories (a scan with its images, guided checks and open question; a plant with its latest scan; a diagnosis with its soil observations; a plan with its open question and task ids; a garden's context), and the projections are literal macros (`scan_fields!`, `plant_fields!`, `question_fields!`, `observation_fields!`, `task_fields!` …) reused by every statement that reads that record. Lists page by a cursor on their sort key and the id (`[name, id] > $after`), fetching `limit + 1` rows to know whether there is a next page ([database 5](../database/README.md#5-reads-indexes-and-vector-search): never `START`).

---

## 7. Shared fragments

Several transactions share pieces: the step claim's re-check, the hand-over, the observation write, the indicator answers, the job enqueue. Each is a `macro_rules!` that expands to a **string literal**, so each whole transaction is still one named `const &str` built at compile time with `concat!` (a `const` can't be passed to `concat!`, a literal macro can). No value is ever formatted in; every value is a bind, and each fragment documents the binds it reads. Fragments that end a transaction put their result last, so the caller takes it at `num_statements() - 2` whatever came before.

@@items fragments.rs macro_rules!:claim_check macro_rules!:handover macro_rules!:claim_entry@@

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

@@items repo/access.rs mod:sql@@

`upsert_user` is called by `authz::authenticate` on every request, so it reads first and creates only on a miss; the new user's key is `rand::ulid()` generated by SurrealDB, because the port passes no id. `display_name` defaults to "Gardener" when the token has none.

@@impl repo/identity.rs impl IdentityRepo for SurrealStore@@

---

## 9. Gardens, plants and the map

| Method | Statement | Database |
|---|---|---|
| `create_garden` | `CREATE_GARDEN`: the garden and the owner's `INSERT RELATION` in one transaction | [4, membership](../database/README.md#membership-) ✓ |
| `accept_invite` | `ACCEPT_INVITE`: a live invite of a live garden, then `INSERT RELATION` unless already a member (whose role never changes) | [4, membership](../database/README.md#membership-) |
| `garden` | `GARDEN`, with `my_role` and `member_count` | — |
| `set_soil_profile`, `soil_profile_failed`, `gardens_due_soil_profile` | `SET_SOIL_PROFILE` (`ready` clears `soil_retry_at`), `SOIL_PROFILE_FAILED` (`unavailable` until `$retry_at`, the profile kept), `DUE_SOIL_PROFILE` (`pending`, or `unavailable` whose retry time has come; the oldest retry first) | schema v1.4 `garden.soil_retry_at` |
| `existing_gardens` | `EXISTING_GARDENS`: `SELECT VALUE id FROM $gardens` over the record ids, so a garden without a record returns no row; one being deleted still does. For the orphan sweep's pass over prefixes whose garden is gone ([blob 9](blob.md#9-the-orphan-sweep)) | — |
| `plant_summaries` | `PLANT_SUMMARIES`: every live plant of the garden by name (index `plant_garden`), with its status, its species only when `species_status` is `confirmed`, and the newest scan's `plant_height_cm` of medium confidence or more (index `observation_growth`), for an answer block in one read | — |
| `put_map`, `map`, `current_map` | `PUT_MAP`, `CURRENT_MAP` | [4, garden map versions](../database/README.md#garden-map-versions-) ✓ |
| lists and edits | `GARDENS` (by name), `UPDATE_GARDEN`, `CHANGE_ROLE` and `REMOVE_MEMBER` (the last owner stays, `invalid_state`), `INVITES` (unused and unexpired, newest first), `MEMBERS` (by `since`), `PLANTS` (by name, with `latest_scan` and its thumbnail), `TIMELINE` (newest first, with the diagnosis and the highest-confidence height and width), `GROWTH_SERIES` (one point per scan and quantity) | [5](../database/README.md#5-reads-indexes-and-vector-search) |

@@items repo/gardens.rs CREATE_GARDEN ACCEPT_INVITE@@

The centroid is `type::point([$lon, $lat])`: ✓ a `($lon, $lat)` literal doesn't take parameters (a parse error on 3.3.4), `type::point($lon, $lat)` wants one argument, and a GeoJSON-shaped object isn't coerced ([database 8, caveat 17](../database/README.md#8-caveats)).

`Invite.created_by` is optional (contracts v0.9): `InviteRow.created_by` is `Option<RecordId>` because the field is `ON DELETE UNSET`, and the domain field is now `Option<UserId>` (section 21).

**The map upload** is one transaction: the version check, unknown plants refused, pins of plants being deleted dropped, new plants created, the version incremented, the map written, older maps deleted, and every plant's `position` set or unset. Two uploads from the same base conflict on the garden row; the loser's retry then fails the version check.

@@items repo/map.rs PUT_MAP@@

@@impl repo/map.rs impl MapRepo for SurrealStore@@

---

## 10. Scans and events

| Method | Statement | Database |
|---|---|---|
| `create_scan` | `CREATE_SCAN`: the scan (and a new plant) with `UPSERT usage_daily … scans += 1` and the limit `THROW` in one transaction | [4, limits](../database/README.md#limits-and-counters-) ✓ |
| `append_event` | `APPEND_EVENT`: the counter on the scan, the event at `scan_event:[scan, seq]` | [4, scan events](../database/README.md#scan-events-a-gap-free-seq-) ✓ |
| `events_after` | `EVENTS_AFTER`: a key-range read, scoped to the proof's garden | [4, scan events](../database/README.md#scan-events-a-gap-free-seq-) ✓ |
| `run_stage_events` | `RUN_STAGE_EVENTS`: the scan's key range filtered by run and type, no garden scope (`api`'s own read for a job it holds the lease of) | [pipeline 10](pipeline.md#10-events) |
| `stream_state` | `STREAM_STATE`, with `step_claimed` from `pipeline_step != NONE` | [contracts 4](../architecture/contracts.md#when-the-stream-closes) |
| `add_image` | `ADD_IMAGE`: the status, `deleting_at`, kind limit and question re-checked; a guided check and its `guided` job created; a question photo's first step and job | [contracts 3](../architecture/contracts.md#post-scansidimages) |
| `remove_image`, `start_run`, `fail_scan`, `record_feedback` | `REMOVE_IMAGE`, `START_RUN`, `FAIL_SCAN`, `RECORD_FEEDBACK` | [contracts 3](../architecture/contracts.md#3-scan-rest-api) |
| `scan`, `scan_detail`, `snapshot` | the loaders of section 6 and `SNAPSHOT_PARTS` | [contracts 3](../architecture/contracts.md#3-scan-rest-api) |

@@items repo/events.rs mod:sql@@

- **Gap-free `seq`.** The scan row is the counter; the increment and the `CREATE` commit together or not at all, so a conflict leaves no gap. ✓ 40 concurrent appends on a 4-thread runtime commit exactly 1…40 after retries (section 20). `DEFINE SEQUENCE` would leave gaps ([database 8, caveat 8](../database/README.md#8-caveats)).
- **Stored body.** `type` beside a `payload` object holding the rest of `ScanEventBody<Stored>` (image and mask ids, never URLs, [contracts 4](../architecture/contracts.md#transport)); the replay puts `type` back and decodes the tagged union.

@@items repo/events.rs EventRow@@

@@impl repo/events.rs impl EventRepo for SurrealStore@@

@@items repo/scans.rs CREATE_SCAN@@

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

@@items repo/steps.rs mod:sql@@

**Hand-over per output.** The step's own next step is computed in Rust and bound as `$handover`; [`handover!`](#7-shared-fragments) replaces it with `score` when `rescore_pending` was set while the step ran:

| Output | Own hand-over |
|---|---|
| `Retrieve` | release when a job was queued, else `score` |
| `Score` | `plan` when `needs_plan`, else release |
| `Ask` | release (`awaiting_answer`) |
| `Verify` | release when a job was queued, else `score` |
| `Plan` | release (and `diagnosed → planned`) |
| `Release` | release |

@@impl repo/steps.rs impl StepRepo for SurrealStore@@

- The two claim statements are conditional on each other, as in [database 4](../database/README.md#steps-between-jobs-): run unconditionally, the second (`SET rescore_pending = true WHERE pipeline_step != NONE`) also runs after a successful claim and flags the scan the caller just claimed (✓ on the pinned image, a fresh claim then reads `rescore_pending = true`).
- **Stamps are compared exactly.** The stamp is the bound `$now`, stored with full precision, and the commit compares `pipeline_step_at != $stamp`. The domain's `Timestamp` keeps nanoseconds in memory and writes whole seconds only on the wire (contracts 12, question 12; section 21).
- The diagnosis is written with a compare-and-set on `scan.diagnosis` (its `supersedes`, or `NONE`); under the claim nothing else writes it, so a loss throws `claim_lost`.
- A job queued by a commit signals `jobs_queued` after the commit returns.

`send_commit` takes the per-output binds as a second closure. Its bound is `Fn(Query<'a, Any>) -> Query<'a, Any> + Send + Sync`: `run_tx`'s closure captures it by reference, and a reference is `Send` only when its referent is `Sync` ([std, `Sync`](https://doc.rust-lang.org/std/marker/trait.Sync.html)).

---

## 12. Observations, questions and indicator answers

**Indicator answers** close [domain 12](domain.md#12-open-questions) item 7: nothing stored the measured and user answers between `retrieve` and `score`. The new table `indicator_answer:[scan, run, indicator_id]` ([database 4, indicator answers](../database/README.md#indicator-answers-), schema v1.4) holds every answer of a run, the VLM's included, and its write keeps the higher-ranked one (user 3 > measurement 2 > vlm 1, [contracts 7](../architecture/contracts.md#measurement)). ✓ A measured answer replaces a VLM one, a user answer survives a later measured one, and an answer of the same rank replaces the stored one.

@@items repo/analysis.rs rank answer_rows@@

The fragments that write observations (replacement per image, [database 5](../database/README.md#5-reads-indexes-and-vector-search)) and answers:

@@items fragments.rs macro_rules!:observation_write macro_rules!:evidence_write@@

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

@@items repo/analysis.rs QUESTION_TO_SCORE@@

@@items fragments.rs macro_rules!:question_step_check@@

---

## 13. Plans and tasks

[Database 4, plan writes](../database/README.md#plan-writes-compare-and-set-) ✓ and [alert dedupe](../database/README.md#alert-dedupe-) ✓; [contracts 7](../architecture/contracts.md#actionplan), re-planning.

| Method | Statement |
|---|---|
| `write_plan` | `COMMIT_PLAN`: `claim_check!`, `plan_cas!`, `diagnosed → planned`, `handover!` (the same transaction as `commit_step(Plan)`) |
| `write_revision` | `WRITE_REVISION`: `plan_cas!` with `$expected = base_plan_id`, no claim, never retried on a lost compare-and-set |
| `create_alert_task` | `CREATE_ALERT_TASK`: the garden-not-deleting and live-plant checks; a task with the same key, or the spell's task seen to the day before its first day or later, continues the spell (`spell_until` moves on, `Ok(None)`); otherwise the `CREATE` with `spell` and `spell_until`; the `task_alert` index makes a racing duplicate `Ok(None)` |
| reads | `CURRENT_PLAN`, `PLAN` (the plan with its open question and task ids), `TASKS` (by due date, undated last, cursor on that key), `TASK`, `ACTIVE_TRIGGERS` (with the step's title and detail from its plan) |
| task writes | `CREATE_TASK` (plant and scan checked live in the insert's transaction ✓), `UPDATE_TASK` (only `user` and `chat` tasks change their text; a repeating task's next occurrence in the same transaction), `DELETE_TASK` |

@@items fragments.rs macro_rules!:plan_cas@@

- ✓ A losing write leaves nothing: the `THROW` (or, when two writers create the same version, the `action_plan_version` unique index, which fails before the `THROW` runs) rolls back the new plan, and the store reads the plan that won into `StalePlan { current }`.
- The supersede dismisses the old plan's open tasks with `plan_superseded` and deactivates its trigger rules in the same transaction ✓; each new task and rule is skipped when its plant isn't live in the garden ([contracts 7](../architecture/contracts.md#task), tasks only on live plants).
- `task_ids` aren't stored on the plan: they are read back after the write (`PLAN_TASK_IDS`).

@@items repo/plans.rs PlanWriteRow TaskRow@@

@@impl repo/plans.rs impl SurrealStore@@

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

@@items repo/jobs.rs CLAIM_JOB REQUEUE_STALE HEARTBEAT@@

- **The claim** is database 4's statement with `$now` for `time::now()`, the fallback's filter `(!$hold_question OR question = NONE OR !$perception_alive)`, and two additions: the fallback's `max_runtime` doubled at claim ([contracts 5](../architecture/contracts.md#leases-runs-and-attempts)), and a run's first job moving the scan `queued → analysing` in the same transaction (`ClaimedJob.started_analysing`). ✓ Two workers that pick the same job conflict, and the loser's retry gets the next one.
- **The requeue** clears the lease token, resets `queued_at`, counts every requeue after the 2nd as an attempt (`rules::jobs::FREE_REQUEUES`), and fails a job out of attempts instead; a `guided` job that runs out marks its check `failed` in the same transaction ([contracts 5](../architecture/contracts.md#leases-runs-and-attempts)). `record::id(id)[0]` reads `worker:[worker_id]`'s key.
- `job.payload` stores the `JobSpec`'s payload; the claim rebuilds the adjacently tagged `{kind, payload}` and decodes it. `Job.attempt` is `attempts + 1`.

**Completing a job.** `complete_job` reads the job first (`LEASE_STATE`): a repeat by the same lease after `done` or `failed` is `AlreadyCompleted` (contracts 5: `204`), and anything but a live lease is `LeaseLost`. `done` stores each stage's observations (replacing per image), the VLM's answers (ranked), the job `done`, a question photo's segmentation moving its question to `measuring`, a `verify` result clearing its question's step, and the claim the result hands to, in one transaction. `failed` with attempts left requeues the job (the token cleared, so the old lease can't complete it); otherwise it fails for good, and a guided job's check is marked `failed` in the same transaction. `check_lease` accepts a job this same lease already finished, so the repeat reaches `complete_job`.

@@items repo/jobs.rs COMPLETE_DONE_CLAIM COMPLETE_FAILED@@

@@items repo/jobs.rs JobWriteRow LeasedJobRow@@

@@impl repo/jobs.rs impl TryFrom<LeasedJobRow> for Job@@

---

## 15. Idempotency, usage and the weather cache

[Contracts 1, idempotency](../architecture/contracts.md#idempotency); [database 4, idempotency keys](../database/README.md#idempotency-keys-) ✓, [limits](../database/README.md#limits-and-counters-) ✓, [cleanup](../database/README.md#cleanup-no-ttl).

@@items repo/idempotency.rs mod:sql@@

@@items repo/idempotency.rs decide@@

@@impl repo/idempotency.rs impl IdempotencyRepo for SurrealStore@@

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
| other chat methods | `CREATE_CONVERSATION` (a live anchor of the garden), `CONVERSATIONS`, `CONVERSATION` + `MESSAGES` (newest first, with their cards), `STORE_MESSAGE` (attachments must be the conversation's; text, tool-call and tool-result parts only; the answer blocks sent, as JSON objects in `blocks`, read as empty from a message stored before the field), `CONTEXT` (oldest first, with tool calls), `CREATE_PENDING_ACTION`, `MARK_CONFIRMED` (a retry keeps the result), `REJECT_PENDING_ACTION`, `RATE_MESSAGE` |
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

@@items repo/memory.rs SEARCH@@

### Farm connectors and records

`repo::farm` implements `FarmRecordStore` ([connectors 4](../connectors/README.md#4-data), [6](../connectors/README.md#6-sync); EON E1.6). Plans write `farm_records` and messages `farm_citations` (`PlanRow`, `MessageRow`, both `#[surreal(default)]`), and `PLAN` and the message projection read them; `DEFAULT []` applies only on write, so a row stored before the fields reads as `NONE`, which the projection leaves out and the domain types' `#[serde(default)]` read as empty.

| Method | Statement |
|---|---|
| `create_connector` | `CREATE_CONNECTOR`: bumps `garden.connector_rev` first, so creates racing on a garden conflict and the retry meets `connector_limit` (`Rejected(InvalidState)`) when the garden holds 2 connectors besides those being deleted |
| `seed_connector` | `SEED_CONNECTOR`: creates the fixed id when it is free; otherwise, for a connector of this garden that is not being deleted (and whose garden is not), re-seals the token and sets `active` with no `last_error`; one elsewhere stays as it is. Every start may run it |
| `connectors`, `connector_row` | `CONNECTORS` (oldest first, without `deleting`), `CONNECTOR_ROW`; `record_count` is counted per connector on `farm_record_connector`. The sealed `secret` and `oauth` are read by `SurrealValue` and removed before the JSON decode of `Connector` |
| `complete_connect` | `COMPLETE_CONNECT`: a compare-and-set on `status = 'pending'`, the nonce and `expires_at`; `Conflict(AlreadyExists)` when it lost |
| `set_sources` | `SET_SOURCES`: the chosen sources' cursors kept (`object::entries` filtered with a closure), `too_large` and `source_missing` cleared, due now |
| `request_sync` | `REQUEST_SYNC` (EON decision 12): `invalid_state` for a connector stopped with `too_large` or `source_missing`, else the compare-and-set below; `SyncRequest::TooSoon { allowed_at }` from the last accepted request |
| `due_connectors` | `DUE_CONNECTORS` on `connector_due`: `active`, `next_sync_at <= $now`, not waiting on new sources, earliest first |
| `finish_sync`, `set_status` | `FINISH_SYNC`, `SET_STATUS`: never on a connector being deleted, never creating one; `FINISH_SYNC` is a transaction that keeps `next_sync_at` (and does not store a stopping failure) when it moved past `SyncOutcome::due_at` during the run |
| `start_connector_deletion`, `delete_connector_batch` | `START_DELETION`; `DELETE_CONNECTOR_RECORDS` 500 at a time (only for a `deleting` connector), then `DELETE_CONNECTOR` |
| `connectors_created_by`, `delete_expired_pending` | `CREATED_BY`; `DELETE_EXPIRED_PENDING`, 500 at a time, leaving a claim younger than `claimed_before` |
| `record_hashes`, `upsert_records` | `RECORD_HASHES`; `UPSERT_RECORDS`, one transaction: a chunk without a vector keeps the stored one only when its `text_hash` is unchanged (`missing_vector` otherwise), and nothing is written for a connector that isn't `active` or `reauth_needed` (`NotFound`) |
| `delete_records`, `delete_chunks_from`, `delete_source_records` | `DELETE_RECORDS` (`vendor_id IN $vendor_ids` on `farm_record_connector`); `DELETE_CHUNKS_FROM`, a key range `farm_record:[$connector, $vendor_id, $from]..=[$connector, $vendor_id, ..]`; `DELETE_SOURCE_RECORDS`, by the `source` stored on each record (index `farm_record_source`), 500 at a time |
| `source_vendor_ids_since` | `SOURCE_VENDOR_IDS_SINCE`, the same source filter with `at >= $since` |
| `garden_record_count` | `GARDEN_RECORD_COUNT` on `farm_record_garden` |
| `garden_has_records` | `GARDEN_HAS_RECORDS`: at most one id on `farm_record_garden`, leaving out a connector being deleted as `SEARCH` does; the plan writer asks before embedding its query (EON E5.4) ✓ `the_search_stays_in_its_garden_and_ranks_by_similarity`, `the_record_check_leaves_out_only_a_deleting_connector` |
| `search_records`, `search_records_for_plan` | `SEARCH` below (EON gap G7): `kind`, `since`, `until` and `author` filters (gap G12) ✓ `the_search_filters_by_until_and_author`. The first takes the member's proof (the chat's tool), the second the scan's garden (the plan writer, EON E5.4); both run the same query ✓ `the_search_stays_in_its_garden_and_ranks_by_similarity` |
| `delete_records_before` | `DELETE_RECORDS_BEFORE`, Slack messages only, on `farm_record_kind_at`, 500 at a time |

@@items repo/farm.rs REQUEST_SYNC SEARCH@@

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

@@items repo/deletion.rs step_statements@@

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

@@items repo/deletion.rs TargetBinds@@

@@impl repo/deletion.rs impl TargetBinds@@

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

`cargo test -p store` runs 7 unit tests (error classification, the back-off range, id round trips, array keys, the JSON conversion, the growth series' one point per scan) and 25 tests, plus the 18 farm connector tests of `tests/farm.rs`, against the **embedded SurrealDB 3.3.4 engine** (`kv-mem`, a dev-dependency only). Each test calls `support::store()`, which connects to a new `mem://` datastore and applies the schema, so tests never share rows. Fixtures go through the store's own methods (`upsert_user`, `create_garden`, the membership check, `create_scan`), so those paths run in every test. `SequentialIds` from `domain`'s `test-util` gives predictable ids; raw fixture SQL uses `type::record('scan', '…')` because sequential test ULIDs are all digits, which a bare `scan:000…1` literal would read as a number.

| Test | What it proves |
|---|---|
| `schema_applies_twice` | The v1.4 schema is idempotent |
| `concurrent_appends_are_gap_free_and_replay_by_range` | 40 appends on a **multi-thread** runtime (conflicts don't happen on a current-thread one, research `surrealdb.md` §3) commit exactly `seq` 1…40; the replay after 35 returns 36…40 with the body decoded |
| `run_stage_events_reads_one_runs_stage_events_in_order` | A run's `run_started` and stage events come back in `seq` order across `seq` 10, without a `status_changed` or another run's events; a run without events reads empty |
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
| `masks_are_stored_and_replaced_with_their_segmentation` | A completion's mask record, read back by the snapshot and `mask_object` with its region's `mask_id`; a partial result's mask for the same photo replaced by the final one, and another photo's mask kept |
| `guided_check_finishes` | A guided result on a `pending` scan: the check `done` with its texture, no claim |
| `ask_and_answer` | `score` asks; the question is due after 10 minutes; the answer stores the user's evidence, claims `score`, and refuses a second answer |
| `pending_scan_deletion` | A pending scan with a shot and a jar test deletes in order; a `queued` one is `InvalidState { status: "queued" }` |
| `gardens_members_and_account_plan` | An invite created, listed and accepted once; members and gardens pages; the last owner can't be demoted; the account plan hands the garden to the editor, and the transfer applies |
| `plants_tasks_and_growth` | The plants page with `latest_scan`, the timeline, task create/list/complete/delete, the growth series and the timeline's height, and `plant_summaries` before and after a height (the newest scan's height; a later low-confidence one, a newer canopy width, a plant being deleted and another garden's plant left out; a suggested species not given) |
| `chat_flow` | A conversation, a message with an attachment and one with a tool call and an answer block, a card confirmed twice and then refused a reject, the context oldest first, the detail with the block, a message stored before `blocks` read as having none, feedback, the list |
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
| `the_record_check_leaves_out_only_a_deleting_connector` | With one of a garden's two connectors being deleted, `garden_has_records` and `search_records_for_plan` follow the other: true and its record only, then false once its records are gone (EON E5.4) |
| `counts_cleanup_and_deletion` | `garden_record_count` and `record_count` (501); the 180-day cleanup in batches; the pending cleanup; a connector's deletion in batches of 500, records before the connector, only once it is `deleting` |
| `plans_and_messages_carry_farm_citations` | A plan's `farm_records` and a step's `farm_citations`, a message's `farm_citations`, through the store; a plan and messages whose fields were removed and defined again (no backfill) read as empty |
| `orphans.rs`: `every_key_field_keeps_its_object_and_a_stray_is_unreferenced` | `UNREFERENCED_KEYS` keeps a photo's `storage_key`, `depth_key` and `depth_confidence_key`, a mask's and a chat image's `storage_key`, and answers only a stray key; no keys answer none |
| `existing_gardens_are_those_with_a_record_being_deleted_or_not` | `EXISTING_GARDENS` answers the garden and not a garden without a record, and still answers the garden once it is marked `deleting_at` |
| `migrations.rs` | The migration runner: order, records, the hash lock, a changed, missing or out-of-order file, a failing file, a raced file, the knowledge set replaced and re-applied after a numbered file (section 4 lists each test) |
| `server.rs` (ignored) | The schema and the committed migrations applied twice, as root, on the pinned image by digest; the loader's fixture as `knowledge.surql` applied once, then skipped |

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
7. **`label_set_version` isn't stored.** The schema keeps no generated files (contracts 10), so `load_knowledge` computes it from every species' and organism's `bioclip_label` with the recipe of [`ml/models/FORMATS.md`](../../ml/models/FORMATS.md): the first 16 hex digits of a SHA-256 over `BIOCLIP_MODEL`, the 80 templates (`bioclip_templates.json`, a copy of `ml/`'s, kept equal by a test) and the sorted `"{table}:{id}\t{label}"` lines. It passes `FORMATS.md`'s test vector (`129f0395e8d89bd5`), so the knowledge loader, `ml/training` and the API agree. The loader now writes its value to `knowledge_meta`, and the runner applies it; reading it from there is work item M5 (section 4).
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
