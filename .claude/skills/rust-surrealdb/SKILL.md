---
name: rust-surrealdb
description: "The store crate on SurrealDB 3.3.4: connect and sign in, SurrealValue and RecordId, binds, run_tx and conflict retries, DbFailure into the domain StoreError, THROW codes, write patterns, range reads, batched deletes, kv-mem store tests. Use for any SurrealDB query in Rust."
---

# SurrealDB from Rust (`store` crate)

This skill covers the `store` crate ([architecture 3](../../../docs/architecture/README.md#rust-crates); [backend store](../../../docs/backend/store.md)): one SurrealDB client per process, and one Rust function per query. It implements the database document in Rust; it does not redefine it.

**The project docs win.** [docs/database/README.md](../../../docs/database/README.md) was verified against the pinned server (its ✓ marks), and [schema.surql](../../../backend/migrations/schema.surql) is the stored shape. Take every SurrealQL statement verbatim from [database 4](../../../docs/database/README.md#4-write-patterns) and [5](../../../docs/database/README.md#5-reads-indexes-and-vector-search), and follow [database 6](../../../docs/database/README.md#6-rust-sdk) for the SDK. Where this skill and those sections disagree, the sections win and the disagreement goes to the docs' owner. [Backend store](../../../docs/backend/store.md) is how the crate applies them, and this skill follows it: store tests on the embedded engine ([backend store 20](../../../docs/backend/store.md#20-testing)), failures leaving the crate as the domain's `StoreError` through a crate-private `DbFailure`, and every statement through `run_tx` ([backend store 5](../../../docs/backend/store.md#5-transactions-retries-and-the-error-mapping)). Store.md keeps one deliberate difference from database 4, which this skill follows: its statements bind `$now` where database 4 writes `time::now()` ([backend store 1](../../../docs/backend/store.md#1-purpose-and-boundaries), the "Clock" row), so time comes from the `Clock` port ([backend README 7](../../../docs/backend/README.md#7-rust-conventions), "Time": every "now" comes from the `Clock` port) and tests control lease, stamp and expiry times; the few methods that pass no time are listed in [store 22](../../../docs/backend/store.md#22-open-questions), item 5.

Versions: crate `surrealdb` 3.3.4 against server `surrealdb/surrealdb:v3.3.4` ([database 1](../../../docs/database/README.md#1-version-and-setup), [versions.md](../rust/versions.md)). The 3.x SDK differs a lot from 2.x: records derive `SurrealValue`, not serde, and errors are one `surrealdb::Error` with a kind and typed details. Don't write it from memory; read `~/.cargo/registry/src/*/surrealdb-3.3.4/` and `surrealdb-types-3.3.4/`. docs.rs failed to build `surrealdb` 3.3.4 (and 3.3.3), so this skill cites its source files (paths relative to `surrealdb-3.3.4/`) and the [Rust SDK reference](https://surrealdb.com/docs/reference/rust/methods); `surrealdb-types` 3.3.4 did build and is linked on docs.rs.

| File | What's in it |
|---|---|
| This file | Connecting, types, queries, errors, the conflict retry, reads, deletes, tests, review checklist |
| [errors-and-retries.md](errors-and-retries.md) | The store kit: `run_tx`, `retry_conflicts` and `backoff`, `DbFailure`, `classify`, the `thrown` codes and `into_store`, mapping into the domain's `StoreError`; compiled and tested |
| [write-patterns.md](write-patterns.md) | One function per database-4 pattern, compiled, with tests on the embedded engine and one ignored test against the pinned image |

Behaviour marked ✓ below was run on 2026-10-15 and 2026-10-16, by the tests in [write-patterns.md](write-patterns.md#the-module) and [rust-testing, database.md](../rust-testing/database.md): on the embedded 3.3.4 engine (`kv-mem`, the same engine code as the image) in the checker's normal run, and against `surrealdb/surrealdb:v3.3.4@sha256:649a60a3…` (`start --user root --pass root memory`) for what needs the server, through `check_examples.py --live` (cargo-nextest 0.9.148); the few marked *probe* were run from a scratch crate against the same image.

## Connecting

1. **Connect once at start-up** with `surrealdb::engine::any::connect("ws://surrealdb:8000")` ([database 1](../../../docs/database/README.md#1-version-and-setup); [connect](https://surrealdb.com/docs/reference/rust/methods/connect); `src/engine/any/mod.rs`). The crate's default features are off, with `protocol-ws` and `rustls` on ([backend store 2](../../../docs/backend/store.md#2-dependencies); [versions.md](../rust/versions.md#dependencies)). `parse` is off, and nothing needs it: in 3.3.4 it gates only `surrealdb::parse::value`, which parses SurrealQL text into a `Value` on the client (`#[cfg(feature = "parse")] pub mod parse` in `src/lib.rs`). Queries are sent as text and parsed by the server or the embedded engine, and `RecordId::parse_simple` lives in `surrealdb-types`, so ✓ every block in this skill compiles and runs without it; the `compile_fail` block below shows `parse::value` is gone.
2. **The SDK checks the server version on connect.** A remote connect calls `version()` and refuses a server outside `>=3.0.0-alpha.1, <4.0.0` (`SUPPORTED_VERSIONS` and `verify_server_version` in `surrealdb-3.3.4/src/lib.rs`). It skips the check when the server's capabilities deny the `version` method. Pin the server and crate together anyway ([database 1](../../../docs/database/README.md#1-version-and-setup)).
3. **Sign in as `api` with `opt::auth::Database`**, not `Root`: `api` is a database-level user ([database 1, users](../../../docs/database/README.md#1-version-and-setup)). ✓ `Root { username: "api", .. }` fails with `NotAllowed(Auth(InvalidAuth))`. Then call `use_ns("pdm").use_db("main")` ([signin](https://surrealdb.com/docs/reference/rust/methods/signin), [use](https://surrealdb.com/docs/reference/rust/methods/use); `Database` in `src/opt/auth.rs`). `connect_api` takes the namespace as a parameter, so the API passes `pdm` and a test passes its own.
4. **Apply the embedded schema as root, before serving.** The users table gives schema application to root ([database 1](../../../docs/database/README.md#1-version-and-setup)). ✓ Run as `api`, the file's statements 0 and 2 (`DEFINE NAMESPACE`, `DEFINE DATABASE`) fail with "IAM error: Not enough permissions to perform this action", and every other statement applies. ✓ A namespace-level `OWNER` may run `DEFINE DATABASE` but not `DEFINE NAMESPACE`, which needs root ([`DEFINE USER`](https://surrealdb.com/docs/reference/query-language/statements/define/user)). So start-up opens a short-lived root connection, runs the file in one `.query()`, checks every statement, and drops it; then it opens the long-lived `api` client. ✓ As root the file applies twice in a row with no errors. `connect_api` and `apply_schema` are in [write-patterns.md](write-patterns.md#the-module).
5. **`DEFINE USER` takes a literal password.** ✓ `PASSWORD $p` is a parse error ("Unexpected token \`a parameter\`, expected a strand") whether `$p` is bound or set by `LET`, and so is `PASSHASH $p`. So the password is written into the statement as a single-quoted string with `\` and `'` escaped by a backslash. SurrealDB's [strings](https://surrealdb.com/docs/reference/query-language/language-primitives/data-types/strings) page doesn't spell out that rule, so the proof is the harness's: ✓ its password contains both characters and signs in, so an escaping mistake fails every server test (`surql_string` in [rust-testing, database.md](../rust-testing/database.md#2-the-pinned-image)). The `api` user is created by the setup script ([database 7](../../../docs/database/README.md#7-operations)), never by the API.
6. **Share the client, don't clone it.** The process has one `Surreal<Any>`, inside `SurrealStore`, which is not `Clone` and lives in `Arc`; `api`'s `AppState` holds it as `Arc<dyn Store>`, never the client itself, and repository code passes `&Surreal<Any>` to each statement ([database 6](../../../docs/database/README.md#6-rust-sdk); [backend store 4](../../../docs/backend/store.md#4-connecting-and-the-schema), [18](../../../docs/backend/store.md#18-concurrency-and-lifetimes)). The functions in [write-patterns.md](write-patterns.md) take `&Surreal<Any>` for that reason. In 3.3.4, `Surreal::clone` gives the clone a new session id on the same WebSocket and replays the session's log (`signin`, `use`, `set`) one command at a time before the clone's first request runs (`impl Clone for Surreal` in `src/lib.rs`; `handle_session_clone` and `replay_session` in `src/engine/remote/ws/mod.rs`), and dropping a clone detaches its session. ✓ A clone reports a different `$session.id`, and a variable `set` on the clone is NONE on the original. This agrees with database 6.

Rule 1, as the compiler sees it with the pinned features: without `parse`, `surrealdb::parse` doesn't exist.

```rust,compile_fail,E0433
fn literal() -> surrealdb::types::Value {
    surrealdb::parse::value("[1, 2]").expect("a SurrealQL value")
}
```

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
4. **`take(n)` with a type.** `Option<T>` takes a statement that returns one row (or none), `Vec<T>` one that returns many, `(n, "field")` one field (`IndexedResults::take` in `src/method/query.rs`; [query](https://surrealdb.com/docs/reference/rust/methods/query)). ✓ A statement that returns an array of two or more, such as a batched delete's `RETURN VALUE $before.id`, fails as `Option` with "Tried to take only a single result from a query that contains multiple", so take it as `Vec` ([backend store 5](../../../docs/backend/store.md#run_tx)); an array of one takes as `Option` either way, so only a test with several rows notices. ✓ `RETURN VALUE scans` gives `[1]`, which takes as `Option<i64>` = `Some(1)`. ✓ An index past the last statement is not an error: it takes as `Ok(None)` or `Ok([])`, so a wrong index reads as "no rows". Name the index next to the constant and test the value.
5. **`.query()` returns `Ok` when statements failed** ([database 6](../../../docs/database/README.md#6-rust-sdk); [error handling](https://surrealdb.com/docs/reference/rust/concepts/error-handling)). ✓ `CREATE nosuch:1; RETURN 1` is `Ok`: statement 0 holds `NotFound(Table)`, and statement 1 still gives `1`. Read the errors before any `take`.
6. **In a transaction, `.check()` hides the cause.** It returns the first failed statement by index (`IndexedResults::check` in `src/method/query.rs`), and in a failed transaction the other statements report errors too ([errors inside a transaction](https://surrealdb.com/docs/reference/rest-api/errors)). ✓ In `BEGIN; CREATE …; IF true { THROW 'stale_plan' }; RETURN 5; COMMIT;`, statement 1 (before the cause) is `Query(NotExecuted)` "The query was not executed due to a failed transaction", 2 is the `THROW`, 3 (after the cause) is `Query(Cancelled)` "The query was not executed due to a cancelled transaction", and 4, the `COMMIT`, is `Query(NotExecuted)` "Cannot COMMIT: the transaction was aborted due to a prior error"; `check()` returns statement 1. ✓ (*probe*) For a conflict, the cause is on the `COMMIT` and the statements before it are `NotExecuted`. `run_tx` reads `take_errors()` through `first_failure`, which skips both `NotExecuted` and `Cancelled` ([errors-and-retries.md](errors-and-retries.md)). Outside a transaction, `.check()` is fine.
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

So the store reads errors in one place, `classify`: `TransactionConflict` and `AlreadyExists(Record)` by details, a lost connection by kind, a `THROW` by the text after `"An error occurred: "`, and a unique index by its name in an `Internal` message. It returns the crate-private `DbFailure`, which keeps the SDK error, and `DbFailure::into_store(what)` maps it into the domain's `StoreError`, after a caller that knows more has mapped its own cases first ([backend store 5, classifying failures](../../../docs/backend/store.md#classifying-failures)). No `surrealdb` type leaves the crate: `StoreError` is a port error of `domain`, defined once in [rust-errors, mapping.md](../rust-errors/mapping.md) as [backend domain 6](../../../docs/backend/domain.md#port-errors) writes it, which also maps it to `ApiError`. The kit is in [errors-and-retries.md](errors-and-retries.md).

## Retrying conflicts

Retry the **whole transaction** on `TransactionConflict`, up to 5 attempts, sleeping 10–50 ms × 2^attempt with jitter between them ([database 4](../../../docs/database/README.md#4-write-patterns), [6](../../../docs/database/README.md#6-rust-sdk); [`QueryError::TransactionConflict`](https://docs.rs/surrealdb-types/3.3.4/surrealdb_types/enum.QueryError.html), [`Error::query_details`](https://docs.rs/surrealdb-types/3.3.4/surrealdb_types/struct.Error.html#method.query_details)). Every statement goes through `run_tx(db, sql, bind)`, which re-sends the whole query on each attempt with the binds `bind` adds; a single statement is its own transaction and can conflict the same way, so reads go through it too ([backend store 5, `run_tx`](../../../docs/backend/store.md#run_tx)). Any other failure returns at once: a lost compare-and-set is final. When the retries run out it is `Conflict(Retries)`, `503 upstream_unavailable` ([backend domain 6](../../../docs/backend/domain.md#port-errors)). Jitter uses `rand::random_range` from `rand` 0.10, which draws from the thread-local generator and returns before the `.await`, so the future stays `Send` ([`random_range`](https://docs.rs/rand/0.10.3/rand/fn.random_range.html)). The kit is compiled in [errors-and-retries.md](errors-and-retries.md), with tests that prove 5 attempts and 150–750 ms of back-off on tokio's paused clock, and why `bind`'s bound names one lifetime `'c`.

## Write patterns

Each pattern in [database 4](../../../docs/database/README.md#4-write-patterns) is one store function in [write-patterns.md](write-patterns.md), with its SurrealQL copied verbatim except where [backend store](../../../docs/backend/store.md) changes it (a bound `$now` where database 4 writes `time::now()`, store.md's one deliberate difference, and store.md's fuller statements where its listings give them; [write-patterns.md](write-patterns.md) names each constant's source and how it differs), and its test:

| Pattern | Function | Returns |
|---|---|---|
| [Scan events](../../../docs/database/README.md#scan-events-a-gap-free-seq-) | `append_scan_event` | the new `seq`, read from the created record's key; `NotFound` for a missing scan |
| [Plan compare-and-set](../../../docs/database/README.md#plan-writes-compare-and-set-) | `set_plan_cas` | `Err(Conflict(StalePlan { current }))` with the plan that won |
| [Steps between jobs](../../../docs/database/README.md#steps-between-jobs-) | `claim_step`, `commit_step`, `reclaim_step` | `claimed`, `busy` (the scan flagged for `score`) or `not_needed`; the next step; `Err(Conflict(ClaimLost))` when the claim, its exact stamp or its run no longer holds |
| [Job claim](../../../docs/database/README.md#job-claim-with-slots-) | `enqueue_job`, `claim_job`, `requeue_stale` | the leased job (`None` when the worker's slots are full or nothing is eligible) and whether it started the run; the requeued jobs, after contracts 5's attempt rules; the enqueue and the requeue signal `jobs_queued` |
| [Idempotency keys](../../../docs/database/README.md#idempotency-keys-) | `begin_idempotent` | `Started`, or `Existing(row)` for contracts 1 to decide replay, `409` or `422` |
| [Membership](../../../docs/database/README.md#membership-) | `add_member` | `Err(Conflict(AlreadyExists))` for a second insert |
| [Alert dedupe](../../../docs/database/README.md#alert-dedupe-) | `create_alert_task` | `Ok(false)` when the `task_alert` index already holds the key |
| [Deletion](../../../docs/database/README.md#deletion-) | `delete_scan_events`, `delete_in_batches` | rows deleted |

The rest of database 4 (limits, garden map versions, indicator answers, cleanup) follows the same shape: the statement in a `const` with its statement indexes, `run_tx` with the binds, a typed `take`, and the failure mapped with `into_store(what)`. `run_tx` retries everything, so a single `CREATE` or `INSERT RELATION` of a key other writers may create at once is covered too ([write-patterns.md, duplicates race too](write-patterns.md#what-each-function-relies-on)). Each `THROW` code is a constant in `thrown`, and a caller that needs more than the default mapping checks `failure.thrown()` first: `stale_map` becomes `StaleMap { current_version }` and `budget_exceeded` `Rejected(BudgetExceeded)` ([backend store 5](../../../docs/backend/store.md#classifying-failures)).

Settled questions behind these functions ([contracts 12](../../../docs/architecture/contracts.md#12-open-questions); details in [write-patterns.md](write-patterns.md#what-each-function-relies-on)): step stamps are stored with full precision and compared exactly, whole seconds being a wire matter (question 12); a stopping worker's jobs are requeued when its heartbeat is 30 s old or its lease expired, with no hand-back call (question 20); and `seq` comes from `append_scan_event`'s transaction, so the append is correct from any number of writers (question 22) ([backend store 21](../../../docs/backend/store.md#21-decisions-on-open-questions)).

**The claim's wake-up.** The store's `jobs_queued` `Notify` is the only wake-up for the broker's long poll (with a 1-second fallback poll): every write that queues or requeues a job calls `notify_waiters()` after its commit, and nothing else signals the broker ([database 4, job claim](../../../docs/database/README.md#job-claim-with-slots-); [backend store 4](../../../docs/backend/store.md#4-connecting-and-the-schema), [14](../../../docs/backend/store.md#14-jobs-and-workers), [18](../../../docs/backend/store.md#18-concurrency-and-lifetimes)).

**Binds to avoid** ([database 8, caveat 16](../../../docs/database/README.md#8-caveats)). ✓ `access`, `auth`, `token` and `session` are protected and fail the whole request, so no example binds `token`. A bound `$before` or `$after` is overridden in a `RETURN` clause by the record's own, a bound `$this` inside a record statement and a bound `$parent` inside a subquery, so binds never use `before`, `this` or `parent` (`$target` names a batch's parent). `$after` is bound only where a `SELECT` reads it, as database 4's replay range does, which caveat 16 calls safe; the page cursor is `$cursor`. ✓ A point takes its coordinates through `type::point([$lon, $lat])`: `($lon, $lat)` is a parse error ([caveat 17](../../../docs/database/README.md#8-caveats)).

The per-scan event writer serialises appends inside one process ([contracts 4](../../../docs/architecture/contracts.md#transport)); the retry covers the rest ([database 4](../../../docs/database/README.md#scan-events-a-gap-free-seq-)). How that writer task is built is in [`rust-async`](../rust-async/SKILL.md).

## Reads

1. **SSE replay is a key-range read**: `SELECT * FROM scan_event:[$scan, $after + 1]..=[$scan, ..]` with `$scan` a `RecordId` and `$after` the `Last-Event-ID` ([database 4](../../../docs/database/README.md#scan-events-a-gap-free-seq-), [contracts 4](../../../docs/architecture/contracts.md#transport)). ✓ After 39 events, `$after = 35` returns 36…39 in order (`events_after`). `$after` must be a number: ✓ bound to NONE, `$after + 1` fails with "Cannot perform addition with 'none' and 'int'". Without `Last-Event-ID` the server replays the whole history ([contracts 4](../../../docs/architecture/contracts.md#transport)), so it binds 0.
2. **Page with a cursor, never `START`** ([database 5](../../../docs/database/README.md#5-reads-indexes-and-vector-search)): `WHERE … AND id > $cursor ORDER BY id LIMIT 50`. ✓ With `$cursor` bound to `None`, `id > $cursor` holds for every record, so one statement serves the first page too (`plants_page`: 61 plants come back as 50, then 11); ✓ (*probe*) the same with `Value::None`, `Value::Null` or the cursor not bound. The store's lists fetch `limit + 1` rows to know whether a next page exists, with the cursor on their sort key and the id ([backend store 6](../../../docs/backend/store.md#reads-into-domain-types)). The cursor is the ULID `id`, which database 5 allows and which is unique, so a page never skips or repeats a row; a cursor on `name` would, since names repeat. ✓ (*probe*) `EXPLAIN` shows an `IndexScan` on `plant_garden (garden, name)` for the filter and a `SortTopKByKey` on `id`, which a garden's few plants keep cheap.
3. **`ONLY` when exactly one record is expected** ([database 5](../../../docs/database/README.md#5-reads-indexes-and-vector-search)). ✓ `SELECT * FROM ONLY $id` for a missing id is not an error: it takes as `None`. Treat `None` as `404` in the handler.
4. **Run `EXPLAIN` on each new hot query** and look for `IndexScan`, `KnnScan` or a key range ([database 5](../../../docs/database/README.md#5-reads-indexes-and-vector-search)).

## Deletes

Follow [database 4, deletion](../../../docs/database/README.md#deletion-): set `deleting_at`, cancel jobs and clear claims, delete the RustFS objects first ([storage 4](../../../docs/storage/README.md#4-writes-and-deletes)), then the records **children first, in batches of 500, each its own small statement, never one large transaction**. The order is fixed by the `REFERENCE ON DELETE REJECT` links; take it from the doc, step by step. Select plans by equality, never `IN` ([#7593](https://github.com/surrealdb/surrealdb/issues/7593)). The cascade runs in a background task on the process `TaskTracker` ([`rust-async`](../rust-async/SKILL.md)) and is retried from the start on failure, which is safe because every step is idempotent ([storage 4](../../../docs/storage/README.md#4-writes-and-deletes)).

Each batch is `DELETE (SELECT VALUE id FROM t WHERE … LIMIT 500) RETURN VALUE $before.id`, repeated until it returns no ids ([database 4, deletion](../../../docs/database/README.md#deletion-)). ✓ `RETURN NONE` gives `[]` both for a batch that deleted 2 rows and for one that deleted none, so it can't tell when to stop. ✓ `RETURN VALUE id` gives NONE for each row, because the record is gone. ✓ (*probe*) `RETURN BEFORE` works too but sends whole records back. ✓ `delete_in_batches` deletes all 1 200 observations of a scan, and the scan's events go in one range delete (`delete_scan_events`).

## Tests

[`rust-testing`](../rust-testing/SKILL.md) owns the test setup. For the store ([backend store 20](../../../docs/backend/store.md#20-testing)):

- **Store tests run on the embedded engine**, never a mock: `surrealdb` with `kv-mem` in `[dev-dependencies]` only, and `TestDb::memory()` from [rust-testing, database.md](../rust-testing/database.md#1-the-embedded-engine), which connects to `mem://` (a new datastore each time), applies the real schema and selects `pdm`/`main`. It is the same 3.3.4 engine code as the image, needs no Docker, and tests never share rows, so they run in parallel under `cargo test` or nextest. Store functions take `&Surreal<Any>` and never select a namespace, so they run unchanged there.
- **Race on a multi-thread runtime.** ✓ Concurrent transactions never conflict on `#[tokio::test]`'s current-thread runtime; a test of the retry, the gap-free `seq` or a racing duplicate uses `#[tokio::test(flavor = "multi_thread", worker_threads = 4)]`.
- **What only the server has** is tested against the pinned image in two tests marked `#[ignore = "needs SurrealDB"]`, each in a namespace of its own (`TestDb::server()`, reading `PDM_TEST_SURREALDB_URL`): the harness's namespace isolation and `api`'s `EDITOR` permissions ([rust-testing, database.md](../rust-testing/database.md#3-the-harness)), and `users_and_sessions`, which signs in as `api`, refuses `api` as root, shows which schema statements need root or a namespace owner, applies the schema twice as root, and shows a clone is a new WebSocket session ([write-patterns.md](write-patterns.md#the-module)). The store crate keeps one such test, `tests/server.rs`, which applies the schema twice as root ([backend store 3](../../../docs/backend/store.md#3-module-layout), [20](../../../docs/backend/store.md#20-testing)). The container: `docker run --rm -p 127.0.0.1:18000:8000 surrealdb/surrealdb:v3.3.4@sha256:649a60a3b9c0177d9923c938e1c2563e48cde2a9d06ac2c0519714be719b02af start --user root --pass root memory` (`memory` is for tests, [database 1](../../../docs/database/README.md#1-version-and-setup)), then `cargo nextest run --run-ignored only`.
- Prove each behaviour the code relies on: the statement index, the failure a `THROW` maps to, NONE versus a value, `Vec` versus `Option` with several rows. [write-patterns.md](write-patterns.md#the-module) has the pattern.

**The embedded engine's cost.** `kv-mem` adds the whole engine to test builds: on 2026-10-15 it took 522 crates against 274 without it, about 2 minutes and 3.4 GB of `target/` from cold ([backend store 2](../../../docs/backend/store.md#2-dependencies): "about five times the clean build"). Dev-dependency features reach only targets that use dev-dependencies, so the API binary never contains it ([Cargo book, feature resolver version 2](https://doc.rust-lang.org/cargo/reference/features.html#feature-resolver-version-2)). In the skills' checker, every block gets it; the measured times are in [rust-testing, database.md](../rust-testing/database.md#4-running-them).

## Review checklist

- [ ] Every value is a bind; no `format!` builds SurrealQL. Every bind name matches its `$name`, and a test would notice a NONE. No bind is named `access`, `auth`, `token` or `session` (protected), or `before`, `this` or `parent` (overridden inside statements); a bound `after` is read only in a `SELECT` (database 8, caveat 16).
- [ ] Each statement is a `const` copied from database 4 or 5 (with `$now` for `time::now()`), with its statement indexes noted.
- [ ] Every statement goes through `run_tx`, so its errors are read with `first_failure` and conflicts are retried; no `.check()` on a `BEGIN … COMMIT`.
- [ ] SDK errors are read only by `classify`; `THROW` codes, `AlreadyExists` and unique-index violations reach callers as `DbFailure` and leave the crate as the domain's `StoreError`, with the operation's name in `Query { what }`.
- [ ] Every write that queues or requeues a job signals `jobs_queued` after its commit.
- [ ] Records derive `SurrealValue`; string enums are `untagged`; ids and links are `RecordId`; optional fields are `Option<T>`; instants are `DateTime<Utc>`.
- [ ] One client inside `SurrealStore` behind `Arc<dyn Store>`, signed in as `api` with `Database` credentials; no `Surreal` clones per request; the schema is applied as root before serving.
- [ ] Reads page by cursor, use `ONLY` for single records, and have an `EXPLAIN` showing an index or key range if hot.
- [ ] Deletes go bottom-up in batches of 500 after the RustFS objects, in a background task, never as one transaction.
- [ ] New store code has a test on the embedded engine (multi-threaded if it races); only what needs the server's users or WebSocket is an ignored test against the pinned image.

## Related skills

[`rust`](../rust/SKILL.md) (entry, versions, checker) · [`rust-errors`](../rust-errors/SKILL.md) (`StoreError` → `ApiError`) · [`rust-async`](../rust-async/SKILL.md) (the per-scan writer, background deletes, shutdown) · [`rust-axum-api`](../rust-axum-api/SKILL.md) (`AppState`, idempotent replay) · [`rust-contract-types`](../rust-contract-types/SKILL.md) (DTOs and wire ids) · [`rust-workers`](../rust-workers/SKILL.md) (the claim from the worker's side) · [`rust-testing`](../rust-testing/SKILL.md) · [`rust-tracing`](../rust-tracing/SKILL.md)
