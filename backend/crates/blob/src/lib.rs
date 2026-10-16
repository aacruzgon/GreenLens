//! The PDM `blob` crate: [`S3BlobStore`], the `domain::ports::BlobStore` over `RustFS` 1.0.1
//! with `aws-sdk-s3` ([storage](https://github.com/aacruzgon/GreenLens/blob/main/docs/storage/README.md)),
//! the [`DeleteQueue`] that keeps deletes from ever being cancelled (storage 4), an in-memory
//! [`MemoryBlobStore`] for tests, and the orphan sweep's listing side. No AWS type appears in
//! the public API: errors are `BlobError` and bodies are the domain's `ByteStream`.

pub mod client;
pub mod delete;
mod error;
pub mod memory;
pub mod s3;
pub mod sweep;

pub use client::{BlobConfig, storage_client};
pub use delete::{DeleteHandle, DeleteJob, DeleteOutcome, DeleteQueue};
pub use memory::MemoryBlobStore;
pub use s3::S3BlobStore;
pub use sweep::orphan_candidates;
