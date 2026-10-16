# Database tests

Part of [`rust-testing`](SKILL.md). Store code is tested against the pinned SurrealDB image, never a mock or a fake ([database 1](../../../docs/database/README.md#1-version-and-setup): "`memory` is for tests"). The store patterns themselves, and what each query must prove, are in [`rust-surrealdb`](../rust-surrealdb/SKILL.md); this file is the harness: the container, one namespace per test, the schema, the `api` user and how CI runs them.

## The container

Same image and digest as production, with the in-memory engine:

```bash
docker run --rm -d --name pdm-test-surrealdb -p 127.0.0.1:18000:8000 \
  surrealdb/surrealdb:v3.3.4@sha256:649a60a3b9c0177d9923c938e1c2563e48cde2a9d06ac2c0519714be719b02af \
  start --user root --pass root memory
until docker exec pdm-test-surrealdb /surreal is-ready --endpoint http://127.0.0.1:8000; do sleep 0.5; done
export PDM_TEST_SURREALDB_URL=ws://127.0.0.1:18000
```

✓ The image starts in about a second and `is-ready` works in it (it is distroless, so there is no `curl`; [database 1](../../../docs/database/README.md#1-version-and-setup)). `root`/`root` are this throwaway container's credentials, not a secret. The `memory` store disappears with the container, so tests never clean up after themselves.

## One namespace per test

Every test builds a `TestDb`: a fresh namespace, `test_<ulid>`, with the whole schema applied in it as root, an `api` user defined in its database, and a client signed in as that `api`. Tests then run in parallel, in any order, as `cargo test` threads or nextest processes, and a test may assume its tables start empty.

Why each test sets up its own, and not once per test binary:

- **nextest runs every test in its own process** ([nextest, why process-per-test](https://nexte.st/docs/design/why-process-per-test/)). A `static` `OnceCell` is per process, so "once per binary" means once per test, all at the same moment. ✓ An earlier harness applied the schema once into a shared `pdm`/`main`: under `cargo nextest run --run-ignored only` (nextest 0.9.148), all 9 tests failed in that step with `TransactionConflict` ("Write conflict, retry the transaction"), nine processes redefining the same tables at once. Separate namespaces don't conflict: ✓ the 13 live tests in this skill and [`rust-surrealdb`](../rust-surrealdb/write-patterns.md#live-tests) pass together under nextest, each applying the schema into its own namespace.
- **Shared tables leak between tests.** The job claim takes the oldest queued job ([database 4, job claim](../../../docs/database/README.md#job-claim-with-slots-)), so two tests sharing one database claim each other's jobs.
- **The schema is embedded, not read from a path.** `include_str!` resolves against the source file at compile time ([`include_str!`](https://doc.rust-lang.org/std/macro.include_str.html)), so the test doesn't depend on the directory it runs from. A path read at run time broke when nextest ran the binary from the crate's directory. In the repository, `backend/crates/store/tests/*.rs` reaches `backend/migrations/schema.surql` with `../../../migrations/schema.surql` ([architecture 13](../../../docs/architecture/README.md#13-repository-layout)).
- **Tests sign in as `api`, not root**, so they run with the API's permissions: `api` is a database-level user with `ROLES EDITOR` ([database 1, users](../../../docs/database/README.md#1-version-and-setup)). A test that ran as root would pass a query `api` isn't allowed to run.

How the schema is pointed at the test's namespace. `schema.surql` defines and selects its own namespace and database with four statements after its header comment (`DEFINE NAMESPACE IF NOT EXISTS pdm; USE NS pdm; DEFINE DATABASE IF NOT EXISTS main STRICT; USE DB main;`, [database 1](../../../docs/database/README.md#1-version-and-setup)), and a `USE` inside a query wins over the client's `use_ns` for the rest of that query (✓ `USE NS elsewhere; RETURN session::ns()` gives `elsewhere`, and the client's next query is back in its own namespace). So `schema_in` replaces the first occurrence of those four lines with the test's namespace, and panics if the file no longer contains them. It checks with `contains`, not `starts_with`: in the real file the header comment takes the first 24 lines.

`DEFINE USER … PASSWORD` takes a string literal, never a parameter: ✓ `PASSWORD $p` is a parse error ("Unexpected token \`a parameter\`, expected a strand") whether `$p` is bound or set with `LET`, and so is `PASSHASH $h` ([`DEFINE USER`](https://surrealdb.com/docs/reference/query-language/statements/define/user)). So the harness writes the password into the statement with `surql_string`, which escapes `\` and `'` inside a single-quoted string ([strings](https://surrealdb.com/docs/reference/query-language/language-primitives/data-types/strings)). The test password contains both characters, so every run proves the escaping: the `api` sign-in fails if it is wrong. Production never does this: the setup script creates `api` ([database 7](../../../docs/database/README.md#7-operations)).

```rust
// canonical: rust-testing/database.md TestDb
/// The real schema, embedded as the store crate embeds it (backend/migrations/schema.surql).
const TEST_SCHEMA: &str = include_str!("../../../migrations/schema.surql");

/// The `api` password in every test namespace. The quote and backslash test `surql_string`.
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

/// One test's database: its own namespace holding the schema and an `api` user.
struct TestDb {
    url: String,
    ns: String,
    /// Signed in as `api` (database level, `ROLES EDITOR`), as the API runs.
    db: surrealdb::Surreal<surrealdb::engine::any::Any>,
}

impl TestDb {
    async fn new() -> Self {
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
        Self { url, ns, db }
    }

    /// Another `api` connection to the same namespace, as a second API process would have.
    async fn api(&self) -> surrealdb::Surreal<surrealdb::engine::any::Any> {
        Self::signed_in_api(&self.url, &self.ns).await
    }

    /// A root connection using the test's namespace, for what `api` may not do.
    async fn root(&self) -> surrealdb::Surreal<surrealdb::engine::any::Any> {
        let root = Self::signed_in_root(&self.url).await;
        root.use_ns(&self.ns)
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

#[tokio::test]
#[ignore = "needs SurrealDB"]
async fn each_test_has_its_own_tables() {
    let first = TestDb::new().await;
    let second = TestDb::new().await;
    let create = "CREATE user:u1 SET auth_id = rand::ulid(), display_name = 'a'";
    first.db.query(create).await.unwrap().check().unwrap();
    let again = first.db.query(create).await.unwrap().check().unwrap_err();
    assert!(
        again.is_already_exists(),
        "user:u1 exists in the first namespace"
    );
    // The same record id is free in the second namespace.
    second.db.query(create).await.unwrap().check().unwrap();
}

#[tokio::test]
#[ignore = "needs SurrealDB"]
async fn the_database_is_strict() {
    let TestDb { db, .. } = TestDb::new().await;
    let error = db
        .query("CREATE not_in_schema:x")
        .await
        .unwrap()
        .check()
        .unwrap_err();
    assert!(error.is_not_found(), "STRICT rejects an undefined table");
}

#[tokio::test]
#[ignore = "needs SurrealDB"]
async fn api_is_an_editor_and_root_is_not_needed_for_data() {
    let t = TestDb::new().await;
    let mut response = t
        .db
        .query("DEFINE TABLE OVERWRITE probe SCHEMALESS; DEFINE USER x ON DATABASE PASSWORD 'y' ROLES VIEWER;")
        .await
        .unwrap();
    let errors = response.take_errors();
    assert!(!errors.contains_key(&0), "an EDITOR may change the schema");
    assert!(errors[&1].is_not_allowed(), "but not define users");

    let mut response =
        t.db.query("RETURN [session::ns(), session::db()]")
            .await
            .unwrap();
    let session: Vec<String> = response.take(0).unwrap();
    assert_eq!(session, vec![t.ns.clone(), "main".to_owned()]);

    // A `USE` inside one query doesn't move the client's session for the next query.
    let root = t.root().await;
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
    let ns: Option<String> = response.take(0).unwrap();
    assert_eq!(ns.as_deref(), Some(t.ns.as_str()));

    let parse = root
        .query("DEFINE USER p ON DATABASE PASSWORD $p ROLES VIEWER")
        .bind(("p", "secret"))
        .await
        .unwrap_err();
    assert!(parse.is_validation(), "PASSWORD takes a literal");
}
```

- `.check()` turns the first failed statement into an `Err`; `.query()` alone returns `Ok` even when a statement failed ([database 6](../../../docs/database/README.md#6-rust-sdk); `IndexedResults::check` in `surrealdb-3.3.4/src/method/query.rs`, since docs.rs has no build of 3.3.4). The tests above use `.check()` on single statements only; inside `BEGIN … COMMIT` it returns a `NotExecuted` instead of the cause ([`rust-surrealdb`, queries](../rust-surrealdb/SKILL.md#queries)).
- Assert the kind of error (`is_already_exists()`, `is_not_found()`, a `StoreError` variant), not just `is_err()`. This skill's choice: a test that accepts any error also passes when the query has a typo or the schema rejects it for another reason.
- One client per test is fine here; the "one client per process" rule ([database 6](../../../docs/database/README.md#6-rust-sdk)) is about the running API.
- Paused time doesn't apply: these tests wait on a real socket.

The region between `// canonical: rust-testing/database.md TestDb` and `// end: TestDb` is the one harness; [`rust-surrealdb`, write-patterns.md](../rust-surrealdb/write-patterns.md#the-module) carries an exact copy, which `check_examples.py` compares line for line. In the backend it lives once in the store crate's `tests/common/mod.rs` ([Cargo book, integration tests](https://doc.rust-lang.org/cargo/reference/cargo-targets.html#integration-tests)).

## Running them

```bash
cargo test -p store -- --ignored                     # cargo test
cargo nextest run -p store --run-ignored only        # nextest
```

The endpoint comes only from `PDM_TEST_SURREALDB_URL`, so a developer's own SurrealDB is never touched by accident. CI starts the container, waits for `is-ready`, runs the ignored tests, and removes the container (`--rm`). The full CI sequence is in [SKILL.md, section 7](SKILL.md#7-running-tests-locally-and-in-ci).

`python3 .claude/skills/rust/scripts/check_examples.py --live` does the same for the skills' own tests: it starts the pinned image on a free port, runs every `#[ignore = "needs SurrealDB"]` test with `cargo nextest run --run-ignored only`, and stops the container.
