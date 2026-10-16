//! The PDM `store` crate: `SurrealStore`, which implements every repository trait of
//! `domain::ports::store` (and so `Store`) and `MemoryStore` over `SurrealDB` 3.3.4
//! ([database](https://github.com/aacruzgon/GreenLens/blob/main/docs/database/README.md)).
//!
//! - [`client`]: connecting, signing in as `api`, the embedded schema.
//! - [`tx`]: `run_tx`, one query per transaction with the conflict retry.
//! - [`error`]: SDK errors to `StoreError`.
//! - [`ids`]: `Id<T>` and `Slug<K>` ↔ `RecordId`, array keys included.
//! - [`rows`]: the literal-union row enums and JSON/time helpers.
//! - [`repo`]: one module per repository area, each with its statements in a `sql` module.

pub mod client;
pub mod error;
mod fragments;
pub mod ids;
pub mod read;
pub mod repo;
pub mod rows;
pub mod tx;

pub use client::{SCHEMA, StoreConfig, SurrealStore, apply_schema_with};

/// `SurrealStore` implements every repository (so `Store`, by its blanket impl) and
/// `MemoryStore`, and can live in `Arc<dyn Store>` across tokio's threads.
const _: () = {
    const fn assert_store<
        T: domain::ports::Store + domain::ports::MemoryStore + Send + Sync + 'static,
    >() {
    }
    assert_store::<SurrealStore>();
};
