//! Object storage ([storage 2], [storage 4], [storage 5]): the key layout and the
//! `BlobStore` port the `blob` crate implements over `RustFS`.

use std::fmt;

use bytes::Bytes;
use futures::stream::BoxStream;

use crate::{
    id::{ChatImageId, ConversationId, GardenId, ImageId, MaskId, ScanId, SpeciesId},
    ports::BlobError,
    url::PhotoHash,
    wire::Timestamp,
};

/// An object key, built only from typed ids, so every segment under `gardens/` is a
/// 26-character ULID and never user input ([storage 2]).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectKey(String);

impl ObjectKey {
    /// `gardens/{garden}/scans/{scan}/{image}.webp`
    pub fn scan_image(garden: GardenId, scan: ScanId, image: ImageId) -> Self {
        Self(format!("gardens/{garden}/scans/{scan}/{image}.webp"))
    }

    /// `gardens/{garden}/scans/{scan}/masks/{mask}.png`
    pub fn mask(garden: GardenId, scan: ScanId, mask: MaskId) -> Self {
        Self(format!("gardens/{garden}/scans/{scan}/masks/{mask}.png"))
    }

    /// `gardens/{garden}/scans/{scan}/depth/{image}.png`: never served.
    pub fn depth(garden: GardenId, scan: ScanId, image: ImageId) -> Self {
        Self(format!("gardens/{garden}/scans/{scan}/depth/{image}.png"))
    }

    /// `gardens/{garden}/scans/{scan}/depth_confidence/{image}.png`: never served.
    pub fn depth_confidence(garden: GardenId, scan: ScanId, image: ImageId) -> Self {
        Self(format!(
            "gardens/{garden}/scans/{scan}/depth_confidence/{image}.png"
        ))
    }

    /// `gardens/{garden}/chat/{conversation}/{image}.webp`
    pub fn chat_image(garden: GardenId, conversation: ConversationId, image: ChatImageId) -> Self {
        Self(format!("gardens/{garden}/chat/{conversation}/{image}.webp"))
    }

    /// `knowledge/species/{species}/{hash}.webp`: written by the knowledge loader only.
    pub fn species_photo(species: &SpeciesId, hash: PhotoHash) -> Self {
        Self(format!("knowledge/species/{species}/{hash}.webp"))
    }

    /// A key read back from a record (`scan_image.storage_key` and the like). Accepts only the
    /// two top-level prefixes and no empty, `.` or `..` segment.
    pub fn from_stored(key: &str) -> Option<Self> {
        let valid_root = key.starts_with("gardens/") || key.starts_with("knowledge/");
        let valid_segments = key
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..");
        (valid_root && valid_segments).then(|| Self(key.to_owned()))
    }

    /// The key.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether the key is under `prefix`.
    pub fn is_under(&self, prefix: &ObjectPrefix) -> bool {
        self.0.starts_with(&prefix.0)
    }
}

impl fmt::Display for ObjectKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A key prefix, ending in `/`, for listing and prefix deletes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectPrefix(String);

impl ObjectPrefix {
    /// Everything of a garden: deleting a garden is one prefix delete.
    pub fn garden(garden: GardenId) -> Self {
        Self(format!("gardens/{garden}/"))
    }

    /// Everything of a scan: a plant's objects are deleted scan by scan.
    pub fn scan(garden: GardenId, scan: ScanId) -> Self {
        Self(format!("gardens/{garden}/scans/{scan}/"))
    }

    /// A conversation's images.
    pub fn chat(garden: GardenId, conversation: ConversationId) -> Self {
        Self(format!("gardens/{garden}/chat/{conversation}/"))
    }

    /// The prefix.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The content types the API stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentType {
    /// Photos.
    Webp,
    /// Masks and depth maps.
    Png,
}

impl ContentType {
    /// The MIME type.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Webp => "image/webp",
            Self::Png => "image/png",
        }
    }
}

