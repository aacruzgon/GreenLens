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

@@file ../Cargo.toml@@

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

@@items index.rs KnowledgeIndex@@

`looks_like` is written once in the seed and both ways by the loader. The index also folds in each profile's own one-way list, so an index built from seeds alone has both directions as well:

@@impl index.rs impl KnowledgeIndex@@

The store keeps the index behind `RwLock<Arc<_>>`. A reader clones the `Arc` and drops the lock at once ([std `RwLock`](https://doc.rust-lang.org/std/sync/struct.RwLock.html)), so no guard is ever held across an `.await`, which the `rust-async` skill (§7, `.claude/skills/`) forbids. `reload` swaps in a new index after a migration without blocking readers. A poisoned lock is recovered with `PoisonError::into_inner`: replacing a pointer can't leave the data half-written.

@@items store.rs SurrealKnowledgeStore@@

@@impl store.rs impl SurrealKnowledgeStore@@

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

@@items candidates.rs ClassifierHit classifier_hits@@

A `ProfilePool<'k>` indexes the profiles the step already holds. `get` returns `&'k ProblemSeed`, borrowed for the records' lifetime and not for the borrow of the pool, so a candidate outlives the lookup that found it:

@@items candidates.rs ProfilePool@@

@@impl candidates.rs impl<'k> ProfilePool<'k>@@

@@items candidates.rs Candidate rank_order CandidateSet RankInput rank_candidates outside_candidates@@

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

@@items weather_fit.rs FORECAST_DAYS ConditionCheck check weather_fit fit_rank@@

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

@@items scoring.rs AnswerSet@@

@@impl scoring.rs impl<'e> AnswerSet<'e>@@

@@items scoring.rs score_candidate Scored score_all@@

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

@@items status.rs FindingSummary StatusInput Ambiguity StatusDecision decide@@

**The question** ([contracts 7](../architecture/contracts.md#diagnosis), ambiguous results). The pipeline asks only while the run may ([`may_ask`](domain.md#7-rules): fewer than 2 asked, still `analysing` or `awaiting_answer`, not a continuation):

| Ambiguity | Question |
|---|---|
| Every score is zero because the indicators came back `unsure` or unanswered | A `photo_request` for the plant or soil shot that most of the unsettled indicators can be seen on, ties broken by the findings order; `leaf_closeup` when nothing says otherwise. A candidate the classifier named at medium or more scores its weight, so that run asks the indicator question below instead: the classifier already read a close-up |
| Otherwise (the lead is too small, or no positive evidence) | The heaviest unsettled indicator of the top two candidates, the leader's on a tie, asked with `yes`, `no`, `unsure`; none left gives no question and the run is diagnosed `uncertain` |

@@items status.rs QuestionChoice choose_question@@

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

@@items error.rs IndexError@@

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

@@test status.rs status_invariants@@

@@test status.rs score_is_arithmetic_and_respects_source_rank@@

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
