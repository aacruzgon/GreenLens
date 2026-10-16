//! [`SurrealKnowledgeStore`]: the `KnowledgeStore` port over an in-memory [`KnowledgeIndex`]
//! and the HNSW chunk search ([database 5], vector search).

use std::sync::{Arc, PoisonError, RwLock};

use domain::{
    chat::{KnowledgeHit, Lookalike},
    id::{OrganismId, ProblemId, SourceId, SpeciesId, TreatmentId},
    knowledge::{
        ClassifierLabels, OrganismSeed, ProblemSeed, ProductsSeed, SeedOrganismRole,
        SoilConditionSeed, SpeciesSeed, TreatmentSeed,
    },
    ports::{
        ChunkFilter, Embedding, KnowledgeRepo, KnowledgeStore, SpeciesEntry, StoreError, VlmOptions,
    },
    records::diagnosis::{Citation, Prey},
    scan::SpeciesRef,
    vlm::{DamagePattern, SoilAspect},
    wire::{Page, PageRequest},
};

use crate::KnowledgeIndex;

/// The knowledge base for `api`, the pipeline and the agent. Lookups are answered from the
/// in-memory index; [`KnowledgeStore::search`] goes to the database.
///
/// The index sits behind `RwLock<Arc<_>>`: a reader clones the `Arc` and drops the lock at
/// once, so no lock is ever held across an `.await`, and [`Self::reload`] swaps in a new index
/// without blocking readers for longer than a pointer copy.
pub struct SurrealKnowledgeStore {
    index: RwLock<Arc<KnowledgeIndex>>,
    repo: Arc<dyn KnowledgeRepo>,
}

impl std::fmt::Debug for SurrealKnowledgeStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurrealKnowledgeStore")
            .finish_non_exhaustive()
    }
}

impl SurrealKnowledgeStore {
    /// Reads every curated record once (`KnowledgeRepo::load_knowledge`) and builds the index.
    /// `api` calls it at start-up, after the migrations, and refuses to start on an error.
    pub async fn load(repo: Arc<dyn KnowledgeRepo>) -> Result<Self, StoreError> {
        let snapshot = repo.load_knowledge().await?;
        let index = KnowledgeIndex::build(snapshot)?;
        Ok(Self {
            index: RwLock::new(Arc::new(index)),
            repo,
        })
    }

    /// Re-reads the knowledge base and swaps the index in.
    pub async fn reload(&self) -> Result<(), StoreError> {
        let index = Arc::new(KnowledgeIndex::build(self.repo.load_knowledge().await?)?);
        *self.index.write().unwrap_or_else(PoisonError::into_inner) = index;
        Ok(())
    }

    /// The current index, for synchronous callers that want borrowed profiles (the agent's
    /// validation context, the species photo index).
    pub fn index(&self) -> Arc<KnowledgeIndex> {
        Arc::clone(&self.index.read().unwrap_or_else(PoisonError::into_inner))
    }
}

fn species_ref(s: &SpeciesSeed) -> SpeciesRef {
    SpeciesRef {
        id: s.id.clone(),
        scientific_name: s.scientific_name.clone(),
        common_name: s.common_names.first().cloned().unwrap_or_default(),
    }
}

fn entry(index: &KnowledgeIndex, s: &SpeciesSeed) -> SpeciesEntry {
    SpeciesEntry {
        species: species_ref(s),
        profile: s.profile,
        classifier_supported: s.classifier_supported,
        photos: index.species_photos.get(&s.id).cloned().unwrap_or_default(),
    }
}

