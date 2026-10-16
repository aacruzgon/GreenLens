//! The in-memory knowledge base: every curated record, keyed and with its graph edges resolved
//! ([architecture 5, knowledge base]; [contracts 10]).

use std::collections::{BTreeMap, BTreeSet};

use domain::{
    garden::species::SpeciesPhoto,
    id::{OrganismId, ProblemId, SourceId, SpeciesId, TreatmentId},
    knowledge::{
        ClassifierLabels, OrganismSeed, ProblemSeed, ProductsSeed, SoilConditionSeed, SourceSeed,
        SpeciesSeed, TreatmentSeed,
    },
    ports::{KnowledgeSnapshot, VlmOptions},
    vlm::{DamagePattern, SoilAspect},
};

use crate::IndexError;

/// Every curated record, read once from the database and indexed for the lookups the pipeline,
/// the agent's tools and `api` make. Immutable once built: a migration that changes the
/// knowledge base restarts `api`, which builds a new index ([`crate::SurrealKnowledgeStore::reload`]
/// swaps it without a restart).
#[derive(Debug, Clone, Default)]
pub struct KnowledgeIndex {
    pub(crate) species: BTreeMap<SpeciesId, SpeciesSeed>,
    pub(crate) species_photos: BTreeMap<SpeciesId, Vec<SpeciesPhoto>>,
    pub(crate) problems: BTreeMap<ProblemId, ProblemSeed>,
    /// `susceptible_to`: species → problems, in problem-id order.
    pub(crate) by_species: BTreeMap<SpeciesId, Vec<ProblemId>>,
    /// `looks_like`, both directions: problem → (look-alike, how to tell).
    pub(crate) lookalikes: BTreeMap<ProblemId, Vec<(ProblemId, String)>>,
    pub(crate) organisms: BTreeMap<OrganismId, OrganismSeed>,
    /// `caused_by`, reversed: organism → problems.
    pub(crate) by_organism: BTreeMap<OrganismId, Vec<ProblemId>>,
    /// `damage_patterns`, reversed: pattern → problems.
    pub(crate) by_pattern: BTreeMap<DamagePattern, Vec<ProblemId>>,
    pub(crate) treatments: BTreeMap<TreatmentId, TreatmentSeed>,
    pub(crate) sources: BTreeMap<SourceId, SourceSeed>,
    pub(crate) products: ProductsSeed,
    pub(crate) classifier_labels: ClassifierLabels,
    pub(crate) soil_conditions: BTreeMap<(SoilAspect, String), SoilConditionSeed>,
    pub(crate) vlm_options: VlmOptions,
}

fn keyed<K: Ord + ToString, V>(
    items: Vec<V>,
    key: impl Fn(&V) -> K,
) -> Result<BTreeMap<K, V>, IndexError> {
    let mut map = BTreeMap::new();
    for item in items {
        let k = key(&item);
        if map.contains_key(&k) {
            return Err(IndexError::Duplicate(k.to_string()));
        }
        map.insert(k, item);
    }
    Ok(map)
}

impl KnowledgeIndex {
    /// Builds the index, re-checking the loader's structural rules ([contracts 10], checks):
    /// unique ids, exactly 3 discriminating indicators, and edges that resolve.
    pub fn build(snapshot: KnowledgeSnapshot) -> Result<Self, IndexError> {
        let KnowledgeSnapshot {
            species,
            species_photos,
            problems,
            lookalikes,
            organisms,
            treatments,
            sources,
            products,
            classifier_labels,
            soil_conditions,
            vlm_options,
        } = snapshot;
        let species = keyed(species, |s| s.id.clone())?;
        let problems = keyed(problems, |p| p.id.clone())?;
        let organisms = keyed(organisms, |o| o.id.clone())?;
        let treatments = keyed(treatments, |t| t.id.clone())?;
        let sources = keyed(sources, |s| s.id.clone())?;

        let mut by_species: BTreeMap<SpeciesId, Vec<ProblemId>> = BTreeMap::new();
        let mut by_organism: BTreeMap<OrganismId, Vec<ProblemId>> = BTreeMap::new();
        let mut by_pattern: BTreeMap<DamagePattern, Vec<ProblemId>> = BTreeMap::new();
        for p in problems.values() {
            let count = p.discriminating().count();
            if count != 3 {
                return Err(IndexError::Discriminating {
                    problem: p.id.clone(),
                    count,
                });
            }
            for s in &p.affects {
                if !species.contains_key(s) {
                    return Err(IndexError::UnknownSpecies {
                        problem: p.id.clone(),
                        species: s.clone(),
                    });
                }
                by_species.entry(s.clone()).or_default().push(p.id.clone());
            }
            for o in &p.caused_by {
                if !organisms.contains_key(o) {
                    return Err(IndexError::UnknownOrganism(o.clone()));
                }
                by_organism.entry(o.clone()).or_default().push(p.id.clone());
            }
            for d in &p.damage_patterns {
                by_pattern.entry(*d).or_default().push(p.id.clone());
            }
        }
        for o in organisms.values() {
            for prey in o.preys_on.iter().flatten() {
                if !organisms.contains_key(prey) {
                    return Err(IndexError::UnknownOrganism(prey.clone()));
                }
            }
        }

        // The snapshot carries both directions already (KnowledgeSnapshot.lookalikes); the
        // profiles' own one-direction `looks_like` lists are folded in too, so an index built
        // from seeds alone (tests) has both directions as well. Duplicates are dropped.
        let mut seen: BTreeSet<(ProblemId, ProblemId)> = BTreeSet::new();
        let mut look: BTreeMap<ProblemId, Vec<(ProblemId, String)>> = BTreeMap::new();
        let from_profiles = problems.values().flat_map(|p| {
            p.looks_like.iter().flat_map(move |l| {
                [
                    (p.id.clone(), l.problem.clone(), l.how_to_tell.clone()),
                    (l.problem.clone(), p.id.clone(), l.how_to_tell.clone()),
                ]
            })
        });
        for (a, b, how) in lookalikes
            .into_iter()
            .chain(from_profiles.collect::<Vec<_>>())
        {
            for id in [&a, &b] {
                if !problems.contains_key(id) {
                    return Err(IndexError::UnknownProblem(id.clone()));
                }
            }
            if seen.insert((a.clone(), b.clone())) {
                look.entry(a).or_default().push((b, how));
            }
        }

        let soil_conditions = soil_conditions
            .into_iter()
            .map(|c| ((c.aspect, c.value.clone()), c))
            .collect();

        Ok(Self {
            species,
            species_photos: species_photos.into_iter().collect(),
            problems,
            by_species,
            lookalikes: look,
            organisms,
            by_organism,
            by_pattern,
            treatments,
            sources,
            products,
            classifier_labels,
            soil_conditions,
            vlm_options,
        })
    }

