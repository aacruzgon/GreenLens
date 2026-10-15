# PDM Database (SurrealDB)

> **Status:** v1, 2026-10-15. The schema is [`schema.surql`](schema.surql). Companion to the [architecture document](../architecture/README.md) (section 5) and [contracts.md](../architecture/contracts.md), which define the shapes these records store.
> **Verified:** every pattern marked ✓ below was run against `surrealdb/surrealdb:v3.3.4` on 2026-10-15. The schema applies cleanly twice in a row, and a scenario script exercised the rest. Points marked *unverified* come from the docs only.

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
  surrealdb:
    image: surrealdb/surrealdb:v3.3.4@sha256:649a60a3b9c0177d9923c938e1c2563e48cde2a9d06ac2c0519714be719b02af
    restart: unless-stopped
    command: start --log info rocksdb:/data/db
    environment:
      SURREAL_USER: ${SURREAL_ROOT_USER:?set in .env}   # creates the root user on first start only
      SURREAL_PASS: ${SURREAL_ROOT_PASS:?set in .env}
      SURREAL_ROCKSDB_BLOCK_CACHE_SIZE: "536870912"     # 512 MiB; the default is half of memory minus 1 GiB
    ports:
      - "127.0.0.1:8000:8000"                           # host-only; the API reaches it as ws://surrealdb:8000
    volumes:
      - surrealdb-data:/data                            # the image runs as UID 65532: a bind mount needs chown 65532
    healthcheck:
      test: ["CMD", "/surreal", "is-ready", "--endpoint", "http://127.0.0.1:8000"]   # ✓ distroless image, no curl
      interval: 10s
      timeout: 5s
      retries: 6
volumes:
  surrealdb-data:
```

**Capabilities.** The 3.3 defaults deny scripting, guest access and outbound network calls, which is what we want. Never start the server with `--unauthenticated`.

**Users.**

| User | Level and role | Used by |
|---|---|---|
| root | Root, from `SURREAL_USER`/`SURREAL_PASS` | Applying `schema.surql`, backups, emergencies |
| `api` | `DEFINE USER api ON DATABASE PASSWORD $secret ROLES EDITOR` | The API at runtime. ✓ It can read, write and change the schema, but `DEFINE USER` fails with "Not enough permissions" |

The API authenticates app users with the auth provider's JWTs itself, and talks to SurrealDB as `api`. SurrealDB could verify the provider's JWTs directly (`DEFINE ACCESS … TYPE JWT URL`), but that needs custom claims, outbound network access and a session per request. It would also duplicate the membership checks the API makes anyway, so we don't use it.

**Applying the schema.** The file moves to `backend/migrations/schema.surql` when the backend is scaffolded; the `store` crate embeds it and the API applies it on start, before it serves traffic:
- ✓ The file is idempotent: `OVERWRITE` for tables, fields and functions, and `IF NOT EXISTS` for indexes. `OVERWRITE` on an index would rebuild it.
- ✓ It first defines the `pdm` namespace and the `main` database. In 3.x, a query against a namespace or database that doesn't exist fails.
- The database is `STRICT`: ✓ a query on an undefined table fails instead of creating it.
- Define indexes before writes start. An index defined while writes are happening can miss them ([#7533](https://github.com/surrealdb/surrealdb/issues/7533)).
- Changing a field's type doesn't convert existing rows; it's checked on each row's next write. Convert explicitly with a batched `UPDATE t SET f = <int> f WHERE f != NONE`.

The official migration tool, SurrealKit, is still in beta (1.0.0-beta.6), and the community `surrealdb-migrations` crate is archived and 2.x-only. An idempotent schema file is enough for the hackathon.

---

## 2. Conventions

| Topic | Rule |
|---|---|
| Record IDs | ULID string keys generated in Rust: `RecordId::new("scan", Ulid::new().to_string())`. Knowledge records use their slugs. A few tables use a natural **array key** (below). The API strips the table prefix in responses (contracts section 1) |
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
| Identity | `user` (unique `auth_id`), `push_token` |
| Gardens | `garden` (stored `weather_cell` and `geohash`), `member_of` (relation user → garden), `invite` (unique `code`), `plant` |
| Scans | `scan`, `scan_image`, `mask`, `scan_event`, `observation`, `question`, `guided_check`, `diagnosis`, `scan_feedback` |
| Plans | `action_plan` (unique `scan, version`), `plan_trigger`, `task` (unique `alert_key`) |
| Chat | `conversation`, `chat_image`, `message`, `pending_action`, `message_feedback` |
| Memory | `memory` |
| Knowledge | `source`, `species`, `disease`, `organism`, `treatment`, `product`, `classifier_label`, `soil_condition`, `knowledge_chunk`; edges `susceptible_to`, `manages`, `caused_by`, `preys_on` (fieldless, `LIGHTWEIGHT`) and `looks_like` (with `how_to_tell`) |
| Operations | `job`, `worker`, `usage_daily`, `idempotency_key`, `weather_cache` |

Changes from the architecture sketch (section 5 there):
- **New tables:** `guided_check`, `chat_image`, `idempotency_key`, `classifier_label`, `soil_condition`.
- **Renamed:** `feedback` → `scan_feedback`, to sit beside `message_feedback`.
- **New fields:**
  - `scan.plan`, which the compare-and-set updates, and `scan.last_event_seq`, the event counter;
  - `task.state`, so the task list filters on one index;
  - `garden.deleting_at` and `plant.deleting_at`, for the background deletion cascade;
  - `action_plan.reason`.
- **Vector indexes:** `memory` and `scan_image` embeddings get no HNSW index (section 5).

---

## 4. Write patterns

Each statement is its own transaction. Statements that must succeed or fail together go in one `BEGIN … COMMIT` sent as **one** query. Isolation is snapshot isolation: there are no locks, and a write-write conflict fails at `COMMIT` with a retryable "Transaction conflict". **The API retries the whole transaction**, up to 5 times with jittered back-off (section 6).

### Scan events: a gap-free `seq` ✓

The scan record is the counter. The per-scan writer (contracts section 4) appends each event in one transaction:

```sql
BEGIN;
LET $seq = (UPDATE ONLY $scan SET last_event_seq += 1 RETURN VALUE last_event_seq);
CREATE type::record('scan_event', [$scan, $seq]) SET run = $run, type = $type, payload = $payload;
COMMIT;
```

✓ With 40 concurrent appends to one scan, 35 committed and 5 failed with a retryable conflict. The stored sequence was exactly 1…38, with no gaps or duplicates (3 events existed before the burst). Because the API's per-scan writer serialises appends, conflicts only happen across API processes or on races with other updates to the scan record, and the retry handles them. Don't use `DEFINE SEQUENCE` here: sequence values are never rolled back, so a failed transaction leaves a gap.

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

- ✓ The inner `SELECT` uses the `job_queue` index in `queued_at` order, with no in-memory sort.
- ✓ The outer `WHERE status = 'queued'` re-checks the row. Two workers that pick the same job conflict at commit, and the loser retries and gets the next one.
- `$eligible_before` is `time::now()` for the GPU workers and `time::now() - 15s` for the fallback worker (contracts section 5).
- The claim is a long poll. The API doesn't re-query in a tight loop: it waits on an in-process `tokio::sync::Notify` that every job insert signals, with a 1-second fallback poll. The API makes every write, so it doesn't need a live query.
- Expired leases and stale workers are requeued by the scheduler with `UPDATE job SET status = 'queued', … WHERE status = 'leased' AND lease_until < time::now()`. ✓ That uses the `job_lease` index.

### Idempotency keys ✓

```sql
CREATE type::record('idempotency_key', [$user, $route, $key])
  SET fingerprint = $fp, expires_at = time::now() + 24h;