/// `GET /species?q=`: the species whose common or scientific names contain `q`
/// (case-insensitive), by common name then id, paged with the last id as the cursor.
fn species_page(index: &KnowledgeIndex, q: Option<&str>, page: &PageRequest) -> Page<SpeciesEntry> {
    let needle = q.map(str::to_lowercase);
    let mut matches: Vec<&SpeciesSeed> = index
        .species
        .values()
        .filter(|s| {
            needle.as_deref().is_none_or(|n| {
                s.scientific_name.to_lowercase().contains(n)
                    || s.common_names.iter().any(|c| c.to_lowercase().contains(n))
            })
        })
        .collect();
    let key = |s: &SpeciesSeed| {
        (
            s.common_names.first().map(|c| c.to_lowercase()),
            s.id.clone(),
        )
    };
    matches.sort_by_key(|s| key(s));
    let start = page
        .cursor
        .as_ref()
        .and_then(|c| matches.iter().position(|s| s.id.as_str() == c.as_str()))
        .map_or(0, |i| i + 1);
    let limit = page.limit() as usize;
    let items: Vec<SpeciesEntry> = matches
        .iter()
        .skip(start)
        .take(limit)
        .map(|s| entry(index, s))
        .collect();
    let next_cursor = (start + limit < matches.len())
        .then(|| {
            items
                .last()
                .and_then(|e| e.species.id.as_str().parse().ok())
        })
        .flatten();
    Page { items, next_cursor }
}

#[async_trait::async_trait]
impl KnowledgeStore for SurrealKnowledgeStore {
    async fn species(&self, id: &SpeciesId) -> Result<Option<SpeciesSeed>, StoreError> {
        Ok(self.index().species(id).cloned())
    }

    async fn species_list(
        &self,
        q: Option<String>,
        page: PageRequest,
    ) -> Result<Page<SpeciesEntry>, StoreError> {
        Ok(species_page(&self.index(), q.as_deref(), &page))
    }

    async fn species_entry(&self, id: &SpeciesId) -> Result<Option<SpeciesEntry>, StoreError> {
        let index = self.index();
        Ok(index.species(id).map(|s| entry(&index, s)))
    }

    async fn problem(&self, id: &ProblemId) -> Result<Option<ProblemSeed>, StoreError> {
        Ok(self.index().problem(id).cloned())
    }

    async fn problems(&self, ids: &[ProblemId]) -> Result<Vec<ProblemSeed>, StoreError> {
        let index = self.index();
        Ok(ids
            .iter()
            .filter_map(|id| index.problem(id).cloned())
            .collect())
    }

    async fn problems_of_species(
        &self,
        species: &SpeciesId,
    ) -> Result<Vec<ProblemSeed>, StoreError> {
        Ok(self.index().problems_of_species(species).cloned().collect())
    }

    async fn problems_pointed_to(
        &self,
        organisms: &[OrganismId],
        patterns: &[DamagePattern],
    ) -> Result<Vec<ProblemSeed>, StoreError> {
        Ok(self
            .index()
            .problems_pointed_to(organisms, patterns)
            .into_iter()
            .cloned()
            .collect())
    }

    async fn lookalikes(&self, problem: &ProblemId) -> Result<Vec<Lookalike>, StoreError> {
        let index = self.index();
        Ok(index
            .lookalikes(problem)
            .iter()
            .filter_map(|(id, how)| {
                index.problem(id).map(|p| Lookalike {
                    problem_id: id.clone(),
                    name: p.name.clone(),
                    how_to_tell: how.clone(),
                })
            })
            .collect())
    }

    async fn organism(&self, id: &OrganismId) -> Result<Option<OrganismSeed>, StoreError> {
        Ok(self.index().organism(id).cloned())
    }

    async fn prey_of(&self, organism: &OrganismId) -> Result<Vec<Prey>, StoreError> {
        let index = self.index();
        let Some(o) = index.organism(organism) else {
            return Ok(Vec::new());
        };
        if o.role != SeedOrganismRole::Beneficial {
            return Ok(Vec::new());
        }
        Ok(o.preys_on
            .iter()
            .flatten()
            .filter_map(|id| {
                index.organism(id).map(|p| Prey {
                    organism_id: id.clone(),
                    name: p.name.clone(),
                })
            })
            .collect())
    }

    async fn treatments(&self, ids: &[TreatmentId]) -> Result<Vec<TreatmentSeed>, StoreError> {
        let index = self.index();
        Ok(ids
            .iter()
            .filter_map(|id| index.treatments.get(id).cloned())
            .collect())
    }

