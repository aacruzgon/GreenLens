//! The daily orphan sweep ([storage 4], uploads; [blob 9]): objects older than an hour under a
//! garden's prefix that no record points to, deleted through the `DeleteQueue` (detached, never
//! cancelled, retried, drained at shutdown). The gardens swept are the live ones and those with
//! a prefix under `gardens/` but no record any more; a garden being deleted is left to its
//! cascade, which deletes its whole prefix. Nothing outside `gardens/` is ever deleted. A garden
//! that fails is logged and skipped, so one bad garden doesn't stop the run; the next run tries
//! it again.

use std::collections::BTreeSet;

use blob::{DeleteJob, DeleteOutcome, DeleteQueue};
use domain::{
    id::GardenId,
    ports::{BlobStore, ObjectPrefix, Store},
    wire::Timestamp,
};

use crate::error::ApiError;

/// What one run did, for its log line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OrphanReport {
    /// Gardens swept, live and gone.
    pub gardens: usize,
    /// Of those, gardens with objects but no record.
    pub gone_gardens: usize,
    /// Objects listed under the gardens' prefixes.
    pub listed: usize,
    /// Of those, objects older than `blob::ORPHAN_MIN_AGE`.
    pub candidates: usize,
    /// Of those, objects no record points to.
    pub orphans: usize,
    /// Objects deleted.
    pub deleted: u64,
    /// Orphans whose delete gave up; the next run finds them again.
    pub not_deleted: usize,
    /// Gardens (or the listing of `gardens/`) that failed and were skipped.
    pub failed: usize,
}

/// One run: the gardens to sweep, then each garden in turn. Only the list of live gardens is
/// fatal; every later failure is logged, counted in `failed` and skipped.
pub async fn sweep(
    store: &dyn Store,
    blob: &dyn BlobStore,
    deletes: &DeleteQueue,
    now: Timestamp,
) -> Result<OrphanReport, ApiError> {
    let mut report = OrphanReport::default();
    let live: BTreeSet<GardenId> = store
        .gardens_by_cell()
        .await?
        .into_iter()
        .flat_map(|cell| cell.gardens)
        .collect();
    let gone = match gone_gardens(store, blob, &live).await {
        Ok(gone) => gone,
        Err(error) => {
            tracing::warn!(error = %error.chain(), "deleted gardens' prefixes not listed; the next sweep retries");
            report.failed += 1;
            Vec::new()
        }
    };
    report.gone_gardens = gone.len();
    for garden in live.into_iter().chain(gone) {
        report.gardens += 1;
        if let Err(error) = sweep_garden(store, blob, deletes, garden, now, &mut report).await {
            tracing::warn!(%garden, error = %error.chain(), "garden not swept; the next sweep retries");
            report.failed += 1;
        }
    }
    Ok(report)
}

/// The gardens with a prefix under `gardens/` and no record: listed after the live gardens were
/// read, so a garden created meanwhile has a record by the time it is checked.
async fn gone_gardens(
    store: &dyn Store,
    blob: &dyn BlobStore,
    live: &BTreeSet<GardenId>,
) -> Result<Vec<GardenId>, ApiError> {
    let unknown: Vec<GardenId> = blob::garden_prefixes(blob)
        .await?
        .into_iter()
        .filter(|g| !live.contains(g))
        .collect();
    if unknown.is_empty() {
        return Ok(Vec::new());
    }
    let existing: BTreeSet<GardenId> = store
        .existing_gardens(&unknown)
        .await?
        .into_iter()
        .collect();
    Ok(unknown
        .into_iter()
        .filter(|g| !existing.contains(g))
        .collect())
}

/// One garden: its old objects, those no record points to, deleted and awaited.
async fn sweep_garden(
    store: &dyn Store,
    blob: &dyn BlobStore,
    deletes: &DeleteQueue,
    garden: GardenId,
    now: Timestamp,
    report: &mut OrphanReport,
) -> Result<(), ApiError> {
    let candidates = blob::orphan_candidates(blob, garden, now).await?;
    report.listed += candidates.listed;
    report.candidates += candidates.old.len();
    if candidates.old.is_empty() {
        return Ok(());
    }
    let prefix = ObjectPrefix::garden(garden);
    let orphans: Vec<_> = store
        .unreferenced_keys(&candidates.old)
        .await?
        .into_iter()
        // The store answers a subset of the keys it was given; this keeps any delete inside the
        // garden's prefix whatever it answers.
        .filter(|k| k.is_under(&prefix))
        .collect();
    if orphans.is_empty() {
        return Ok(());
    }
    report.orphans += orphans.len();
    let count = orphans.len();
    match deletes.submit(DeleteJob::Keys(orphans)).outcome().await {
        Some(DeleteOutcome::Done(done)) => report.deleted += done.deleted,
        Some(DeleteOutcome::Failed { remaining, .. }) => {
            let left = if remaining.is_empty() {
                count
            } else {
                remaining.len()
            };
            report.not_deleted += left;
            tracing::warn!(%garden, left, "orphans not deleted; the next sweep retries");
        }
        None => {
            report.not_deleted += count;
            tracing::warn!(%garden, "orphan delete stopped by shutdown; the next sweep retries");
        }
    }
    Ok(())
}
