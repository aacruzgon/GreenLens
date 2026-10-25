# PDM Backend: `blob` crate

> **Status:** Draft v0.2.3, 2026-10-24. v0.2.3: the orphan sweep also sweeps prefixes under `gardens/` whose garden record is gone, found with `BlobStore::list_prefixes` (`ListObjectsV2` with the delimiter `/`) and `blob::garden_prefixes`; `orphan_candidates` also returns how many objects it listed (`Candidates`), for the sweep's log line; section 7 no longer says a stored key's segments are checked as ULIDs. v0.2.2: `DeleteQueue::queued_store` gives `pipeline` an `Arc<dyn BlobStore>` whose deletes go through the queue (`QueuedDeletes`), for the masks it deletes when a job's lease is lost after their puts. v0.2.1: the research notes are no longer named by a path outside the repository. v0.2: a delete's only deadline is the client's 120-second operation timeout per attempt (storage v1.2.3), no task timeout around it. v0.1: `S3BlobStore`, the object store over `RustFS` 1.0.1 with `aws-sdk-s3` 1.152, a `DeleteQueue` that runs every delete detached so it is never cancelled, an in-memory store for tests and the orphan sweep's listing side, run against the pinned `RustFS` image.

Companion to the [backend architecture](README.md) and the [storage notes](../storage/README.md). The port it implements, `BlobStore`, and the key types `ObjectKey` and `ObjectPrefix` are in [domain 9](domain.md#9-boundary-traits). Every listing is copied from the compiled source of `crates/blob` with the docgen tool: `cargo check -p blob`, `cargo clippy -p blob --all-targets` and `cargo test -p blob` (6 unit tests) pass on Rust 1.99.1, and the ignored integration test passes against `rustfs/rustfs:1.0.1` by digest.

## Contents

1. [Purpose and boundaries](#1-purpose-and-boundaries)
2. [Dependencies](#2-dependencies)
3. [Module layout](#3-module-layout)
4. [The client](#4-the-client)
5. [The object store](#5-the-object-store)
6. [Deletes that are never cancelled](#6-deletes-that-are-never-cancelled)
7. [Keys](#7-keys)
8. [The in-memory store](#8-the-in-memory-store)
9. [The orphan sweep](#9-the-orphan-sweep)
10. [Concurrency and lifetimes](#10-concurrency-and-lifetimes)
11. [Errors](#11-errors)
12. [Testing](#12-testing)
13. [Decisions](#13-decisions)
14. [Open questions](#14-open-questions)
15. [Sources](#15-sources)

---

## 1. Purpose and boundaries

**What it owns.** Every call to `RustFS` ([storage](../storage/README.md)): `PutObject` with `If-None-Match: *`, streamed `GetObject`, `ListObjectsV2` and `DeleteObjects` in batches of 1000; the rule that a delete is never cancelled ([storage 4](../storage/README.md#4-writes-and-deletes)); the listing side of the daily orphan sweep.

**What it never does.** It builds no key from user input (keys come from `domain`'s `ObjectKey`, built only from typed ids); it never presigns a URL (the API serves the bytes, [storage 3](../storage/README.md#3-how-images-are-served)); it never touches the database. No AWS type appears in its public API: errors are the domain's `BlobError` and bodies are the domain's `ByteStream`, so `api` doesn't depend on `aws-sdk-s3`.

**Who calls it.** `api` holds `Arc<dyn BlobStore>` for puts and gets, and a `DeleteQueue` for every delete; the deletion cascade and the orphan sweep go through the queue. `pipeline` holds only `Arc<dyn BlobStore>`, so `api` gives it the queue's store (`DeleteQueue::queued_store`), whose deletes go through the queue too.

**Where it departs from the storage notes** (each is in section 13):

| Topic | Storage 5 | Here | Why |
|---|---|---|---|
| Behaviour version | `BehaviorVersion::latest()` | `BehaviorVersion::v2026_01_12()` | `latest()` moves with the SDK; pinning keeps retry and timeout behaviour fixed across upgrades (research `http-s3.md` §9) |
| The trait | "a small `ObjectStore` trait (`put`, `get_stream`, `delete_prefix`)" | The domain's `BlobStore` (`put`, `get`, `get_bytes`, `delete_keys`, `delete_prefix`, `list`, `list_prefixes`) | The measurement stage needs whole depth maps, a removed shot is deleted by key, and the orphan sweep lists |
| Features | default features (the skills' `versions.md` turns them off) | default features off, `default-https-client`, `rt-tokio`, `http-1x` | The default `rustls` feature is the legacy client: a second HTTP stack (`hyper` 0.14, `rustls` 0.21) |

---

## 2. Dependencies

```toml
[package]
name = "blob"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
async-trait.workspace = true
aws-sdk-s3.workspace = true
bytes.workspace = true
chrono.workspace = true
domain.workspace = true
futures.workspace = true
tokio.workspace = true
tokio-util.workspace = true
tracing.workspace = true

[dev-dependencies]
ulid.workspace = true
domain = { workspace = true, features = ["test-util"] }
tokio = { workspace = true, features = ["test-util"] }
```

| Crate | Version | Why |
|---|---|---|
| `aws-sdk-s3` | 1.152.0, default features off, `default-https-client`, `rt-tokio`, `http-1x` | The client `RustFS` documents and uses in its own tests ([storage 5](../storage/README.md#5-rust-client)); feature set from `versions.md` |
| `domain` | path | The port, keys, `BlobError`, `ByteStream`, `Clock` |
| `bytes`, `futures` | 1.12.1, 0.3.34 | `Bytes` bodies; `try_unfold` and `BoxStream` for the streamed body |
| `tokio`, `tokio-util` | 1.53.3, 0.7.20 (`rt`) | `timeout`, `sleep`, `oneshot`; `TaskTracker` for detached deletes |
| `chrono` | 0.4.45 | `last_modified` from the SDK's `DateTime` |
| `async-trait`, `tracing` | 0.1.92, 0.1.44 | The port is `#[async_trait]`; warnings with the full SDK error chain |
| `ulid` (dev) | 3.0.0 | Fresh bucket names and mask ids in the integration test |

---

## 3. Module layout

```
crates/blob/
├─ Cargo.toml
├─ src/
│  ├─ lib.rs       crate docs and re-exports
│  ├─ client.rs    BlobConfig, storage_client (storage 5 with a pinned behaviour version)
│  ├─ s3.rs        S3BlobStore: the BlobStore port over aws-sdk-s3
│  ├─ error.rs     map_sdk_error: SdkError → BlobError, 412 by status or code
│  ├─ delete.rs    DeleteQueue, DeleteJob, DeleteHandle, DeleteOutcome, QueuedDeletes: detached, never cancelled
│  ├─ memory.rs    MemoryBlobStore, the in-memory fake with RustFS's rules
│  └─ sweep.rs     orphan_candidates: the garden's objects older than an hour; garden_prefixes
└─ tests/rustfs.rs ignored: put, 412, streamed get, the delimiter listing, 1005 keys listed and deleted against RustFS 1.0.1
```

---

## 4. The client

```rust
/// The client's per-attempt operation timeout ([storage 4]: "the client's 120-second operation
/// timeout", a last resort for a hung call; never a shorter task timeout around a delete).
pub const OPERATION_TIMEOUT: Duration = Duration::from_secs(120);

/// How the API reaches `RustFS` ([storage 1], [storage 2]).
#[derive(Debug, Clone)]
pub struct BlobConfig {
    /// `http://rustfs:9000` on the Docker network.
    pub endpoint: String,
    /// `PDMAPI01`.
    pub access_key: String,
    /// Its secret, from the environment.
    pub secret_key: String,
    /// `pdm-images`.
    pub bucket: String,
    /// Must match `RUSTFS_REGION`: `us-east-1`.
    pub region: String,
}

/// Storage 5's client, with the behaviour version pinned to `v2026_01_12` instead of
/// `latest()`: `latest()` moves with the SDK, and `v2026_01_12` turns on retries by default and a
/// 3.1-second connect timeout (research `http-s3.md` §9), so an SDK upgrade can't change retry or
/// timeout behaviour silently. Static credentials skip the default provider chain (no IMDS or
/// environment lookups); path-style addressing is needed unless `RUSTFS_SERVER_DOMAINS` is set.
/// Each attempt has a 120-second operation timeout ([`OPERATION_TIMEOUT`], storage 4).
/// One client per process: it is `Clone + Send + Sync + 'static` and pools its connections.
pub fn storage_client(config: &BlobConfig) -> Client {
    Client::from_conf(
        Builder::new()
            .behavior_version(BehaviorVersion::v2026_01_12())
            .region(Region::new(config.region.clone()))
            .endpoint_url(&config.endpoint)
            .credentials_provider(Credentials::new(
                &config.access_key,
                &config.secret_key,
                None,
                None,
                "rustfs",
            ))
            .force_path_style(true)
            .timeout_config(
                TimeoutConfig::builder()
                    .operation_attempt_timeout(OPERATION_TIMEOUT)
                    .build(),
            )
            .build(),
    )
}
```

✓ Every builder method exists with that spelling in 1.152 and the client works against `RustFS` 1.0.1 (research `http-s3.md` §9; section 12). `Client` is `Clone + Send + Sync + 'static`, an `Arc` inside: one per process. The SDK sends CRC32 checksums by default, and `RustFS` accepts them ✓; `.request_checksum_calculation(WhenRequired)` is the first fallback if a swapped backend rejects them ([storage 8](../storage/README.md#8-fallback)).

---

## 5. The object store

```rust
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
```

```rust
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

    /// `ListObjectsV2` with `delimiter("/")`: each page's `CommonPrefixes`, no object read.
    async fn list_prefixes(&self, prefix: &ObjectPrefix) -> Result<Vec<ObjectPrefix>, BlobError> {
        let mut pages = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(prefix.as_str())
            .delimiter("/")
            .into_paginator()
            .send();
        let mut out = Vec::new();
        while let Some(page) = pages.next().await {
            let page = page.map_err(|e| map_sdk_error("ListObjectsV2", e))?;
            out.extend(
                page.common_prefixes()
                    .iter()
                    .filter_map(|p| p.prefix())
                    .filter_map(ObjectPrefix::from_listed),
            );
        }
        Ok(out)
    }
}
```

| Method | S3 call | Rule |
|---|---|---|
| `put` | `PutObject` with `Content-Type` and `If-None-Match: *` | A key is never overwritten; ✓ a second put gets `412 PreconditionFailed`, which is `AlreadyExists` ([storage 4](../storage/README.md#4-writes-and-deletes)) |
| `get` | `GetObject`, streamed | The body is never buffered ([storage 5](../storage/README.md#5-rust-client)); `ETag` keeps its quotes and is passed through verbatim ✓ |
| `get_bytes` | `GetObject`, collected | For the measurement stage's depth maps and the face blur, which need the whole image |
| `delete_keys` | `DeleteObjects`, ≤ 1000 keys a call | `quiet`, and `errors()` read on every response (a `200` can carry per-key failures) |
| `delete_prefix` | `ListObjectsV2` (paginated), then `DeleteObjects` per page | A page is at most 1000 keys, so one page is one batch ✓ |
| `list` | `ListObjectsV2`, every page | The orphan sweep |
| `list_prefixes` | `ListObjectsV2` with `delimiter("/")`, every page | The orphan sweep's pass over deleted gardens: `CommonPrefixes` only, no object read. *Unverified* on `RustFS` 1.0.1 until the ignored test runs; the port's default body (used by the in-memory store) derives the prefixes from `list` |

The SDK body is turned into the domain's stream without buffering. `ByteStream` isn't a `futures::Stream` without an unstable feature, so `try_unfold` drives its `try_next()`:

```rust
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
```

`api` can also stream a body straight into an axum response; `Body::new(out.body.into_inner())` keeps frame boundaries (research `http-s3.md` §4). The domain stream costs one small future per chunk and keeps AWS types out of `api`.

```rust
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
```

---

## 6. Deletes that are never cancelled

[Storage 4](../storage/README.md#4-writes-and-deletes): on `RustFS` 1.0.1 a `DeleteObject(s)` call interrupted by a client disconnect or a timeout can leave objects that can't be listed or deleted, and their space is lost ([#8374](https://github.com/rustfs/rustfs/issues/8374)). A delete awaited inside a request future is cancelled when the client hangs up, because axum drops the future. So deletes have their own type:

```rust
/// Attempts per job; a missing key is a no-op, so every retry is safe.
pub const DELETE_ATTEMPTS: u32 = 5;

/// What to delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteJob {
    /// Everything under a prefix: a garden, a scan, a conversation's images.
    Prefix(ObjectPrefix),
    /// Single objects: a removed shot with its depth maps, the orphan sweep's keys.
    Keys(Vec<ObjectKey>),
}

/// How a job ended.
#[derive(Debug)]
pub enum DeleteOutcome {
    /// Every key is gone.
    Done(DeleteReport),
    /// Attempts ran out; the database records must stay so a later run retries
    /// ([storage 4]: "a failure partway is retried from the start").
    Failed {
        /// The keys still reported as failed, when the last attempt got that far.
        remaining: Vec<ObjectKey>,
        /// The last error, if the last attempt failed outright.
        error: Option<BlobError>,
    },
}

/// The caller's side of a submitted delete. Awaiting it is optional; dropping it never stops the
/// delete. `#[must_use]` only reminds the caller that the outcome decides whether the records may
/// go.
#[derive(Debug)]
#[must_use = "the outcome says whether the database records may be deleted"]
pub struct DeleteHandle {
    outcome: oneshot::Receiver<DeleteOutcome>,
}

/// The only path to `BlobStore::delete_keys` and `delete_prefix` in `api`: it owns the store
/// behind `Arc<dyn BlobStore>` and runs each job detached. `Clone` shares the tracker, so every
/// clone's jobs are waited for by one `drain`.
#[derive(Clone)]
pub struct DeleteQueue {
    store: Arc<dyn BlobStore>,
    tracker: TaskTracker,
    backoff: Duration,
}
```

```rust
impl DeleteQueue {
    /// A queue over `store`, with 1 s, 2 s, 4 s, 8 s between attempts.
    pub fn new(store: Arc<dyn BlobStore>) -> Self {
        Self::with_backoff(store, Duration::from_secs(1))
    }

    /// A queue with another first back-off (tests use a paused clock instead).
    pub fn with_backoff(store: Arc<dyn BlobStore>, backoff: Duration) -> Self {
        Self {
            store,
            tracker: TaskTracker::new(),
            backoff,
        }
    }

    /// Runs `job` in a detached task. Must be called inside a tokio runtime.
    ///
    /// The spawned future is `'static` (it owns `Arc<dyn BlobStore>` and the job), as
    /// `tokio::spawn` requires; `dyn BlobStore` is `Send + Sync` and `#[async_trait]` futures are
    /// `Send`, so the task can run on any worker thread.
    pub fn submit(&self, job: DeleteJob) -> DeleteHandle {
        let (tx, rx) = oneshot::channel();
        let store = Arc::clone(&self.store);
        let backoff = self.backoff;
        self.tracker.spawn(async move {
            let outcome = run(store.as_ref(), job, backoff).await;
            if !outcome.is_done() {
                tracing::error!(?outcome, "delete gave up; the records stay for a later run");
            }
            // The caller may have gone; the delete finished either way.
            let _ = tx.send(outcome);
        });
        DeleteHandle { outcome: rx }
    }

    /// Stops accepting the idea of new work and waits for every submitted delete to finish
    /// (at most `DELETE_ATTEMPTS` × the 120-second operation timeout, plus back-off). Called at shutdown, after the
    /// HTTP server stopped, so nothing submits meanwhile.
    pub async fn drain(&self) {
        self.tracker.close();
        self.tracker.wait().await;
    }

    /// Deletes in flight.
    pub fn in_flight(&self) -> usize {
        self.tracker.len()
    }

    /// The queue's store, with every delete sent through the queue: for code that holds only
    /// `Arc<dyn BlobStore>` (`pipeline`, whose deletes would otherwise run inside a request
    /// future). See [`QueuedDeletes`].
    pub fn queued_store(&self) -> Arc<dyn BlobStore> {
        Arc::new(QueuedDeletes {
            queue: self.clone(),
        })
    }
}
```

```rust
/// A [`BlobStore`] whose `delete_keys` and `delete_prefix` [`submit`](DeleteQueue::submit) a
/// job and wait for its outcome, so a caller dropped while it waits leaves the delete running;
/// puts, gets and lists go straight to the queue's store. A delete that gave up is the
/// `DeleteReport` of the keys still failed, or the last attempt's error.
pub struct QueuedDeletes {
    queue: DeleteQueue,
}
```

```rust
/// One job: up to [`DELETE_ATTEMPTS`] attempts, retrying what failed; each S3 call is bounded only
/// by the client's operation timeout. A prefix job falls back to the keys `RustFS` reported as
/// failed.
async fn run(store: &dyn BlobStore, job: DeleteJob, first_backoff: Duration) -> DeleteOutcome {
    let mut job = job;
    let mut deleted = 0;
    let mut last_error = None;
    let mut remaining = Vec::new();
    for attempt in 0..DELETE_ATTEMPTS {
        if attempt > 0 {
            tokio::time::sleep(first_backoff * 2u32.pow(attempt - 1)).await;
        }
        let result = match &job {
            DeleteJob::Prefix(prefix) => store.delete_prefix(prefix).await,
            DeleteJob::Keys(keys) => store.delete_keys(keys).await,
        };
        match result {
            Ok(report) if report.failed.is_empty() => {
                return DeleteOutcome::Done(DeleteReport {
                    deleted: deleted + report.deleted,
                    failed: Vec::new(),
                });
            }
            Ok(report) => {
                deleted += report.deleted;
                remaining.clone_from(&report.failed);
                job = DeleteJob::Keys(report.failed);
                last_error = None;
            }
            // A timed-out attempt arrives here as `Unavailable` from the client.
            Err(error) => last_error = Some(error),
        }
    }
    DeleteOutcome::Failed {
        remaining,
        error: last_error,
    }
}
```

**What guarantees it.**

- `submit` moves the work into a task spawned on a `TaskTracker`: the runtime owns the future, not the caller. Dropping the `DeleteHandle`, or the request future that awaits it, only drops the receiving end of a `oneshot` channel; the task's `send` then fails silently and the delete has already run. ✓ `a_dropped_handle_never_stops_the_delete` drops the handle at once and finds the objects gone.
- The task is never aborted: `drain` closes the tracker and waits for every submitted delete at shutdown, after the HTTP server stopped. Nothing calls `JoinHandle::abort`, and a `TaskTracker` (unlike a `JoinSet`) doesn't abort its tasks when dropped ([tokio-util `TaskTracker`](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html)).
- No `tokio::time::timeout` wraps a delete: the only deadline is the client's 120-second operation timeout per attempt (`OPERATION_TIMEOUT`, set in `storage_client`; storage v1.2.3 §4), a last resort for a hung call that comes back as `Unavailable`.
- Retries are safe because deleting a missing key is a no-op ✓; a prefix job falls back to the keys `RustFS` reported as failed.
- `DeleteOutcome::is_done()` decides whether the cascade may delete the database records next: the objects first, then the records ([storage 4](../storage/README.md#4-writes-and-deletes); [database 4](../database/README.md#deletion-) step 3).
- `api` reaches `delete_keys` and `delete_prefix` only through the queue; the port has them so a fake and a swapped backend implement them, but no handler calls them directly.

---

## 7. Keys

Keys are the domain's `ObjectKey` and `ObjectPrefix` ([storage 2](../storage/README.md#2-bucket-keys-and-access); [domain 9](domain.md#9-boundary-traits)): the builders take only typed ids, so every id segment under `gardens/` is a ULID, never user input. A key read back from a record or a listing goes through `ObjectKey::from_stored`, which checks only that it starts with `gardens/` or `knowledge/` and has no empty, `.` or `..` segment ✓ (`RustFS` rejects those with `400`), not that its segments are ULIDs; a listed prefix goes through `ObjectPrefix::from_listed`, the same check plus the trailing `/`, and `ObjectPrefix::garden_id` names a garden only for a whole `gardens/{ULID}/`. `blob` adds no key builder of its own; listed keys that don't parse are skipped.

| Key | Built by |
|---|---|
| `gardens/{garden}/scans/{scan}/{image}.webp` | `ObjectKey::scan_image` |
| `gardens/{garden}/scans/{scan}/masks/{mask}.png` | `ObjectKey::mask` |
| `gardens/{garden}/scans/{scan}/depth/{image}.png`, `depth_confidence/{image}.png` | `ObjectKey::depth`, `depth_confidence` (never served) |
| `gardens/{garden}/chat/{conversation}/{image}.webp` | `ObjectKey::chat_image` |
| `knowledge/species/{species}/{hash}.webp` | `ObjectKey::species_photo` (written by the knowledge loader only; the API's key can't write there) |
| prefixes | `ObjectPrefix::garden`, `scan`, `chat`; `gardens` (`gardens/`, for the sweep's delimiter listing) |

---

## 8. The in-memory store

```rust
/// The fake. Its `Mutex` is a `std` one: it is never held across an `.await`, and every method
/// takes it for a map operation only.
pub struct MemoryBlobStore {
    objects: Mutex<BTreeMap<ObjectKey, Stored>>,
    clock: Arc<dyn Clock>,
}
```

```rust
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
```

It follows `RustFS`'s rules, so a test of a handler or the cascade sees the same behaviour: a put never overwrites (`AlreadyExists`), deleting a missing key counts as deleted, listing is by prefix in key order, and each object is dated by the injected `Clock` so the orphan sweep's age check is testable. The `std::sync::Mutex` is never held across an `.await` ([Tokio, shared state](https://tokio.rs/tokio/tutorial/shared-state)).

---

## 9. The orphan sweep

[Storage 4](../storage/README.md#4-writes-and-deletes): a crash between the put and the record leaves an object without a record, never the reverse. The daily sweep runs in three steps across three crates:

1. `blob::orphan_candidates` lists the garden's objects and keeps those older than an hour (`ORPHAN_MIN_AGE`);
2. the store's `DeletionRepo::unreferenced_keys` keeps those no record points to, counting every key field (`storage_key`, `depth_key`, `depth_confidence_key`, masks and chat images) ([store 17](store.md#17-deletion-and-cleanup));
3. `DeleteQueue::submit(DeleteJob::Keys(..))` deletes them.

```rust
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
```

The gardens swept are chosen in `api` ([api 16](api.md#16-the-scheduler), `scheduler::orphans`): the live ones from `WeatherCacheRepo::gardens_by_cell`, then every garden `garden_prefixes` finds under `gardens/` that isn't live and has no record (`GardenRepo::existing_gardens`). That second pass catches objects that landed after their garden's cascade deleted its prefix, such as an upload racing the deletion; a garden still being deleted has its record and is left to its cascade. Both passes run the same three steps, so the hour applies to them too. Nothing outside `gardens/` is swept: the `knowledge/` prefix belongs to the loader, and a prefix under `gardens/` whose segment isn't a ULID is skipped.

---

## 10. Concurrency and lifetimes

- **`ByteStream` is `BoxStream<'static, Result<Bytes, BlobError>>`** ([domain 9](domain.md#9-boundary-traits)): `try_unfold` takes the SDK body by value as its state, so the stream owns it and borrows nothing; `api` can move it into the response body and across tasks. Dropping it closes the connection to `RustFS`, which is fine for reads: the never-cancel rule is for deletes only.
- **`DeleteQueue::submit`** spawns a `'static + Send` future: it owns an `Arc<dyn BlobStore>` (the trait is `Send + Sync`, [domain 10](domain.md#10-concurrency-and-lifetimes)) and the `DeleteJob`, and `#[async_trait]` futures are `Send`, so the task runs on any worker thread ([Tokio, spawning](https://tokio.rs/tokio/tutorial/spawning)). `run` borrows the store as `&dyn BlobStore` inside that task.
- **`map_sdk_error<E>`** is generic over the operation's error with `E: ProvideErrorMetadata + Error + Send + Sync + 'static`: `ProvideErrorMetadata` gives the S3 code, and the auto-trait bounds make `SdkError<E, HttpResponse>` a valid `BoxError` source.
- `S3BlobStore` is `Clone` (cheap) and `Send + Sync + 'static`; `api` holds it once as `Arc<dyn BlobStore>`.

---

## 11. Errors

```rust
/// Maps an SDK error of `operation`.
///
/// - `412 PreconditionFailed` (an `If-None-Match: *` put of an existing key) arrives as the
///   operation's `Unhandled` variant in 1.152, so it is matched by status or code, never by
///   variant ✓ (run against `RustFS` 1.0.1).
/// - `404` / `NoSuchKey` is `NotFound`.
/// - Timeouts, dispatch and response failures and `5xx` are `Unavailable`; anything else is
///   `Rejected` (a policy, a bad key).
///
/// The SDK error stays the `source`, so the log keeps its chain; `DisplayErrorContext` prints
/// the whole chain (a plain `to_string()` is just "service error"). It includes the raw
/// response, so it is logged, never returned to a client.
pub fn map_sdk_error<E>(operation: &'static str, error: SdkError<E, HttpResponse>) -> BlobError
where
    E: ProvideErrorMetadata + std::error::Error + Send + Sync + 'static,
{
    let status = error.raw_response().map(|r| r.status().as_u16());
    let code = error.code().map(str::to_owned);
    match (status, code.as_deref()) {
        (Some(412), _) | (_, Some("PreconditionFailed")) => return BlobError::AlreadyExists,
        (Some(404), _) | (_, Some("NoSuchKey" | "NotFound")) => return BlobError::NotFound,
        _ => {}
    }
    tracing::warn!(operation, error = %DisplayErrorContext(&error), "storage call failed");
    let transient = matches!(
        error,
        SdkError::TimeoutError(_) | SdkError::DispatchFailure(_) | SdkError::ResponseError(_)
    ) || status.is_some_and(|s| s >= 500);
    if transient {
        BlobError::Unavailable(Box::new(error))
    } else {
        BlobError::Rejected {
            operation,
            source: Box::new(error),
        }
    }
}
```

| `BlobError` | When | `api` maps it to |
|---|---|---|
| `NotFound` | `404`, `NoSuchKey` | `404 not_found` (a record without its object) |
| `AlreadyExists` | `412`, `PreconditionFailed` (an `Unhandled` variant in 1.152, so matched by status or code) ✓ | an internal error: keys are fresh ULIDs |
| `Unavailable` | timeouts, dispatch and response failures, `5xx` | `503 upstream_unavailable` with `upstream: "storage"` (`rust-errors`) |
| `Rejected { operation }` | anything else: a policy, a bad key | `500 internal`, logged with its chain |

The SDK error stays the `source`; `DisplayErrorContext` prints the whole chain into the warning, because a plain `to_string()` is just "service error". The chain includes the raw response, so it is logged, never returned.

---

## 12. Testing

| Test | What it proves |
|---|---|
| `memory::puts_never_overwrite_and_gets_stream` | The fake's put, `AlreadyExists`, the streamed body, `NotFound` |
| `delete::a_dropped_handle_never_stops_the_delete` | A handle dropped at once; the prefix is still deleted |
| `delete::the_outcome_reports_the_deleted_keys` | A missing key counts as deleted |
| `delete::a_queued_stores_delete_outlives_its_caller` | `queued_store`'s put goes to the store; its delete, dropped after one poll, still runs and `drain` waits for it |
| `sweep::only_objects_older_than_an_hour_are_candidates` | The age cut-off, on the injected clock |
| `tests/rustfs.rs` (ignored) | Against `rustfs/rustfs:1.0.1` by digest: the put, the `412` as `AlreadyExists`, the streamed get with its quoted `ETag`, `NotFound`, 1005 keys listed in two pages and deleted in two batches through the `DeleteQueue`, another prefix left alone ✓ (2026-10-16) |

The integration test's file header has the `docker run` command; run it with `PDM_TEST_RUSTFS_URL=http://127.0.0.1:19000 cargo test -p blob --test rustfs -- --ignored`. It creates a fresh bucket per run as the root key.

---

## 13. Decisions

| Topic | Decision | Settled in |
|---|---|---|
| Storage 5, behaviour version | Pinned `BehaviorVersion::v2026_01_12()` instead of `latest()` | Section 4 |
| Storage 5, the trait | The domain's `BlobStore` replaces the sketched `ObjectStore` | Section 5 |
| Storage 4, never cancel a delete | `DeleteQueue`: detached on a `TaskTracker`, drained at shutdown, the client's 120-second operation timeout per attempt (no task timeout), 5 attempts with 1–8 s back-off | Sections 4, 6 |
| Contracts 12–24 | None is the object store's: it stores no timestamps on the wire and has no HTTP surface | — |

---

## 14. Open questions

1. **The client's 120-second timeout can still interrupt a hung delete.** Storage v1.2.3 makes it a last resort; an attempt cut by it is retried, which re-deletes what is still listed but can't recover what an interrupted call leaked on 1.0.1 (caveat 1). The capacity check in [storage 6](../storage/README.md#6-operations) would show lost space until `RustFS` 1.0.2 is final.
2. **`delete_prefix` lists every page before deleting.** It holds a garden's keys in memory (thousands of short strings) and deletes page by page after; deleting while paginating would bound memory but relies on S3 continuation tokens surviving deletes, which isn't verified on `RustFS`.

---

## 15. Sources

**Rust and Tokio**
- The Book: [16.4 `Send` and `Sync`](https://doc.rust-lang.org/book/ch16-04-extensible-concurrency-sync-and-send.html) · Tokio tutorial: [spawning](https://tokio.rs/tokio/tutorial/spawning) · [shared state](https://tokio.rs/tokio/tutorial/shared-state) · [`tokio::time::timeout`](https://docs.rs/tokio/1.53.3/tokio/time/fn.timeout.html) · [`oneshot`](https://docs.rs/tokio/1.53.3/tokio/sync/oneshot/index.html) · [tokio-util 0.7.20 `TaskTracker`](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html)
- [futures 0.3.34 `stream::try_unfold`](https://docs.rs/futures/0.3.34/futures/stream/fn.try_unfold.html)

**Crates at the pinned versions**
- aws-sdk-s3 1.152.0: [`Client`](https://docs.rs/aws-sdk-s3/1.152.0/aws_sdk_s3/struct.Client.html) · [`config::Builder`](https://docs.rs/aws-sdk-s3/1.152.0/aws_sdk_s3/config/struct.Builder.html) · [`PutObjectError`](https://docs.rs/aws-sdk-s3/1.152.0/aws_sdk_s3/operation/put_object/enum.PutObjectError.html) · [`GetObjectError`](https://docs.rs/aws-sdk-s3/1.152.0/aws_sdk_s3/operation/get_object/enum.GetObjectError.html) · [`DisplayErrorContext`](https://docs.rs/aws-sdk-s3/1.152.0/aws_sdk_s3/error/struct.DisplayErrorContext.html) · [`BehaviorVersion`](https://docs.rs/aws-smithy-runtime-api/1.19.0/aws_smithy_runtime_api/client/behavior_version/struct.BehaviorVersion.html) · aws-smithy-types 1.8.1 `ByteStream` (source: `src/byte_stream.rs`, `try_next`, `collect`)

**Project documents, notes and skills**
- [Storage](../storage/README.md) · [database](../database/README.md) · [domain](domain.md) · [store](store.md)
- Research notes and an HTTP probe from writing this design (not kept in the repository): the 412 as `Unhandled`, the pinned behaviour version, `DisplayErrorContext`, pagination and batch deletes run against `RustFS` 1.0.1
- Skills in the main checkout's `.claude/skills/`: `rust/versions.md`, `rust-errors`, `rust-async`, `rust-testing`

The crate pages were checked against the pinned sources on 2026-10-16.
