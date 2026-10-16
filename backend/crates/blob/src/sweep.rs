//! The orphan sweep's listing side ([storage 4]: "A daily sweep lists each garden's prefix and
//! deletes objects older than an hour that no record points to").
//!
//! The sweep is three steps across three crates: this module lists a garden's objects older than
//! the cutoff; the store's `DeletionRepo::unreferenced_keys` keeps those no record points to
//! (every key field counts, depth maps included); the [`crate::DeleteQueue`] deletes them. The
//! `knowledge/` prefix is never swept: the knowledge loader owns it.

use std::time::Duration;

use domain::{
    id::GardenId,
    ports::{BlobError, BlobStore, ObjectKey, ObjectPrefix},
    wire::Timestamp,
};

/// Objects younger than this are never candidates: an upload writes the object before its
/// record (storage 4), so a fresh object without a record may be mid-upload.
pub const ORPHAN_MIN_AGE: Duration = Duration::from_hours(1);

/// The garden's objects last modified before `now − 1 hour`, in key order.
pub async fn orphan_candidates(
    blob: &dyn BlobStore,
    garden: GardenId,
    now: Timestamp,
) -> Result<Vec<ObjectKey>, BlobError> {
    let cutoff = now.minus(ORPHAN_MIN_AGE);
    Ok(blob
        .list(&ObjectPrefix::garden(garden))
        .await?
        .into_iter()
        .filter(|o| o.last_modified < cutoff)
        .map(|o| o.key)
        .collect())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bytes::Bytes;
    use domain::{
        id::{ImageId, ScanId},
        ports::ContentType,
        testing::FixedClock,
    };

    use super::*;
    use crate::memory::MemoryBlobStore;

    #[tokio::test]
    async fn only_objects_older_than_an_hour_are_candidates() {
        let t0: Timestamp = "2026-10-18T14:00:00Z".parse().unwrap();
        let clock = Arc::new(FixedClock::at(t0));
        let store = MemoryBlobStore::new(clock.clone());
        let g: GardenId = "01JAX3K9T6R2B8V0QZ4M7D1E5G".parse().unwrap();
        let s: ScanId = "01JAX3K9T6R2B8V0QZ4M7D1E5S".parse().unwrap();
        let old: ImageId = "01JAX3K9T6R2B8V0QZ4M7D1E5A".parse().unwrap();
        let new: ImageId = "01JAX3K9T6R2B8V0QZ4M7D1E5B".parse().unwrap();
        store
            .put(
                &ObjectKey::scan_image(g, s, old),
                Bytes::new(),
                ContentType::Webp,
            )
            .await
            .unwrap();
        clock.advance(Duration::from_hours(2));
        store
            .put(
                &ObjectKey::scan_image(g, s, new),
                Bytes::new(),
                ContentType::Webp,
            )
            .await
            .unwrap();
        let now = t0.plus(Duration::from_hours(2) + Duration::from_mins(1));
        let candidates = orphan_candidates(&store, g, now).await.unwrap();
        assert_eq!(candidates, vec![ObjectKey::scan_image(g, s, old)]);
    }
}