```

✓ A second `CREATE` with the same key fails with "already exists". The API then reads the record and applies contracts section 1: replay the stored response, return `409 request_in_progress`, or return `422 idempotency_conflict`. When the request finishes, the API sets `state = 'done'`, `response_status` and `response_body`. A response that must not be stored deletes the key instead.

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

### Deletion ✓

Parent links use `REFERENCE ON DELETE REJECT`. ✓ `DELETE garden:g1` while a plant still points at it fails with "Cannot delete `garden:g1` as it is referenced by `plant:p1`". A missed step in the cascade therefore fails loudly rather than orphaning records.

The API deletes **bottom-up in batches**, never as one large transaction:
1. Set `deleting_at` on the garden or plant. Every read filters it out, which is what makes `202 Accepted` safe.
2. Cancel the jobs.
3. Delete the objects in RustFS ([storage](../storage/README.md)).
4. Delete the records, children first: scan events (one range delete per scan), then observations, questions, diagnoses and plans (after unsetting `scan.plan` and `scan.diagnosis`), then scans, conversations, memories, tasks and plants, then the garden.

✓ This order deletes a whole garden cleanly.

Each batch is `DELETE (SELECT VALUE id FROM t WHERE … LIMIT 500) RETURN NONE`, repeated until empty. Small transactions don't conflict with live writers.

A few links behave differently:
- **CASCADE** (✓ deleting a user deletes their push tokens): `push_token.user`, `invite.garden`, `scan_feedback.user`. These records have nothing outside the database.
- **UNSET** (✓ the field disappears): `scan.created_by`, `task.created_by`, `memory.author`, `invite.created_by`/`used_by`. This keeps garden history when a member deletes their account (architecture 5, deletion).
- **Plain links with no reference:** `memory.source_msg` and `action_plan.constraints[*].from_pending_action`, whose targets can be deleted first.

### Cleanup (no TTL)

SurrealDB has no record expiry ([#1317](https://github.com/surrealdb/surrealdb/issues/1317)). The scheduler runs these every 10 minutes, in batches of 500:

| What | Statement |
|---|---|
| Idempotency keys | `DELETE (SELECT VALUE id FROM idempotency_key WHERE expires_at < time::now() LIMIT 500) RETURN NONE` |
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
- **Check every statement.** `.query()` returns `Ok` even when a statement inside failed. Call `response.take_errors()`, or `.check()`, before `take(n)`.
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

---

## 9. Open questions

1. **Text-embedding dimension** for `knowledge_chunk.chunk_vec`, set with the model on day 1 (architecture open decision 13).
2. **Image-embedding model** for `scan_image.embedding`, and whether `find_similar_cases` needs more than an exact search per garden.
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
- Issues: [#1317](https://github.com/surrealdb/surrealdb/issues/1317) (TTL), [#5567](https://github.com/surrealdb/surrealdb/issues/5567) (spatial index), [#7423](https://github.com/surrealdb/surrealdb/issues/7423) (filtered KNN after deletes), [#7532](https://github.com/surrealdb/surrealdb/issues/7532), [#7533](https://github.com/surrealdb/surrealdb/issues/7533), [#7593](https://github.com/surrealdb/surrealdb/issues/7593) (index bugs)
