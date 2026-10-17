//! The belief mirror (architecture 7, memory): every **shared** garden memory (`pipeline` or
//! `user`) is mirrored into `SurrealDB` Agent Memory as one triple, through the
//! [`BeliefStore`] port. The `memory` table stays the system of record; a private chat memory
//! never leaves the database.
//!
//! - [`MirroredMemory`] wraps the store's [`MemoryStore`]. `api` installs it as `Ports.memory`,
//!   so every write in `api`, the agent and the pipeline is mirrored without the callers
//!   knowing. A failed record is logged and left unsynced; [`sync_pending`] retries it.
//! - A delete of a shared memory erases its subject's scope in Agent Memory **before** the row
//!   goes, then marks the subject's remaining shared memories unsynced, so the sweep records
//!   them again oldest first and the history is rebuilt in order. A memory is therefore never
//!   left in Agent Memory after its row is gone.
//! - [`current_beliefs`] and [`search_beliefs`] are the reads the context builder, the
//!   `search_memory` tool and the plan step use. They never fail: Agent Memory off, down or
//!   slow gives an empty list, bounded by a timeout.

use std::{sync::Arc, time::Duration};

use domain::{
    authz::{Editor, Membership, Viewer},
    chat::{Belief, BeliefHit, Memory, MemoryQuery, MemorySource},
    id::{GardenId, MemoryId, PlantId, UserId},
    ports::{
        BeliefStore, BeliefSubject, BeliefWrite, Clock, Embedding, MemoryHit, MemoryStore,
        NewMemory, StoreError,
    },
    wire::{Page, PageRequest, Timestamp},
};

use crate::error::BeliefSyncError;

/// A [`MemoryStore`] that mirrors shared memories into a [`BeliefStore`]. Every method
/// delegates to `inner`; the writes of shared memories and the delete add the mirror's steps.
#[derive(Clone)]
pub struct MirroredMemory {
    inner: Arc<dyn MemoryStore>,
    beliefs: Arc<dyn BeliefStore>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for MirroredMemory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MirroredMemory")
            .field("beliefs", &self.beliefs.enabled())
            .finish_non_exhaustive()
    }
}

impl MirroredMemory {
    /// The store, mirrored into `beliefs`; `clock` stamps `belief_synced_at`.
    pub fn new(
        inner: Arc<dyn MemoryStore>,
        beliefs: Arc<dyn BeliefStore>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            inner,
            beliefs,
            clock,
        }
    }

    /// Records a just-written memory when it is shared and Agent Memory is on, then marks it
    /// synced. Never fails the write: a failure is logged and the sweep retries.
    async fn mirror(&self, memory: &Memory) {
        if !self.beliefs.enabled() {
            return;
        }
        let Some(write) = BeliefWrite::from_memory(memory) else {
            return;
        };
        if let Err(e) = self.beliefs.record(&write).await {
            tracing::warn!(memory = %memory.id, error = %Chain(&e), "belief not recorded; the sweep retries it");
            return;
        }
        if let Err(e) = self.inner.mark_synced(memory.id, self.clock.now()).await {
            // Recording is idempotent, so the sweep sending it again is harmless.
            tracing::warn!(memory = %memory.id, error = %Chain(&e), "belief recorded but not marked synced");
        }
    }
}

#[async_trait::async_trait]
impl MemoryStore for MirroredMemory {
    async fn remember(
        &self,
        member: Membership<Viewer>,
        memory: NewMemory,
    ) -> Result<Memory, StoreError> {
        // A private chat memory: never mirrored.
        self.inner.remember(member, memory).await
    }

    async fn add_note(
        &self,
        member: Membership<Editor>,
        memory: NewMemory,
    ) -> Result<Memory, StoreError> {
        let m = self.inner.add_note(member, memory).await?;
        self.mirror(&m).await;
        Ok(m)
    }

    async fn add_pipeline_memory(
        &self,
        garden: GardenId,
        memory: NewMemory,
    ) -> Result<Memory, StoreError> {
        let m = self.inner.add_pipeline_memory(garden, memory).await?;
        self.mirror(&m).await;
        Ok(m)
    }

