//! Deletes that are never cancelled ([storage 4]: "run deletes in a detached background task,
//! not inside a request; give them a 120-second timeout; retry on failure").
//!
//! On `RustFS` 1.0.1 a `DeleteObject(s)` call interrupted by a client disconnect can leave objects
//! that can't be listed or deleted (#8374). A delete awaited inside a request future is
//! cancelled when the client hangs up and axum drops the future. [`DeleteQueue::submit`]
//! therefore moves the work into a task owned by the runtime (through a `TaskTracker`), and
//! hands back a [`DeleteHandle`]: dropping the handle, or the future awaiting it, detaches from
//! the result without touching the delete. No `tokio::time::timeout` wraps a call: the only
//! deadline is the client's 120-second operation timeout per attempt (storage v1.2.3 §4,
//! [`crate::client::OPERATION_TIMEOUT`]), a last resort for a hung call. Shutdown waits for
//! submitted deletes ([`DeleteQueue::drain`]) instead of aborting them.

use std::{sync::Arc, time::Duration};

use domain::ports::{BlobError, BlobStore, DeleteReport, ObjectKey, ObjectPrefix};
use tokio::sync::oneshot;
use tokio_util::task::TaskTracker;

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

impl DeleteOutcome {
    /// Whether the objects are gone, so the records may be deleted next.
    pub fn is_done(&self) -> bool {
        matches!(self, Self::Done(_))
    }
}

/// The caller's side of a submitted delete. Awaiting it is optional; dropping it never stops the
/// delete. `#[must_use]` only reminds the caller that the outcome decides whether the records may
/// go.
#[derive(Debug)]
#[must_use = "the outcome says whether the database records may be deleted"]
pub struct DeleteHandle {
    outcome: oneshot::Receiver<DeleteOutcome>,
}

impl DeleteHandle {
    /// Waits for the outcome. `None` only if the runtime shut down mid-delete.
    pub async fn outcome(self) -> Option<DeleteOutcome> {
        self.outcome.await.ok()
    }
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

impl std::fmt::Debug for DeleteQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeleteQueue")
            .field("in_flight", &self.tracker.len())
            .finish_non_exhaustive()
    }
}

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

/// A [`BlobStore`] whose `delete_keys` and `delete_prefix` [`submit`](DeleteQueue::submit) a
/// job and wait for its outcome, so a caller dropped while it waits leaves the delete running;
/// puts, gets and lists go straight to the queue's store. A delete that gave up is the
/// `DeleteReport` of the keys still failed, or the last attempt's error.
pub struct QueuedDeletes {
    queue: DeleteQueue,
}

impl QueuedDeletes {
    async fn run(&self, job: DeleteJob) -> Result<DeleteReport, BlobError> {
        match self.queue.submit(job).outcome().await {
            Some(DeleteOutcome::Done(report)) => Ok(report),
            Some(DeleteOutcome::Failed {
                remaining,
                error: None,
            }) => Ok(DeleteReport {
                deleted: 0,
                failed: remaining,
            }),
            Some(DeleteOutcome::Failed {
                error: Some(error), ..
            }) => Err(error),
            None => Err(BlobError::Unavailable("the delete queue stopped".into())),
        }
    }
}

#[async_trait::async_trait]
impl BlobStore for QueuedDeletes {
    async fn put(
        &self,
        key: &ObjectKey,
        body: bytes::Bytes,
        content_type: domain::ports::ContentType,
    ) -> Result<(), BlobError> {
        self.queue.store.put(key, body, content_type).await
    }

    async fn get(&self, key: &ObjectKey) -> Result<domain::ports::BlobObject, BlobError> {
        self.queue.store.get(key).await
    }

    async fn get_bytes(&self, key: &ObjectKey) -> Result<bytes::Bytes, BlobError> {
        self.queue.store.get_bytes(key).await
    }

    async fn delete_keys(&self, keys: &[ObjectKey]) -> Result<DeleteReport, BlobError> {
        self.run(DeleteJob::Keys(keys.to_vec())).await
    }

