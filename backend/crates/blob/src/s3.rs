//! `S3BlobStore`: the `BlobStore` port over `aws-sdk-s3` 1.152 ([storage 4], [storage 5]).

use aws_sdk_s3::{
    Client,
    primitives::ByteStream as S3Body,
    types::{Delete, ObjectIdentifier},
};
use bytes::Bytes;
use domain::{
    ports::{
        BlobError, BlobObject, BlobStore, ByteStream, ContentType, DeleteReport, ObjectInfo,
        ObjectKey, ObjectPrefix,
    },
    wire::Timestamp,
};
use futures::{StreamExt, TryStreamExt, stream};

use crate::{
    client::{BlobConfig, storage_client},
    error::map_sdk_error,
};

/// The most keys one `DeleteObjects` call takes, and the size of one `ListObjectsV2` page, so
/// one page is one batch ✓ (1105 objects: two pages, two batches).
pub const DELETE_BATCH: usize = 1000;

/// The object store over `RustFS`. `Clone` is cheap (the client is an `Arc` inside), and the
/// store is `Send + Sync + 'static`, so `api` holds it as `Arc<dyn BlobStore>`.
#[derive(Debug, Clone)]
pub struct S3BlobStore {
    client: Client,
    bucket: String,
}

impl S3BlobStore {
    /// The store for `config`, with storage 5's client.
    pub fn new(config: &BlobConfig) -> Self {
        Self::from_client(storage_client(config), config.bucket.clone())
    }

    /// A store over an existing client (tests point one at a local `RustFS`).
    pub fn from_client(client: Client, bucket: String) -> Self {
        Self { client, bucket }
    }

    /// One `DeleteObjects` call of at most [`DELETE_BATCH`] keys, `quiet` so only failures come
    /// back. A `200` can still carry per-key errors, so `errors()` is always read (storage 4).
    async fn delete_batch(&self, keys: &[String]) -> Result<DeleteReport, BlobError> {
        let objects = keys
            .iter()
            .map(|k| ObjectIdentifier::builder().key(k).build())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| BlobError::Rejected {
                operation: "DeleteObjects",
                source: Box::new(e),
            })?;
        let delete = Delete::builder()
            .set_objects(Some(objects))
            .quiet(true)
            .build()
            .map_err(|e| BlobError::Rejected {
                operation: "DeleteObjects",
                source: Box::new(e),
            })?;
        let out = self
            .client
            .delete_objects()
            .bucket(&self.bucket)
            .delete(delete)
            .send()
            .await
            .map_err(|e| map_sdk_error("DeleteObjects", e))?;
        let failed: Vec<ObjectKey> = out
            .errors()
            .iter()
            .filter_map(|e| e.key())
            .filter_map(ObjectKey::from_stored)
            .collect();
        for e in out.errors() {
            tracing::warn!(key = ?e.key(), code = ?e.code(), message = ?e.message(), "key not deleted");
        }
        Ok(DeleteReport {
            deleted: (keys.len() - failed.len()) as u64,
            failed,
        })
    }

    /// Every page of `ListObjectsV2` under `prefix`.
    async fn list_pages(&self, prefix: &ObjectPrefix) -> Result<Vec<Vec<ObjectInfo>>, BlobError> {
        let mut pages = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(prefix.as_str())
            .into_paginator()
            .send();
        let mut out = Vec::new();
        // `PaginationStream` has an inherent `next()`; it isn't a `futures::Stream`.
        while let Some(page) = pages.next().await {
            let page = page.map_err(|e| map_sdk_error("ListObjectsV2", e))?;
            let infos = page
                .contents()
                .iter()
                .filter_map(|o| {
                    let key = ObjectKey::from_stored(o.key()?)?;
                    let modified = o.last_modified()?;
                    let at =
                        chrono::DateTime::from_timestamp(modified.secs(), modified.subsec_nanos())?;
                    Some(ObjectInfo {
                        key,
                        last_modified: Timestamp::from(at),
                        size: u64::try_from(o.size().unwrap_or(0)).unwrap_or(0),
                    })
                })
                .collect();
            out.push(infos);
        }
        Ok(out)
    }
}

/// The SDK body as the domain's stream of chunks. `ByteStream` isn't a `futures::Stream`
/// without an unstable feature, so `try_unfold` drives its `try_next()`. The stream owns the
/// body (`'static`), so `api` can move it into the response; dropping it closes the connection.
fn body_stream(body: S3Body) -> ByteStream {
    stream::try_unfold(body, |mut body| async move {
        match body.try_next().await {
            Ok(Some(chunk)) => Ok(Some((chunk, body))),
            Ok(None) => Ok(None),
            Err(e) => Err(BlobError::Unavailable(Box::new(e))),
        }
    })
    .boxed()
}

#[async_trait::async_trait]
impl BlobStore for S3BlobStore {
    async fn put(
        &self,
        key: &ObjectKey,
        body: Bytes,
        content_type: ContentType,
    ) -> Result<(), BlobError> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .content_type(content_type.as_str())
            .if_none_match("*")
            .body(S3Body::from(body))
            .send()
            .await
            .map(drop)
            .map_err(|e| map_sdk_error("PutObject", e))
    }

    async fn get(&self, key: &ObjectKey) -> Result<BlobObject, BlobError> {
        let out = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .send()
            .await
            .map_err(|e| map_sdk_error("GetObject", e))?;
        Ok(BlobObject {
            content_type: out
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_owned(),
            length: out.content_length().and_then(|n| u64::try_from(n).ok()),
            // The S3 ETag already carries its quotes; it is passed through verbatim.
            etag: out.e_tag().map(str::to_owned),
            body: body_stream(out.body),
        })
    }

    async fn get_bytes(&self, key: &ObjectKey) -> Result<Bytes, BlobError> {
        let object = self.get(key).await?;
        let chunks: Vec<Bytes> = object.body.try_collect().await?;
        Ok(chunks.concat().into())
    }

    async fn delete_keys(&self, keys: &[ObjectKey]) -> Result<DeleteReport, BlobError> {
        let mut report = DeleteReport::default();
        let keys: Vec<String> = keys.iter().map(|k| k.as_str().to_owned()).collect();
        for batch in keys.chunks(DELETE_BATCH) {
            let r = self.delete_batch(batch).await?;
            report.deleted += r.deleted;
            report.failed.extend(r.failed);
        }
        Ok(report)
    }

    async fn delete_prefix(&self, prefix: &ObjectPrefix) -> Result<DeleteReport, BlobError> {
        let mut report = DeleteReport::default();
        for page in self.list_pages(prefix).await? {
            let keys: Vec<String> = page.iter().map(|o| o.key.as_str().to_owned()).collect();
            if keys.is_empty() {
                continue;
            }
            let r = self.delete_batch(&keys).await?;
            report.deleted += r.deleted;
            report.failed.extend(r.failed);
        }
        Ok(report)
    }

    async fn list(&self, prefix: &ObjectPrefix) -> Result<Vec<ObjectInfo>, BlobError> {
        Ok(self
            .list_pages(prefix)
            .await?
            .into_iter()
            .flatten()
            .collect())
    }
}