    async fn memories(
        &self,
        member: Membership<Viewer>,
        query: MemoryQuery,
        page: PageRequest,
    ) -> Result<Page<Memory>, StoreError> {
        self.inner.memories(member, query, page).await
    }

    async fn search(
        &self,
        member: Membership<Viewer>,
        query: &Embedding,
        plant: Option<PlantId>,
        k: usize,
    ) -> Result<Vec<MemoryHit>, StoreError> {
        self.inner.search(member, query, plant, k).await
    }

    async fn shared_for_plan(
        &self,
        garden: GardenId,
        plant: PlantId,
    ) -> Result<Vec<Memory>, StoreError> {
        self.inner.shared_for_plan(garden, plant).await
    }

    async fn share(
        &self,
        member: Membership<Editor>,
        memory: MemoryId,
    ) -> Result<Memory, StoreError> {
        let m = self.inner.share(member, memory).await?;
        self.mirror(&m).await;
        Ok(m)
    }

    async fn delete(&self, member: Membership<Viewer>, memory: MemoryId) -> Result<(), StoreError> {
        if self.beliefs.enabled()
            && let Some(m) = self.inner.memory(memory).await?
            // Outside the proof's garden the inner delete answers `NotFound`; never erase
            // another garden's beliefs.
            && m.garden_id == member.garden()
            && m.shared
        {
            let subject = BeliefSubject::of(&m);
            self.beliefs
                .forget(subject)
                .await
                .map_err(|e| StoreError::Unavailable(Box::new(e)))?;
            // The row goes before the rest are marked unsynced, so the sweep can't record it
            // again in between. If the delete fails, the row is rebuilt with the rest.
            let deleted = self.inner.delete(member, memory).await;
            // A garden's scope holds its plants' too, so the whole garden is re-recorded.
            if let Err(error) = self
                .inner
                .mark_unsynced(subject.garden, subject.plant)
                .await
            {
                tracing::error!(memory = %memory, error = %Chain(&error), "beliefs erased but not marked for the rebuild");
            }
            return deleted;
        }
        self.inner.delete(member, memory).await
    }

    async fn memory_owner(
        &self,
        memory: MemoryId,
    ) -> Result<Option<(MemorySource, Option<UserId>)>, StoreError> {
        self.inner.memory_owner(memory).await
    }

    async fn memory(&self, memory: MemoryId) -> Result<Option<Memory>, StoreError> {
        self.inner.memory(memory).await
    }

    async fn unsynced_shared(&self, limit: u32) -> Result<Vec<Memory>, StoreError> {
        self.inner.unsynced_shared(limit).await
    }

    async fn mark_synced(&self, memory: MemoryId, at: Timestamp) -> Result<(), StoreError> {
        self.inner.mark_synced(memory, at).await
    }

    async fn mark_unsynced(
        &self,
        garden: GardenId,
        plant: Option<PlantId>,
    ) -> Result<(), StoreError> {
        self.inner.mark_unsynced(garden, plant).await
    }
}

/// The sweep: records up to `limit` unsynced shared memories, oldest first, marking each
/// synced. Stops at the first failure, so a newer value never lands before an older one of the
/// same key (Agent Memory supersedes by arrival). Returns how many were recorded; 0 when Agent
/// Memory is off.
pub async fn sync_pending(
    memory: &dyn MemoryStore,
    beliefs: &dyn BeliefStore,
    clock: &dyn Clock,
    limit: u32,
) -> Result<usize, BeliefSyncError> {
    if !beliefs.enabled() {
        return Ok(0);
    }
    let pending = memory.unsynced_shared(limit).await?;
    let mut synced = 0;
    for m in &pending {
        let Some(write) = BeliefWrite::from_memory(m) else {
            continue;
        };
        beliefs
            .record(&write)
            .await
            .map_err(|source| BeliefSyncError::Record { synced, source })?;
        memory.mark_synced(m.id, clock.now()).await?;
        synced += 1;
    }
    Ok(synced)
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
struct Chain<'a>(&'a (dyn std::error::Error + 'static));

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
