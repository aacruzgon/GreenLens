//! The belief store: shared memories mirrored into `SurrealDB` Agent Memory (architecture 7,
//! memory; docs/database/agent-memory.md).
//!
//! The `memory` table stays the system of record. Each **shared** memory (`pipeline` and
//! `user`) is also recorded in Agent Memory as one triple: entity `plant_<id>` or `garden_<id>`,
//! a key from [`belief_key`], and the memory's text as the value. Agent Memory then supersedes the
//! older value of the same key and keeps the history. A private chat memory never leaves the
//! database.
//!
//! Three facts measured against the hosted service on 2026-10-17 shape the port:
//! - entities are matched by **name across the whole context**, so the entity name is our own
//!   id, never a name a model extracted;
//! - a read at a scope path returns only facts tagged at that exact path, so a plant read asks
//!   for the plant's path **or** the garden's;
//! - `/query` takes 2 to 10 s, so only `search` uses it, behind a timeout; the context builder
//!   and the plan writer use `current`, a direct entity read of about 0.4 s.

use crate::{
    chat::{Belief, BeliefHit, Memory, MemoryKind, MemorySource, MemoryTopic},
    id::{GardenId, MemoryId, PlantId},
    ports::ProviderError,
    wire::Timestamp,
};

/// The provider name on every [`ProviderError`] of the belief store.
pub const BELIEFS_PROVIDER: &str = "agent_memory";

/// What a belief is about: one plant, or its garden as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BeliefSubject {
    /// The garden.
    pub garden: GardenId,
    /// The plant; none for a garden-level fact.
    pub plant: Option<PlantId>,
}

impl BeliefSubject {
    /// The subject a memory belongs to.
    pub const fn of(memory: &Memory) -> Self {
        Self {
            garden: memory.garden_id,
            plant: memory.plant_id,
        }
    }

    /// `gardens/<garden>` or `gardens/<garden>/plants/<plant>`: the scope path its facts are
    /// tagged with, and the subtree a forget erases. Ids are lowercased: Agent Memory refuses an
    /// uppercase letter in a path segment (`400 illegal character in segment`, measured
    /// 2026-10-17), and a ULID is case-insensitive.
    pub fn scope_path(&self) -> String {
        let garden = self.garden.to_string().to_ascii_lowercase();
        match self.plant {
            Some(p) => format!(
                "gardens/{garden}/plants/{}",
                p.to_string().to_ascii_lowercase()
            ),
            None => format!("gardens/{garden}"),
        }
    }

    /// `plant_<id>` or `garden_<id>`, lowercased like the path: unique in the context, because
    /// Agent Memory matches entities by name across every scope.
    pub fn entity_name(&self) -> String {
        match self.plant {
            Some(p) => format!("plant_{p}"),
            None => format!("garden_{}", self.garden),
        }
        .to_ascii_lowercase()
    }
}

/// The belief key of a memory. A newer memory with the same key on the same subject supersedes
/// the older one; `other` facts get a key of their own, so they never supersede anything.
///
/// | kind | key |
/// |---|---|
/// | `summary` | `scan_summary` |
/// | `fact`, topic `other` | `note_<memory id>` |
/// | `fact` | the topic: `watering`, `container`, … |
/// | `event` | `last_<topic>`: `last_treatment`, `last_other`, … |
pub fn belief_key(kind: MemoryKind, topic: MemoryTopic, id: MemoryId) -> String {
    match (kind, topic) {
        (MemoryKind::Summary, _) => "scan_summary".to_owned(),
        (MemoryKind::Fact, MemoryTopic::Other) => format!("note_{id}"),
        (MemoryKind::Fact, t) => t.as_str().to_owned(),
        (MemoryKind::Event, t) => format!("last_{}", t.as_str()),
    }
}

/// One shared memory, ready to record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeliefWrite {
    /// The plant or garden.
    pub subject: BeliefSubject,
    /// From [`belief_key`].
    pub key: String,
    /// The memory's text.
    pub value: String,
    /// The memory, sent as the label `memory=<id>` so a belief traces back to its row.
    pub memory: MemoryId,
    /// `pipeline` or `user`, sent as the label `source=<source>`.
    pub source: MemorySource,
    /// The memory's `created_at`, sent as `observed_at`, so a rebuild replays the history in
    /// its original order.
    pub observed_at: Timestamp,
}

impl BeliefWrite {
    /// The write for a memory; `None` for a private chat memory, which is never recorded.
    pub fn from_memory(memory: &Memory) -> Option<Self> {
        if !memory.shared {
            return None;
        }
        Some(Self {
            subject: BeliefSubject::of(memory),
            key: belief_key(memory.kind, memory.topic, memory.id),
            value: memory.text.clone(),
            memory: memory.id,
            source: memory.source,
            observed_at: memory.created_at,
        })
    }
}