/// An object body as a stream of chunks. `'static` and `Send` so `api` can hand it to the
/// response body and move it across tasks; independent of `aws_smithy_types::ByteStream` so no
/// crate but `blob` sees AWS types.
pub type ByteStream = BoxStream<'static, Result<Bytes, BlobError>>;

/// A fetched object: its metadata and its streaming body.
pub struct BlobObject {
    /// `Content-Type`.
    pub content_type: String,
    /// `Content-Length`, when known.
    pub length: Option<u64>,
    /// `ETag`, sent to the client.
    pub etag: Option<String>,
    /// The body, streamed, never buffered.
    pub body: ByteStream,
}

impl fmt::Debug for BlobObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlobObject")
            .field("content_type", &self.content_type)
            .field("length", &self.length)
            .field("etag", &self.etag)
            .finish_non_exhaustive()
    }
}

/// A listed object, for the orphan sweep ([storage 4]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectInfo {
    /// Its key.
    pub key: ObjectKey,
    /// When it was written.
    pub last_modified: Timestamp,
    /// Its size.
    pub size: u64,
}

/// What a batch delete removed and what it couldn't ([storage 4], deletes: check the `Errors`
/// list of every `DeleteObjects` response).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeleteReport {
    /// Keys deleted (a missing key counts: deleting it is a no-op).
    pub deleted: u64,
    /// Keys `RustFS` reported as failed; the caller retries.
    pub failed: Vec<ObjectKey>,
}

/// The object store ([storage 5]: "a small `ObjectStore` trait (`put`, `get_stream`,
/// `delete_prefix`)"). Deletes must never be cancelled mid-call on `RustFS` 1.0.1 ([storage 4]):
/// callers run them in a detached task with a 120-second timeout, and implementations don't
/// race them against anything.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait BlobStore: Send + Sync {
    /// `PutObject` with `If-None-Match: *`: an existing key is [`BlobError::AlreadyExists`].
    async fn put(
        &self,
        key: &ObjectKey,
        body: Bytes,
        content_type: ContentType,
    ) -> Result<(), BlobError>;

    /// `GetObject`, streamed.
    async fn get(&self, key: &ObjectKey) -> Result<BlobObject, BlobError>;

    /// `GetObject`, buffered: for the measurement stage's depth maps and the face blur, which
    /// need the whole image.
    async fn get_bytes(&self, key: &ObjectKey) -> Result<Bytes, BlobError>;

    /// `DeleteObjects` in batches of up to 1000.
    async fn delete_keys(&self, keys: &[ObjectKey]) -> Result<DeleteReport, BlobError>;

    /// `ListObjectsV2` (paginated), then `DeleteObjects` in batches of up to 1000.
    async fn delete_prefix(&self, prefix: &ObjectPrefix) -> Result<DeleteReport, BlobError>;

    /// `ListObjectsV2`, every page, for the daily orphan sweep.
    async fn list(&self, prefix: &ObjectPrefix) -> Result<Vec<ObjectInfo>, BlobError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_follow_the_layout() {
        let g: GardenId = "01JAX3K9T6R2B8V0QZ4M7D1E5G".parse().unwrap();
        let s: ScanId = "01JAX3K9T6R2B8V0QZ4M7D1E5S".parse().unwrap();
        let i: ImageId = "01JAX3K9T6R2B8V0QZ4M7D1E5A".parse().unwrap();
        let key = ObjectKey::depth(g, s, i);
        assert_eq!(
            key.as_str(),
            "gardens/01JAX3K9T6R2B8V0QZ4M7D1E5G/scans/01JAX3K9T6R2B8V0QZ4M7D1E5S/depth/01JAX3K9T6R2B8V0QZ4M7D1E5A.png"
        );
        assert!(key.is_under(&ObjectPrefix::scan(g, s)));
        assert!(ObjectKey::from_stored("gardens//scans/x").is_none());
        assert!(ObjectKey::from_stored("gardens/../x").is_none());
        assert!(ObjectKey::from_stored("other/x").is_none());
    }
}
