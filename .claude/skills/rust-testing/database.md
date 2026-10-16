# Database tests

Part of [`rust-testing`](SKILL.md). Store code is tested against SurrealDB itself, never a mock or a fake. **The default is the embedded 3.3.4 engine**: `surrealdb` with the `kv-mem` feature in `[dev-dependencies]`, one new in-memory datastore per test, the real schema applied ([backend store 2](../../../docs/backend/store.md#2-dependencies), [20](../../../docs/backend/store.md#20-testing)). It is the same engine code as the server image, needs no Docker, and keeps tests apart without any naming scheme. A few tests that need what only the server has (users and permissions, the WebSocket session) stay `#[ignore = "needs SurrealDB"]` and run against the pinned image, as the store crate's `tests/server.rs` does ([backend store 3](../../../docs/backend/store.md#3-module-layout)). The store patterns themselves, and what each query must prove, are in [`rust-surrealdb`](../rust-surrealdb/SKILL.md); this file is the harness.

## 1. The embedded engine

```toml
# crates/store/Cargo.toml (backend store 2)
[dev-dependencies]
surrealdb = { workspace = true, features = ["kv-mem"] }
```

- **Tests only.** With resolver 2 and later, a dev-dependency's features are enabled only when Cargo builds a target that needs dev-dependencies (tests, examples, benches), so the API binary never contains the engine ([Cargo book, feature resolver version 2](https://doc.rust-lang.org/cargo/reference/features.html#feature-resolver-version-2)). The other side: every test and example target of the crate gets it. In the skills' checker every block is a test or example target of one crate, and [versions.md](../rust/versions.md#dev-dependencies) puts `kv-mem` in its dev-dependencies, so the engine is built for every block.
- **One datastore per test.** `surrealdb::engine::any::connect("mem://")` starts a new, empty datastore each time ([connect](https://surrealdb.com/docs/reference/rust/methods/connect); `src/engine/any/mod.rs` in `surrealdb-3.3.4`). ✓ Two datastores in one test don't see each other's records (`each_memory_db_is_its_own_datastore`). Tests run in parallel, as `cargo test` threads or nextest processes, and a test may assume its tables start empty.
- **The schema is the real one**, embedded with `include_str!`, which resolves against the source file at compile time, so the test doesn't depend on the directory it runs from ([`include_str!`](https://doc.rust-lang.org/std/macro.include_str.html)). The path is relative to the file that names it, so it depends on that file's depth: from a top-level test file, `backend/crates/store/tests/*.rs`, `../../../migrations/schema.surql` reaches `backend/migrations/schema.surql`; from `tests/embedded/support.rs` it would need one `../` more and from `src/client.rs` it needs the same three ([architecture 13](../../../docs/architecture/README.md#13-repository-layout)). The store crate's harness avoids the question by using the crate's own `SCHEMA` constant, the `include_str!` in `src/client.rs` ([backend store 4](../../../docs/backend/store.md#4-connecting-and-the-schema)). The checker writes each block as `tests/<block>.rs` of a crate at `<tmp>/backend/crates/skill-examples/` and copies the schema to `<tmp>/backend/migrations/schema.surql`, so the blocks embed it with the top-level test file's three `../` ([rust, checking the examples](../rust/SKILL.md#checking-the-examples)). The file defines and selects `pdm`/`main`, but a `USE` inside a query doesn't move the client's session, so `TestDb::memory()` selects them afterwards, as store.md's `from_client` does ([backend store 4](../../../docs/backend/store.md#4-connecting-and-the-schema)). ✓ The database is `STRICT` here too (`the_engine_is_strict_and_parses_like_the_server`).
- **No users.** The embedded engine runs without authentication ([backend store 4](../../../docs/backend/store.md#4-connecting-and-the-schema): "the embedded `mem://` engine in tests, which has no auth"), so a test there runs with every permission. What `api` may and may not do is tested on the server (section 2).
- **Conflicts need a multi-thread runtime.** ✓ 40 concurrent read-modify-write transactions on one record never conflict on `#[tokio::test]`'s current-thread runtime, and on a 4-thread runtime some fail with `TransactionConflict` and commit nothing (`conflicts_need_a_multi_thread_runtime`; [backend store 20](../../../docs/backend/store.md#20-testing)). So a test of the retry, the gap-free `seq` or a racing duplicate uses `#[tokio::test(flavor = "multi_thread", worker_threads = 4)]` ([`tokio::test`](https://docs.rs/tokio/1.53.3/tokio/attr.test.html#multi-threaded-runtime)), or it proves nothing.
- **Don't pause the clock** in a database test. This skill's choice: the engine runs its own work on the runtime, and auto-advance would fire any `timeout` around a query as soon as the runtime looks idle ([SKILL.md, section 2](SKILL.md#2-async-tests-and-the-paused-clock)).
- **Parsing is the server's.** The engine parses SurrealQL with the same code, so a parse error shows here as it would on the server: ✓ `DEFINE USER … PASSWORD $p` fails with "expected a strand" on the embedded engine as on the image ([database 1, users](../../../docs/database/README.md#1-version-and-setup)).

## 2. The pinned image

For what the embedded engine can't show: a user signing in, the `api` user's `EDITOR` permissions, root-only statements, and a session on the WebSocket. Same image and digest as production, with the in-memory store:

```bash
docker run --rm -d --name pdm-test-surrealdb -p 127.0.0.1:18000:8000 \
  surrealdb/surrealdb:v3.3.4@sha256:649a60a3b9c0177d9923c938e1c2563e48cde2a9d06ac2c0519714be719b02af \
  start --user root --pass root memory
until docker exec pdm-test-surrealdb /surreal is-ready --endpoint http://127.0.0.1:8000; do sleep 0.5; done
export PDM_TEST_SURREALDB_URL=ws://127.0.0.1:18000
```

✓ The image starts in about a second and `is-ready` works in it (it is distroless, so there is no `curl`; [database 1](../../../docs/database/README.md#1-version-and-setup)). `root`/`root` are this throwaway container's credentials, not a secret. The `memory` store disappears with the container, so tests never clean up after themselves.

`TestDb::server()` gives each test a namespace of its own, `test_<ulid>`, with the whole schema applied in it as root, an `api` user defined in its database, and a client signed in as that `api`:

- **One namespace per test, not one per binary.** nextest runs every test in its own process ([nextest, why process-per-test](https://nexte.st/docs/design/why-process-per-test/)), so a `static` set up "once per binary" runs once per test, all at the same moment. ✓ An earlier harness applied the schema once into a shared `pdm`/`main`: under `cargo nextest run --run-ignored only` (nextest 0.9.148), all 9 tests failed in that step with `TransactionConflict`, nine processes redefining the same tables at once. Shared tables also leak: the job claim takes the oldest queued job ([database 4, job claim](../../../docs/database/README.md#job-claim-with-slots-)), so two tests sharing a database claim each other's jobs.
- **Signed in as `api`, not root**, so the test runs with the API's permissions: `api` is a database-level user with `ROLES EDITOR` ([database 1, users](../../../docs/database/README.md#1-version-and-setup)). A test that ran as root would pass a query `api` isn't allowed to run.
- **The namespace.** `schema.surql` selects `pdm`/`main` with four statements after its header comment, and a `USE` inside a query wins over the client's `use_ns` for the rest of that query (✓ `USE NS elsewhere; RETURN session::ns()` gives `elsewhere`, and the client's next query is back in its own namespace). So `schema_in` replaces the first occurrence of those four lines with the test's namespace, and panics if the file no longer contains them. It checks with `contains`, not `starts_with`: the header comment comes first.
- **The `api` password is a literal.** `DEFINE USER … PASSWORD` takes a string literal, never a parameter (✓ `PASSWORD $p` and `PASSHASH $h` are parse errors; [`DEFINE USER`](https://surrealdb.com/docs/reference/query-language/statements/define/user)), so the harness writes it into the statement with `surql_string`, which escapes `\` and `'` with a backslash inside a single-quoted string. SurrealDB's [strings](https://surrealdb.com/docs/reference/query-language/language-primitives/data-types/strings) page doesn't spell out that rule, so this test is the proof: the password contains both characters, so every server run proves the escaping, because the `api` sign-in fails if it is wrong (✓ `the_server_harness_signs_in_as_api_in_its_own_namespace`). Production never does this: the setup script creates `api` ([database 7](../../../docs/database/README.md#7-operations)).

## 3. The harness

The region between `// canonical: rust-testing/database.md TestDb` and `// end: TestDb` is the one harness; [`rust-surrealdb`, write-patterns.md](../rust-surrealdb/write-patterns.md#the-module) carries an exact copy, which `check_examples.py` compares line for line. In the backend it lives once in the store crate's `tests/embedded/support.rs` ([backend store 3](../../../docs/backend/store.md#3-module-layout); [Cargo book, integration tests](https://doc.rust-lang.org/cargo/reference/cargo-targets.html#integration-tests)), with `TEST_SCHEMA` replaced by the crate's `SCHEMA`: the `include_str!` path below is right for a top-level test file such as the checker's, and one level short from `tests/embedded/`.

```rust
use std::sync::Arc;

use surrealdb::types::{QueryError, RecordId};

// canonical: rust-testing/database.md TestDb
/// The real schema, backend/migrations/schema.surql, from a top-level test file
/// (`crates/<crate>/tests/*.rs`); the store crate's own harness uses its `SCHEMA`.
const TEST_SCHEMA: &str = include_str!("../../../migrations/schema.surql");

/// The `api` password in every server test namespace. The quote and backslash test
/// `surql_string`.
const TEST_API_PASS: &str = r"api-test-'pass'\1";

/// The four statements in schema.surql that select `pdm`/`main`.
const SCHEMA_HEADER: &str = "DEFINE NAMESPACE IF NOT EXISTS pdm;\nUSE NS pdm;\n\
DEFINE DATABASE IF NOT EXISTS main STRICT;\nUSE DB main;\n";

/// `schema` with its `pdm` header pointed at the namespace `ns`.
fn schema_in(schema: &str, ns: &str) -> String {
    assert!(
        schema.contains(SCHEMA_HEADER),
        "schema.surql no longer selects pdm/main with the four statements the harness rewrites"
    );
    let header = format!(
        "DEFINE NAMESPACE IF NOT EXISTS {ns};\nUSE NS {ns};\n\
         DEFINE DATABASE IF NOT EXISTS main STRICT;\nUSE DB main;\n"
    );
    schema.replacen(SCHEMA_HEADER, &header, 1)
}

/// A quoted string literal, for the one statement that takes no parameter: `DEFINE USER`.
fn surql_string(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// Where a server test's database is: the pinned image's URL and the test's namespace.
struct ServerNs {
    url: String,
    ns: String,
}

/// One test's database, with the schema applied and `main` selected.
struct TestDb {
    /// The embedded engine, or on the server a client signed in as `api` (database level,
    /// `ROLES EDITOR`), as the API runs.
    db: surrealdb::Surreal<surrealdb::engine::any::Any>,
    /// `Some` for [`TestDb::server`].
    server: Option<ServerNs>,
}

impl TestDb {
    /// The default: a new datastore of the embedded engine (`kv-mem`), shared with no other
    /// test, with the schema applied and `pdm`/`main` selected.
    async fn memory() -> Self {
        let db = surrealdb::engine::any::connect("mem://")
            .await
            .expect("start the embedded engine");
        let mut response = db.query(TEST_SCHEMA).await.expect("send the schema");
        let errors = response.take_errors();
        assert!(errors.is_empty(), "the schema failed: {errors:?}");
        db.use_ns("pdm")
            .use_db("main")
            .await
            .expect("select pdm/main");
        Self { db, server: None }
    }

    /// The pinned image at `PDM_TEST_SURREALDB_URL`, for what only the server has: a
    /// namespace of the test's own, the schema applied as root, an `api` user, and a client
    /// signed in as `api`.
    async fn server() -> Self {
        let url = std::env::var("PDM_TEST_SURREALDB_URL")
            .expect("start the test container and set PDM_TEST_SURREALDB_URL (database.md)");
        let ns = format!("test_{}", ulid::Ulid::generate().to_string().to_lowercase());
        let root = Self::signed_in_root(&url).await;
        let setup = format!(
            "{}\nDEFINE USER api ON DATABASE PASSWORD {} ROLES EDITOR;",
            schema_in(TEST_SCHEMA, &ns),
            surql_string(TEST_API_PASS)
        );
        let mut response = root.query(setup).await.expect("send the schema");
        let errors = response.take_errors();
        assert!(errors.is_empty(), "the schema failed in {ns}: {errors:?}");
        let db = Self::signed_in_api(&url, &ns).await;
        Self {
            db,
            server: Some(ServerNs { url, ns }),
        }
    }

    /// The server and namespace of a [`TestDb::server`].
    fn on_server(&self) -> &ServerNs {
        self.server
            .as_ref()
            .expect("only TestDb::server() has a server")
    }

    /// Another `api` connection to the same namespace, as a second API process would have.
    async fn api(&self) -> surrealdb::Surreal<surrealdb::engine::any::Any> {
        let server = self.on_server();
        Self::signed_in_api(&server.url, &server.ns).await
    }

    /// A root connection using the test's namespace, for what `api` may not do.
    async fn root(&self) -> surrealdb::Surreal<surrealdb::engine::any::Any> {
        let server = self.on_server();
        let root = Self::signed_in_root(&server.url).await;
        root.use_ns(&server.ns)
            .use_db("main")
            .await
            .expect("select the test namespace");
        root
    }

    async fn signed_in_root(url: &str) -> surrealdb::Surreal<surrealdb::engine::any::Any> {
        let root = surrealdb::engine::any::connect(url)
            .await
            .expect("connect to SurrealDB");
        root.signin(surrealdb::opt::auth::Root {
            username: "root".to_owned(),
            password: "root".to_owned(),
        })
        .await
        .expect("sign in as the test container's root");
        root
    }

    async fn signed_in_api(url: &str, ns: &str) -> surrealdb::Surreal<surrealdb::engine::any::Any> {
        let db = surrealdb::engine::any::connect(url)
            .await
            .expect("connect to SurrealDB");
        db.signin(surrealdb::opt::auth::Database {
            namespace: ns.to_owned(),
            database: "main".to_owned(),
            username: "api".to_owned(),
            password: TEST_API_PASS.to_owned(),
        })
        .await
        .expect("sign in as api");
        db.use_ns(ns)
            .use_db("main")
            .await
            .expect("select the test namespace");
        db
    }
}
// end: TestDb

const CREATE_U1: &str = "CREATE user:u1 SET auth_id = rand::ulid(), display_name = 'a'";

#[tokio::test]
async fn each_memory_db_is_its_own_datastore() {
    let first = TestDb::memory().await;
    let second = TestDb::memory().await;
    first.db.query(CREATE_U1).await.unwrap().check().unwrap();
    let again = first
        .db
        .query(CREATE_U1)
        .await
        .unwrap()
        .check()
        .unwrap_err();
    assert!(again.is_already_exists(), "user:u1 exists in the first");
    // The same record id is free in the second datastore.
    second.db.query(CREATE_U1).await.unwrap().check().unwrap();
}

#[tokio::test]
async fn the_engine_is_strict_and_parses_like_the_server() {
    let TestDb { db, .. } = TestDb::memory().await;
    let error = db
        .query("CREATE not_in_schema:x")
        .await
        .unwrap()
        .check()
        .unwrap_err();
    assert!(error.is_not_found(), "STRICT rejects an undefined table");

    let parse = db
        .query("DEFINE USER p ON DATABASE PASSWORD $p ROLES VIEWER")
        .bind(("p", "secret"))
        .await
        .unwrap_err();
    assert!(parse.is_validation(), "{}", parse.message());
    assert!(parse.message().contains("expected a strand"));
}

type Db = Arc<surrealdb::Surreal<surrealdb::engine::any::Any>>;

/// 40 concurrent `+= 1` transactions on one record; returns how many conflicted.
async fn concurrent_increments(db: &Db) -> i64 {
    const INCREMENT: &str = "BEGIN;
UPSERT type::record('usage_daily', [$user, '2026-10-16']) SET scans += 1;
COMMIT;";
    let mut tasks = Vec::new();
    for _ in 0..40 {
        let db = Arc::clone(db);
        tasks.push(tokio::spawn(async move {
            let mut response = db
                .query(INCREMENT)
                .bind(("user", RecordId::new("user", "u1")))
                .await
                .expect("send the increment");
            response
                .take_errors()
                .values()
                .any(|error| error.query_details() == Some(&QueryError::TransactionConflict))
        }));
    }
    let mut conflicts = 0;
    for task in tasks {
        conflicts += i64::from(task.await.expect("the task ran"));
    }
    conflicts
}

async fn scans(db: &Db) -> i64 {
    let mut response = db
        .query("SELECT VALUE scans FROM ONLY type::record('usage_daily', [user:u1, '2026-10-16'])")
        .await
        .expect("read the count");
    let scans: Option<i64> = response.take(0).expect("an int");
    scans.unwrap_or(0)
}

#[test]
fn conflicts_need_a_multi_thread_runtime() {
    let current = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (conflicts, total) = current.block_on(async {
        let db = Arc::new(TestDb::memory().await.db);
        (concurrent_increments(&db).await, scans(&db).await)
    });
    assert_eq!((conflicts, total), (0, 40), "one thread: never a conflict");

    let threads = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    let (conflicts, rounds, total) = threads.block_on(async {
        let db = Arc::new(TestDb::memory().await.db);
        // One round doesn't always interleave; stop at the first that does.
        let (mut conflicts, mut rounds) = (0, 0);
        while conflicts == 0 && rounds < 20 {
            conflicts = concurrent_increments(&db).await;
            rounds += 1;
        }
        (conflicts, rounds, scans(&db).await)
    });
    assert!(conflicts > 0, "four threads: some transactions conflict");
    assert_eq!(
        total,
        40 * rounds - conflicts,
        "a conflicted transaction committed nothing"
    );
}

#[tokio::test]
#[ignore = "needs SurrealDB"]
async fn the_server_harness_signs_in_as_api_in_its_own_namespace() {
    let first = TestDb::server().await;
    let second = TestDb::server().await;
    first.db.query(CREATE_U1).await.unwrap().check().unwrap();
    let again = first
        .db
        .query(CREATE_U1)
        .await
        .unwrap()
        .check()
        .unwrap_err();
    assert!(again.is_already_exists());
    second.db.query(CREATE_U1).await.unwrap().check().unwrap();

    let mut response = first
        .db
        .query("DEFINE TABLE OVERWRITE probe SCHEMALESS; DEFINE USER x ON DATABASE PASSWORD 'y' ROLES VIEWER;")
        .await
        .unwrap();
    let errors = response.take_errors();
    assert!(!errors.contains_key(&0), "an EDITOR may change the schema");
    assert!(errors[&1].is_not_allowed(), "but not define users");

    let ns = first.on_server().ns.clone();
    let mut response = first
        .db
        .query("RETURN [session::ns(), session::db()]")
        .await
        .unwrap();
    let session: Vec<String> = response.take(0).unwrap();
    assert_eq!(session, vec![ns.clone(), "main".to_owned()]);

    // A `USE` inside one query doesn't move the client's session for the next query.
    let root = first.root().await;
    let mut response = root
        .query("USE NS elsewhere; RETURN session::ns();")
        .await
        .unwrap();
    let inside: Option<String> = response.take(1).unwrap();
    assert_eq!(
        inside.as_deref(),
        Some("elsewhere"),
        "USE wins inside its query"
    );
    let mut response = root.query("RETURN session::ns()").await.unwrap();
    let after: Option<String> = response.take(0).unwrap();
    assert_eq!(after.as_deref(), Some(ns.as_str()));
}
```

- `.check()` turns the first failed statement into an `Err`; `.query()` alone returns `Ok` even when a statement failed ([database 6](../../../docs/database/README.md#6-rust-sdk); `IndexedResults::check` in `surrealdb-3.3.4/src/method/query.rs`, since docs.rs has no build of 3.3.4). The tests above use `.check()` on single statements only; inside `BEGIN … COMMIT` it returns a `NotExecuted` instead of the cause ([`rust-surrealdb`, queries](../rust-surrealdb/SKILL.md#queries)), and store code uses `run_tx` ([`rust-surrealdb`, errors and retries](../rust-surrealdb/errors-and-retries.md)).
- Assert the kind of error (`is_already_exists()`, `is_not_found()`, a `StoreError` variant), not just `is_err()`. This skill's choice: a test that accepts any error also passes when the query has a typo or the schema rejects it for another reason.
- One client per test is fine here; the "one client per process" rule ([database 6](../../../docs/database/README.md#6-rust-sdk)) is about the running API.

## 4. Running them

```bash
cargo nextest run -p store                        # the embedded-engine tests, no Docker
PDM_TEST_SURREALDB_URL=ws://127.0.0.1:18000 \
  cargo nextest run -p store --run-ignored only   # the server tests, container from section 2
```

The endpoint comes only from `PDM_TEST_SURREALDB_URL`, so a developer's own SurrealDB is never touched by accident. CI runs the embedded tests with the rest of the workspace, then starts the container, waits for `is-ready`, runs the ignored tests and removes the container (`--rm`). The full CI sequence is in [SKILL.md, section 7](SKILL.md#7-running-tests-locally-and-in-ci).

In the skills' checker, the embedded-engine tests run in every normal run, beside the other tests: they need nothing but the engine, which the checker builds anyway. `python3 .claude/skills/rust/scripts/check_examples.py --live` also starts the pinned image on a free port, runs every `#[ignore = "needs SurrealDB"]` test with `cargo nextest run --run-ignored only`, and stops the container.

Measured on 2026-10-16 (Apple silicon, Rust 1.99.1, the shared build cache warm): `--skill rust-surrealdb --skill rust-testing` takes 15 s (43 tests, the 15 on the embedded engine included; the write-pattern tests run in under 5 s), 21 s right after an edit to these files, and 17 s with `--live`; the full run takes 45 s (269 tests). The engine costs nothing extra once built, so its tests stay in the normal run. From cold, the engine is most of the dependency build: the probe crate with every pin and `kv-mem` built 621 crates in 1 min 39 s ([versions.md, compatibility checks](../rust/versions.md#compatibility-checks)), and on 2026-10-15 `kv-mem` alone took about 2 minutes and 3.4 GB of `target/` ([rust-surrealdb, tests](../rust-surrealdb/SKILL.md#tests)). A cold run of the checker itself wasn't timed: its cache is shared, and a second target directory doesn't fit the disk.
