//! The daily orphan sweep (`scheduler::orphans::sweep`) over a `MemoryBlobStore` and the fake
//! store: it deletes objects older than an hour that no record points to, under live gardens
//! and under prefixes whose garden record is gone; it leaves a garden being deleted to its
//! cascade and never touches anything outside `gardens/`; and a garden that fails is skipped
//! while the rest are still swept. The key check itself (`UNREFERENCED_KEYS`) runs on the
//! embedded engine in `crates/store/tests/orphans.rs`.

use std::{collections::BTreeSet, sync::Arc, time::Duration};

use bytes::Bytes;
use domain::{
    garden::gardens::TimeZone,
    id::{GardenId, ImageId, ScanId},
    ports::{BlobStore, CellGardens, ContentType, ObjectKey, ObjectPrefix, StoreError},
    records::weather::{Coordinates, WeatherCell},
    testing::FixedClock,
    wire::Timestamp,
};

use super::{NOW, fake_store::FakeStore};
use crate::scheduler::orphans::{OrphanReport, sweep};

/// In key order, so the listing of `gardens/` answers them in this order.
const LIVE: &str = "01JAX3K9T6R2B8V0QZ4M7D1EA1";
const GONE: &str = "01JAX3K9T6R2B8V0QZ4M7D1EA2";
const DELETING: &str = "01JAX3K9T6R2B8V0QZ4M7D1EA3";
const SCAN_ID: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5S";

fn g(id: &str) -> GardenId {
    id.parse().unwrap()
}

fn image(n: u128) -> ImageId {
    ImageId::from_ulid(ulid::Ulid(n))
}

fn scan() -> ScanId {
    SCAN_ID.parse().unwrap()
}

fn t0() -> Timestamp {
    NOW.parse().unwrap()
}

/// Every garden in one cell.
fn cells(gardens: &[&str]) -> Vec<CellGardens> {
    vec![CellGardens {
        cell: WeatherCell::of(Coordinates {
            lat: 43.65,
            lon: -79.38,
        }),
        time_zone: TimeZone("America/Toronto".into()),
        gardens: gardens.iter().map(|id| g(id)).collect(),
    }]
}

/// A store whose `old` objects were put at `t0` and `young` ones two hours later; the sweep runs
/// a minute after that, so only the `old` ones are past the hour.
async fn blobs(old: &[ObjectKey], young: &[ObjectKey]) -> Arc<blob::MemoryBlobStore> {
    let clock = Arc::new(FixedClock::at(t0()));
    let store = Arc::new(blob::MemoryBlobStore::new(clock.clone()));
    for key in old {
        store
            .put(key, Bytes::new(), ContentType::Webp)
            .await
            .unwrap();
    }
    clock.advance(Duration::from_hours(2));
    for key in young {
        store
            .put(key, Bytes::new(), ContentType::Webp)
            .await
            .unwrap();
    }
    store
}

fn sweep_time() -> Timestamp {
    t0().plus(Duration::from_hours(2) + Duration::from_mins(1))
}

async fn keys_left(store: &blob::MemoryBlobStore) -> BTreeSet<ObjectKey> {
    let gardens = store.list(&ObjectPrefix::gardens()).await.unwrap();
    let knowledge = ObjectPrefix::from_listed("knowledge/species/").unwrap();
    let knowledge = store.list(&knowledge).await.unwrap();
    gardens
        .into_iter()
        .chain(knowledge)
        .map(|o| o.key)
        .collect()
}

/// The store answers every key but `referenced` as unreferenced.
fn referencing(store: &mut FakeStore, referenced: Vec<ObjectKey>) {
    store
        .deletion_repo
        .expect_unreferenced_keys()
        .returning(move |keys| {
            Ok(keys
                .iter()
                .filter(|k| !referenced.contains(k))
                .cloned()
                .collect())
        });
}

