---
name: rust-surrealdb
description: "The store crate against SurrealDB 3.3.4 with the surrealdb 3.3.4 SDK: connect and sign in, SurrealValue types and RecordId, binds, transactions, per-statement errors, THROW and conflict retries, range reads, batched deletes, store tests. Use when writing or reviewing any SurrealDB query in Rust."
---

# SurrealDB from Rust (`store` crate)

This skill covers the `store` crate ([architecture 3](../../../docs/architecture/README.md#rust-crates)): one SurrealDB client per process, and one Rust function per query. It implements the database document in Rust; it does not redefine it.

**The project docs win.** [docs/database/README.md](../../../docs/database/README.md) was verified against the pinned server (its ✓ marks), and [schema.surql](../../../docs/database/schema.surql) is the stored shape. Take every SurrealQL statement verbatim from [database 4](../../../docs/database/README.md#4-write-patterns) and [5](../../../docs/database/README.md#5-reads-indexes-and-vector-search), and follow [database 6](../../../docs/database/README.md#6-rust-sdk) for the SDK. Where this skill and those sections disagree, the sections win and the disagreement goes to the docs' owner.

Versions: crate `surrealdb` 3.3.4 against server `surrealdb/surrealdb:v3.3.4` ([database 1](../../../docs/database/README.md#1-version-and-setup), [versions.md](../rust/versions.md)). The 3.x SDK differs a lot from 2.x: records derive `SurrealValue`, not serde, and errors are one `surrealdb::Error` with a kind and typed details. Don't write it from memory; read `~/.cargo/registry/src/*/surrealdb-3.3.4/` and `surrealdb-types-3.3.4/`. docs.rs failed to build `surrealdb` 3.3.4 (and 3.3.3), so this skill cites its source files (paths relative to `surrealdb-3.3.4/`) and the [Rust SDK reference](https://surrealdb.com/docs/reference/rust/methods); `surrealdb-types` 3.3.4 did build and is linked on docs.rs.

| File | What's in it |
|---|---|
| This file | Connecting, types, queries, errors, the conflict retry, reads, deletes, tests, review checklist |
| [errors-and-retries.md](errors-and-retries.md) | `StoreError`, `first_failure`, `checked` and `retry_conflicts`, compiled and tested |
| [write-patterns.md](write-patterns.md) | One function per database-4 pattern, compiled, with ignored tests that ran against the pinned image |

Behaviour marked ✓ below was run on 2026-10-15 and 2026-10-16 against `surrealdb/surrealdb:v3.3.4@sha256:649a60a3…` (`start --user root --pass root memory`) by the ignored tests in [write-patterns.md](write-patterns.md#live-tests) (through `check_examples.py --live`, cargo-nextest 0.9.148) or the offline tests below; the few marked *probe* were run the same days from a scratch crate against the same image.

## Connecting

1. **Connect once at start-up** with `surrealdb::engine::any::connect("ws://surrealdb:8000")` ([database 1](../../../docs/database/README.md#1-version-and-setup); [connect](https://surrealdb.com/docs/reference/rust/methods/connect); `src/engine/any/mod.rs`). The default features (`protocol-ws`, `rustls`, `parse`) cover it.
2. **The SDK checks the server version on connect.** A remote connect calls `version()` and refuses a server outside `>=3.0.0-alpha.1, <4.0.0` (`SUPPORTED_VERSIONS` and `verify_server_version` in `surrealdb-3.3.4/src/lib.rs`). It skips the check when the server's capabilities deny the `version` method. Pin the server and crate together anyway ([database 1](../../../docs/database/README.md#1-version-and-setup)).
3. **Sign in as `api` with `opt::auth::Database`**, not `Root`: `api` is a database-level user ([database 1, users](../../../docs/database/README.md#1-version-and-setup)). ✓ `Root { username: "api", .. }` fails with `NotAllowed(Auth(InvalidAuth))`. Then call `use_ns("pdm").use_db("main")` ([signin](https://surrealdb.com/docs/reference/rust/methods/signin), [use](https://surrealdb.com/docs/reference/rust/methods/use); `Database` in `src/opt/auth.rs`). `connect_api` takes the namespace as a parameter, so the API passes `pdm` and a test passes its own.
4. **Apply the embedded schema as root, before serving.** The users table gives schema application to root ([database 1](../../../docs/database/README.md#1-version-and-setup)). ✓ Run as `api`, the file's statements 0 and 2 (`DEFINE NAMESPACE`, `DEFINE DATABASE`) fail with "IAM error: Not enough permissions to perform this action", and every other statement applies. ✓ A namespace-level `OWNER` may run `DEFINE DATABASE` but not `DEFINE NAMESPACE`, which needs root ([`DEFINE USER`](https://surrealdb.com/docs/reference/query-language/statements/define/user)). So start-up opens a short-lived root connection, runs the file in one `.query()`, checks every statement, and drops it; then it opens the long-lived `api` client. ✓ As root the file applies twice in a row with no errors. `connect_api` and `apply_schema` are in [write-patterns.md](write-patterns.md#the-module).
5. **`DEFINE USER` takes a literal password.** ✓ `PASSWORD $p` is a parse error ("Unexpected token \`a parameter\`, expected a strand") whether `$p` is bound or set by `LET`, and so is `PASSHASH $p`. So the password is written into the statement as a single-quoted string with `\` and `'` escaped ([strings](https://surrealdb.com/docs/reference/query-language/language-primitives/data-types/strings)); ✓ the test harness's password contains both and signs in (`surql_string` in [rust-testing, database.md](../rust-testing/database.md#one-namespace-per-test)). The `api` user is created by the setup script ([database 7](../../../docs/database/README.md#7-operations)), never by the API.
6. **Share the client, don't clone it.** Store one `Surreal<Any>` in `Arc<AppState>` and pass `&Surreal<Any>` to store functions ([database 6](../../../docs/database/README.md#6-rust-sdk)). In 3.3.4, `Surreal::clone` gives the clone a new session id on the same WebSocket and replays the session's log (`signin`, `use`, `set`) one command at a time before the clone's first request runs (`impl Clone for Surreal` in `src/lib.rs`; `handle_session_clone` and `replay_session` in `src/engine/remote/ws/mod.rs`), and dropping a clone detaches its session. ✓ A clone reports a different `$session.id`, and a variable `set` on the clone is NONE on the original. This agrees with database 6.

## Types

1. **Records derive `surrealdb::types::SurrealValue`**, not serde; `#[serde]` attributes do nothing on them. Rename a field with `#[surreal(rename = "type")]` ([database 6](../../../docs/database/README.md#6-rust-sdk); [SurrealValue attributes](https://surrealdb.com/docs/reference/rust/concepts/surrealvalue-attributes)). ✓ A struct may name fewer fields than the record has; the rest are ignored on read.
2. **String enums need `#[surreal(untagged, rename_all = "snake_case")]`.** Without `untagged`, a unit variant is written as `{ Leased: {} }`, and ✓ reading `status: 'leased'` into it fails with "no variants matched". With `untagged` it is the string `'leased'`, which the schema's literal unions expect ([SurrealValue attributes](https://surrealdb.com/docs/reference/rust/concepts/surrealvalue-attributes); `Strategy::for_enum` in `surrealdb-types-derive-3.3.4/src/structs/strategy.rs`). This is the opposite of serde, where a unit variant is a string by default.
3. **Every `id` and every `record<…>` field is a `RecordId`** ([database 6](../../../docs/database/README.md#6-rust-sdk)). Build it with `RecordId::new(table, key)`: a `String` key for ULIDs, an `Array` for array keys such as `scan_event:[scan, seq]`. `RecordId::parse_simple("scan:1")` always gives a string key ([`RecordId`](https://docs.rs/surrealdb-types/3.3.4/surrealdb_types/struct.RecordId.html)). In SurrealQL, prefer `type::record('scan_event', [$scan, $seq])` with binds, as database 4 does.
4. **ULIDs: `Ulid::generate().to_string()`.** `ulid` 3.0.0 has no `Ulid::new()` ([`Ulid`](https://docs.rs/ulid/3.0.0/ulid/struct.Ulid.html)), and `RecordIdKey` has no `From<Ulid>` (`surrealdb-types` uses `ulid` 1.2.1 internally), so pass the string. The two blocks below prove both.
5. **`Option<T>` is NONE.** `None` binds as NONE, and ✓ an `option<string>` field accepts it; ✓ binding `Value::Null` there fails with "Expected `none | string` but found `NULL`" ([database 8, caveat 3](../../../docs/database/README.md#8-caveats)). A `serde_json::Value` binds as SurrealDB values and a JSON `null` becomes NULL (`impl SurrealValue for serde_json::Value` in `surrealdb-types-3.3.4/src/traits/surreal_value.rs`): keep nulls out of typed fields.
6. **Instants are `chrono::DateTime<Utc>`**, bound and read directly; never a formatted string ([database 2](../../../docs/database/README.md#2-conventions)). `surrealdb-types` implements `SurrealValue` for it (`traits/surreal_value.rs`).
7. **Records never reach the wire.** API DTOs derive serde separately and carry the bare ULID string without the table prefix ([contracts 1](../../../docs/architecture/contracts.md#wire-format)); see [`rust-contract-types`](../rust-contract-types/SKILL.md) for the DTO side.

The test below runs offline and proves rules 2, 3 and 5; `wire_id` is the record-to-DTO step.

```rust
use surrealdb::types::{Array, RecordId, RecordIdKey, SurrealValue, Value};

#[derive(Debug, PartialEq, SurrealValue)]
#[surreal(untagged, rename_all = "snake_case")]
enum JobStatus {
    Queued,
    Leased,
}

#[derive(Debug, PartialEq, SurrealValue)]
enum Tagged {
    Queued,
}

#[derive(Debug, SurrealValue)]
struct EventRow {
    id: RecordId,
    #[surreal(rename = "type")]
    kind: String,
    detail: Option<String>,
}

// The bare ULID for a DTO; None for a key that isn't a string.
fn wire_id(id: &RecordId) -> Option<&str> {
    match &id.key {
        RecordIdKey::String(key) => Some(key),
        _ => None,
    }
}

#[test]
fn surreal_value_shapes() {
    assert_eq!(
        JobStatus::Leased.into_value(),
        Value::String("leased".into())
    );
    assert!(matches!(Tagged::Queued.into_value(), Value::Object(_)));
    assert!(Tagged::from_value(Value::String("queued".into())).is_err());
    assert_eq!(
        JobStatus::from_value(Value::String("queued".into())).unwrap(),
        JobStatus::Queued
    );

    let key = ulid::Ulid::generate().to_string();
    let scan = RecordId::new("scan", key.clone());
    assert_eq!(wire_id(&scan), Some(key.as_str()));
    assert_eq!(RecordId::new("scan", 1).key, RecordIdKey::Number(1));
    assert_eq!(
        RecordId::parse_simple("scan:1").unwrap().key,
        RecordIdKey::String("1".into())
    );

    let event = RecordId::new(
        "scan_event",
        Array::from(vec![scan.into_value(), 7_i64.into_value()]),
    );
    let row = EventRow {
        id: event,
        kind: "finding".into(),
        detail: None,
    };
    let Value::Object(object) = row.into_value() else {
        panic!("a struct is an object");
    };
    assert_eq!(object.get("type"), Some(&Value::String("finding".into())));
    assert_eq!(object.get("detail"), Some(&Value::None));
}
```

Rule 4, as the compiler sees it with the pins: `Ulid::new` doesn't exist (E0599), and a `Ulid` doesn't convert into a `RecordIdKey` (E0277).

```rust,compile_fail,E0599
fn scan_key() -> ulid::Ulid {
    ulid::Ulid::new()
}
```

```rust,compile_fail,E0277
fn scan_id() -> surrealdb::types::RecordId {
    surrealdb::types::RecordId::new("scan", ulid::Ulid::generate())
}
```

## Queries

1. **Only `.bind()`.** Never format a value into SurrealQL ([database 6](../../../docs/database/README.md#6-rust-sdk)). Bind one pair at a time, `.bind(("scan", scan))`, or a `SurrealValue` struct whose fields become the parameters ([query, binding parameters](https://surrealdb.com/docs/reference/rust/methods/query); `Query::bind` in `src/method/query.rs`). ✓ A misspelled name is not an error: `RETURN $nmae` with `name` bound returns NONE. Each query needs a test that would see a NONE.
2. **One transaction is one `.query()` call** holding `BEGIN; … COMMIT;` ([database 4](../../../docs/database/README.md#4-write-patterns)). Keep the statement in a `const &str` copied from the database document.
3. **Statement indexes count every statement, `BEGIN` included.** ✓ `BEGIN; LET …; CREATE …; COMMIT;` has 4 results: 0 is `BEGIN`, 1 the `LET` (NONE), 2 the `CREATE`'s rows, 3 the `COMMIT`. Name the index next to the constant.
4. **`take(n)` with a type.** `Option<T>` takes a statement that returns one row (or none), `Vec<T>` one that returns many, `(n, "field")` one field (`IndexedResults::take` in `src/method/query.rs`; [query](https://surrealdb.com/docs/reference/rust/methods/query)). ✓ `RETURN VALUE scans` gives `[1]`, which takes as `Option<i64>` = `Some(1)`. ✓ An index past the last statement is not an error: it takes as `Ok(None)` or `Ok([])`, so a wrong index reads as "no rows". Name the index next to the constant and test the value.
5. **`.query()` returns `Ok` when statements failed** ([database 6](../../../docs/database/README.md#6-rust-sdk); [error handling](https://surrealdb.com/docs/reference/rust/concepts/error-handling)). ✓ `CREATE nosuch:1; RETURN 1` is `Ok`: statement 0 holds `NotFound(Table)`, and statement 1 still gives `1`. Read the errors before any `take`.
6. **In a transaction, `.check()` hides the cause.** It returns the first failed statement by index (`IndexedResults::check` in `src/method/query.rs`), and in a failed transaction the other statements report errors too ([errors inside a transaction](https://surrealdb.com/docs/reference/rest-api/errors)). ✓ In `BEGIN; CREATE …; IF true { THROW 'stale_plan' }; RETURN 5; COMMIT;`, statement 1 (before the cause) is `Query(NotExecuted)` "The query was not executed due to a failed transaction", 2 is the `THROW`, 3 (after the cause) is `Query(Cancelled)` "The query was not executed due to a cancelled transaction", and 4, the `COMMIT`, is `Query(NotExecuted)` "Cannot COMMIT: the transaction was aborted due to a prior error"; `check()` returns statement 1. ✓ (*probe*) For a conflict, the cause is on the `COMMIT` and the statements before it are `NotExecuted`. Use `take_errors()` and `first_failure` ([errors-and-retries.md](errors-and-retries.md)), which skips both `NotExecuted` and `Cancelled`. Outside a transaction, `.check()` is fine.
7. **Not `Surreal::begin`.** The SDK's client-side transaction (`Surreal::begin` in `src/method/mod.rs`, the `Begin` future in `src/method/begin.rs`) takes the client by value, so from the shared `&Surreal<Any>` it needs a clone, which is a new session that replays the sign-in (connecting, rule 6), and it sends each statement as its own request. Keep `BEGIN … COMMIT` in one `.query()` ([database 4](../../../docs/database/README.md#4-write-patterns)).

## How failures arrive

Every server failure is a `surrealdb::Error` with a kind and, for some kinds, typed details ([`Error`](https://docs.rs/surrealdb-types/3.3.4/surrealdb_types/struct.Error.html)). The SurrealDB docs say to match on the kind, not the message, but two of the project's patterns only show up in the message. Each row is ✓ on 3.3.4.

| Failure | Kind and details | Message |
|---|---|---|
| Write conflict at `COMMIT` | `Query(Some(TransactionConflict))`, on the `COMMIT` statement | "Cannot COMMIT: Transaction conflict: Write conflict, retry the transaction. This transaction can be retried" |
| Conflict on a single statement (its own transaction), including a duplicate `CREATE` that races | `Query(Some(TransactionConflict))` | "There was a problem with the key-value store: Transaction conflict: Write conflict, retry the transaction. This transaction can be retried" |
| `THROW 'stale_plan'` | `Thrown` (`is_thrown()`), no details | "An error occurred: stale_plan" |
| `CREATE` of an existing id, `INSERT RELATION` of an existing edge | `AlreadyExists(Some(Record { id }))` | "Database record \`idempotency_key:[…]\` already exists" |
| A `UNIQUE` index (`task_alert`, `user_auth`) | `Internal`, no details | "Database index \`task_alert\` already contains 'k', with record \`task:…\`" |
| `DELETE` of a record still referenced | `Internal` | "Cannot delete \`garden:g1\` as it is referenced by \`plant:p1\` with an ON DELETE REJECT clause" |
| A field type or literal-union mismatch | `Internal` | "Couldn't coerce value for field …" |
| Wrong credentials | `NotAllowed(Some(Auth(InvalidAuth)))` | "There was a problem with authentication" |

So the store maps errors in one place: `TransactionConflict` and `AlreadyExists(Record)` by details, a `THROW` by the text after `"An error occurred: "`, and a unique index by its name in the message (`is_unique_violation` in [write-patterns.md](write-patterns.md#the-module)). `StoreError` and its `From<surrealdb::Error>` are defined once, in [rust-errors, mapping.md](../rust-errors/mapping.md), which also maps them to `ApiError`; [errors-and-retries.md](errors-and-retries.md) and [write-patterns.md](write-patterns.md) carry exact copies.

## Retrying conflicts

Retry the **whole transaction** on `TransactionConflict`, up to 5 attempts, sleeping 10–50 ms × 2^attempt with jitter between them ([database 4](../../../docs/database/README.md#4-write-patterns), [6](../../../docs/database/README.md#6-rust-sdk); [`QueryError::TransactionConflict`](https://docs.rs/surrealdb-types/3.3.4/surrealdb_types/enum.QueryError.html), [`Error::query_details`](https://docs.rs/surrealdb-types/3.3.4/surrealdb_types/struct.Error.html#method.query_details)). The closure rebuilds and re-sends the query each time. Jitter uses `rand::random_range` from `rand` 0.10, which draws from the thread-local generator and returns before the `.await`, so the future stays `Send` ([`random_range`](https://docs.rs/rand/0.10.3/rand/fn.random_range.html)). `StoreError`, its `From<surrealdb::Error>`, `first_failure`, `checked` and `retry_conflicts` are compiled in [errors-and-retries.md](errors-and-retries.md), with tests that prove 5 attempts and 150–750 ms of back-off on tokio's paused clock.

## Write patterns

Each pattern in [database 4](../../../docs/database/README.md#4-write-patterns) is one store function in [write-patterns.md](write-patterns.md), with its SurrealQL copied verbatim and its live test:

| Pattern | Function | Returns |
|---|---|---|
| [Scan events](../../../docs/database/README.md#scan-events-a-gap-free-seq-) | `append_scan_event` | the new `seq`, read from the created record's key |
| [Plan compare-and-set](../../../docs/database/README.md#plan-writes-compare-and-set-) | `set_plan_cas` | `Err(StoreError::Conflict { what: "plan" })` when another plan won |
| [Job claim](../../../docs/database/README.md#job-claim-with-slots-) | `claim_job` | `Option<LeasedJob>`; `None` when the worker's slots are full or nothing is queued |
| [Idempotency keys](../../../docs/database/README.md#idempotency-keys-) | `begin_idempotent` | `Started`, or `Existing(row)` for contracts 1 to decide replay, `409` or `422` |
| [Membership](../../../docs/database/README.md#membership-) | `add_member` | `Err(StoreError::AlreadyExists(_))` for a second insert |
| [Alert dedupe](../../../docs/database/README.md#alert-dedupe-) | `create_alert_task` | `Ok(false)` when the `task_alert` index already holds the key |
| [Deletion](../../../docs/database/README.md#deletion-) | `delete_scan_events`, `delete_in_batches` | rows deleted |

The rest of database 4 (limits, steps between jobs, garden map versions, cleanup) follows the same shape: the statement in a `const`, binds, `checked`, a typed `take`, and `retry_conflicts` around anything in `BEGIN … COMMIT` or anything that races, including a single `CREATE` or `INSERT RELATION` of a key other connections may write at once ([write-patterns.md, duplicates race too](write-patterns.md#what-each-function-relies-on)). Each `THROW` code is mapped in `StoreError`'s one `From` impl ([rust-errors, mapping.md](../rust-errors/mapping.md)): `stale_plan` and `claim_lost` are `Conflict { what }`, any other code is `Thrown(code)`.

The per-scan event writer serialises appends inside one process ([contracts 4](../../../docs/architecture/contracts.md#transport)); the retry covers the rest ([database 4](../../../docs/database/README.md#scan-events-a-gap-free-seq-)). How that writer task is built is in [`rust-async`](../rust-async/SKILL.md).

## Reads

1. **SSE replay is a key-range read**: `SELECT * FROM scan_event:[$scan, $after + 1]..=[$scan, ..]` with `$scan` a `RecordId` and `$after` the `Last-Event-ID` ([database 4](../../../docs/database/README.md#scan-events-a-gap-free-seq-), [contracts 4](../../../docs/architecture/contracts.md#transport)). ✓ After 39 events, `$after = 35` returns 36…39 in order (`events_after`). `$after` must be a number: ✓ bound to NONE, `$after + 1` fails with "Cannot perform addition with 'none' and 'int'". Without `Last-Event-ID` the server replays the whole history ([contracts 4](../../../docs/architecture/contracts.md#transport)), so it binds 0.
2. **Page with a cursor, never `START`** ([database 5](../../../docs/database/README.md#5-reads-indexes-and-vector-search)): `WHERE … AND id > $after ORDER BY id LIMIT 50`. ✓ With `$after` bound to `None`, `id > $after` holds for every record, so one statement serves the first page too (`plants_page`: 61 plants come back as 50, then 11); ✓ (*probe*) the same with `Value::None`, `Value::Null` or `$after` not bound. The cursor is the ULID `id`, which database 5 allows and which is unique, so a page never skips or repeats a row; a cursor on `name` would, since names repeat. ✓ (*probe*) `EXPLAIN` shows an `IndexScan` on `plant_garden (garden, name)` for the filter and a `SortTopKByKey` on `id`, which a garden's few plants keep cheap.
3. **`ONLY` when exactly one record is expected** ([database 5](../../../docs/database/README.md#5-reads-indexes-and-vector-search)). ✓ `SELECT * FROM ONLY $id` for a missing id is not an error: it takes as `None`. Treat `None` as `404` in the handler.
4. **Run `EXPLAIN` on each new hot query** and look for `IndexScan`, `KnnScan` or a key range ([database 5](../../../docs/database/README.md#5-reads-indexes-and-vector-search)).

## Deletes

Follow [database 4, deletion](../../../docs/database/README.md#deletion-): set `deleting_at`, cancel jobs and clear claims, delete the RustFS objects first ([storage 4](../../../docs/storage/README.md#4-writes-and-deletes)), then the records **children first, in batches of 500, each its own small statement, never one large transaction**. The order is fixed by the `REFERENCE ON DELETE REJECT` links; take it from the doc, step by step. Select plans by equality, never `IN` ([#7593](https://github.com/surrealdb/surrealdb/issues/7593)). The cascade runs in a background task on the process `TaskTracker` ([`rust-async`](../rust-async/SKILL.md)) and is retried from the start on failure, which is safe because every step is idempotent ([storage 4](../../../docs/storage/README.md#4-writes-and-deletes)).

Each batch is `DELETE (SELECT VALUE id FROM t WHERE … LIMIT 500) RETURN VALUE $before.id`, repeated until it returns no ids ([database 4, deletion](../../../docs/database/README.md#deletion-)). ✓ `RETURN NONE` gives `[]` both for a batch that deleted 2 rows and for one that deleted none, so it can't tell when to stop. ✓ `RETURN VALUE id` gives NONE for each row, because the record is gone. ✓ (*probe*) `RETURN BEFORE` works too but sends whole records back. ✓ `delete_in_batches` deletes all 1 200 observations of a scan, and the scan's events go in one range delete (`delete_scan_events`).

## Tests

[`rust-testing`](../rust-testing/SKILL.md) owns the test setup. For the store:

- Store tests run against the pinned image, never a mock: `docker run --rm -p 127.0.0.1:18000:8000 surrealdb/surrealdb:v3.3.4@sha256:649a60a3b9c0177d9923c938e1c2563e48cde2a9d06ac2c0519714be719b02af start --user root --pass root memory` (`memory` is for tests, [database 1](../../../docs/database/README.md#1-version-and-setup)).
- Mark each `#[ignore = "needs SurrealDB"]` and read the endpoint from `PDM_TEST_SURREALDB_URL` (`ws://127.0.0.1:18000`), so `cargo test` passes without a server and `cargo nextest run --run-ignored only` (or `cargo test -- --ignored`) runs them.
- Every store test starts with `TestDb::new()` from [rust-testing, database.md](../rust-testing/database.md#one-namespace-per-test): a fresh namespace with the real schema applied as root, an `api` user (`ROLES EDITOR`) in it, and a client signed in as that user, so the test runs with the API's permissions and sees only its own rows. Store functions take `&Surreal<Any>` and never select a namespace, so they run unchanged there.
- Don't share one namespace between tests. nextest runs each test in its own process, so a once-per-binary setup runs in every test at once: ✓ when the tests applied the schema once into a shared `pdm`/`main`, all 9 failed in that step with `TransactionConflict` under `cargo nextest run --run-ignored only`. Shared tables also let a claim take another test's job.
- Prove each behaviour the code relies on: the statement index, the error a `THROW` maps to, NONE versus a value. [write-patterns.md](write-patterns.md#live-tests) has the pattern.

**No embedded engine.** The `kv-mem` feature would run tests in-process, but it adds the whole engine: 522 crates against 274 with the defaults, about 2 minutes and 3.4 GB of `target/` from cold (✓ built on 2026-10-15, and it applied the schema and returned the same `THROW` error). It is not the binary the API talks to in production, and the pinned image is. So it is not in versions.md, and store tests use the container.

## Review checklist

- [ ] Every value is a bind; no `format!` builds SurrealQL. Every bind name matches its `$name`, and a test would notice a NONE.
- [ ] Each statement is a `const` copied from database 4 or 5, with its statement indexes noted.
- [ ] Every `.query()` result goes through `take_errors()` (`checked`), not a bare `take`; no `.check()` on a `BEGIN … COMMIT`.
- [ ] Everything in `BEGIN … COMMIT`, and every write that can race, is inside `retry_conflicts`, and the closure rebuilds the whole query.
- [ ] `THROW` codes, `AlreadyExists` and unique-index violations map to typed `StoreError` variants in one place.
- [ ] Records derive `SurrealValue`; string enums are `untagged`; ids and links are `RecordId`; optional fields are `Option<T>`; instants are `DateTime<Utc>`.
- [ ] One client in `Arc<AppState>`, signed in as `api` with `Database` credentials; no `Surreal` clones per request; the schema is applied as root before serving.
- [ ] Reads page by cursor, use `ONLY` for single records, and have an `EXPLAIN` showing an index or key range if hot.
- [ ] Deletes go bottom-up in batches of 500 after the RustFS objects, in a background task, never as one transaction.
- [ ] New store code has an ignored test against the pinned image.

## Related skills

[`rust`](../rust/SKILL.md) (entry, versions, checker) · [`rust-errors`](../rust-errors/SKILL.md) (`StoreError` → `ApiError`) · [`rust-async`](../rust-async/SKILL.md) (the per-scan writer, background deletes, shutdown) · [`rust-axum-api`](../rust-axum-api/SKILL.md) (`AppState`, idempotent replay) · [`rust-contract-types`](../rust-contract-types/SKILL.md) (DTOs and wire ids) · [`rust-workers`](../rust-workers/SKILL.md) (the claim from the worker's side) · [`rust-testing`](../rust-testing/SKILL.md) · [`rust-tracing`](../rust-tracing/SKILL.md)
