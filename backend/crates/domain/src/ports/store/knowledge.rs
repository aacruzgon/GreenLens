//! The knowledge tables as the `knowledge` crate reads them ([database 3], knowledge;
//! [database 5], vector search).

use crate::{
    chat::KnowledgeHit,
    garden::species::SpeciesPhoto,
    id::{ProblemId, SpeciesId},
    knowledge::{
        ClassifierLabels, OrganismSeed, ProblemSeed, ProductsSeed, SoilConditionSeed, SourceSeed,
        SpeciesSeed, TreatmentSeed,
    },
    ports::{ChunkFilter, Embedding, ObjectRef, StoreError, VlmOptions},
    url::PhotoHash,
};

/// The whole curated knowledge base, read once at start: a few hundred records that change
/// only with a migration. Edges are resolved: `looks_like` in both directions, `preys_on` on
/// each organism.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct KnowledgeSnapshot {
    /// Every species.
    pub species: Vec<SpeciesSeed>,
    /// Each species' stored photos (hash and credit), in seed order.
    pub species_photos: Vec<(SpeciesId, Vec<SpeciesPhoto>)>,
    /// Every problem.
    pub problems: Vec<ProblemSeed>,
    /// Both directions of every `looks_like` edge: (problem, look-alike, how to tell).
    pub lookalikes: Vec<(ProblemId, ProblemId, String)>,
    /// Every organism, with `preys_on` filled.
    pub organisms: Vec<OrganismSeed>,
    /// Every treatment.
    pub treatments: Vec<TreatmentSeed>,
    /// Every source.
    pub sources: Vec<SourceSeed>,
    /// `products.yaml`.
    pub products: ProductsSeed,
    /// The current classifier labels.
    pub classifier_labels: ClassifierLabels,
    /// Every soil condition.
    pub soil_conditions: Vec<SoilConditionSeed>,
    /// The generated VLM options.
    pub vlm_options: VlmOptions,
}

/// The knowledge reads.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait KnowledgeRepo: Send + Sync {
    /// Every curated record, in one read.
    async fn load_knowledge(&self) -> Result<KnowledgeSnapshot, StoreError>;

    /// HNSW search over `knowledge_chunk.embedding` with the filters applied first:
    /// `WHERE … AND embedding <|k, 40|> $q`.
    async fn search_chunks(
        &self,
        query: &Embedding,
        filter: ChunkFilter,
        k: usize,
    ) -> Result<Vec<KnowledgeHit>, StoreError>;

    /// `GET /species/{id}/images/{hash}`: the photo's key under `knowledge/`.
    async fn species_photo_object(
        &self,
        species: &SpeciesId,
        hash: PhotoHash,
    ) -> Result<Option<ObjectRef>, StoreError>;
}
