//! The schema against the pinned server image. Ignored by default:
//!
//! ```sh
//! docker run -d --rm --name pdm-surreal -p 127.0.0.1:18000:8000 \
//!   surrealdb/surrealdb:v3.3.4@sha256:649a60a3… start --user root --pass root memory
//! PDM_TEST_SURREALDB_URL=ws://127.0.0.1:18000 cargo test -p store --test server -- --ignored
//! ```

use store::SurrealStore;

#[tokio::test]
#[ignore = "needs SurrealDB"]
async fn schema_applies_twice_on_the_server() {
    let url =
        std::env::var("PDM_TEST_SURREALDB_URL").unwrap_or_else(|_| "ws://127.0.0.1:18000".into());
    SurrealStore::apply_schema(&url, "root", "root")
        .await
        .expect("first application");
    SurrealStore::apply_schema(&url, "root", "root")
        .await
        .expect("second application");
}