    async fn citations(&self, ids: &[SourceId]) -> Result<Vec<Citation>, StoreError> {
        let index = self.index();
        let mut seen = std::collections::BTreeSet::new();
        Ok(ids
            .iter()
            .filter(|id| seen.insert(*id))
            .filter_map(|id| {
                index.sources.get(id).map(|s| Citation {
                    source_id: s.id.clone(),
                    title: s.title.clone(),
                    publisher: s.publisher.clone(),
                    url: s.url.clone(),
                })
            })
            .collect())
    }

    async fn products(&self) -> Result<ProductsSeed, StoreError> {
        Ok(self.index().products.clone())
    }

    async fn classifier_labels(&self) -> Result<ClassifierLabels, StoreError> {
        Ok(self.index().classifier_labels.clone())
    }

    async fn soil_condition(
        &self,
        aspect: SoilAspect,
        value: &str,
    ) -> Result<Option<SoilConditionSeed>, StoreError> {
        Ok(self
            .index()
            .soil_conditions
            .get(&(aspect, value.to_owned()))
            .cloned())
    }

    async fn vlm_options(&self) -> Result<VlmOptions, StoreError> {
        Ok(self.index().vlm_options.clone())
    }

    /// `WHERE <filters> AND embedding <|k, 40|> $q`: the indexed filters are applied before the
    /// graph search, so a filter never leaves fewer than `k` results ✓ ([database 5]).
    async fn search(
        &self,
        query: &Embedding,
        filter: ChunkFilter,
        k: usize,
    ) -> Result<Vec<KnowledgeHit>, StoreError> {
        self.repo.search_chunks(query, filter, k).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use domain::ports::MockKnowledgeRepo;

    async fn store() -> SurrealKnowledgeStore {
        let mut repo = MockKnowledgeRepo::new();
        repo.expect_load_knowledge()
            .returning(|| Ok(fixtures::snapshot()));
        repo.expect_search_chunks()
            .returning(|_, _, _| Ok(Vec::new()));
        SurrealKnowledgeStore::load(Arc::new(repo)).await.unwrap()
    }

    #[tokio::test]
    async fn lookups_come_from_the_index() {
        let s = store().await;
        let blight: ProblemId = "tomato_early_blight".parse().unwrap();
        assert_eq!(s.problem(&blight).await.unwrap().unwrap().id, blight);
        let alikes = s
            .lookalikes(&"tomato_septoria_leaf_spot".parse().unwrap())
            .await
            .unwrap();
        assert_eq!(alikes[0].problem_id, blight, "both directions");
        let prey = s.prey_of(&"ladybug_larva".parse().unwrap()).await.unwrap();
        assert_eq!(prey[0].organism_id.as_str(), "flea_beetle");
        assert_eq!(
            s.prey_of(&"flea_beetle".parse().unwrap()).await.unwrap(),
            Vec::<Prey>::new()
        );
        let unknown: ProblemId = "no_such_problem".parse().unwrap();
        assert_eq!(
            s.problems(&[unknown, blight.clone()]).await.unwrap().len(),
            1
        );
        let hits = s
            .search(&Embedding(vec![0.0; 4]), ChunkFilter::default(), 5)
            .await
            .unwrap();
        assert_eq!(hits, Vec::<KnowledgeHit>::new());
    }

    #[tokio::test]
    async fn species_list_filters_and_pages() {
        let s = store().await;
        let page = s
            .species_list(Some("TOM".into()), PageRequest::default())
            .await
            .unwrap();
        assert_eq!(page.items.len(), 1);
        let first = s
            .species_list(
                None,
                PageRequest {
                    cursor: None,
                    limit: Some(1),
                },
            )
            .await
            .unwrap();
        assert_eq!(first.items[0].species.id.as_str(), "potato");
        let second = s
            .species_list(
                None,
                PageRequest {
                    cursor: first.next_cursor,
                    limit: Some(1),
                },
            )
            .await
            .unwrap();
        assert_eq!(second.items[0].species.id.as_str(), "tomato");
        assert!(second.next_cursor.is_none());
    }
}