    async fn delete_prefix(&self, prefix: &ObjectPrefix) -> Result<DeleteReport, BlobError> {
        self.run(DeleteJob::Prefix(prefix.clone())).await
    }

    async fn list(
        &self,
        prefix: &ObjectPrefix,
    ) -> Result<Vec<domain::ports::ObjectInfo>, BlobError> {
        self.queue.store.list(prefix).await
    }
}

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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bytes::Bytes;
    use domain::{
        id::{GardenId, ImageId, ScanId},
        ports::{BlobStore, ContentType, ObjectKey, ObjectPrefix},
        testing::FixedClock,
    };

    use super::*;
    use crate::memory::MemoryBlobStore;

    fn ids() -> (GardenId, ScanId, ImageId) {
        (
            "01JAX3K9T6R2B8V0QZ4M7D1E5G".parse().unwrap(),
            "01JAX3K9T6R2B8V0QZ4M7D1E5S".parse().unwrap(),
            "01JAX3K9T6R2B8V0QZ4M7D1E5A".parse().unwrap(),
        )
    }

    #[tokio::test]
    async fn a_dropped_handle_never_stops_the_delete() {
        let clock = Arc::new(FixedClock::at("2026-10-18T14:00:00Z".parse().unwrap()));
        let store = Arc::new(MemoryBlobStore::new(clock));
        let (g, s, i) = ids();
        store
            .put(
                &ObjectKey::scan_image(g, s, i),
                Bytes::from_static(b"x"),
                ContentType::Webp,
            )
            .await
            .unwrap();
        let queue = DeleteQueue::new(store.clone());
        // The request that submitted it is gone at once.
        drop(queue.submit(DeleteJob::Prefix(ObjectPrefix::scan(g, s))));
        queue.drain().await;
        assert_eq!(
            store.list(&ObjectPrefix::garden(g)).await.unwrap(),
            Vec::new()
        );
    }

    #[tokio::test]
    async fn the_outcome_reports_the_deleted_keys() {
        let clock = Arc::new(FixedClock::at("2026-10-18T14:00:00Z".parse().unwrap()));
        let store = Arc::new(MemoryBlobStore::new(clock));
        let (g, s, i) = ids();
        let key = ObjectKey::depth(g, s, i);
        store
            .put(&key, Bytes::from_static(b"d"), ContentType::Png)
            .await
            .unwrap();
        let queue = DeleteQueue::new(store);
        let outcome = queue
            .submit(DeleteJob::Keys(vec![key.clone(), key]))
            .outcome()
            .await;
        let Some(DeleteOutcome::Done(report)) = outcome else {
            panic!("done");
        };
        assert_eq!(report.deleted, 2, "a missing key counts as deleted");
    }

    /// The queued store's delete runs on the queue: a caller dropped while it waits leaves it
    /// running, and `drain` waits for it. Puts go straight to the store.
    #[tokio::test]
    async fn a_queued_stores_delete_outlives_its_caller() {
        let clock = Arc::new(FixedClock::at("2026-10-18T14:00:00Z".parse().unwrap()));
        let store = Arc::new(MemoryBlobStore::new(clock));
        let (g, s, i) = ids();
        let key = ObjectKey::depth(g, s, i);
        let queue = DeleteQueue::new(store.clone());
        let queued = queue.queued_store();
        queued
            .put(&key, Bytes::from_static(b"d"), ContentType::Png)
            .await
            .unwrap();
        assert_eq!(store.list(&ObjectPrefix::garden(g)).await.unwrap().len(), 1);

        // Polled once, so the job is submitted, then dropped.
        let keys = [key];
        let mut delete = queued.delete_keys(&keys);
        assert!(futures::poll!(delete.as_mut()).is_pending());
        drop(delete);
        queue.drain().await;
        assert_eq!(
            store.list(&ObjectPrefix::garden(g)).await.unwrap(),
            Vec::new()
        );
        let report = queued.delete_keys(&keys).await.unwrap();
        assert_eq!(report.deleted, 1, "a missing key counts as deleted");
    }
}
