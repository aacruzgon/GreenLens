//! `S3BlobStore` against the pinned `rustfs/rustfs:1.0.1` image. Ignored by default:
//!
//! ```sh
//! docker run -d --rm --name pdm-rustfs -p 127.0.0.1:19000:9000 \
//!   -e RUSTFS_ACCESS_KEY=TESTROOT -e RUSTFS_SECRET_KEY=testrootsecret \
//!   -e RUSTFS_CONSOLE_ENABLE=false -e RUSTFS_VOLUMES=/data rustfs/rustfs:1.0.1@sha256:1803faef…
//! PDM_TEST_RUSTFS_URL=http://127.0.0.1:19000 cargo test -p blob --test rustfs -- --ignored
//! ```

use std::sync::Arc;

use blob::{BlobConfig, DeleteJob, DeleteOutcome, DeleteQueue, S3BlobStore, storage_client};
use bytes::Bytes;
use domain::{
    id::{GardenId, ImageId, MaskId, ScanId},
    ports::{BlobError, BlobStore, ContentType, ObjectKey, ObjectPrefix},
};
use futures::TryStreamExt;

fn config() -> BlobConfig {
    BlobConfig {
        endpoint: std::env::var("PDM_TEST_RUSTFS_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:19000".into()),
        access_key: "TESTROOT".into(),
        secret_key: "testrootsecret".into(),
        bucket: format!(
            "pdm-test-{}",
            ulid::Ulid::generate().to_string().to_lowercase()
        ),
        region: "us-east-1".into(),
    }
}

#[tokio::test]
#[ignore = "needs RustFS"]
async fn put_get_list_and_delete_against_rustfs() {
    let config = config();
    let client = storage_client(&config);
    client
        .create_bucket()
        .bucket(&config.bucket)
        .send()
        .await
        .expect("bucket");
    let store = Arc::new(S3BlobStore::from_client(client, config.bucket.clone()));

    let g: GardenId = "01JAX3K9T6R2B8V0QZ4M7D1E5G".parse().unwrap();
    let s: ScanId = "01JAX3K9T6R2B8V0QZ4M7D1E5S".parse().unwrap();
    let i: ImageId = "01JAX3K9T6R2B8V0QZ4M7D1E5A".parse().unwrap();
    let key = ObjectKey::scan_image(g, s, i);

    store
        .put(&key, Bytes::from_static(b"hello"), ContentType::Webp)
        .await
        .expect("put");
    let again = store
        .put(&key, Bytes::from_static(b"other"), ContentType::Webp)
        .await;
    assert!(
        matches!(again, Err(BlobError::AlreadyExists)),
        "412 If-None-Match: {again:?}"
    );

    let object = store.get(&key).await.expect("get");
    assert_eq!(object.content_type, "image/webp");
    assert_eq!(object.length, Some(5));
    assert!(
        object.etag.as_deref().is_some_and(|e| e.starts_with('"')),
        "the ETag keeps its quotes"
    );
    let chunks: Vec<Bytes> = object.body.try_collect().await.expect("stream");
    assert_eq!(chunks.concat(), b"hello");
    assert_eq!(
        store.get_bytes(&key).await.unwrap(),
        Bytes::from_static(b"hello")
    );
    assert!(matches!(
        store.get(&ObjectKey::depth(g, s, i)).await,
        Err(BlobError::NotFound)
    ));

    // 1004 masks plus the photo: two list pages and two DeleteObjects batches.
    for n in 0..1004u128 {
        let mask = MaskId::from_ulid(ulid::Ulid(n + 1));
        store
            .put(
                &ObjectKey::mask(g, s, mask),
                Bytes::from_static(b"m"),
                ContentType::Png,
            )
            .await
            .unwrap();
    }
    let other: ScanId = "01JAX3K9T6R2B8V0QZ4M7D1E5T".parse().unwrap();
    store
        .put(
            &ObjectKey::scan_image(g, other, i),
            Bytes::from_static(b"keep"),
            ContentType::Webp,
        )
        .await
        .unwrap();
    assert_eq!(
        store.list(&ObjectPrefix::scan(g, s)).await.unwrap().len(),
        1005
    );

    let queue = DeleteQueue::new(store.clone());
    let outcome = queue
        .submit(DeleteJob::Prefix(ObjectPrefix::scan(g, s)))
        .outcome()
        .await;
    let Some(DeleteOutcome::Done(report)) = outcome else {
        panic!("delete: {outcome:?}");
    };
    assert_eq!(report.deleted, 1005);
    assert_eq!(
        store.list(&ObjectPrefix::scan(g, s)).await.unwrap(),
        Vec::new()
    );
    assert_eq!(
        store.list(&ObjectPrefix::garden(g)).await.unwrap().len(),
        1,
        "other prefixes stay"
    );
    queue.drain().await;
}