/// Shared memories in `SurrealDB` Agent Memory. `integrations` implements it over the REST API;
/// [`NoBeliefs`] stands in when it isn't configured. Every method is scoped to one garden by
/// its scope paths, and the caller has already checked membership.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait BeliefStore: Send + Sync {
    /// `false` for [`NoBeliefs`]: callers skip the work, and `GET /plants/{id}/beliefs` answers
    /// `available: false`.
    fn enabled(&self) -> bool;

    /// Records one shared memory as a triple, registering its scope path first. Idempotent:
    /// re-sending the same write leaves one current value.
    async fn record(&self, write: &BeliefWrite) -> Result<(), ProviderError>;

    /// The current beliefs of a plant and of its garden, or of the garden alone: one per key.
    async fn current(
        &self,
        garden: GardenId,
        plant: Option<PlantId>,
    ) -> Result<Vec<Belief>, ProviderError>;

    /// Every belief of one subject, current and superseded, newest first.
    async fn history(&self, subject: BeliefSubject) -> Result<Vec<Belief>, ProviderError>;

    /// The `k` current beliefs of the plant and its garden (or the garden alone) closest to a
    /// query. Slow (2 to 10 s measured), so callers bound it with a timeout.
    async fn search(
        &self,
        garden: GardenId,
        plant: Option<PlantId>,
        query: &str,
        k: usize,
    ) -> Result<Vec<BeliefHit>, ProviderError>;

    /// Erases everything under the subject's scope path, permanently: a garden takes its
    /// plants with it. Idempotent.
    async fn forget(&self, subject: BeliefSubject) -> Result<(), ProviderError>;
}

/// The belief store when Agent Memory isn't configured: records nothing, knows nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoBeliefs;

#[async_trait::async_trait]
impl BeliefStore for NoBeliefs {
    fn enabled(&self) -> bool {
        false
    }

    async fn record(&self, _write: &BeliefWrite) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn current(
        &self,
        _garden: GardenId,
        _plant: Option<PlantId>,
    ) -> Result<Vec<Belief>, ProviderError> {
        Ok(Vec::new())
    }

    async fn history(&self, _subject: BeliefSubject) -> Result<Vec<Belief>, ProviderError> {
        Ok(Vec::new())
    }

    async fn search(
        &self,
        _garden: GardenId,
        _plant: Option<PlantId>,
        _query: &str,
        _k: usize,
    ) -> Result<Vec<BeliefHit>, ProviderError> {
        Ok(Vec::new())
    }

    async fn forget(&self, _subject: BeliefSubject) -> Result<(), ProviderError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory(kind: MemoryKind, topic: MemoryTopic, shared: bool) -> Memory {
        Memory {
            id: "01JAX3K9T6R2B8V0QZ4M7D1E5F".parse().unwrap(),
            garden_id: "01JAX3K9T6R2B8V0QZ4M7D1E5G".parse().unwrap(),
            plant_id: Some("01JAX3K9T6R2B8V0QZ4M7D1E5H".parse().unwrap()),
            kind,
            topic,
            text: "Watered every 2 days.".to_owned(),
            source: if shared {
                MemorySource::User
            } else {
                MemorySource::Chat
            },
            shared,
            created_at: "2026-10-17T15:00:00Z".parse().unwrap(),
        }
    }

    #[test]
    fn keys_follow_kind_and_topic() {
        let id: MemoryId = "01JAX3K9T6R2B8V0QZ4M7D1E5F".parse().unwrap();
        assert_eq!(
            belief_key(MemoryKind::Summary, MemoryTopic::Other, id),
            "scan_summary"
        );
        assert_eq!(
            belief_key(MemoryKind::Fact, MemoryTopic::Watering, id),
            "watering"
        );
        assert_eq!(
            belief_key(MemoryKind::Event, MemoryTopic::Treatment, id),
            "last_treatment"
        );
        assert_eq!(
            belief_key(MemoryKind::Fact, MemoryTopic::Other, id),
            "note_01JAX3K9T6R2B8V0QZ4M7D1E5F"
        );
    }

    #[test]
    fn subjects_name_paths_and_entities_by_id() {
        let m = memory(MemoryKind::Fact, MemoryTopic::Watering, true);
        let plant = BeliefSubject::of(&m);
        assert_eq!(
            plant.scope_path(),
            "gardens/01jax3k9t6r2b8v0qz4m7d1e5g/plants/01jax3k9t6r2b8v0qz4m7d1e5h"
        );
        assert_eq!(plant.entity_name(), "plant_01jax3k9t6r2b8v0qz4m7d1e5h");
        let garden = BeliefSubject {
            plant: None,
            ..plant
        };
        assert_eq!(garden.scope_path(), "gardens/01jax3k9t6r2b8v0qz4m7d1e5g");
        assert_eq!(garden.entity_name(), "garden_01jax3k9t6r2b8v0qz4m7d1e5g");
    }

    #[test]
    fn only_shared_memories_become_writes() {
        assert!(
            BeliefWrite::from_memory(&memory(MemoryKind::Fact, MemoryTopic::Watering, false))
                .is_none()
        );
        let w = BeliefWrite::from_memory(&memory(MemoryKind::Fact, MemoryTopic::Watering, true))
            .unwrap();
        assert_eq!(w.key, "watering");
        assert_eq!(w.value, "Watered every 2 days.");
        assert_eq!(w.observed_at, "2026-10-17T15:00:00Z".parse().unwrap());
    }
}
