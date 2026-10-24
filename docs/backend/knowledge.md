# PDM Backend: `knowledge` crate

> **Status:** Draft v0.3, 2026-10-24. v0.3: the disease classifier's call counts in a candidate's score (`classifier_weight`), `confirmed` needs a matched indicator, and open questions 2 and 3 are settled (backend roadmap S4). v0.2: weather fit states that its only shared weather definition is the humid hour of `domain::rules::weather::WeatherThresholds`, and the test fixture's `frost_risk` follows that type's rule. v0.1: the knowledge store over an in-memory index and the HNSW chunk search, plus the pure rules of retrieve-then-verify (candidate ranking, weather fit, scoring, diagnosis status and the question choice), compiled on Rust 1.99.1 with 19 tests including two property tests.

Companion to the [backend architecture](README.md). It implements [architecture 5, knowledge base](../architecture/README.md#knowledge-base) and [6.3, retrieve then verify](../architecture/README.md#63-retrieve-then-verify), [contracts 7, diagnosis](../architecture/contracts.md#diagnosis) and [contracts 10](../architecture/contracts.md#10-knowledge-seed-format), over [database 5](../database/README.md#5-reads-indexes-and-vector-search). Every listing is copied from `crates/knowledge` by the docgen tool. `cargo check`, `cargo clippy --all-targets` and `cargo test -p knowledge` pass.

## Contents

1. [Purpose and boundaries](#1-purpose-and-boundaries)
2. [Dependencies](#2-dependencies)
3. [Module layout](#3-module-layout)
4. [The knowledge store](#4-the-knowledge-store)
5. [Candidate ranking](#5-candidate-ranking)
6. [Weather fit](#6-weather-fit)
7. [Scoring](#7-scoring)
8. [Diagnosis status and the question](#8-diagnosis-status-and-the-question)
9. [Concurrency and lifetimes](#9-concurrency-and-lifetimes)
10. [Errors](#10-errors)
11. [Testing](#11-testing)
12. [Open questions](#12-open-questions)
13. [Sources](#13-sources)

---

## 1. Purpose and boundaries

**What it owns.**

- `SurrealKnowledgeStore`, the implementation of `domain::ports::KnowledgeStore` ([domain 9](domain.md#9-boundary-traits)). It reads the curated records once through `KnowledgeRepo::load_knowledge`, builds an in-memory `KnowledgeIndex` with every graph edge resolved, and answers every lookup from memory. Chunk search goes to the database's HNSW index.
- The pure rules of retrieve-then-verify:
  - the pre-VLM candidate set (about 9 profiles);
  - the weather fit;
  - the deterministic score, with one answer per indicator;
  - the diagnosis status with `low_evidence`;
  - the choice between an indicator question and a photo request.

**What it never does.** It never writes. The loader in `ml/knowledge/` owns the seed and the migration ([contracts 10, checks](../architecture/contracts.md#checks-and-generated-files)). It never calls a model, holds no clock and sends no event. It doesn't answer measurable indicators: that needs measurements, so it lives in [pipeline 6](pipeline.md#6-retrieve).

**Who calls it.**

- `api` builds the store at start-up and passes it as `Arc<dyn KnowledgeStore>` to the pipeline, the agent's tools and the species endpoints.
- `pipeline` calls the rules inside its `retrieve` and `score` steps.

**Where it departs from the crate graph.** The [fixed crate graph](README.md#5-crate-graph-and-dependency-rules) draws `knowledge → store`. The crate names only the `KnowledgeRepo` port from `domain`, which `store` implements and `api` wires, so the edge is left out of `Cargo.toml`. Tests then run on a mock without a database, and the build doesn't wait on `store`.

---

## 2. Dependencies

```toml
[package]
name = "knowledge"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
# The shapes, the `KnowledgeStore` port it implements and the `KnowledgeRepo` port it reads.
domain.workspace = true
# `#[async_trait]` on the `KnowledgeStore` implementation (dyn-compatible async, domain.md 9).
async-trait.workspace = true
thiserror.workspace = true
tracing.workspace = true

[dev-dependencies]
domain = { workspace = true, features = ["mock", "test-util"] }
proptest.workspace = true
serde_json.workspace = true
tokio = { workspace = true, features = ["test-util"] }
```

| Crate | Why |
|---|---|
| `domain` | The seed types (`ProblemSeed`, `SpeciesSeed`, …), the `KnowledgeStore` and `KnowledgeRepo` ports, `CandidateScore`, `IndicatorEvidence`, the limits |
| `async-trait` | The port is a dyn-compatible async trait ([domain 9](domain.md#9-boundary-traits)), implemented with `#[async_trait]` |
| `thiserror` | `IndexError` |
| `tracing` | Logs from the store |
| `proptest` (dev) | Property tests of scoring and status, as the `rust-testing` skill (§5, `.claude/skills/`) describes |
| `domain` with `mock`, `test-util` (dev) | `MockKnowledgeRepo` |

---

## 3. Module layout

```
crates/knowledge/src/
├─ lib.rs          re-exports; crate docs
├─ error.rs        IndexError, mapped into StoreError
├─ index.rs        KnowledgeIndex: every record keyed, edges resolved, loader checks re-run
├─ store.rs        SurrealKnowledgeStore: the KnowledgeStore port
├─ candidates.rs   ClassifierHit, ProfilePool<'k>, Candidate<'k>, CandidateSet<'k>, RankInput<'a, 'k>, ranking
├─ weather_fit.rs  ConditionCheck, weather_fit, fit_rank
├─ scoring.rs      AnswerSet<'e>, score_candidate, Scored<'k>, score_all
├─ status.rs       FindingSummary, StatusInput<'a>, StatusDecision, Ambiguity, QuestionChoice<'k>; proptests
└─ fixtures.rs     (tests) a small tomato knowledge base
```

---

## 4. The knowledge store

The knowledge base is a few hundred records that change only with a migration ([architecture 5](../architecture/README.md#knowledge-base)). Reading them once at start-up turns every profile lookup in a scan's steps into a map read. The index re-checks the loader's structural rules ([contracts 10, checks](../architecture/contracts.md#checks-and-generated-files)): unique ids, exactly 3 discriminating indicators, and resolvable `affects`, `caused_by`, `preys_on` and `looks_like` edges. A hand-edited database therefore fails `api`'s start instead of producing confident wrong advice.

```rust
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
```

`looks_like` is written once in the seed and both ways by the loader. The index also folds in each profile's own one-way list, so an index built from seeds alone has both directions as well:

```rust
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
```

The store keeps the index behind `RwLock<Arc<_>>`. A reader clones the `Arc` and drops the lock at once ([std `RwLock`](https://doc.rust-lang.org/std/sync/struct.RwLock.html)), so no guard is ever held across an `.await`, which the `rust-async` skill (§7, `.claude/skills/`) forbids. `reload` swaps in a new index after a migration without blocking readers. A poisoned lock is recovered with `PoisonError::into_inner`: replacing a pointer can't leave the data half-written.

```rust
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
```

```rust
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
```

| Port method | Answer | Source |
|---|---|---|
| `species`, `species_entry` | From the index; `common_name` is the first of `common_names` | [contracts 10, other entity shapes](../architecture/contracts.md#other-entity-shapes) |
| `species_list` | Common or scientific name contains `q`, case-insensitive; sorted by common name, then id; the cursor is the last id | [contracts 9, species](../architecture/contracts.md#species) |
| `problem`, `problems` | `problems` keeps the order asked and leaves out unknown ids | |
| `problems_of_species` | `susceptible_to` (the reverse of `affects`), pests and wildlife included | [architecture 6.3](../architecture/README.md#63-retrieve-then-verify) |
| `problems_pointed_to` | `caused_by` reversed and `damage_patterns` reversed, without duplicates | [architecture 6.5](../architecture/README.md#65-organisms-and-damage) |
| `lookalikes` | Both directions, with how to tell them apart | |
| `prey_of` | Beneficial organisms only; others get an empty list | [contracts 7](../architecture/contracts.md#diagnosis) `organisms[].preys_on` |
| `citations` | Sources resolved, without duplicates, in the order asked | |
| `search` | `KnowledgeRepo::search_chunks`: `WHERE <filters> AND embedding <\|k, 40\|> $q`, with the indexed filters applied before the graph search ✓ | [database 5](../database/README.md#5-reads-indexes-and-vector-search) |

The `KnowledgeStore` port has no separate methods for the BioCLIP label set version, the species options or the organism options: `vlm_options()` returns all three as the loader generated them, and `classifier_labels()` carries `model_version`, which the pipeline sends as `classifier_labels_version`.

---

## 5. Candidate ranking

[Architecture 6.3](../architecture/README.md#63-retrieve-then-verify) and [contracts 4](../architecture/contracts.md#event-catalogue) (`candidates_selected`):

| Step | Rule |
|---|---|
| Pool | The classifier's top 3 (`source: classifier`) and the problems the working species is `susceptible_to` (`source: species`). A problem in both keeps `classifier` |
| Order | Classifier score, higher first, none last; then weather fit (favourable, neutral, unfavourable); then prevalence (common, occasional, rare); then the id, so the order is total |
| Cap | At most [`MAX_CANDIDATES`](domain.md#7-rules) (9) from the pool |
| Fill | Look-alikes of the ranked candidates, in rank order, fill the slots left (`source: lookalike`) |
| Indicators | Each candidate brings its 3 `discriminating` indicators, chosen at ingestion |
| Outside the cap | Problems the VLM's organisms or damage point to are added later by one `verify` job (`source: organism` or `damage`) |

The classifier candidates come first: the 3 distinct problems with the highest confidence across all regions, ignoring `healthy` ([contracts 5, results](../architecture/contracts.md#results)).

```rust
/// One of the classifier's candidates: a problem and its best confidence across regions.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassifierHit {
    /// The problem.
    pub problem: ProblemId,
    /// Its highest confidence across the leaf regions.
    pub confidence: Confidence,
}

/// The scan's classifier candidates ([contracts 5], classifier candidates): the 3 distinct
/// problems with the highest confidence across all regions' `top` lists, ignoring `healthy`.
pub fn classifier_hits(
    results: impl IntoIterator<Item = (ProblemOrHealthy, Confidence)>,
) -> Vec<ClassifierHit> {
    let mut best: BTreeMap<ProblemId, Confidence> = BTreeMap::new();
    for (problem, confidence) in results {
        if let ProblemOrHealthy::Problem(id) = problem {
            let entry = best.entry(id).or_insert(confidence);
            if confidence > *entry {
                *entry = confidence;
            }
        }
    }
    let mut hits: Vec<ClassifierHit> = best
        .into_iter()
        .map(|(problem, confidence)| ClassifierHit {
            problem,
            confidence,
        })
        .collect();
    hits.sort_by(|a, b| {
        b.confidence
            .get()
            .total_cmp(&a.confidence.get())
            .then_with(|| a.problem.cmp(&b.problem))
    });
    hits.truncate(3);
    hits
}
```

A `ProfilePool<'k>` indexes the profiles the step already holds. `get` returns `&'k ProblemSeed`, borrowed for the records' lifetime and not for the borrow of the pool, so a candidate outlives the lookup that found it:

```rust
/// The profiles a step holds, looked up by id. Borrows them for `'k`.
#[derive(Debug, Clone, Default)]
pub struct ProfilePool<'k> {
    by_id: BTreeMap<&'k ProblemId, &'k ProblemSeed>,
}
```

```rust
impl<'k> ProfilePool<'k> {
    /// Indexes the profiles; a later duplicate is ignored.
    pub fn new(profiles: impl IntoIterator<Item = &'k ProblemSeed>) -> Self {
        let mut by_id = BTreeMap::new();
        for p in profiles {
            by_id.entry(&p.id).or_insert(p);
        }
        Self { by_id }
    }

    /// A profile, borrowed for `'k` (not for the borrow of `self`).
    pub fn get(&self, id: &ProblemId) -> Option<&'k ProblemSeed> {
        self.by_id.get(id).copied()
    }
}
```

```rust
/// A selected candidate: its profile, why it is in the set, its classifier score and its
/// weather fit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate<'k> {
    /// The profile.
    pub profile: &'k ProblemSeed,
    /// Why it was selected.
    pub source: SelectionSource,
    /// The classifier's confidence, when it was a classifier candidate.
    pub classifier: Option<Confidence>,
    /// The weather against its conditions.
    pub weather_fit: WeatherFit,
}

/// The ranking order: classifier score (higher first, none last), weather fit (favourable
/// first), prevalence (common first), then the id, so the order is total and stable.
pub fn rank_order(a: &Candidate<'_>, b: &Candidate<'_>) -> std::cmp::Ordering {
    let score = |c: &Candidate<'_>| c.classifier.map_or(-1.0, Confidence::get);
    score(b)
        .total_cmp(&score(a))
        .then_with(|| fit_rank(a.weather_fit).cmp(&fit_rank(b.weather_fit)))
        .then_with(|| a.profile.prevalence.cmp(&b.profile.prevalence))
        .then_with(|| a.profile.id.cmp(&b.profile.id))
}

/// The run's candidate set, in rank order, at most [`MAX_CANDIDATES`] plus any a `verify` job
/// added.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CandidateSet<'k> {
    candidates: Vec<Candidate<'k>>,
}

/// What the ranking reads. `'a` borrows the inputs for the call; `'k` is the profiles'
/// lifetime, which the result keeps.
#[derive(Debug, Clone, Copy)]
pub struct RankInput<'a, 'k> {
    /// Every profile the ranking may select: the classifier hits', the species', and their
    /// look-alikes'.
    pub pool: &'a ProfilePool<'k>,
    /// The classifier's top 3, empty when it didn't run.
    pub classifier: &'a [ClassifierHit],
    /// The problems the working species is `susceptible_to` (pests and wildlife included).
    pub species_problems: &'a [ProblemId],
    /// `looks_like`, both directions.
    pub lookalikes: &'a BTreeMap<ProblemId, Vec<ProblemId>>,
    /// The run's weather, when it could be fetched.
    pub weather: Option<&'a WeatherSummary>,
    /// The cap, [`MAX_CANDIDATES`] in production.
    pub cap: usize,
}

/// Ranks the candidate set: classifier hits and species problems by [`rank_order`], cut at
/// the cap; then look-alikes of the ranked candidates, in rank order, fill what is left.
/// A look-alike already in the set isn't added twice, and a classifier hit that is also a
/// species problem keeps the source `classifier`.
pub fn rank_candidates<'k>(input: &RankInput<'_, 'k>) -> CandidateSet<'k> {
    let make = |id: &ProblemId, source: SelectionSource| -> Option<Candidate<'k>> {
        let profile = input.pool.get(id)?;
        Some(Candidate {
            profile,
            source,
            classifier: input
                .classifier
                .iter()
                .find(|h| &h.problem == id)
                .map(|h| h.confidence),
            weather_fit: weather_fit(&profile.conditions, input.weather),
        })
    };
    let mut seen: BTreeSet<ProblemId> = BTreeSet::new();
    let mut primary: Vec<Candidate<'k>> = Vec::new();
    let tagged = input
        .classifier
        .iter()
        .map(|h| (&h.problem, SelectionSource::Classifier))
        .chain(
            input
                .species_problems
                .iter()
                .map(|p| (p, SelectionSource::Species)),
        );
    for (id, source) in tagged {
        if seen.contains(id) {
            continue;
        }
        if let Some(c) = make(id, source) {
            seen.insert(id.clone());
            primary.push(c);
        }
    }
    primary.sort_by(rank_order);
    primary.truncate(input.cap);

    let mut set = primary.clone();
    'fill: for c in &primary {
        for alike in input.lookalikes.get(&c.profile.id).into_iter().flatten() {
            if set.len() >= input.cap {
                break 'fill;
            }
            if seen.contains(alike) {
                continue;
            }
            if let Some(l) = make(alike, SelectionSource::Lookalike) {
                seen.insert(alike.clone());
                set.push(l);
            }
        }
    }
    CandidateSet { candidates: set }
}

/// The candidates a `verify` job adds ([architecture 6.3], step 2): the problems the VLM's
/// organisms (`source: organism`) and damage patterns (`source: damage`) point to that aren't
/// in the set, at most [`MAX_CANDIDATES`].
pub fn outside_candidates<'k>(
    set: &CandidateSet<'k>,
    pointed: &[(&'k ProblemSeed, SelectionSource)],
    weather: Option<&WeatherSummary>,
) -> Vec<Candidate<'k>> {
    let mut seen = BTreeSet::new();
    pointed
        .iter()
        .filter(|(p, _)| !set.contains(&p.id) && seen.insert(&p.id))
        .take(MAX_CANDIDATES)
        .map(|&(profile, source)| Candidate {
            profile,
            source,
            classifier: None,
            weather_fit: weather_fit(&profile.conditions, weather),
        })
        .collect()
}
```

`score` doesn't re-rank: it rebuilds the stored set (`ScanSnapshot.candidates`) with `CandidateSet::from_selected`, so a sweep's re-run scores the same candidates as the original.

---

## 6. Weather fit

Contracts 7 names `favourable`, `neutral` and `unfavourable` but not the arithmetic, so this crate fixes it (open question 1). Each condition the profile states is checked against the current weather, the past 7 days and the next 3 forecast days:

| Condition | Met when |
|---|---|
| `temp_c` | The current temperature, or a forecast day's mean of `t_min_c` and `t_max_c`, is inside the range |
| `rh_pct_min` | The current humidity, the past week's mean, or a forecast day's mean is at or above it |
| `humid_hours_min` | The past week's hours over 90%, or the forecast days' sum, is at or above it (the leaf-wetness stand-in, [contracts 7, WeatherSummary](../architecture/contracts.md#weathersummary)) |

| Met | Fit |
|---|---|
| All stated | `favourable` |
| None | `unfavourable` |
| Some, or the profile states none, or no weather | `neutral` |

Every number compared here is the profile's own. The one shared weather definition weather fit relies on is the humid hour, RH strictly over 90% (`WeatherThresholds::HUMID_HOUR_RH_PCT`, what `hours_rh_over_90` counts). Frost, the humid day and the humid stretch are the shared `domain::rules::weather::WeatherThresholds` ([domain 7, weather thresholds](domain.md#weather-thresholds)), read by the pipeline's fusion and `api`'s triggers, not here; the test fixture builds its `frost_risk` with that type's rule so it can't drift.

```rust
/// How many forecast days count, from today.
pub const FORECAST_DAYS: usize = 3;

/// One condition's result, for the fusion drivers' wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConditionCheck {
    /// Conditions the profile states (0–3).
    pub stated: u8,
    /// Of those, how many the weather meets.
    pub met: u8,
}

/// Checks each stated condition:
/// - `temp_c`: the current temperature, or a forecast day's mean of `t_min_c` and `t_max_c`,
///   inside the range;
/// - `rh_pct_min`: the current humidity, the past 7 days' mean, or a forecast day's mean at or
///   above it;
/// - `humid_hours_min`: the past 7 days' hours over 90%, or the forecast days' sum, at or above
///   it (the leaf-wetness stand-in, [contracts 7], `WeatherSummary`).
pub fn check(conditions: &Conditions, weather: &WeatherSummary) -> ConditionCheck {
    let days = || weather.forecast.iter().take(FORECAST_DAYS);
    let mut stated = 0;
    let mut met = 0;
    if let Some(range) = conditions.temp_c {
        stated += 1;
        let inside = |t: f64| t >= range.min && t <= range.max;
        if inside(weather.current.temp_c)
            || days().any(|d| inside(f64::midpoint(d.t_min_c, d.t_max_c)))
        {
            met += 1;
        }
    }
    if let Some(rh) = conditions.rh_pct_min {
        stated += 1;
        if weather.current.rh_pct >= rh
            || weather.past_7_days.mean_rh_pct >= rh
            || days().any(|d| d.mean_rh_pct >= rh)
        {
            met += 1;
        }
    }
    if let Some(hours) = conditions.humid_hours_min {
        stated += 1;
        let ahead: u32 = days().map(|d| d.hours_rh_over_90).sum();
        if weather.past_7_days.hours_rh_over_90 >= hours || ahead >= hours {
            met += 1;
        }
    }
    ConditionCheck { stated, met }
}

/// The fit of `conditions` under `weather`; `neutral` without weather.
pub fn weather_fit(conditions: &Conditions, weather: Option<&WeatherSummary>) -> WeatherFit {
    weather.map_or(WeatherFit::Neutral, |w| check(conditions, w).fit())
}

/// The ranking order of a fit: favourable first.
pub const fn fit_rank(fit: WeatherFit) -> u8 {
    match fit {
        WeatherFit::Favourable => 0,
        WeatherFit::Neutral => 1,
        WeatherFit::Unfavourable => 2,
    }
}
```

---

## 7. Scoring

[Architecture 6.3](../architecture/README.md#63-retrieve-then-verify) step 3 and [contracts 7, measurable indicators](../architecture/contracts.md#measurement):

| Rule | Here |
|---|---|
| Score | Sum of the weights of matched (`yes`) indicators minus the weights of contradicted (`no`) ones, plus `classifier_weight` |
| `classifier_weight` | The disease classifier's call on the problem: 3 at high confidence, 2 at medium, 0 at low or when it didn't name it (`scoring::classifier_weight`). Without it the head only ranked candidates, and on the S5 rig run a candidate the photos couldn't show (blossom end rot, no fruit in either shot) won at 0 over the head's early blight at −2 |
| `unsure` | Counts for nothing; listed in `unsure` |
| One answer per indicator | Ranked `user`, then `measurement`, then `vlm` (the order of `EvidenceSource`); between two of the same source, the later one wins |
| Settled | An indicator answered by the user or a measurement is never asked again |
| Ranking | Score, higher first; then the candidate ranking order |

```rust
/// One answer per indicator, borrowed from the stored evidence for `'e`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnswerSet<'e> {
    by_indicator: BTreeMap<&'e IndicatorId, &'e IndicatorEvidence>,
}
```

```rust
impl<'e> AnswerSet<'e> {
    /// Keeps the highest-ranked answer of each indicator (`EvidenceSource`'s order:
    /// `user` > `measurement` > `vlm`); between two of the same source, the later one in the
    /// list wins, so a re-measure or a repeated VLM answer replaces the earlier.
    pub fn new(evidence: &'e [IndicatorEvidence]) -> Self {
        let mut by_indicator: BTreeMap<&'e IndicatorId, &'e IndicatorEvidence> = BTreeMap::new();
        for e in evidence {
            match by_indicator.get(&e.indicator_id) {
                Some(kept) if kept.source > e.source => {}
                _ => {
                    by_indicator.insert(&e.indicator_id, e);
                }
            }
        }
        Self { by_indicator }
    }

    /// The kept answer of an indicator.
    pub fn get(&self, id: &IndicatorId) -> Option<&'e IndicatorEvidence> {
        self.by_indicator.get(id).copied()
    }

    /// Whether the user or a measurement answered it: such an indicator is never asked again
    /// ([contracts 7]: questions only from indicators neither answered).
    pub fn settled(&self, id: &IndicatorId) -> bool {
        self.get(id)
            .is_some_and(|e| e.source >= EvidenceSource::Measurement)
    }

    /// Every kept answer.
    pub fn iter(&self) -> impl Iterator<Item = &'e IndicatorEvidence> + '_ {
        self.by_indicator.values().copied()
    }
}
```

```rust
/// Scores one candidate over its discriminating indicators. An indicator with no answer
/// appears in no list; an answer to an indicator the profile doesn't ask is ignored.
pub fn score_candidate(candidate: &Candidate<'_>, answers: &AnswerSet<'_>) -> CandidateScore {
    let profile: &ProblemSeed = candidate.profile;
    let mut matched = Vec::new();
    let mut contradicted = Vec::new();
    let mut unsure = Vec::new();
    for ind in profile.discriminating() {
        if let Some(e) = answers.get(&ind.id) {
            match e.answer {
                Answer::Yes => matched.push(e.clone()),
                Answer::No => contradicted.push(e.clone()),
                Answer::Unsure => unsure.push(e.clone()),
            }
        }
    }
    let sum = |list: &[IndicatorEvidence]| -> i32 {
        list.iter().map(|e| i32::from(e.weight.get())).sum()
    };
    let classifier_weight = classifier_weight(candidate);
    CandidateScore {
        problem_id: profile.id.clone(),
        name: profile.name.clone(),
        kind: profile.kind,
        score: sum(&matched) - sum(&contradicted) + i32::from(classifier_weight),
        classifier_weight,
        matched,
        contradicted,
        unsure,
        weather_fit: candidate.weather_fit,
    }
}

/// A scored candidate with the profile it came from, for the status and question rules.
#[derive(Debug, Clone, PartialEq)]
pub struct Scored<'k> {
    /// The candidate.
    pub candidate: Candidate<'k>,
    /// Its score.
    pub score: CandidateScore,
}

/// Scores every candidate and ranks them: score (higher first), then the candidate ranking
/// order (classifier score, weather fit, prevalence, id).
pub fn score_all<'k>(candidates: &[Candidate<'k>], answers: &AnswerSet<'_>) -> Vec<Scored<'k>> {
    let mut scored: Vec<Scored<'k>> = candidates
        .iter()
        .map(|c| Scored {
            candidate: *c,
            score: score_candidate(c, answers),
        })
        .collect();
    scored.sort_by(|a, b| {
        b.score
            .score
            .cmp(&a.score.score)
            .then_with(|| rank_order(&a.candidate, &b.candidate))
    });
    scored
}
```

---

## 8. Diagnosis status and the question

[Contracts 7, diagnosis](../architecture/contracts.md#diagnosis): the first row that holds wins.

| Status | Condition | In code |
|---|---|---|
| `healthy` | No candidate above zero; no pest (or invasive) organism and no damage pattern at medium or more; no classifier candidate other than `healthy` at medium or more; **and** positive evidence: the classifier's best result is `healthy` at medium or more, or every candidate has at least 2 of its 3 indicators answered `no`. A measured `no` counts only from an image whose `lesion_count` and `hole_count` both have a `max` of 0 | `nothing_points && (classifier_healthy \|\| all_no)` |
| `confirmed` | The top candidate above zero with at least one matched indicator, leading the second (or zero, alone) by at least the margin, and no question open. The classifier's weight never confirms alone | `top.score > 0 && !top.matched.is_empty() && lead >= margin && !question_open` |
| `uncertain` | Everything else | |
| `low_evidence` | `uncertain` and the first three conditions of `healthy` | `nothing_points` on an uncertain result |

Two readings are fixed here:

1. "Every candidate answered `no`" needs at least one candidate. A plant with an empty candidate set (a `label_only` species and no classifier) is never healthy by that clause.
2. A pest organism includes `invasive` ones, since the contracts treat invasive species as pests to report. "Other" (`unknown`) organisms don't count.

```rust
/// What the findings say, apart from the scores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FindingSummary {
    /// A pest (or invasive) organism, or any damage pattern, found with medium confidence or
    /// more.
    pub pest_or_damage: bool,
    /// A classifier candidate other than `healthy` with medium confidence or more.
    pub classifier_problem: bool,
    /// The classifier's highest-confidence result across the leaf regions is `healthy`, with
    /// medium confidence or more.
    pub classifier_healthy: bool,
}

/// What the status rule reads.
#[derive(Debug, Clone, Copy)]
pub struct StatusInput<'a> {
    /// Every candidate's score, ranked.
    pub scores: &'a [CandidateScore],
    /// The other findings.
    pub findings: FindingSummary,
    /// Images whose `lesion_count` and `hole_count` measurements both have a `max` of 0: a
    /// measured `no` from any other image isn't evidence of health.
    pub clean_images: &'a BTreeSet<ImageId>,
    /// A question is still open (the scheduler continued without an answer).
    pub question_open: bool,
    /// How far the top candidate must lead the second (open question 3 of contracts 12).
    pub margin: i32,
}

/// Why a result is ambiguous, which picks the kind of question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ambiguity {
    /// Every score is zero because the indicators came back `unsure` (or unanswered), and the
    /// classifier named no candidate at medium or more (its weight makes a score): ask for
    /// a photo.
    AllUnsure,
    /// Scores differ but the lead is too small, or there is no positive evidence: ask an
    /// indicator.
    Undecided,
}

/// The status rule's result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusDecision {
    /// The status.
    pub status: DiagnosisStatus,
    /// `uncertain`, and nothing points to any problem.
    pub low_evidence: bool,
    /// Set when neither `healthy` nor `confirmed`.
    pub ambiguity: Option<Ambiguity>,
}

/// Decides the status. An empty candidate set never gives the second kind of positive
/// evidence: "every candidate answered no" needs at least one candidate.
pub fn decide(input: &StatusInput<'_>) -> StatusDecision {
    let scores = input.scores;
    let nothing_points = scores.iter().all(|s| s.score <= 0)
        && !input.findings.pest_or_damage
        && !input.findings.classifier_problem;
    let all_no = !scores.is_empty()
        && scores.iter().all(|s| {
            s.contradicted
                .iter()
                .filter(|e| counts_as_health(e, input.clean_images))
                .count()
                >= 2
        });
    if nothing_points && (input.findings.classifier_healthy || all_no) {
        return StatusDecision {
            status: DiagnosisStatus::Healthy,
            low_evidence: false,
            ambiguity: None,
        };
    }
    if let Some(top) = scores.first() {
        let second = scores.get(1).map_or(0, |s| s.score);
        // The classifier's weight ranks a candidate but never confirms it alone: retrieve, then
        // verify ([architecture 6.3]).
        if top.score > 0
            && !top.matched.is_empty()
            && top.score - second >= input.margin
            && !input.question_open
        {
            return StatusDecision {
                status: DiagnosisStatus::Confirmed,
                low_evidence: false,
                ambiguity: None,
            };
        }
    }
    let all_unsure = scores
        .iter()
        .all(|s| s.score == 0 && s.matched.is_empty() && s.contradicted.is_empty());
    StatusDecision {
        status: DiagnosisStatus::Uncertain,
        low_evidence: nothing_points,
        ambiguity: Some(if all_unsure {
            Ambiguity::AllUnsure
        } else {
            Ambiguity::Undecided
        }),
    }
}
```

**The question** ([contracts 7](../architecture/contracts.md#diagnosis), ambiguous results). The pipeline asks only while the run may ([`may_ask`](domain.md#7-rules): fewer than 2 asked, still `analysing` or `awaiting_answer`, not a continuation):

| Ambiguity | Question |
|---|---|
| Every score is zero because the indicators came back `unsure` or unanswered | A `photo_request` for the plant or soil shot that most of the unsettled indicators can be seen on, ties broken by the findings order; `leaf_closeup` when nothing says otherwise. A candidate the classifier named at medium or more scores its weight, so that run asks the indicator question below instead: the classifier already read a close-up |
| Otherwise (the lead is too small, or no positive evidence) | The heaviest unsettled indicator of the top two candidates, the leader's on a tie, asked with `yes`, `no`, `unsure`; none left gives no question and the run is diagnosed `uncertain` |

```rust
/// The question an ambiguous run asks ([contracts 7]: one targeted question, or a request for
/// a specific photo).
#[derive(Debug, Clone, PartialEq)]
pub enum QuestionChoice<'k> {
    /// An indicator question, asked with `yes` / `no` / `unsure`.
    Indicator {
        /// The indicator, borrowed from its profile.
        indicator: &'k IndicatorSeed,
        /// The problems it separates: the top two.
        candidates: Vec<ProblemId>,
    },
    /// A `photo_request` for the shot the unsure indicators need.
    Photo {
        /// The kind to take; `leaf_closeup` when nothing says otherwise.
        kind: ImageKind,
        /// The problems the photo should separate: the top three.
        candidates: Vec<ProblemId>,
    },
}

/// Picks the question. Only indicators that neither the user nor a measurement answered are
/// asked. For `Undecided`: the heaviest such indicator of the top two candidates (the leader's
/// first on a tie); none left gives no question, and the run continues `uncertain`. For
/// `AllUnsure`: the plant or soil kind that most of the candidates' indicators can be seen on,
/// ties broken by the findings order.
pub fn choose_question<'k>(
    ambiguity: Ambiguity,
    ranked: &[Scored<'k>],
    answers: &AnswerSet<'_>,
) -> Option<QuestionChoice<'k>> {
    if ranked.is_empty() {
        return None;
    }
    match ambiguity {
        Ambiguity::Undecided => {
            let top: Vec<&Scored<'k>> = ranked.iter().take(2).collect();
            let mut best: Option<&'k IndicatorSeed> = None;
            for s in &top {
                for ind in s.candidate.indicators() {
                    if answers.settled(&ind.id) {
                        continue;
                    }
                    if best.is_none_or(|b| ind.weight > b.weight) {
                        best = Some(ind);
                    }
                }
            }
            best.map(|indicator| QuestionChoice::Indicator {
                indicator,
                candidates: top.iter().map(|s| s.candidate.profile.id.clone()).collect(),
            })
        }
        Ambiguity::AllUnsure => {
            let mut counts: Vec<(ImageKind, usize)> = Vec::new();
            for s in ranked {
                for ind in s.candidate.indicators() {
                    if answers.settled(&ind.id) {
                        continue;
                    }
                    for k in ind.image_kinds.iter().filter(|k| k.is_plant_or_soil()) {
                        match counts.iter_mut().find(|(kind, _)| kind == k) {
                            Some((_, n)) => *n += 1,
                            None => counts.push((*k, 1)),
                        }
                    }
                }
            }
            let order = |k: ImageKind| {
                FINDINGS_ORDER
                    .iter()
                    .position(|&(kind, _)| kind == k)
                    .unwrap_or(FINDINGS_ORDER.len())
            };
            counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| order(a.0).cmp(&order(b.0))));
            let kind = counts.first().map_or(ImageKind::LeafCloseup, |c| c.0);
            Some(QuestionChoice::Photo {
                kind,
                candidates: ranked
                    .iter()
                    .take(3)
                    .map(|s| s.candidate.profile.id.clone())
                    .collect(),
            })
        }
    }
}
```

---

## 9. Concurrency and lifetimes

- **What runs where.** The store's lookups run on the caller's task and only take the read lock for a pointer copy. `search` awaits the database. The rules are synchronous and run inside the pipeline's steps; they allocate only the owned results (`CandidateScore`, `SelectedCandidate`) that are stored.
- **Why owned at the port, borrowed inside.** `KnowledgeStore` returns owned records (`Vec<ProblemSeed>`): they cross an `.await` and a trait object, and `#[async_trait]` futures must be `Send` ([domain 10](domain.md#10-concurrency-and-lifetimes)). Inside a step the profiles sit in one `Vec`, and the rules borrow them ([The Rust Programming Language, ch. 10.3](https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html)).

| Lifetime | Meaning |
|---|---|
| `'k` in `ProfilePool<'k>`, `Candidate<'k>`, `CandidateSet<'k>`, `Scored<'k>`, `QuestionChoice<'k>` | The knowledge records the step holds. Every candidate and the chosen indicator point into them |
| `'a` in `RankInput<'a, 'k>`, `StatusInput<'a>` | The shorter borrow of a rule's other inputs, for one call. The results keep only `'k`, which is why the two are separate parameters: one lifetime would tie the candidates to the input struct |
| `'e` in `AnswerSet<'e>` | The stored evidence (`ScanSnapshot.evidence`) |
| `use<'k>` on `Candidate::indicators` and `KnowledgeIndex::problems_of_species` | Edition 2024 captures every in-scope lifetime in `impl Trait` ([Edition Guide, RPIT lifetime capture](https://doc.rust-lang.org/edition-guide/rust-2024/rpit-lifetime-capture.html)). Without `use<'k>` the iterator would also capture the borrow of `self`, and `score_all` couldn't return candidates past it |

---

## 10. Errors

```rust
/// Why a [`crate::KnowledgeIndex`] could not be built.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IndexError {
    /// An id is used twice in one table.
    #[error("duplicate id {0}")]
    Duplicate(String),
    /// A problem doesn't have exactly 3 discriminating indicators.
    #[error("problem {problem} has {count} discriminating indicators, not 3")]
    Discriminating {
        /// The problem.
        problem: ProblemId,
        /// How many it has.
        count: usize,
    },
    /// A problem's `affects` names an unknown species.
    #[error("problem {problem} affects unknown species {species}")]
    UnknownSpecies {
        /// The problem.
        problem: ProblemId,
        /// The species.
        species: SpeciesId,
    },
    /// A `caused_by` or `preys_on` entry names an unknown organism.
    #[error("unknown organism {0}")]
    UnknownOrganism(OrganismId),
    /// A `looks_like` edge names an unknown problem.
    #[error("unknown problem {0}")]
    UnknownProblem(ProblemId),
}
```

The port's methods return `StoreError` ([domain 6](domain.md#6-errors)). `IndexError` becomes `StoreError::Query { what: "knowledge index" }` with the index error as its source, and `api` refuses to start. The rules don't fail: an unknown profile is left out of the set.

---

## 11. Testing

`cargo test -p knowledge` runs 19 tests against the fixture knowledge base: tomato early blight, Septoria (its look-alike, with a measurable small-spot indicator), late blight, flea beetle damage and grey mould.

| What | Tests |
|---|---|
| Index | Both look-alike directions, reverse `caused_by` and `damage_patterns`, a profile without 3 discriminating indicators refused |
| Store | Lookups through a `MockKnowledgeRepo`, `prey_of` only for beneficials, the species list filtered and paged |
| Ranking | Classifier hits (3 distinct, `healthy` ignored); the classifier first, look-alikes filling, the cap before look-alikes; outside candidates skip the set |
| Weather fit | Early blight in warm humid, cold dry and warm dry weather; no stated conditions |
| Scoring | Source ranking; matched minus contradicted with `unsure` at zero; the classifier's weight by band |
| Status | `confirmed` needs the margin and no open question; all `unsure` is ambiguous and asks for a close-up; early blight at 0.71 is not `low_evidence`; `healthy` from the classifier or from `no` answers; a measured `no` counts only from a clean image; the indicator question skips settled indicators |

Two property tests (proptest 1.12, 256 cases each):

```rust
/// Healthy implies no candidate above zero, and low_evidence implies uncertain; a
/// confirmed status has a positive leader with the margin and no open question.
#[test]
fn status_invariants(
    answers in proptest::collection::vec(proptest::collection::vec(answer(), 3), 1..5),
    pest in any::<bool>(), problem in any::<bool>(), healthy in any::<bool>(),
    open in any::<bool>(), margin in 1i32..4,
    heads in proptest::collection::vec(proptest::option::of(0.0f64..=1.0), 4),
) {
    let profiles = fixtures::problems();
    let names = ["tomato_early_blight", "tomato_septoria_leaf_spot", "tomato_late_blight", "tomato_flea_beetle_damage"];
    let mut cands = fixtures::candidates(&profiles, &names[..answers.len().min(4)]);
    for (c, h) in cands.iter_mut().zip(&heads) {
        c.classifier = h.map(Confidence::saturating);
    }
    let mut ev = Vec::new();
    for (c, row) in cands.iter().zip(&answers) {
        for (ind, a) in c.indicators().zip(row) {
            if let Some((ans, src)) = a {
                ev.push(fixtures::evidence(&ind.id, ind.weight.get(), *ans, *src));
            }
        }
    }
    let scores: Vec<_> = score_all(&cands, &AnswerSet::new(&ev)).into_iter().map(|s| s.score).collect();
    let clean = BTreeSet::new();
    let d = decide(&StatusInput {
        scores: &scores,
        findings: FindingSummary { pest_or_damage: pest, classifier_problem: problem, classifier_healthy: healthy },
        clean_images: &clean,
        question_open: open,
        margin,
    });
    if d.status == DiagnosisStatus::Healthy {
        prop_assert!(scores.iter().all(|s| s.score <= 0));
        prop_assert!(!pest && !problem);
    }
    if d.low_evidence {
        prop_assert_eq!(d.status, DiagnosisStatus::Uncertain);
    }
    if d.status == DiagnosisStatus::Confirmed {
        prop_assert!(scores[0].score > 0 && !open);
        prop_assert!(!scores[0].matched.is_empty(), "the classifier never confirms alone");
        prop_assert!(scores[0].score - scores.get(1).map_or(0, |s| s.score) >= margin);
    }
    prop_assert_eq!(d.ambiguity.is_some(), d.status == DiagnosisStatus::Uncertain);
    for w in scores.windows(2) {
        prop_assert!(w[0].score >= w[1].score, "ranked by score");
    }
}
```

```rust
/// The score is matched minus contradicted weights plus the classifier's, and a
/// lower-ranked answer never changes an indicator a higher-ranked source answered.
#[test]
fn score_is_arithmetic_and_respects_source_rank(
    row in proptest::collection::vec(answer(), 3), extra in answer(),
    head in proptest::option::of(0.0f64..=1.0),
) {
    let profiles = fixtures::problems();
    let mut cands = fixtures::candidates(&profiles, &["tomato_early_blight"]);
    cands[0].classifier = head.map(Confidence::saturating);
    let inds: Vec<_> = cands[0].indicators().collect();
    let mut ev = Vec::new();
    for (ind, a) in inds.iter().zip(&row) {
        if let Some((ans, src)) = a {
            ev.push(fixtures::evidence(&ind.id, ind.weight.get(), *ans, *src));
        }
    }
    let before = crate::scoring::score_candidate(&cands[0], &AnswerSet::new(&ev));
    let expected: i32 = before.matched.iter().map(|e| i32::from(e.weight.get())).sum::<i32>()
        - before.contradicted.iter().map(|e| i32::from(e.weight.get())).sum::<i32>()
        + i32::from(before.classifier_weight);
    prop_assert_eq!(before.score, expected);
    prop_assert_eq!(before.classifier_weight > 0, head.is_some_and(|h| h >= 0.5));
    if let (Some((ans, src)), Some(Some((_, kept)))) = (extra, row.first())
        && src < *kept
    {
        let mut more = ev.clone();
        more.push(fixtures::evidence(&inds[0].id, inds[0].weight.get(), ans, src));
        let after = crate::scoring::score_candidate(&cands[0], &AnswerSet::new(&more));
        prop_assert_eq!(after.score, before.score);
    }
}
```

---

## 12. Open questions

These belong in [contracts 12](../architecture/contracts.md#12-open-questions):

1. **Weather fit arithmetic.** Contracts 7 defines the three values but not how they are computed. Section 6 fixes "all stated conditions met / none / some" over now, the past week and 3 forecast days.
2. **The confirmed margin.** Settled for the submission in backend roadmap S4 (contracts 12, question 3): 2, one weight-2 indicator, in `PipelineConfig::diagnosis_margin`, with the classifier's weight in the score. It is fitted with the indicator weights once the evaluation harness exists.
3. **Empty candidate sets.** Settled in backend roadmap S4: a `label_only` species with no classifier result has no candidates; it is `uncertain` (with `low_evidence` when no pest or damage finding is at medium or more), never `healthy` without the classifier's own `healthy` result, and `choose_question` asks nothing, since a photo can't add a candidate the knowledge base doesn't hold (test `an_empty_candidate_set_is_low_evidence_and_asks_nothing`).

---

## 13. Sources

**Project documents**

- [Architecture 5, knowledge base](../architecture/README.md#knowledge-base); [6.3](../architecture/README.md#63-retrieve-then-verify); [6.5](../architecture/README.md#65-organisms-and-damage)
- [Contracts 4, event catalogue](../architecture/contracts.md#event-catalogue); [5, results](../architecture/contracts.md#results); [7, diagnosis](../architecture/contracts.md#diagnosis); [10](../architecture/contracts.md#10-knowledge-seed-format)
- [Database 5](../database/README.md#5-reads-indexes-and-vector-search)
- [domain](domain.md), [pipeline](pipeline.md)

**Rust**

- [The Rust Programming Language, 10.3: validating references with lifetimes](https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html)
- [Rust Edition Guide: RPIT lifetime capture rules](https://doc.rust-lang.org/edition-guide/rust-2024/rpit-lifetime-capture.html)
- [std `RwLock`](https://doc.rust-lang.org/std/sync/struct.RwLock.html), [`PoisonError::into_inner`](https://doc.rust-lang.org/std/sync/struct.PoisonError.html#method.into_inner)

**Crates**

- [async-trait 0.1](https://docs.rs/async-trait/0.1.92/async_trait/), [proptest 1.12](https://docs.rs/proptest/1.12.0/proptest/), [mockall 0.15](https://docs.rs/mockall/0.15.0/mockall/)
