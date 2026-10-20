//! What the chat turn, the tools and memory extraction run on: one struct of `Arc<dyn Port>`s,
//! built once by `api` and shared by every turn behind an `Arc<AgentDeps>`.
//!
//! The store is split into the repositories the agent uses rather than one `Arc<dyn Store>`, so
//! a test fakes only the areas it touches (`domain`'s `mock` feature has one mock per area) and
//! the type says which tables chat can reach. [`AgentStores::from_store`] builds them from the
//! single store `api` holds by trait upcasting (stable since Rust 1.86).

use std::sync::Arc;

use domain::ports::{
    AnalysisRepo, BeliefStore, BlobStore, ChatModel, ChatRepo, Clock, FarmRecordStore, GardenRepo,
    IdGen, KnowledgeStore, MapRepo, MemoryStore, PlanRepo, PlantRepo, ScanRepo, Store,
    TextClassifier, TextEmbedder, UsageRepo, WeatherCacheRepo, WeatherProvider,
};

use crate::{config::AgentConfig, safety::ProductCatalogue};

/// The store areas the agent reads and writes.
#[derive(Clone)]
pub struct AgentStores {
    /// Conversations, messages, cards.
    pub chats: Arc<dyn ChatRepo>,
    /// Scans and their images (`get_scan`, `view_image`).
    pub scans: Arc<dyn ScanRepo>,
    /// Observations and similar cases.
    pub analysis: Arc<dyn AnalysisRepo>,
    /// Plans and tasks.
    pub plans: Arc<dyn PlanRepo>,
    /// Plants, timelines, measurements.
    pub plants: Arc<dyn PlantRepo>,
    /// The garden's context (time zone, weather cell, soil profile).
    pub gardens: Arc<dyn GardenRepo>,
    /// The garden map.
    pub maps: Arc<dyn MapRepo>,
    /// The daily token budget.
    pub usage: Arc<dyn UsageRepo>,
    /// The weather cache.
    pub weather_cache: Arc<dyn WeatherCacheRepo>,
    /// Farm connectors and their synced records: whether the garden has any to search, and
    /// `search_farm_records`.
    pub farm: Arc<dyn FarmRecordStore>,
}

impl AgentStores {
    /// Every area from the one store `api` holds: each `Arc<dyn Store>` clone is upcast to the
    /// area's trait object.
    pub fn from_store(store: &Arc<dyn Store>) -> Self {
        Self {
            chats: store.clone(),
            scans: store.clone(),
            analysis: store.clone(),
            plans: store.clone(),
            plants: store.clone(),
            gardens: store.clone(),
            maps: store.clone(),
            usage: store.clone(),
            weather_cache: store.clone(),
            farm: store.clone(),
        }
    }
}

/// Every port the agent uses, plus its settings.
#[derive(Clone)]
pub struct AgentDeps {
    /// The store areas.
    pub stores: AgentStores,
    /// Garden memory. Its shared memories reach Agent Memory through `api`'s belief sweep
    /// ([`crate::beliefs::sync_pending`]).
    pub memory: Arc<dyn MemoryStore>,
    /// Shared memories in Agent Memory, read for the context's current beliefs and
    /// `search_memory`; `NoBeliefs` when it isn't configured.
    pub beliefs: Arc<dyn BeliefStore>,
    /// The knowledge base.
    pub knowledge: Arc<dyn KnowledgeStore>,
    /// The LLM API.
    pub chat: Arc<dyn ChatModel>,
    /// Jev or the LLM fallback.
    pub classifier: Arc<dyn TextClassifier>,
    /// The pinned text-embedding model. Queries (the user's message, `search_memory`,
    /// `search_knowledge`, `search_farm_records`) are embedded as `EmbedMode::Query`; memory texts stored by
    /// `remember` and extraction as `EmbedMode::Document`, like the knowledge chunks they are
    /// searched beside.
    pub embedder: Arc<dyn TextEmbedder>,
    /// Object storage, for `view_image`.
    pub blob: Arc<dyn BlobStore>,
    /// Open-Meteo, when the weather cache has no entry for the hour.
    pub weather: Arc<dyn WeatherProvider>,
    /// Time.
    pub clock: Arc<dyn Clock>,
    /// ULIDs.
    pub ids: Arc<dyn IdGen>,
    /// The product list for the dose check and plan validation.
    pub products: Arc<ProductCatalogue>,
    /// Limits and texts.
    pub config: AgentConfig,
}

impl std::fmt::Debug for AgentDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentDeps")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}