#[tokio::test]
async fn the_sweep_deletes_old_orphans_of_live_and_gone_gardens_only() {
    let (s, live, gone, deleting) = (scan(), g(LIVE), g(GONE), g(DELETING));
    let kept_photo = ObjectKey::scan_image(live, s, image(1));
    let kept_depth = ObjectKey::depth(live, s, image(1));
    let orphan = ObjectKey::scan_image(live, s, image(2));
    let fresh = ObjectKey::scan_image(live, s, image(3));
    let gone_old = ObjectKey::mask(gone, s, domain::id::MaskId::from_ulid(ulid::Ulid(4)));
    let gone_fresh = ObjectKey::scan_image(gone, s, image(5));
    let deleting_old = ObjectKey::scan_image(deleting, s, image(6));
    let not_a_garden = ObjectKey::from_stored("gardens/not-a-garden/x.webp").unwrap();
    let knowledge = ObjectKey::from_stored("knowledge/species/tomato/x.webp").unwrap();
    let blob = blobs(
        &[
            kept_photo.clone(),
            kept_depth.clone(),
            orphan.clone(),
            gone_old.clone(),
            deleting_old.clone(),
            not_a_garden.clone(),
            knowledge.clone(),
        ],
        &[fresh.clone(), gone_fresh.clone()],
    )
    .await;

    let mut store = FakeStore::default();
    store
        .weather_cache_repo
        .expect_gardens_by_cell()
        .returning(|| Ok(cells(&[LIVE])));
    store
        .garden_repo
        .expect_existing_gardens()
        .withf(|gardens| gardens == [g(GONE), g(DELETING)])
        .times(1)
        .returning(|_| Ok(vec![g(DELETING)]));
    referencing(&mut store, vec![kept_photo.clone(), kept_depth.clone()]);

    let deletes = blob::DeleteQueue::new(blob.clone());
    let report = sweep(&store, blob.as_ref(), &deletes, sweep_time())
        .await
        .expect("the sweep runs");

    assert_eq!(
        keys_left(&blob).await,
        BTreeSet::from([
            kept_photo,
            kept_depth,
            fresh,
            gone_fresh,
            deleting_old,
            not_a_garden,
            knowledge
        ]),
        "the live garden's old orphan and the gone garden's old object are deleted"
    );
    assert_eq!(
        report,
        OrphanReport {
            gardens: 2,
            gone_gardens: 1,
            listed: 6,
            candidates: 4,
            orphans: 2,
            deleted: 2,
            not_deleted: 0,
            failed: 0,
        }
    );
    deletes.drain().await;
}

#[tokio::test]
async fn a_garden_that_fails_is_skipped_and_the_rest_still_swept() {
    let (s, a, b) = (scan(), g(LIVE), g(GONE));
    let a_orphan = ObjectKey::scan_image(a, s, image(1));
    let b_orphan = ObjectKey::scan_image(b, s, image(2));
    let blob = blobs(&[a_orphan.clone(), b_orphan.clone()], &[]).await;

    let mut store = FakeStore::default();
    store
        .weather_cache_repo
        .expect_gardens_by_cell()
        .returning(|| Ok(cells(&[LIVE, GONE])));
    store.garden_repo.expect_existing_gardens().never();
    let failing = a_orphan.clone();
    store
        .deletion_repo
        .expect_unreferenced_keys()
        .returning(move |keys| {
            if keys.contains(&failing) {
                Err(StoreError::Unavailable("database down".into()))
            } else {
                Ok(keys.to_vec())
            }
        });

    let deletes = blob::DeleteQueue::new(blob.clone());
    let report = sweep(&store, blob.as_ref(), &deletes, sweep_time())
        .await
        .expect("one garden's failure doesn't fail the run");

    assert_eq!(keys_left(&blob).await, BTreeSet::from([a_orphan]));
    assert_eq!(
        (report.gardens, report.failed, report.deleted),
        (2, 1, 1),
        "the failed garden is counted, the next one is swept"
    );
    deletes.drain().await;
}

#[tokio::test]
async fn a_failed_garden_list_fails_the_run() {
    let blob = blobs(&[], &[]).await;
    let mut store = FakeStore::default();
    store
        .weather_cache_repo
        .expect_gardens_by_cell()
        .returning(|| Err(StoreError::Unavailable("database down".into())));
    let deletes = blob::DeleteQueue::new(blob.clone());
    assert!(
        sweep(&store, blob.as_ref(), &deletes, sweep_time())
            .await
            .is_err()
    );
}
