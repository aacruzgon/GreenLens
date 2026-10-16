//! Every boundary trait ([architecture 2], "a trait at every external boundary"): the leaf
//! crates implement them, the binaries wire them, and every other crate depends only on these.
//!
//! **Dispatch.** Runtime-swappable adapters are held as `Arc<dyn Trait>`, so every trait here
//! is dyn compatible and `Send + Sync`. A trait with a native `async fn` (or any
//! `-> impl Trait` method) is not dyn compatible on Rust 1.99.1 (`E0038`; Rust Reference,
//! "Dyn compatibility"), so async methods use `#[async_trait]`, which rewrites each into a
//! method returning `Pin<Box<dyn Future + Send + 'async_trait>>`. Synchronous ports (`Clock`,
//! `IdGen`, `UrlSigner`, `SpeciesPhotos`, `SpotSegmenter`, `WebhookVerifier`) stay plain. With
//! the `mock` feature, `mockall` generates a `Mock<Trait>` for every trait except `ChatModel`,
//! whose stream borrows its request.

mod auth;
mod blob;
mod errors;
mod knowledge;
mod models;
mod providers;
pub mod store;
mod system;

pub use auth::*;
pub use blob::*;
pub use errors::*;
pub use knowledge::*;
pub use models::*;
pub use providers::*;
pub use store::*;
pub use system::*;
