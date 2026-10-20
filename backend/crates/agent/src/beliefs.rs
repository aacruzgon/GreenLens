//! The beliefs (architecture 7, memory): every **shared** garden memory (`pipeline` or
//! `user`) is recorded in `SurrealDB` Agent Memory as one triple, through the [`BeliefStore`]
//! port. The `memory` table stays the system of record; a private chat memory never leaves
//! the database.
//!
//! - **One writer.** Only [`sync_pending`], run by `api`'s `beliefs` task, talks to Agent
//!   Memory, so nothing records a memory while its subject is being erased, and no request
//!   waits on it. A shared memory is created with `belief_synced_at` unset; deleting one, or
//!   starting a plant's or garden's cascade, queues its subject in `belief_forget` in the same
//!   transaction.
//! - **Erase first.** Each run erases the queued subjects, re-marks their remaining shared
//!   memories unsynced, then records what is unsynced. A memory recorded from a stale read
//!   just before its delete is erased by the next run.
//! - **Order.** Agent Memory supersedes by `observed_at` (the memory's `created_at`): a write
//!   older than the current value of its key is dropped, measured 2026-10-17. So a retry or a
//!   rebuild never puts an older value back, and one failing memory doesn't hold up the rest.
//! - [`current_beliefs`] and [`search_beliefs`] are the reads the context builder, the
//!   `search_memory` tool and the plan step use. They never fail: Agent Memory off, down or
//!   slow gives an empty list, bounded by a timeout.

use std::time::Duration;

use domain::{
    chat::{Belief, BeliefHit},
    id::{GardenId, PlantId},
    ports::{BeliefStore, BeliefSubject, BeliefWrite, Clock, MemoryStore, StoreError},
};

/// What one run of [`sync_pending`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SweepReport {
    /// Subjects erased in Agent Memory.
    pub forgotten: usize,
    /// Memories recorded.
    pub recorded: usize,
    /// Erases and records Agent Memory refused or missed; each is tried again next run.
    pub failed: usize,
}

/// The sweep, the only code that writes to Agent Memory:
/// 1. erases up to `limit` queued subjects, and after each erase re-marks the subject's
///    remaining shared memories unsynced and removes it from the queue;
/// 2. records up to `limit` unsynced shared memories, oldest first, marking each synced.
///
/// A failure is logged and counted, and the run goes on: a failed erase stays queued and holds
/// back the records under its subject (a garden's covers its plants), and a failed record
/// stays unsynced. With Agent Memory off, the queue is just emptied.
pub async fn sync_pending(
    memory: &dyn MemoryStore,
    beliefs: &dyn BeliefStore,
    clock: &dyn Clock,
    limit: u32,
) -> Result<SweepReport, StoreError> {
    let mut report = SweepReport::default();
    let mut held: Vec<BeliefSubject> = Vec::new();
    for subject in memory.belief_forgets(limit).await? {
        if beliefs.enabled() {
            if let Err(e) = beliefs.forget(subject).await {
                tracing::warn!(scope = %subject.scope_path(), error = %Chain(&e), "beliefs not erased; tried again next run");
                report.failed += 1;
                held.push(subject);
                continue;
            }
            report.forgotten += 1;
        }
        memory.finish_belief_forget(subject).await?;
    }
    if !beliefs.enabled() {
        return Ok(report);
    }
    for m in memory.unsynced_shared(limit).await? {
        let Some(write) = BeliefWrite::from_memory(&m) else {
            continue;
        };
        let under_held = held.iter().any(|h| {
            h.garden == write.subject.garden
                && (h.plant.is_none() || h.plant == write.subject.plant)
        });
        if under_held {
            continue;
        }
        match beliefs.record(&write).await {
            Ok(()) => {
                memory.mark_synced(m.id, clock.now()).await?;
                report.recorded += 1;
            }
            Err(e) => {
                tracing::warn!(memory = %m.id, error = %Chain(&e), "belief not recorded; tried again next run");
                report.failed += 1;
            }
        }
    }
    Ok(report)
}

/// The current beliefs of a plant and its garden, or of the garden alone. Empty when Agent
/// Memory is off, fails, or takes longer than `timeout`; a failure is logged.
pub async fn current_beliefs(
    beliefs: &dyn BeliefStore,
    garden: GardenId,
    plant: Option<PlantId>,
    timeout: Duration,
) -> Vec<Belief> {
    if !beliefs.enabled() {
        return Vec::new();
    }
    match tokio::time::timeout(timeout, beliefs.current(garden, plant)).await {
        Ok(Ok(found)) => found,
        Ok(Err(e)) => {
            tracing::warn!(error = %Chain(&e), "current beliefs unavailable");
            Vec::new()
        }
        Err(_) => {
            tracing::warn!(
                timeout_ms = timeout.as_millis(),
                "current beliefs timed out"
            );
            Vec::new()
        }
    }
}

/// The `k` current beliefs closest to `query`. Empty when Agent Memory is off, fails, or takes
/// longer than `timeout`; a failure is logged.
pub async fn search_beliefs(
    beliefs: &dyn BeliefStore,
    garden: GardenId,
    plant: Option<PlantId>,
    query: &str,
    k: usize,
    timeout: Duration,
) -> Vec<BeliefHit> {
    if !beliefs.enabled() {
        return Vec::new();
    }
    match tokio::time::timeout(timeout, beliefs.search(garden, plant, query, k)).await {
        Ok(Ok(hits)) => hits,
        Ok(Err(e)) => {
            tracing::warn!(error = %Chain(&e), "belief search unavailable");
            Vec::new()
        }
        Err(_) => {
            tracing::warn!(timeout_ms = timeout.as_millis(), "belief search timed out");
            Vec::new()
        }
    }
}

/// The log form of an error: its message, then each source's, joined by ": ".
pub(crate) struct Chain<'a>(pub &'a (dyn std::error::Error + 'static));

impl std::fmt::Display for Chain<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)?;
        let mut source = self.0.source();
        while let Some(error) = source {
            write!(f, ": {error}")?;
            source = error.source();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
