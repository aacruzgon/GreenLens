# PDM Database (SurrealDB)

> **Status:** v1.8.3, 2026-10-19. v1.8.3: the connector limit now holds under racing creates (a `garden.connector_rev` counter makes them conflict), a farm record stores its `source` so a dropped source deletes only its own records, the 180-day cleanup deletes Slack messages only (index `farm_record_kind_at` replaces `farm_record_at`), and the sync's end keeps a Sync now or new sources that landed during the run. v1.8.2: the farm connectors' repository is built (EON E1.6): **Sync now**'s compare-and-set on `sync_requested_at`, the due query, the upserts that keep a vector, the deletes and the exact search within one garden ran on the embedded 3.3.4 engine, not yet on the server image (sections 3, 5). v1.8.1: `connector.last_error` gains `source_missing`, and the connectors' tokens are revoked by `api` after the deletion step commits (EON E3.6). v1.8: the `connector` and `farm_record` tables for the farm connectors are in `schema.surql`, with `farm_records` on `action_plan`, `farm_citations` on `message` and a deletion step for them (sections 3, 4); only the embedded-engine test has run them. v1.7: `knowledge_meta` is defined in `schema.surql`, and the knowledge loader in `ml/knowledge/` writes `knowledge.surql`; an embedded-engine test loads its output for a fixture seed (section 1). v1.6: `memory` gains `topic` and `belief_synced_at`, and shared memories are mirrored into SurrealDB Agent Memory, described in the new [agent-memory.md](agent-memory.md) (section 3). v1.5.1: section 1's compose service gains `surrealdb-volume`, which gives the named volume to UID 65532. Without it, SurrealDB can't create its RocksDB directory on a new volume, because the image's `/data` is owned by root. The service runs as root with only `CHOWN`, no network and a read-only root. v1.5 proposes how `api` applies the numbered migrations and the generated `knowledge.surql` after the schema, with a `migration` record of each file's hash and a `knowledge_meta` record, and records the checks run on the pinned image (section 1, section 7). v1.4.3: the schema now lives at `backend/migrations/schema.surql`, beside the `store` crate that embeds it, instead of in this folder. v1.4.2 shows the entry point's step claim as the `store` crate runs it, with the flag set only when the claim comes back `NONE`, and corrects caveat 16: `access` and `session` are protected too, and a bound `$before`, `$after`, `$this` or `$parent` is used in some places and overridden in others. v1.4.1 records the embedding models chosen in [docs/vision](../vision/README.md): text embeddings are 384-dimensional (`snowflake-arctic-embed-xs`), so `chunk_vec`'s `DIMENSION 384` is final, and scan-image embeddings are BioCLIP 2's 768-dimensional image vectors, searched exactly per garden. No schema statement changed. v1.4 stores the indicator answers between the `retrieve` and `score` steps in a new `indicator_answer` table keyed `[scan, run, indicator]`, whose writes keep the higher-ranked answer (user, then measurement, then the VLM), and adds `scan.candidates`, `question.indicator`, `mask.rle`, `garden.soil_retry_at` and the account tombstone `deleted_user`, the deletion order for the answers, and two caveats found while building the `store` crate (protected parameter names, and `type::point` for a bound centroid). v1.3.3: the `api` user's password is a string literal in the setup script (a parameter is a parse error), ULIDs come from `Ulid::generate()`, the batched delete returns `$before.id` so it can tell when a table is empty, and statement errors are read with `take_errors()`, skipping `NotExecuted` and `Cancelled`. v1.3.2: `POST /tasks` checks its `scan_id` in the insert's transaction (✓ the schema accepts a link to a missing scan, and the check refuses it), and species photos and `preys_on` are computed when read. v1.3.1: `task.scan` is `CASCADE`, so a pending scan's jar-test task is deleted with it, and event payloads store image IDs, never signed URLs. v1.3 adds the fields the app's screens need (contracts v0.8): `task.scan`, `scan.deleting_at` and the deletion of a pending scan, `diagnosis.low_evidence`, `species.reference_images`, and `city` as a garden location precision. v1.2 ties step claims to their run with a commit check, adds `job.question` to the fallback's claim, deletes plans by equality (the `IN` bug), and clears claims when deletion starts. v1.1 adds the LiDAR records (section 3), a deletion order that removes tasks and trigger rules before their plans, per-image replacement of observations, and claimed steps between jobs that the scheduler's sweep can recover. v1: The schema is [`schema.surql`](../../backend/migrations/schema.surql). Companion to the [architecture document](../architecture/README.md) (section 5) and [contracts.md](../architecture/contracts.md), which define the shapes these records store.
> **Verified:** every pattern marked ✓ below was run against `surrealdb/surrealdb:v3.3.4` on 2026-10-15; the v1.3.3 changes were run against the same image, by digest, on 2026-10-16. The v1.4 points marked ✓ were run on 2026-10-16 against the embedded 3.3.4 engine (crate `surrealdb = "=3.3.4"` with `kv-mem`, the same engine code as the server image), by the `store` crate's tests; the v1.4 schema also applies twice in a row, as root, on the pinned image by digest (Docker, `memory` engine). The v1.4.2 points were run on 2026-10-16 against the pinned image by digest, through the Rust SDK over `ws`, and against the embedded engine, with the same results. The schema applies cleanly twice in a row, and a scenario script exercised the rest. The v1.5 migration points were run on 2026-10-16 against the pinned image by digest (RocksDB engine), as root through the Rust SDK over `ws`, with the proposed tables added to the schema. Points marked *unverified* come from the docs only.

## Contents

