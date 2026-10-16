//! An in-memory `BlobStore` for tests, with `RustFS`'s rules: a put never overwrites (the
//! `If-None-Match: *` 412 is `AlreadyExists`), deleting a missing key is a no-op that counts as
//! deleted, and listing is by prefix in key order.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, PoisonError},
};

use bytes::Bytes;
use domain::{
    ports::{
        BlobError, BlobObject, BlobStore, Clock, ContentType, DeleteReport, ObjectInfo, ObjectKey,
        ObjectPrefix,
    },
    wire::Timestamp,
};
use futures::{StreamExt, stream};

#[derive(Debug, Clone)]
struct Stored {
    body: Bytes,
    content_type: ContentType,
    at: Timestamp,
}

/// The fake. Its `Mutex` is a `std` one: it is never held across an `.await`, and every method
/// takes it for a map operation only.
pub struct MemoryBlobStore {
    objects: Mutex<BTreeMap<ObjectKey, Stored>>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for MemoryBlobStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryBlobStore").finish_non_exhaustive()
    }
}

impl MemoryBlobStore {
    /// An empty store; `clock` dates each put, for the orphan sweep's age check.
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            objects: Mutex::new(BTreeMap::new()),
            clock,
        }
    }

    fn objects(&self) -> std::sync::MutexGuard<'_, BTreeMap<ObjectKey, Stored>> {
        // reason: a poisoned lock means a test already panicked; the map is still usable.
        self.objects.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn remove(&self, keys: &[ObjectKey]) -> DeleteReport {
        let mut objects = self.objects();
        for key in keys {
            objects.remove(key);
        }
        DeleteReport {
            deleted: keys.len() as u64,
            failed: Vec::new(),
        }
    }
}

#[async_trait::async_trait]
impl BlobStore for MemoryBlobStore {
    async fn put(
        &self,
        key: &ObjectKey,
        body: Bytes,
        content_type: ContentType,
    ) -> Result<(), BlobError> {
        let at = self.clock.now();
        let mut objects = self.objects();
        if objects.contains_key(key) {
            return Err(BlobError::AlreadyExists);
        }
        objects.insert(
            key.clone(),
            Stored {
                body,
                content_type,
                at,
            },
        );
        Ok(())
    }

    async fn get(&self, key: &ObjectKey) -> Result<BlobObject, BlobError> {
        let stored = self
            .objects()
            .get(key)
            .cloned()
            .ok_or(BlobError::NotFound)?;
        Ok(BlobObject {
            content_type: stored.content_type.as_str().to_owned(),
            length: Some(stored.body.len() as u64),
            etag: None,
            body: stream::iter([Ok(stored.body)]).boxed(),
        })
    }

    async fn get_bytes(&self, key: &ObjectKey) -> Result<Bytes, BlobError> {
        self.objects()
            .get(key)
            .map(|s| s.body.clone())
            .ok_or(BlobError::NotFound)
    }

    async fn delete_keys(&self, keys: &[ObjectKey]) -> Result<DeleteReport, BlobError> {
        Ok(self.remove(keys))
    }

    async fn delete_prefix(&self, prefix: &ObjectPrefix) -> Result<DeleteReport, BlobError> {
        let keys: Vec<ObjectKey> = self
            .objects()
            .keys()
            .filter(|k| k.is_under(prefix))
            .cloned()
            .collect();
        Ok(self.remove(&keys))
    }

    async fn list(&self, prefix: &ObjectPrefix) -> Result<Vec<ObjectInfo>, BlobError> {
        Ok(self
            .objects()
            .iter()
            .filter(|(k, _)| k.is_under(prefix))
            .map(|(k, s)| ObjectInfo {
                key: k.clone(),
                last_modified: s.at,
                size: s.body.len() as u64,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use domain::{
        id::{GardenId, ImageId, ScanId},
        testing::FixedClock,
    };
    use futures::TryStreamExt;

    use super::*;

    #[tokio::test]
    async fn puts_never_overwrite_and_gets_stream() {
        let store = MemoryBlobStore::new(Arc::new(FixedClock::at(
            "2026-10-18T14:00:00Z".parse().unwrap(),
        )));
        let g: GardenId = "01JAX3K9T6R2B8V0QZ4M7D1E5G".parse().unwrap();
        let s: ScanId = "01JAX3K9T6R2B8V0QZ4M7D1E5S".parse().unwrap();
        let i: ImageId = "01JAX3K9T6R2B8V0QZ4M7D1E5A".parse().unwrap();
        let key = ObjectKey::scan_image(g, s, i);
        store
            .put(&key, Bytes::from_static(b"webp"), ContentType::Webp)
            .await
            .unwrap();
        assert!(matches!(
            store
                .put(&key, Bytes::from_static(b"other"), ContentType::Webp)
                .await,
            Err(BlobError::AlreadyExists)
        ));
        let object = store.get(&key).await.unwrap();
        assert_eq!(object.content_type, "image/webp");
        let chunks: Vec<Bytes> = object.body.try_collect().await.unwrap();
        assert_eq!(chunks.concat(), b"webp");
        assert!(matches!(
            store.get_bytes(&ObjectKey::depth(g, s, i)).await,
            Err(BlobError::NotFound)
        ));
    }
}
