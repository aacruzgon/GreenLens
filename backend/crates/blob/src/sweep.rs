//! The orphan sweep's listing side ([storage 4]: "A daily sweep lists each garden's prefix and
//! deletes objects older than an hour that no record points to").
//!
//! The sweep is three steps across three crates: [`orphan_candidates`] lists a garden's objects
//! older than [`ORPHAN_MIN_AGE`]; the store's `DeletionRepo::unreferenced_keys` keeps those no
//! record points to (every key field counts, depth maps included); the [`crate::DeleteQueue`]
//! deletes them. The gardens swept are the live ones and, through [`garden_prefixes`], every
//! prefix under `gardens/` whose garden has no record any more, such as an upload's object that
//! landed after its garden's cascade deleted the prefix. Nothing outside `gardens/` is swept:
//! the knowledge loader owns `knowledge/`.

use std::time::Duration;

use domain::{
    id::GardenId,
    ports::{BlobError, BlobStore, ObjectKey, ObjectPrefix},
    wire::Timestamp,
};

/// Objects younger than this are never candidates: an upload writes the object before its
/// record (storage 4), so a fresh object without a record may be mid-upload.
pub const ORPHAN_MIN_AGE: Duration = Duration::from_hours(1);

/// One garden's listing, for the sweep and its log line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Candidates {
    /// Objects listed under the garden's prefix.
    pub listed: usize,
    /// Those last modified before `now − ORPHAN_MIN_AGE`, in key order.
    pub old: Vec<ObjectKey>,
}

/// The garden's objects, and those last modified before `now − 1 hour`.
pub async fn orphan_candidates(
    blob: &dyn BlobStore,
    garden: GardenId,
    now: Timestamp,
) -> Result<Candidates, BlobError> {
    let cutoff = now.minus(ORPHAN_MIN_AGE);
    let objects = blob.list(&ObjectPrefix::garden(garden)).await?;
    Ok(Candidates {
        listed: objects.len(),
        old: objects
            .into_iter()
            .filter(|o| o.last_modified < cutoff)
            .map(|o| o.key)
            .collect(),
    })
}

/// Every garden with a prefix under `gardens/` (a delimiter listing, no object read), live or
/// not. A prefix whose segment isn't a garden id is skipped, never swept.
pub async fn garden_prefixes(blob: &dyn BlobStore) -> Result<Vec<GardenId>, BlobError> {
    Ok(blob
        .list_prefixes(&ObjectPrefix::gardens())
        .await?
        .iter()
        .filter_map(ObjectPrefix::garden_id)
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
        assert_eq!(
            candidates,
            Candidates {
                listed: 2,
                old: vec![ObjectKey::scan_image(g, s, old)],
            }
        );
    }

    #[tokio::test]
    async fn garden_prefixes_are_the_gardens_under_gardens_only() {
        let store = MemoryBlobStore::new(Arc::new(FixedClock::at(
            "2026-10-18T14:00:00Z".parse().unwrap(),
        )));
        let a: GardenId = "01JAX3K9T6R2B8V0QZ4M7D1E5A".parse().unwrap();
        let b: GardenId = "01JAX3K9T6R2B8V0QZ4M7D1E5B".parse().unwrap();
        let s: ScanId = "01JAX3K9T6R2B8V0QZ4M7D1E5S".parse().unwrap();
        let i: ImageId = "01JAX3K9T6R2B8V0QZ4M7D1E5C".parse().unwrap();
        let keys = [
            ObjectKey::scan_image(a, s, i),
            ObjectKey::depth(a, s, i),
            ObjectKey::scan_image(b, s, i),
            ObjectKey::from_stored("gardens/not-a-garden/x.webp").unwrap(),
            ObjectKey::from_stored("knowledge/species/tomato/x.webp").unwrap(),
        ];
        for key in &keys {
            store
                .put(key, Bytes::new(), ContentType::Webp)
                .await
                .unwrap();
        }
        assert_eq!(garden_prefixes(&store).await.unwrap(), vec![a, b]);
    }
}