1. [Version and setup](#1-version-and-setup)
2. [Conventions](#2-conventions)
3. [Tables](#3-tables)
4. [Write patterns](#4-write-patterns)
5. [Reads, indexes and vector search](#5-reads-indexes-and-vector-search)
6. [Rust SDK](#6-rust-sdk)
7. [Operations](#7-operations)
8. [Caveats](#8-caveats)
9. [Open questions](#9-open-questions)
10. [Sources](#10-sources)

---

## 1. Version and setup

| Item | Pin |
|---|---|
| Server | `surrealdb/surrealdb:v3.3.4@sha256:649a60a3b9c0177d9923c938e1c2563e48cde2a9d06ac2c0519714be719b02af` (published 2026-10-13) |
| Rust crate | `surrealdb = "=3.3.4"` |
| Storage engine | RocksDB (`rocksdb:/data/db`). SurrealKV is still beta; `memory` is for tests |

**Why 3.x.** The 3.x docs and SDK are current. 3.x has `COMPUTED` fields, record references, literal enum types, and vector search that filters before it searches (3.3). The 3.x SDK refuses to talk to a 2.x server, and the reverse is also true, so the server and crate move together. GitHub release notes stop at 3.3.0. The 3.3.1–3.3.4 patch releases are on crates.io and Docker Hub, and they carry vector and index fixes we rely on.

**Docker Compose service.**

```yaml
services:
  surrealdb-volume:                                     # gives the volume to UID 65532, then exits
    image: busybox:1.38.0@sha256:fd7dc98638c8e305f4dc34e979f1c0fdfdcaeb0fbf8fcff77ae834b6da3d7e6e
    command: ["chown", "65532:65532", "/data"]
    volumes:
      - surrealdb-data:/data
    restart: "no"
    network_mode: none                                  # it runs as root, so nothing else
    read_only: true
    cap_drop: [ALL]
    cap_add: [CHOWN]                                    # ✓ cap_drop ALL alone: "Operation not permitted"
    security_opt: ["no-new-privileges:true"]
  surrealdb:
    image: surrealdb/surrealdb:v3.3.4@sha256:649a60a3b9c0177d9923c938e1c2563e48cde2a9d06ac2c0519714be719b02af
    restart: unless-stopped
    depends_on:
      surrealdb-volume: { condition: service_completed_successfully }
    command: start --log info rocksdb:/data/db
    environment:
      SURREAL_USER: ${SURREAL_ROOT_USER:?set in .env}   # creates the root user on first start only
      SURREAL_PASS: ${SURREAL_ROOT_PASS:?set in .env}
      SURREAL_ROCKSDB_BLOCK_CACHE_SIZE: "536870912"     # 512 MiB; the default is half of memory minus 1 GiB
    ports:
      - "127.0.0.1:8000:8000"                           # host-only; the API reaches it as ws://surrealdb:8000
    volumes:
      - surrealdb-data:/data                            # owned by 65532, set by surrealdb-volume
    healthcheck:
      test: ["CMD", "/surreal", "is-ready", "--endpoint", "http://127.0.0.1:8000"]   # ✓ distroless image, no curl
      interval: 10s
      timeout: 5s
      retries: 6
volumes:
  surrealdb-data:
```

**The volume's owner.** ✓ The image runs as UID 65532 (`nonroot`), but its `/data` is owned by root, and Docker copies that owner into a new named volume. RocksDB then fails at start with "Failed to create RocksDB directory: Permission denied" (run on 2026-10-16 with the image above). `surrealdb-volume` sets the owner on every `docker compose up`; on a volume that already has it, it changes nothing. A restart by the `restart:` policy or `docker compose restart` skips it, which is harmless once the owner is set. It runs as root, so it gets only `CHOWN`, no network and a read-only root. ✓ With it, SurrealDB runs as `nonroot`, turns healthy on a new volume, and keeps its data across a restart (`infra/dev.compose.yml`, 2026-10-16). A bind mount needs the same `chown 65532:65532` on the host.

**Capabilities.** The 3.3 defaults deny scripting, guest access and outbound network calls, which is what we want. Never start the server with `--unauthenticated`.

**Users.**

| User | Level and role | Used by |
|---|---|---|
| root | Root, from `SURREAL_USER`/`SURREAL_PASS` | Applying `schema.surql`, backups, emergencies |
| `api` | `DEFINE USER api ON DATABASE PASSWORD '…' ROLES EDITOR` | The API at runtime. ✓ It can read, write and change the schema, but `DEFINE USER` fails with "Not enough permissions" |

The setup script creates `api` as root, with the password written into the statement as a single-quoted string, `\` and `'` escaped. ✓ It can't be a parameter: `PASSWORD $secret` is a parse error ("Unexpected token \`a parameter\`, expected a strand") whether `$secret` is bound or set with `LET`, and so is `PASSHASH $hash`. ✓ A password containing `'` and `\`, escaped that way, signs in.

✓ Privileges for the statements at the top of `schema.surql`: `DEFINE NAMESPACE` needs root; a namespace-level `OWNER` can run `DEFINE DATABASE` but not `DEFINE NAMESPACE`; `api`, a database-level `EDITOR`, can run neither ("IAM error: Not enough permissions to perform this action"), and every other statement in the file applies as `api`.

The API authenticates app users with the auth provider's JWTs itself, and talks to SurrealDB as `api`. SurrealDB could verify the provider's JWTs directly (`DEFINE ACCESS … TYPE JWT URL`), but that needs custom claims, outbound network access and a session per request. It would also duplicate the membership checks the API makes anyway, so we don't use it.

**Applying the schema.** The schema is [`backend/migrations/schema.surql`](../../backend/migrations/schema.surql), next to the code that runs it; the `store` crate embeds it and the API applies it on start, before it serves traffic:
- ✓ The file is idempotent: `OVERWRITE` for tables, fields and functions, and `IF NOT EXISTS` for indexes. `OVERWRITE` on an index would rebuild it.
- ✓ It first defines the `pdm` namespace and the `main` database. In 3.x, a query against a namespace or database that doesn't exist fails.
- The database is `STRICT`: ✓ a query on an undefined table fails instead of creating it.
- Define indexes before writes start. An index defined while writes are happening can miss them ([#7533](https://github.com/surrealdb/surrealdb/issues/7533)).
- Changing a field's type doesn't convert existing rows; it's checked on each row's next write. Convert explicitly with a batched `UPDATE t SET f = <int> f WHERE f != NONE`.

The official migration tool, SurrealKit, is still in beta (1.0.0-beta.6), and the community `surrealdb-migrations` crate is archived and 2.x-only. An idempotent schema file, plus the small runner below, is enough for the hackathon.

**Applying the migrations (proposed).** Today the `store` crate applies only `schema.surql`, and nothing loads the knowledge base the loader generates ([contracts 10](../architecture/contracts.md#checks-and-generated-files)). The design below fills that gap. It is not code yet; the work items are in [backend store 4](../backend/store.md#4-connecting-and-the-schema).

`backend/migrations/` holds three kinds of file, all committed. A build script in `store` embeds every `*.surql` file in the directory, so the binary and its migrations can't drift and a new file needs no code change.

| File | Written by | Applied | Recorded in `migration` |
|---|---|---|---|
| `schema.surql` | Hand | On every start, first | No: it is idempotent and changes often |
| `NNNN_<slug>.surql`: 4 digits from `0001`, then lower-case words joined by `_` | Hand, in a pull request | Once, in number order, after the schema | Yes. A changed hash or a missing file stops the start |
| `knowledge.surql` | The knowledge loader in `ml/knowledge/`, never by hand | Last, when its hash differs from the recorded one, or when a numbered file was applied in the same start | Yes. Its hash changes with every seed change, by design |

Why one regenerated knowledge file instead of numbered ones. The loader's checks are whole-set checks (unique IDs, edges that resolve), so the file it can vouch for is the whole reviewed set. A numbered diff would need the loader to know what each environment already holds. A numbered full copy would add about 20 MB to the repository per seed change. One file written in a fixed order changes only the lines whose records changed, so Git stores a small delta. The roadmap's first proposal, every file hash-locked, fits the hand-written files only.

What a file may contain:
- No `USE`, `BEGIN`, `COMMIT`, `CANCEL`, `OPTION`, or `DEFINE NAMESPACE`, `DATABASE`, `USER` or `ACCESS`. The runner selects `pdm`/`main` and wraps each file in its own transaction. A test fails on any of these.
- A numbered file does what can't be idempotent in `schema.surql`: a type conversion (`UPDATE t SET f = <int> f`), an index change, or a remap of stored IDs. Tables and fields stay in `schema.surql`, so stored shapes keep one home.
- `knowledge.surql` writes only the knowledge tables and `knowledge_meta`, and ends every write with `RETURN NONE`.

**The knowledge file** replaces the whole knowledge set. A record that isn't in the file is deleted, so a renamed record is the old ID deleted and the new ID written; an ID is never reused ([contracts 10](../architecture/contracts.md#files)). Proposed layout, the shape the loader writes:

```surql
-- Proposed. Generated by ml/knowledge from the reviewed seed. Do not edit.
UPSERT knowledge_meta:current CONTENT { label_set_version: '…', classifier_labels_version: '…',
  embedding_model: 'snowflake-arctic-embed-xs@d8c86521', embedding_dimension: 384 } RETURN NONE;
DELETE susceptible_to; DELETE manages; DELETE caused_by; DELETE preys_on; DELETE looks_like;
DELETE source WHERE id NOT IN [source:`umn-tomato`, …];
UPSERT source:`umn-tomato` CONTENT { … } RETURN NONE;
-- the same for species, disease, organism, treatment, product, classifier_label,
-- soil_condition and knowledge_chunk: delete the stale rows, then upsert in id order
RELATE species:`tomato`->susceptible_to->disease:`tomato_early_blight` RETURN NONE;
INSERT RELATION INTO looks_like { id: looks_like:[disease:`a`, disease:`b`], in: disease:`a`,
  out: disease:`b`, how_to_tell: '…' } RETURN NONE;
```

- Edges are deleted first and rebuilt last. Each table's stale rows go before its upserts, so a product renamed to a new ID with the same name doesn't trip the `product_name` unique index.
- `UPSERT`, not delete-all and insert: an unchanged chunk keeps its row, and the HNSW index sees fewer deletes ([#7423](https://github.com/surrealdb/surrealdb/issues/7423)). Whether the index skips a vector that didn't change is *unverified*.
- User records link to knowledge with plain links (`plant.species`, `scan.working_species`, `question.candidates`, `diagnosis.primary_problem`), not `REFERENCE`. ✓ Deleting a linked species succeeds and the link is left pointing at a missing record. Readers must then treat a missing knowledge record as unknown; no code checks that yet ([backend store 4](../backend/store.md#4-connecting-and-the-schema), open question 1). The loader lists every removed species, problem and organism ID in its output, so the reviewer sees them in the pull request.
- The output is deterministic: tables in the order above, records by ID, each float in the shortest form that reads back as the same `f32`. The same seed gives the same bytes, so the same hash, and `api` skips the file.
- ✓ 4,000 chunks of 384 floats and 340 characters of text make a 21.0 MB file. ✓ One query of 62.9 MB applies over WebSocket; one of 68.2 MB is refused before it is sent ("Message too long"), which fits a 64 MiB message cap (*unverified* in the source). The loader fails when the file passes 48 MiB, about 9,000 chunks.
- ✓ Without `RETURN NONE`, a 62.8 MB file returned every record it wrote, the connection reset, and the transaction had committed all the same. With it, the same file applied in 5.3 s.

**How `api` applies them**, at start-up step 3 ([backend api 4](../backend/api.md#4-start-up-and-shutdown)), over the short-lived root connection that applies the schema today:

1. Apply `schema.surql`.
2. Select `pdm`/`main`, and read `SELECT record::id(id) AS name, kind, hash FROM migration`.
3. Check the record against the files. Stop the start, naming the file, when:
   - a recorded numbered file has a different hash (both hashes are logged);
   - a recorded file is missing from the build (a renamed file, or a database ahead of this build);
   - an unrecorded numbered file sorts before the highest recorded one (merged out of order: give it the next number).
4. Apply each new numbered file in order, each in its own transaction.
5. Apply `knowledge.surql` if its hash differs from the recorded one, or if step 4 applied any file. A numbered file can delete knowledge rows, as a dimension change does below.
6. Close the root connection, then sign in as `api` and load the knowledge index, as today.

Each file runs in one transaction with its own record, so a file and its record commit together or not at all:

```surql
-- Proposed wrapper; $name, $kind and $hash are bound, and the file goes where the comment is.
BEGIN;
LET $prev = (SELECT VALUE hash FROM ONLY type::record('migration', $name));
IF $prev != $expected { THROW 'migration raced: ' + $name };
-- the file
UPSERT type::record('migration', $name) CONTENT { kind: $kind, hash: $hash, applied_at: time::now() };
COMMIT;
```

- `$expected` is the hash step 2 read, or `NONE` for a file never applied. If another process applied the file in between, the `THROW` rolls this attempt back, and the runner reads the record again and moves on. Production runs one `api`, so this is a guard, not a lock.
- It also covers a lost reply: a file that committed before the connection dropped is recorded, so the next start skips it.
- The hash is the SHA-256 of the file, as 64 lower-case hex digits, with `\r\n` read as `\n`, so a Windows checkout hashes the same.

**The record of applied files**, proposed for `schema.surql`:

```surql
-- Proposed. One row per applied migration file; id = the file name, e.g. migration:`0002_x.surql`.
DEFINE TABLE OVERWRITE migration SCHEMAFULL;
DEFINE FIELD OVERWRITE kind       ON migration TYPE "versioned" | "knowledge";
DEFINE FIELD OVERWRITE hash       ON migration TYPE string ASSERT $value = /^[0-9a-f]{64}$/;
DEFINE FIELD OVERWRITE applied_at ON migration TYPE datetime;

-- In schema.surql since v1.7. The generated values of the current knowledge set: one record, knowledge_meta:current.
DEFINE TABLE OVERWRITE knowledge_meta SCHEMAFULL;
DEFINE FIELD OVERWRITE label_set_version         ON knowledge_meta TYPE string;
DEFINE FIELD OVERWRITE classifier_labels_version ON knowledge_meta TYPE string;
DEFINE FIELD OVERWRITE embedding_model           ON knowledge_meta TYPE string;
DEFINE FIELD OVERWRITE embedding_dimension       ON knowledge_meta TYPE int;
```

`knowledge_meta` stores the versions the loader computes, so `load_knowledge` reads `label_set_version` instead of deriving its own hash ([backend store 22](../backend/store.md#22-open-questions), item 7).

**Checked on the pinned image.** ✓ Run on 2026-10-16 against `surrealdb/surrealdb:v3.3.4` by digest, RocksDB engine, as root over WebSocket with the `surrealdb = "=3.3.4"` client, using the real `schema.surql` plus the two proposed tables:
- ✓ The wrapper around a 21 MB file of 4,000 chunks commits in 2.2 s on empty tables and in 6–7 s over the same rows, so skipping an unchanged file saves real time at each start. The record's hash matches the file.
- ✓ After the load, `<|5, 40|>` runs as a `KnnScan` and returns the nearest chunk first; after a file that drops 100 chunks, a dropped chunk's own vector no longer finds it. Right after the load, the server logged that HNSW compaction was behind the writes; the search still answered.
- ✓ A file with an invalid `topic` halfway through fails; no row changes and the record keeps the previous hash.
- ✓ A stale `$expected` throws "migration raced" and rolls back. Re-sending a recorded file throws the same way.
- ✓ `REMOVE INDEX` and `DELETE` inside the wrapper roll back with the file.
- ✓ `DELETE knowledge_chunk` followed by `DEFINE INDEX chunk_vec … DIMENSION 8` in the same transaction fails ("Incorrect vector dimension (384)"): the index build still sees the deleted rows. In two files, one after the other, it works, and afterwards a 384-float vector is refused.
- ✓ `<|$k, 40|>` with a bound `k` is a parse error ("expected an unsigned integer"), so `k` must be a literal.

**Changing the text-embedding model.** One pull request: the new model in the manifest; the loader re-embeds every chunk (its cache is keyed by text hash and model), so `knowledge.surql` changes with the new `embedding_model`. If the dimension changes too:
- change `DIMENSION` in `schema.surql`, for new databases;
- add `NNNN_drop_chunk_vec.surql` (`REMOVE INDEX chunk_vec ON knowledge_chunk; DELETE knowledge_chunk;`) and `NNNN+1_chunk_vec_<dim>.surql` (the new `DEFINE INDEX`). Two files, because one transaction can't do both.

Since a numbered file ran, `knowledge.surql` is applied again in the same start. If the dimension files are forgotten, the old index refuses the new vectors and the start fails, which is the safe failure. `api` also stops when `knowledge_meta.embedding_model` differs from the embedder it loaded, since its query vectors would not match the stored ones.

---

## 2. Conventions

| Topic | Rule |
|---|---|
| Record IDs | ULID string keys generated in Rust: `RecordId::new("scan", Ulid::generate().to_string())`. `ulid` 3.0.0 has no `Ulid::new()`, and `RecordIdKey` has no `From<Ulid>`, so the key is the string. Knowledge records use their slugs. A few tables use a natural **array key** (below). The API strips the table prefix in responses (contracts section 1) |
| Array keys | Used where the key *is* the uniqueness rule or the access path: `scan_event:[scan, seq]`, `member_of:[user, garden]`, `idempotency_key:[user, route, key]`, `usage_daily:[user, day]`, `weather_cache:[hour, cell]`, `push_token:[token]`, `worker:[worker_id]`, `message_feedback:[message]`, `classifier_label:[model_version, index]`, `soil_condition:[aspect, value]`. ✓ `DEFINE FIELD id … TYPE [record<scan>, int]` rejects a key of the wrong shape |
| Optional fields | `option<T>`. A field is absent (`NONE`), never `NULL`. ✓ `option<string>` refuses `NULL`. The Rust SDK sends `None` as `NONE` |
| Enums | Literal union types: `TYPE "open" \| "answering"`. ✓ A bad value fails with ``Expected `'pending' \| 'queued'` `` |
| Instants | `datetime`. Bind `chrono::DateTime<Utc>`, never a formatted string, which is stored as a string |
| Local dates | `"YYYY-MM-DD"` strings checked by `fn::is_local_date`. ✓ `'20/10/2026'` is rejected |
| Nested shapes | Shapes owned by contracts.md and validated in Rust are `object FLEXIBLE` or `array<object> FLEXIBLE`. Anything a query filters on is a typed top-level field. ✓ A SCHEMAFULL table rejects unknown fields, including inside non-FLEXIBLE objects. ✓ Typed sub-fields of a FLEXIBLE array are still checked |
| Links | `record<t>` fields. Parent links carry `REFERENCE ON DELETE REJECT`, a few leaf links `CASCADE`, and author links `UNSET` (section 4, deletion) |
| Timestamps | `created_at … DEFAULT time::now() READONLY` on every table that has one |

---

## 3. Tables

| Area | Tables |
|---|---|
| Identity | `user` (unique `auth_id`), `push_token`, `deleted_user` (the account tombstone) |
| Gardens | `garden` (stored `weather_cell` and `geohash`), `member_of` (relation user → garden), `invite` (unique `code`), `plant`, `garden_map` (unique `garden, version`) |
| Scans | `scan`, `scan_image`, `mask`, `scan_event`, `observation`, `question`, `indicator_answer`, `guided_check`, `diagnosis`, `scan_feedback` |
| Plans | `action_plan` (unique `scan, version`), `plan_trigger`, `task` (unique `alert_key`) |
| Chat | `conversation`, `chat_image`, `message`, `pending_action`, `message_feedback` |
| Connectors | `connector`, `farm_record` ([connectors 4](../connectors/README.md#4-data)); not run on the server image, only on the embedded engine |
| Memory | `memory` (also mirrored, when shared, into [SurrealDB Agent Memory](agent-memory.md)) |
| Knowledge | `source`, `species`, `disease`, `organism`, `treatment`, `product`, `classifier_label`, `soil_condition`, `knowledge_chunk`, `knowledge_meta` (one record, the generated versions); edges `susceptible_to`, `manages`, `caused_by`, `preys_on` (fieldless, `LIGHTWEIGHT`) and `looks_like` (with `how_to_tell`) |
| Operations | `job`, `worker`, `usage_daily`, `idempotency_key`, `weather_cache` |

Changes from the architecture sketch (section 5 there):
- **New tables:** `guided_check`, `chat_image`, `idempotency_key`, `classifier_label`, `soil_condition`.
- **Renamed:** `feedback` → `scan_feedback`, to sit beside `message_feedback`.
- **New fields:**
  - `scan.plan`, which the compare-and-set updates, and `scan.last_event_seq`, the event counter;
  - `task.state`, so the task list filters on one index;
  - `garden.deleting_at` and `plant.deleting_at`, for the background deletion cascade;
  - `action_plan.reason`.
- **For the app's screens** (contracts v0.8):
  - `task.scan`, the scan a guided-check task belongs to, with `REFERENCE ON DELETE CASCADE`: such a task can only be done on that scan;
  - `scan.deleting_at`, for `DELETE /scans/{id}` of a pending scan;
  - `diagnosis.low_evidence`, default `false`;
  - `species.reference_images` (at most 3, each with `storage_key`, `credit` and `source`), checked by an `ASSERT` on the length;
  - `city` in `garden.location_precision`.
  - Thumbnails, the plant's latest scan and the garden's weather are computed when read, from `scan_image`, `scan` and `weather_cache`, so they add no fields.
  - So are `SpeciesCandidate.reference_image`, from `species.reference_images[0]` each time a candidate is read or sent in an event, and `Diagnosis.organisms[].preys_on`, from the `preys_on` graph. Neither is stored on `plant.species_candidates`, `diagnosis.organisms` or an event payload, so a reseed shows the new photo and prey everywhere.
  - `reference_images.*.storage_key` is `knowledge/species/{species_id}/{hash}.webp`, with a content hash, so a changed photo gets a new key ([storage 2](../storage/README.md#2-bucket-keys-and-access)).
- **Agent Memory** (v1.6, [agent-memory.md](agent-memory.md)): `memory.topic`, a string union of watering, feeding, treatment, pruning, planting, location, container, soil, light, health, harvest and other (default `other`), and `memory.belief_synced_at`, an `option<datetime>` set when a shared memory has been recorded as a belief, with an index for the sweep that records the unsynced ones oldest first. *Unverified* on the pinned image by digest.
- **Farm connectors** (v1.8, [connectors 4](../connectors/README.md#4-data); `schema.surql` is the source):
  - `connector`: `garden` and `created_by` (`REFERENCE ON DELETE REJECT`), `kind`, `status`, `endpoint`, `account`, `sources`, the sealed `secret`, the pending connect's `oauth`, `cursor`, `synced_at`, `next_sync_at` (default now), `sync_requested_at`, `last_error` and `created_at`. `last_error` is a literal union of `rate_limited`, `vendor_unavailable`, `auth_refused`, `too_large` and `source_missing`, never the vendor's text. Indexes `connector_garden (garden)` and `connector_due (status, next_sync_at)`, which the `connectors` task reads for what is due.
  - `sync_requested_at` is for **Sync now** ([EON E1.6](../EON/README.md#e1-contracts-schema-and-store), decision 12). In one transaction, after checking the connector is the garden's and `active`, `UPDATE $connector SET sync_requested_at = $now, next_sync_at = $now WHERE sync_requested_at = NONE OR sync_requested_at <= $cutoff RETURN VALUE id` (`$cutoff` = now − 5 minutes) is the compare-and-set: it returns the id when the request is accepted and nothing within 5 minutes of the last, and the transaction's last statement returns the last request's time, from which the API gives `retry_after_s`. ✓ (embedded 3.3.4) accepted at t, refused at t + 4:59, refused from a second store over the same datastore (a restarted process), accepted at t + 5:00 (`sync_now_is_one_request_in_five_minutes`, `backend/crates/store/tests/farm.rs`). Two racing requests write the same record, so one conflicts and is retried by `run_tx`, and then sees the other's time.
  - The repository (`store::repo::farm`, [store 16](../backend/store.md#16-chat-memory-and-knowledge)) is built and ✓ ran on the embedded 3.3.4 engine (`tests/farm.rs`, 14 tests; **Sync now** on a connector stopped by `too_large` or `source_missing` is `invalid_state`, two racing requests accept exactly one, and the run's end keeps a later `next_sync_at`): the due query (`status = 'active' AND next_sync_at <= $now`, earliest first) leaves out `pending`, `reauth_needed`, `deleting` and a connector waiting on new sources (`too_large`, `source_missing`); the connect is a compare-and-set on `status = 'pending'`, the nonce and its expiry; a garden holds at most 2 connectors besides those being deleted, also under racing creates: a count alone doesn't hold it, because snapshot isolation lets two creates of different keys both commit (✓ on the embedded engine, racing creates on a garden with one connector were accepted more than once before the counter), so each create first bumps `garden.connector_rev`, the creates conflict and the retry meets the limit; an upsert by `farm_record:[connector, vendor_id, n]` keeps the stored vector only while `text_hash` is unchanged; `vendor_id IN $ids` on `farm_record_connector` selects and deletes the right rows; a re-chunked document's extra chunks go by the key range `farm_record:[$c, $v, $from]..=[$c, $v, ..]`; a source's records go by the `source` stored on each record (the Slack channel id or the Drive folder id), 500 at a time, so a folder swap keeps the new folder's records; Slack messages older than 180 days (documents stay while they are under the folder) and pending connectors older than 10 minutes go in batches of 500; a connector's records go before the connector, and only once it is `deleting`.
  - `farm_record`: the record id is the array `[connector, vendor_id, n]`, because a Slack `ts` holds a dot; `FarmRecordId` is that key as one string (contracts 9). Fields `garden` and `connector` (both `REJECT`), `kind`, `vendor_id`, `source`, `title`, `author?`, `link`, `text` (one chunk, at most 2,000 characters), `text_hash`, `embedding`, `embedding_model` and `at`. Indexes `farm_record_garden`, `farm_record_connector (connector, vendor_id)` `farm_record_source (connector, source)` and `farm_record_kind_at (kind, at)` (the 180-day cleanup of messages; it replaces `farm_record_at`, which the schema removes with `REMOVE INDEX IF EXISTS`, because an `IF NOT EXISTS` redefinition would not rebuild an index).
  - `action_plan.farm_records` and `message.farm_citations`: `array<object> FLEXIBLE DEFAULT []`, the `FarmCitation`s as they were, kept when a record is deleted.
  - ✓ (embedded 3.3.4) a plan and messages written through the store keep `farm_records` and `farm_citations`. `DEFAULT []` applies only on write: a row stored before the field existed has none, and reads as empty (`plans_and_messages_carry_farm_citations`, which removes the fields, unsets them and defines them again).
  - The embedded engine test `farm_connectors_store_and_go_with_their_garden` (`backend/crates/store/tests/embedded.rs`) ran: a connector and a record with a dotted `ts` key are written and read back as a `FarmRecordId`, the plan and message fields are stored, and the garden cascade deletes records, then connectors, then the garden. Nothing here has run on the server image.
- **Vector indexes:** `memory`, `scan_image` and `farm_record` embeddings get no HNSW index (section 5).
- **LiDAR** (architecture 6.7, 6.8): the `garden_map` table with the `garden.map_version` counter; `plant.position`; `scan_image.depth_key`, `depth_confidence_key` and `depth_meta`; on measurement observations, top-level `plant`, `quantity`, `measured_value` and `taken_at` for the growth series (index `observation_growth`); `question.step` and `step_at` for photo answers (index `question_stalled`); and `plan_trigger.plant` indexed (`plan_trigger_plant`).
- **Indicator answers** (v1.4, contracts 7, measurable indicators): the `indicator_answer` table, `indicator_answer:[scan, run, indicator_id]`, holds every answer of a run (the user's, the measurements' and the VLM's) between the steps that write them and the `score` step that reads them (section 4, indicator answers). Also `scan.candidates`, the run's candidate set, which `retrieve` writes, a `verify` adds to and a new run resets; `question.indicator`, the indicator an `indicator` question asks about (server-side only), so an answer becomes that indicator's user evidence; and `mask.rle`, the worker's run-length mask, kept for measurement.
- **Account deletion and soil retries** (v1.4): `deleted_user:[auth_id]`, the account tombstone (section 4, deletion), and `garden.soil_retry_at`, the earliest next SoilGrids attempt after a failure.
- **Recovery between jobs:** `scan.pipeline_step`, `pipeline_step_run`, `pipeline_step_at` and `rescore_pending` (index `scan_stalled`); `job.question`, so the fallback's claim can hold back a photo answer's segmentation job; and `observation.image`, so a stage's observations are replaced per image (section 5). `observation.quantity` also takes `hole_count`.

---

## 4. Write patterns

Each statement is its own transaction. Statements that must succeed or fail together go in one `BEGIN … COMMIT` sent as **one** query. Isolation is snapshot isolation: there are no locks, and a write-write conflict fails at `COMMIT` with a retryable `TransactionConflict`: ✓ "Transaction conflict: Write conflict, retry the transaction. This transaction can be retried". A single statement is its own transaction and can conflict the same way. **The API retries the whole transaction**, up to 5 times with jittered back-off (section 6).

### Scan events: a gap-free `seq` ✓

The scan record is the counter. The per-scan writer (contracts section 4) appends each event in one transaction:

```sql
BEGIN;
LET $seq = (UPDATE ONLY $scan SET last_event_seq += 1 RETURN VALUE last_event_seq);
CREATE type::record('scan_event', [$scan, $seq]) SET run = $run, type = $type, payload = $payload;
COMMIT;
```

✓ With 40 concurrent appends to one scan, 35 committed and 5 failed with a retryable conflict. The stored sequence was exactly 1…38, with no gaps or duplicates (3 events existed before the burst). Because the API's per-scan writer serialises appends, conflicts only happen across API processes or on races with other updates to the scan record, and the retry handles them. Don't use `DEFINE SEQUENCE` here: sequence values are never rolled back, so a failed transaction leaves a gap.

`payload` stores image and mask IDs, never signed URLs: the API adds fresh URLs each time it sends or replays an event (contracts 4), so a replay hours later doesn't hand out expired links.

SSE replay after `Last-Event-ID` is a range read of the record keys, with no index involved. ✓ Bound parameters work inside the range:

```sql
SELECT * FROM scan_event:[$scan, $after + 1]..=[$scan, ..];
```

### Plan writes: compare-and-set ✓

```sql
BEGIN;
CREATE $new_plan CONTENT $plan;
LET $won = (UPDATE $scan SET plan = $new_plan WHERE plan = $expected RETURN VALUE plan);   -- first plan: WHERE plan = NONE
IF array::len($won) = 0 { THROW 'stale_plan' };
COMMIT;
```

- ✓ When another plan won in the meantime, the `THROW` rolls back the new plan too, so a losing write leaves nothing behind. The API sees the `stale_plan` error.
- An automatic re-plan then starts again from the new current plan; a chat revision returns `409` (contracts section 7, re-planning).
- ✓ The unique index on `action_plan (scan, version)` is the backstop against two writers both creating version 2.

### Job claim with `slots` ✓

```sql
BEGIN;
LET $held = (SELECT count() AS n FROM job WHERE worker = $worker AND status = 'leased' GROUP ALL)[0].n ?? 0;
IF $held < $slots {
  UPDATE (SELECT VALUE id FROM job
          WHERE status = 'queued' AND kind IN $kinds AND queued_at <= $eligible_before
          ORDER BY queued_at LIMIT 1)
  SET status = 'leased', worker = $worker, lease_token = rand::ulid(),
      claimed_at = time::now(), lease_until = time::now() + 2m
  WHERE status = 'queued'
  RETURN AFTER;
} ELSE { RETURN []; };
COMMIT;
```

- ✓ The inner `SELECT` uses the `job_queue` index in `queued_at` order. With one kind there is no in-memory sort; ✓ with several kinds (the fallback worker's list) the plan adds a top-k sort over the matching queued rows, which the queue's small size keeps cheap.
- ✓ The outer `WHERE status = 'queued'` re-checks the row. Two workers that pick the same job conflict at commit, and the loser retries and gets the next one.
- `$eligible_before` is `time::now()` for the GPU workers and `time::now() - 15s` for the fallback worker (contracts section 5).
- The fallback worker's claim adds `AND (question = NONE OR !$perception_alive)`, where `$perception_alive` is `(SELECT count() AS n FROM worker WHERE caps CONTAINS 'perception' AND last_heartbeat > time::now() - 30s GROUP ALL)[0].n ?? 0) > 0`. A photo answer's segmentation job (top-level `job.question`) therefore waits for the perception worker while the rig is up. ✓ With a perception worker alive it skips that job; with none for 30 seconds it takes it. The filter is applied after the index and adds no sort.
- The claim is a long poll. The API doesn't re-query in a tight loop: it waits on an in-process `tokio::sync::Notify` that every job insert signals, with a 1-second fallback poll. The API makes every write, so it doesn't need a live query.
- Expired leases and stale workers are requeued by the scheduler with `UPDATE job SET status = 'queued', … WHERE status = 'leased' AND lease_until < time::now()`. ✓ That uses the `job_lease` index.

### Idempotency keys ✓

```sql
CREATE type::record('idempotency_key', [$user, $route, $key])
  SET fingerprint = $fp, expires_at = time::now() + 24h;
```

✓ A second `CREATE` with the same key fails with "already exists". ✓ When several connections create the same key at once, a loser can get the retryable conflict instead (8 of 320 in one run); the API's retry then gets "already exists". The same holds for `INSERT RELATION` in membership. The API then reads the record and applies contracts section 1: replay the stored response, return `409 request_in_progress`, or return `422 idempotency_conflict`. When the request finishes, the API sets `state = 'done'`, `response_status` and `response_body`. A response that must not be stored deletes the key instead.

### Limits and counters ✓

```sql
UPSERT type::record('usage_daily', [$user, $utc_day]) SET scans += 1 RETURN VALUE scans;
```

✓ `+=` on a missing record starts from the field's default. Run it in the same transaction as the scan it counts, and `THROW` when the result is over 30.

### Membership ✓

```sql
INSERT RELATION INTO member_of { id: type::record('member_of', [$user, $garden]), in: $user, out: $garden, role: $role };
```

- ✓ `INSERT RELATION` fails when the member exists, so accepting an invite twice never changes a role.
- ✓ `RELATE` with the same fixed id **overwrites** the existing edge, so don't use it here.
- The membership check is a direct read of `member_of:[$user, $garden]`. ✓ Deleting a user or a garden removes their `member_of` edges.

### Alert dedupe ✓

`task.alert_key` has a unique index. ✓ Any number of tasks can have no key, and a second task with the same key fails, which makes the scheduler's dedupe race-free (architecture 8.3).

### Steps between jobs ✓

`api`'s own work between jobs (contracts 3, steps between jobs) holds a claim on the scan: `pipeline_step`, `pipeline_step_run` and `pipeline_step_at`. The sweep can then recover a step after a restart, the sweep and a slow original never both commit, and a step of an older run never commits.

```sql
-- claim, by an entry point (in the result handler's transaction for a job result).
-- Claiming score clears rescore_pending: the step reads every input when it starts.
LET $claimed = (UPDATE ONLY $scan SET pipeline_step = $step, pipeline_step_run = run, pipeline_step_at = time::now(),
                      rescore_pending = IF $step = 'score' { false } ELSE { rescore_pending }
  WHERE pipeline_step = NONE AND plant.deleting_at = NONE AND garden.deleting_at = NONE RETURN AFTER);
-- NONE back: another step holds the claim, so flag the new input instead
IF $claimed = NONE { UPDATE ONLY $scan SET rescore_pending = true WHERE pipeline_step != NONE; };

-- commit: re-check the claim, write the output and hand over, in one transaction
BEGIN;
LET $s = (SELECT pipeline_step, pipeline_step_run, pipeline_step_at, run, rescore_pending,
                 plant.deleting_at AS plant_deleting, garden.deleting_at AS garden_deleting
          FROM ONLY $scan);
IF $s.pipeline_step != $step OR $s.pipeline_step_at != $stamp OR $s.pipeline_step_run != $s.run
   OR $s.plant_deleting != NONE OR $s.garden_deleting != NONE { THROW "claim_lost"; };
-- … the step's output: the queued job, the diagnosis, the question or the plan …
LET $next = IF $s.rescore_pending { 'score' } ELSE { $handover };   -- $handover: contracts 3
UPDATE $scan SET pipeline_step = $next,
                 pipeline_step_run = IF $next = NONE { NONE } ELSE { run },
                 pipeline_step_at = IF $next = NONE { NONE } ELSE { time::now() },
                 rescore_pending = false;
COMMIT;

-- the sweep: re-claim a stale step by its old stamp and run; NONE back means someone else has it
UPDATE ONLY $scan SET pipeline_step_at = time::now()
  WHERE pipeline_step = $step AND pipeline_step_at = $seen AND pipeline_step_run = run
    AND plant.deleting_at = NONE AND garden.deleting_at = NONE
  RETURN AFTER;
```

- ✓ The flag is set only when the claim came back `NONE`. Run unconditionally, the second statement also matches a scan the first one just claimed (its `pipeline_step` is now set) and flags it, so the step would hand over to `score` for no new input. The `store` crate also returns `not_needed` for a scan being deleted or one whose status claims nothing ([store](../backend/store.md)).
- A flag still set when a step commits means input arrived after the step read its inputs, so the commit hands over to `score` instead of its own next step. A question already asked or a plan already written is not lost: `score` simply runs again with the new input, under the re-planning rules (contracts 7).
- Starting a run, failing the scan, and setting `deleting_at` on the plant or garden clear the claim (`pipeline_step = NONE`) in the same transaction.
- Photo answers use `question.step`, `step_at` and the question's `run` the same way, and superseding a question clears its step.
- Two claims on the same row conflict at commit, and the API retries the loser, which then finds the claim taken.

✓ Of two claims, the first wins and the second returns NONE. A slow original whose stamp the sweep replaced throws `claim_lost`, and its diagnosis is never written. A claim whose run is older than the scan's is refused the same way.

### Indicator answers ✓

Each indicator keeps one answer per run, ranked user, then measurement, then the VLM (contracts 7). The key `indicator_answer:[scan, run, indicator_id]` is that rule, and a computed `rank` field (3, 2, 1) lets a write keep the higher-ranked answer without reading it into Rust first:

```sql
-- re-measuring a photo first drops that photo's measured answers (contracts 7: a re-measure replaces that photo's measurements only)
IF array::len($measured_images) > 0 {
  DELETE indicator_answer WHERE scan = $scan AND run = $run AND source = 'measurement' AND image IN $measured_images;
};
FOR $a IN $answers {
  IF ((SELECT VALUE rank FROM ONLY $a.id) ?? 0) <= $a.rank { UPSERT $a.id CONTENT $a.row; };
};
```

- ✓ A measured answer replaces a VLM answer, a user answer is kept when a measured one arrives for the same indicator, and an answer of the same rank replaces the stored one.
- The fragment runs inside the transaction that stores what produced the answers: a step's commit (under its claim), a photo answer's step, a job's completion, or `POST /scans/{id}/answers`.
- `score` reads `SELECT VALUE evidence FROM indicator_answer WHERE scan = $scan AND run = $run` (index `indicator_answer_scan`). A new run starts with none; the earlier run's answers stay until the scan is deleted.
- `scan` repeats the key's scan as a field with `REFERENCE ON DELETE REJECT`, and `image` and `question` are rejecting references too, so ✓ deleting a scan while an answer remains fails with "Cannot delete `scan:s` as it is referenced by `indicator_answer:[scan:s, 1, 'a.b']`".

### Garden map versions ✓

`PUT /gardens/{id}/map` runs in one transaction: check that the upload's `base_version` equals `garden.map_version` (otherwise `409`, contracts 9), increment it, create the `garden_map` with the new version, delete the garden's older maps, and set or unset `position` on the garden's plants. Pins for plants with `deleting_at` set are dropped first. Two concurrent uploads conflict on the garden row; the API retries the loser, whose retry then fails the `base_version` check and returns `409`, so an upload never silently replaces a map made since its session started. Only the current map is kept, so a pin can only point at a live plant: deleting a plant first removes its pin from that map in place.

✓ Two uploads in a row leave one map, at version 2, with the unpinned plant's `position` gone; a duplicate `[garden, version]` is rejected by the unique index, and so is a `north_deg` outside 0–360. ✓ `DELETE garden` is rejected while a map exists, so the cascade must delete the map first. ✓ An upload whose `base_version` is stale throws inside the transaction (`IF garden.map_version != $base { THROW … }`), and nothing changes: the version, the map and every `position` stay as they were.

### Deletion ✓

Parent links use `REFERENCE ON DELETE REJECT`. ✓ `DELETE garden:g1` while a plant still points at it fails with "Cannot delete `garden:g1` as it is referenced by `plant:p1`". A missed step in the cascade therefore fails loudly rather than orphaning records.

The API deletes **bottom-up in batches**, never as one large transaction:
1. Set `deleting_at` on the garden or plant. Every read filters it out, which is what makes `202 Accepted` safe.
2. Cancel the jobs, and clear the scans' step claims (`pipeline_step = NONE`) and their questions' `step`. A step already running then fails its commit check, which also re-checks `deleting_at`, so it can't write after the records below are gone.
3. Delete the objects in RustFS ([storage](../storage/README.md)).
4. Delete the records, children first. Every step below holds a `REFERENCE ON DELETE REJECT` to something a later step deletes, so the order is fixed:
   1. scan events (one range delete per scan) and the scans' jobs, which point at their scan;
   2. tasks and plan trigger rules, which point at their plan: for a garden, all of them; for a plant, its own, those its plans made for neighbours (found through `task.plan` and `plan_trigger.plan`), and those other plants' plans made for it (`task.plant`, `plan_trigger.plant`). The other plants' plans themselves are not changed (contracts 7);
   3. plans and diagnoses, after unsetting `scan.plan`, `scan.diagnosis` and `scan.open_question`. Select plans by equality (`WHERE plant = $p`, `WHERE garden = $g`, or one scan at a time), never `WHERE scan IN $scans`: ✓ on 3.3.4, a `DELETE` over that `IN` filter on the unique `action_plan_version` index deletes nothing ([#7593](https://github.com/surrealdb/surrealdb/issues/7593)), and the scan delete then fails;
   4. masks, guided checks and indicator answers, which point at scan images (indicator answers also at questions), then observations, questions and scan feedback;
   5. scan images, then scans;
   6. conversations: for a garden, every conversation of the garden (`conversation.garden`), whatever it is anchored to, since any left would block the garden; for a plant, those anchored to the plant or its scans (`conversation.anchor`). For each, pending actions, message feedback, messages and chat images, then the conversation;
   7. memories, then plants (a single plant first loses its pin on the garden map), then the garden map;
   8. for a garden only (v1.8, `CascadeStep::FarmRecordsAndConnectors`): its farm records, then its connectors, since a record points at its connector and both at the garden. The tokens are revoked by `api` after this step commits, not before (decided in [EON E3.6](../EON/README.md#e3-configuration-tokens-and-connecting); not built yet); a plant has no such step. Then the garden.

✓ This order deletes a whole garden, with plans, tasks, trigger rules, jobs, conversations and a map, and a single plant whose plan made a neighbour task on another plant, cleanly. ✓ (v1.4) So does a garden whose scan has an indicator answer, deleted in step 4.

**A pending scan** (`DELETE /scans/{id}`, contracts 3) uses the same steps for that one scan. Set `scan.deleting_at`, cancel its guided jobs, delete its objects, then delete its records: its events (one range delete) and jobs; its guided checks; its observations; its scan images; the conversations anchored to it, as in step 6; and then the scan, which deletes the tasks naming it through `task.scan`'s `CASCADE`. A pending scan has no plan, diagnosis, question or step claim, and no scan feedback (contracts 3 refuses feedback before submit). ✓ On 3.3.4 a pending scan with a guided check, its observation, its event and a queued guided job deletes cleanly in this order. Deleting the scan while its job still exists fails with "Cannot delete `scan:…` as it is referenced by `job:…`". ✓ Deleting the scan deletes a task naming it and leaves the plant's other tasks alone; a plant cascade, which deletes tasks before scans, is unchanged.

The `CASCADE` only reaches tasks that exist when the scan is deleted, and the schema accepts a link to a missing scan (✓ a task with `scan = scan:missing` is created). So `POST /tasks` with a `scan_id` checks the scan in the same transaction as the insert, as it does the plant (contracts 9): the scan exists, is of that garden and plant, and has no `deleting_at`, otherwise it throws and nothing is created. ✓ On 3.3.4 that transaction creates the task for a live scan and creates nothing for a missing scan or one with `deleting_at`.

Each batch is `DELETE (SELECT VALUE id FROM t WHERE … LIMIT 500) RETURN VALUE $before.id`, repeated until it returns no ids. ✓ `RETURN NONE` can't be used to stop: it returns `[]` both for a batch that deleted 2 rows and for one that deleted none. ✓ `RETURN VALUE id` gives `NONE` for each row, since the record is gone; `RETURN BEFORE` works but returns whole records. Small transactions don't conflict with live writers.

**Account deletion** (architecture 5) starts with a tombstone, `CREATE deleted_user:[auth_id] SET user = $user, started_at = $now`. ✓ A second `CREATE` fails with "already exists", so only one cascade starts however many requests or webhooks arrive. `user` is the ULID as a string, not a link, because the `user` record is deleted before the auth provider's account. While the tombstone exists, a sign-in with that auth id is refused (the API answers `401`) instead of recreating the user: the check runs on the read of an existing user and in the transaction that would create one ✓. The tombstone is the cascade's marker after a restart (`clerk_deleted_at = NONE`, index `deleted_user_pending`); `clerk_deleted_at` is set once the auth provider confirms, and the cleanup deletes tombstones 24 hours after that.

A few links behave differently:
- **`connector.created_by`** holds `REFERENCE ON DELETE REJECT` to the user, so deleting an account has to delete that user's connectors first; that step is [EON E3.6](../EON/README.md#e3-configuration-tokens-and-connecting) and does not exist yet.
- **CASCADE** (✓ deleting a user deletes their push tokens): `push_token.user`, `invite.garden`, `scan_feedback.user`, and `task.scan` (✓ a jar-test task goes with its pending scan). These records have nothing outside the database.
- **UNSET** (✓ the field disappears): `scan.created_by`, `task.created_by`, `memory.author`, `invite.created_by`/`used_by`, and `garden_map.created_by`. This keeps garden history when a member deletes their account (architecture 5, deletion).
- **Plain links with no reference:** `memory.source_msg` and `action_plan.constraints[*].from_pending_action`, whose targets can be deleted first.

### Cleanup (no TTL)

SurrealDB has no record expiry ([#1317](https://github.com/surrealdb/surrealdb/issues/1317)). The scheduler runs these every 10 minutes, in batches of 500:

| What | Statement |
|---|---|
| Idempotency keys | `DELETE (SELECT VALUE id FROM idempotency_key WHERE expires_at < time::now() LIMIT 500) RETURN VALUE $before.id`, until it returns no ids |
| Weather cache older than 2 hours | ✓ `DELETE weather_cache:[NONE, NONE]..[$cutoff, NONE]`, a key range because the hour comes first in the key |
| Pending actions | `UPDATE pending_action SET status = 'expired' WHERE status = 'proposed' AND expires_at < time::now()` |
| Finished jobs older than 7 days | Delete in batches. Their payloads are the bulk of the table |

Reads hide anything expired but not yet deleted (`WHERE expires_at > time::now()`).

---

## 5. Reads, indexes and vector search

**Every hot query is indexed.** ✓ `EXPLAIN` shows an `IndexScan` for each of these:

| Query | Index |
|---|---|
| Task list: `WHERE garden = $g AND state = 'open' ORDER BY due LIMIT 20` | `task_garden (garden, state, due)`, in index order with no sort step |
| Unanswered-question sweep: `WHERE status = 'open' AND continue_at < $now` | `question_sweep` |
| Scheduler weather groups: `WHERE weather_cell = $cell` | `garden_cell` |
| Expired leases: `WHERE status = 'leased' AND lease_until < $now` | `job_lease` |
| Job claim | `job_queue (status, kind, queued_at)` |

Added with LiDAR, ✓ each an `IndexScan` with no sort step:

| Query | Index |
|---|---|
| Growth series: `WHERE plant = $p AND quantity = $q AND retracted_at = NONE ORDER BY taken_at` | `observation_growth (plant, quantity, taken_at)` |
| Stalled photo answers: `WHERE step = 'measuring' AND step_at < $now - 2m` | `question_stalled` |
| Stalled run steps: `WHERE pipeline_step = $step AND pipeline_step_at < $now - $limit` | `scan_stalled` |
| Current garden map: `WHERE garden = $g AND version = $garden.map_version` | `garden_map_version` |

The growth series reads every measurement observation of the plant, not the scans' current `run`: a species-change run keeps the earlier run's measurements (contracts 7, growth). That works because re-running a pipeline stage deletes that stage's earlier observations **for the same images** (`observation.image`) in the transaction that stores the new ones, so only current observations remain. A run covers all of the scan's images, and also replaces the stage's observations with no image (species candidates, the plant check, the VLM's species guess); a question photo's segmentation and measurements cover that photo alone and replace nothing else; re-measuring one photo replaces that photo's measurements only. `taken_at` is the scan's `created_at`, so a re-measure doesn't move its point.

Rules for new queries:
- Run `EXPLAIN` on every new hot query, and look for `IndexScan`, `KnnScan` or a key range, not `TableScan` or `SortByKey`.
- Put the equality fields first in a composite index and the range or sort field last.
- Page with a cursor on the ULID (`WHERE id > $cursor ORDER BY id LIMIT 50`) or on the indexed sort field, never `START`, which walks every skipped row.
- Use `ONLY` when the API expects exactly one record.

**Regional alerts without a spatial index.** SurrealDB has no spatial index ([#5567](https://github.com/surrealdb/surrealdb/issues/5567)). `garden.geohash` (5 characters, cells of about 5 km) is indexed:

```sql
SELECT * FROM garden WHERE geohash IN $cell_and_neighbours AND geo::distance(centroid, $point) < 5000;
```

**Vector search: one HNSW index.**

| Data | How it's searched | Why |
|---|---|---|
| `knowledge_chunk.embedding` | HNSW: `WHERE topic = $t AND embedding <\|5, 40\|> $q` | The whole knowledge base, thousands of chunks. ✓ Filters on indexed fields are applied **before** the graph search (`KnnScan` with the predicate), so a filter never leaves fewer than k results |
| `memory.embedding` | ✓ Exact, within one garden: `SELECT text, vector::similarity::cosine(embedding, $q) AS s FROM memory WHERE garden = $g AND (shared = true OR author = $u) ORDER BY s DESC LIMIT 5`. ✓ It never returns another member's private memories | A garden has at most a few thousand memories, so exact search is cheap and always correct. It also avoids the open filtered-KNN bug after deletes ([#7423](https://github.com/surrealdb/surrealdb/issues/7423)), and memories are deleted often (Undo, account deletion) |
| `scan_image.embedding` | Exact, within one garden, the same way | `find_similar_cases` only searches the garden's own scans |
| `farm_record.embedding` | ✓ (embedded 3.3.4) Exact, within one garden: `memory`'s query without its sharing filter, `WHERE garden = $garden AND ($kind = NONE OR kind = $kind) AND ($since = NONE OR at >= $since) AND connector.status != 'deleting' ORDER BY similarity DESC LIMIT $k`. ✓ Another garden's record with the exact query vector never comes back, and hits rank by cosine | A garden holds at most 10,000 chunks ([connectors 4](../connectors/README.md#4-data)); the same filtered-KNN bug avoided, and records are deleted often (a dropped source, the 180-day cleanup) |

- `<|k|>` and `<|k, ef|>` use the HNSW index. `<|k, COSINE|>` is a brute-force scan even when an HNSW index exists.
- ✓ A vector of the wrong length is rejected ("Expected a vector of 384 dimension").
- `DIMENSION 384` in `schema.surql` is a placeholder until the text-embedding model is picked on day 1. Changing it means `REMOVE INDEX chunk_vec ON knowledge_chunk`, then re-defining it, then re-embedding every chunk (architecture 10).
- The HNSW graph is kept on disk and loaded into memory on first use. `SURREAL_HNSW_CACHE_SIZE` (default 256 MiB) bounds the cached vectors, not the graph. At under 100k chunks the default is enough.

---

## 6. Rust SDK

- **One client for the process.** Connect once at start-up, sign in as `api`, select the namespace and database, then store the client in `Arc<AppState>` and call `state.db.query(…)`. Don't clone `Surreal<Any>` per request: in 3.x a clone opens a new server session and replays the sign-in.
- **Types.** Records derive `surrealdb::types::SurrealValue`, not serde, and `#[serde(...)]` attributes are ignored on them. Use `#[surreal(rename = "…")]`.
  - Every `id` is a `RecordId`, never a `String`.
  - `record<…>` fields are `RecordId`.
  - Optional fields are `Option<T>`.
  - Datetimes are `chrono::DateTime<Utc>`.
  - API DTOs derive serde separately and turn `RecordId` into the bare ULID string.
- **Always `.bind()`.** Never format values into SurrealQL. A misspelled bind name silently becomes `NONE`, so test each query.
- **Check every statement.** `.query()` returns `Ok` even when a statement inside failed. Call `response.take_errors()` before `take(n)`. ✓ Inside `BEGIN … COMMIT`, `.check()` returns the lowest-index error, which is not the cause: the statements before the cause report `NotExecuted`, those after it `Cancelled`, and the `COMMIT` `NotExecuted`. So take the lowest-index error that is neither. `.check()` is fine for a single statement.
- **Retry conflicts** around whole transactions:

```rust
use surrealdb::types::QueryError;

fn is_conflict(e: &surrealdb::Error) -> bool {
    matches!(e.query_details(), Some(QueryError::TransactionConflict))
}
// for attempt in 0..5 { run the BEGIN…COMMIT query; if take_errors() has a conflict,
//   sleep 10–50 ms × 2^attempt with jitter and retry; otherwise return }
```

- Building a `RecordId` with `RecordId::parse_simple("t:1")` gives a **string** key. Use `RecordId::new` with typed keys, including array keys such as `[scan, seq]`.
- Over WebSocket, live queries end when the connection drops. We don't use them: the API makes every write and broadcasts SSE itself (contracts section 4).

---

## 7. Operations

**Backups.**
- `surreal export --endpoint http://127.0.0.1:8000 -u root -p … --ns pdm --db main backup.surql`, nightly and before every upgrade.
- Export goes over HTTP and runs in one read-only transaction (from the source code; *unverified* in the docs).
- It covers one database, and root users aren't included. The setup script re-creates the `api` user after a restore if it's missing.
- Restore with `surreal import`.
- Copying the RocksDB directory is safe only while the server is stopped.

**Upgrades.**
1. Export a backup.
2. Change the pinned tag.
3. Restart. Since 3.3.0, data migrations run automatically on start.
4. Run `REBUILD INDEX` on the vector and composite indexes, which picks up index-format fixes.

Rolling back means importing the export into a fresh datastore, because an older binary can't read a datastore a newer one has opened. Don't upgrade during build week unless a fix requires it.

**Migrations** ([section 1](#1-version-and-setup), applying the migrations; proposed):
- **A file failed.** `api` exits, naming the file, the statement and the error, and Compose restarts it into the same failure until a fixed build ships. The failed file committed nothing, so it may be edited. Files before it stay applied.
- **A recorded file changed.** `api` exits with both hashes. Revert the edit and put the change in a new numbered file. For an edit to comments alone, an operator may set the new hash by hand as root (`UPDATE migration:\`0003_x.surql\` SET hash = '…'`) after reading the diff.
- **Rolling back `api`.** An older build refuses to start on a database that recorded a file it doesn't have. Roll back by restoring the backup taken before the deploy, as for an upgrade.
- **Re-applying the knowledge set** by hand: `DELETE migration:\`knowledge.surql\`` as root, then restart `api`.
- **A development database** that disagrees with the files is cheaper to drop than to repair: `docker compose -f infra/dev.compose.yml down -v`.

**Monitoring.** The status page (contracts section 9) and `/surreal is-ready`. Log at `info`. Watch the transaction-conflict rate: a steady stream points to a hot record that needs its writes serialised.

---

## 8. Caveats

Verified on 3.3.4 unless marked otherwise.

1. ✓ The namespace and database must exist before anything else runs. `schema.surql` defines them first.
2. ✓ SCHEMAFULL rejects unknown fields, both top-level and inside non-FLEXIBLE objects. A Rust struct with an extra field fails to write.
3. ✓ `option<T>` is NONE, not NULL. Binding a JSON `null` into an `option<T>` field fails.
4. ✓ `<string>` of a float adds an `f` suffix (`"43.65f"`). Cast through `<decimal>` first, as `garden.weather_cell` does.
5. ✓ Indexes take fields, not expressions. `FIELDS name.lowercase()` fails, so store a key field (`product.name_key`) and index that.
6. ✓ `RELATE` with a fixed id overwrites an existing edge. Use `INSERT RELATION` to fail on duplicates.
7. ✓ Write conflicts are errors to retry, not waits.
8. `DEFINE SEQUENCE` leaves gaps; use the counter-record pattern (section 4).
9. Composite UNIQUE indexes have open bugs:
   - a range on the leading field returns wrong rows ([#7532](https://github.com/surrealdb/surrealdb/issues/7532));
   - `UPDATE`/`DELETE … WHERE leading IN [...]` matches nothing ([#7593](https://github.com/surrealdb/surrealdb/issues/7593)).

   Our one composite UNIQUE index (`action_plan (scan, version)`) is only written and read by equality.
10. Define indexes before traffic. One defined during writes can miss them ([#7533](https://github.com/surrealdb/surrealdb/issues/7533)).
11. Use `IF NOT EXISTS` for indexes in the schema file. `OVERWRITE` rebuilds them on every start.
12. Changing a field's type doesn't convert existing rows; they fail on their next write until converted.
13. Imports don't fire `DEFINE EVENT`s. We define no events: the API does the work they'd do, which keeps writes predictable.
14. There is no record expiry; cleanup is scheduled (section 4).
15. There is no spatial index; regional alerts use the geohash field (section 5).
16. ✓ Some parameter names are protected. Binding `auth`, `token`, `access` or `session` fails the whole request with "'auth' is a protected variable and cannot be set", and a `LET` of one fails the same way. The `store` crate binds `auth_id`, `lease_token` and `push_token` instead, and binds no protected name. ✓ Names the engine gives a meaning inside a statement can be bound, but the bind doesn't always win:
   - A bound `$before` or `$after` is used in a `SELECT` and in an `UPDATE`'s `SET` and `WHERE`. In a `RETURN` clause the record's own `$before` or `$after` wins.
   - A bound `$this` is ignored inside a record statement (projection, `WHERE`, `SET` and `RETURN`), and a bound `$parent` is ignored inside a subquery.
   - A bound `$value`, `$input` or `$event` was used as bound in the same places. They have meanings in field and event definitions, and we bind none of them there.

   The `store` binds `$after` as a page cursor, read only in `SELECT` statements (in a `WHERE` or a record range), which is safe. It binds no `before`, `this` or `parent` (cleanup binds its cutoff as `$cutoff`).
17. ✓ A point literal doesn't take parameters: `centroid = ($lon, $lat)` is a parse error. Use `type::point([$lon, $lat])` (longitude first); `type::point($lon, $lat)` fails with "Expected 1 argument", and a GeoJSON-shaped object isn't coerced to `geometry`.

---

## 9. Open questions

1. **Text-embedding dimension:** decided. `snowflake-arctic-embed-xs` gives 384 dimensions, which `chunk_vec` already uses (architecture open decision 13, [backend vision](../backend/vision.md)).
2. **Image embeddings:** the model is decided: BioCLIP 2's whole-image vector, 768 dimensions. Still open: whether `find_similar_cases` needs more than an exact search per garden. Measure it once real gardens hold a few hundred photos.
3. **RocksDB block cache** on the control-plane host: 512 MiB is proposed. Measure memory once the knowledge base is loaded.

---

## 10. Sources

- Release and version information: https://surrealdb.com/releases, https://crates.io/crates/surrealdb, Docker Hub `surrealdb/surrealdb`
- 2.x → 3.x changes: https://surrealdb.com/docs/build/migrating/from-old-surrealdb-versions/2x-to-3x
- `DEFINE TABLE`, `FIELD`, `INDEX`, `EVENT`, `USER`, `SEQUENCE`: https://surrealdb.com/docs/reference/query-language/statements/define/
- Record IDs and ranges: https://surrealdb.com/docs/reference/query-language/language-primitives/data-types/record-ids
- Record references: https://surrealdb.com/docs/reference/query-language/language-primitives/record-references
- NONE and NULL: https://surrealdb.com/docs/reference/query-language/language-primitives/data-types/none-and-null
- Transactions and isolation: https://surrealdb.com/docs/learn/querying/concepts-and-guides/transactions
- Vector indexes and filtered search: https://surrealdb.com/docs/learn/data-models/vector-search/vector-indexes, https://surrealdb.com/docs/learn/data-models/vector-search/similarity-search
- Performance best practices: https://surrealdb.com/docs/surrealdb/reference-guide/performance-best-practices
- Rust SDK types: https://surrealdb.com/docs/reference/rust/concepts/working-with-types
- Docker, CLI, capabilities, environment variables: https://surrealdb.com/docs/surrealdb/installation/running/docker, https://surrealdb.com/docs/surrealdb/security/capabilities, https://surrealdb.com/docs/reference/cli/surrealdb-cli/environment-variables
- Export, import and upgrades: https://surrealdb.com/docs/surrealdb/cli/export, https://surrealdb.com/docs/manage/self-hosted/upgrades-and-patching
- SurrealDB Agent Memory (the hosted memory service the `memory` table is mirrored into): [agent-memory.md](agent-memory.md)
- Issues: [#1317](https://github.com/surrealdb/surrealdb/issues/1317) (TTL), [#5567](https://github.com/surrealdb/surrealdb/issues/5567) (spatial index), [#7423](https://github.com/surrealdb/surrealdb/issues/7423) (filtered KNN after deletes), [#7532](https://github.com/surrealdb/surrealdb/issues/7532), [#7533](https://github.com/surrealdb/surrealdb/issues/7533), [#7593](https://github.com/surrealdb/surrealdb/issues/7593) (index bugs)
