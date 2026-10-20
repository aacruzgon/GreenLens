//! The schema and the migrations against the pinned server image. Ignored by default:
//!
//! ```sh
//! docker run -d --rm --name pdm-surreal -p 127.0.0.1:18000:8000 \
//!   surrealdb/surrealdb:v3.3.4@sha256:649a60a3… start --user root --pass root memory
//! PDM_TEST_SURREALDB_URL=ws://127.0.0.1:18000 cargo test -p store --test server -- --ignored
//! ```

use store::{Applied, Migration, SurrealStore, migrate_with};
use surrealdb::opt::auth::Root;

/// The knowledge loader's output for the fixture seed (`embedded.rs` loads the same copy).
const KNOWLEDGE: Migration = Migration {
    name: "knowledge.surql",
    sql: include_str!("embedded/knowledge_fixture.surql"),
};

#[tokio::test]
#[ignore = "needs SurrealDB"]
async fn migrations_apply_twice_on_the_server() {
    let url =
        std::env::var("PDM_TEST_SURREALDB_URL").unwrap_or_else(|_| "ws://127.0.0.1:18000".into());
    SurrealStore::migrate(&url, "root", "root")
        .await
        .expect("first application");
    let second = SurrealStore::migrate(&url, "root", "root")
        .await
        .expect("second application");
    assert_eq!(second, Applied::default(), "a second start applies nothing");

    // A knowledge file over WebSocket, as root: applied once, then skipped while unchanged.
    let root = surrealdb::engine::any::connect(url.as_str())
        .await
        .expect("connect");
    root.signin(Root {
        username: "root".into(),
        password: "root".into(),
    })
    .await
    .expect("sign in as root");
    let first = migrate_with(&root, &[KNOWLEDGE])
        .await
        .expect("the knowledge file");
    assert!(first.knowledge);
    let again = migrate_with(&root, &[KNOWLEDGE])
        .await
        .expect("the same file");
    assert_eq!(again, Applied::default());
    // Forget it again, so the database matches the committed files for the next run.
    let mut r = root
        .query("DELETE migration:`knowledge.surql`;")
        .await
        .expect("query sent");
    assert!(r.take_errors().is_empty());
}