    /// A species profile.
    pub fn species(&self, id: &SpeciesId) -> Option<&SpeciesSeed> {
        self.species.get(id)
    }

    /// A problem profile.
    pub fn problem(&self, id: &ProblemId) -> Option<&ProblemSeed> {
        self.problems.get(id)
    }

    /// The problems a species is `susceptible_to`, in id order.
    pub fn problems_of_species<'k>(
        &'k self,
        species: &SpeciesId,
    ) -> impl Iterator<Item = &'k ProblemSeed> + use<'k> {
        let ids = self.by_species.get(species).cloned().unwrap_or_default();
        ids.into_iter().filter_map(move |id| self.problems.get(&id))
    }

    /// Both directions of `looks_like`: (look-alike, how to tell).
    pub fn lookalikes(&self, problem: &ProblemId) -> &[(ProblemId, String)] {
        self.lookalikes.get(problem).map_or(&[], Vec::as_slice)
    }

    /// The problems an organism (`caused_by`) or a damage pattern points to, deduplicated, in
    /// the order first met.
    pub fn problems_pointed_to(
        &self,
        organisms: &[OrganismId],
        patterns: &[DamagePattern],
    ) -> Vec<&ProblemSeed> {
        let mut seen = BTreeSet::new();
        organisms
            .iter()
            .filter_map(|o| self.by_organism.get(o))
            .chain(patterns.iter().filter_map(|d| self.by_pattern.get(d)))
            .flatten()
            .filter(|id| seen.insert(*id))
            .filter_map(|id| self.problems.get(id))
            .collect()
    }

    /// An organism profile.
    pub fn organism(&self, id: &OrganismId) -> Option<&OrganismSeed> {
        self.organisms.get(id)
    }

    /// Every record of a table, for the agent's validation context.
    pub fn all_treatments(&self) -> impl Iterator<Item = &TreatmentSeed> {
        self.treatments.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    #[test]
    fn builds_both_lookalike_directions_and_reverse_edges() {
        let index = KnowledgeIndex::build(fixtures::snapshot()).unwrap();
        let blight: ProblemId = "tomato_early_blight".parse().unwrap();
        let septoria: ProblemId = "tomato_septoria_leaf_spot".parse().unwrap();
        assert_eq!(index.lookalikes(&blight)[0].0, septoria);
        assert_eq!(index.lookalikes(&septoria)[0].0, blight);
        let tomato: SpeciesId = "tomato".parse().unwrap();
        assert_eq!(index.problems_of_species(&tomato).count(), 3);
        let flea: OrganismId = "flea_beetle".parse().unwrap();
        let pointed = index.problems_pointed_to(&[flea], &[DamagePattern::ShotHoles]);
        assert_eq!(pointed.len(), 1);
    }

    #[test]
    fn rejects_a_profile_without_three_discriminating_indicators() {
        let mut snap = fixtures::snapshot();
        snap.problems[0].indicators[0].discriminating = false;
        assert!(matches!(
            KnowledgeIndex::build(snap),
            Err(IndexError::Discriminating { count: 2, .. })
        ));
    }
}
