//! The pre-VLM candidate set ([architecture 6.3]; [contracts 4], `candidates_selected`):
//! about 9 profiles, from the disease classifier's top 3 and the problems the working species is
//! `susceptible_to`, ranked by classifier score, then weather fit, then prevalence, with
//! look-alikes of the top-ranked ones filling the remaining slots. Each brings its 3
//! discriminating indicators.
//!
//! **Lifetimes.** `'k` is the lifetime of the knowledge records the caller holds (the profiles
//! it fetched for this step). Every [`Candidate<'k>`] borrows its profile for `'k`, so ranking
//! clones nothing. `'a` in [`RankInput<'a, 'k>`] is the shorter borrow of the ranking's other
//! inputs; the result keeps only `'k`.

use std::collections::{BTreeMap, BTreeSet};

use domain::{
    events::{SelectedCandidate, SelectionSource},
    id::{ProblemId, ProblemOrHealthy},
    knowledge::{IndicatorSeed, ProblemSeed},
    records::{diagnosis::WeatherFit, weather::WeatherSummary},
    rules::limits::MAX_CANDIDATES,
    wire::Confidence,
};

use crate::weather_fit::{fit_rank, weather_fit};

/// One of the classifier's candidates: a problem and its best confidence across regions.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassifierHit {
    /// The problem.
    pub problem: ProblemId,
    /// Its highest confidence across the leaf regions.
    pub confidence: Confidence,
    /// Its highest confidence from the disease head itself, the only call that adds
    /// `classifier_weight` to a score; `None` when only the fallback's LLM named it, since the
    /// same model answers the indicators there and would count twice.
    pub head: Option<Confidence>,
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
            head: None,
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

/// The profiles a step holds, looked up by id. Borrows them for `'k`.
#[derive(Debug, Clone, Default)]
pub struct ProfilePool<'k> {
    by_id: BTreeMap<&'k ProblemId, &'k ProblemSeed>,
}

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
    /// The disease head's own confidence ([`ClassifierHit::head`]), which alone adds
    /// `classifier_weight`.
    pub head: Option<Confidence>,
    /// The weather against its conditions.
    pub weather_fit: WeatherFit,
}

impl<'k> Candidate<'k> {
    /// Its 3 discriminating indicators, the only ones ever asked.
    pub fn indicators(&self) -> impl Iterator<Item = &'k IndicatorSeed> + use<'k> {
        self.profile.discriminating()
    }

    /// The `candidates_selected` entry.
    pub fn selected(&self) -> SelectedCandidate {
        SelectedCandidate {
            problem_id: self.profile.id.clone(),
            name: self.profile.name.clone(),
            kind: self.profile.kind,
            source: self.source,
        }
    }
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

impl<'k> CandidateSet<'k> {
    /// The candidates, in rank order.
    pub fn as_slice(&self) -> &[Candidate<'k>] {
        &self.candidates
    }

    /// Whether a problem is in the set.
    pub fn contains(&self, id: &ProblemId) -> bool {
        self.candidates.iter().any(|c| &c.profile.id == id)
    }

    /// The `candidates_selected` payload, also what `retrieve` stores for `score`.
    pub fn selected(&self) -> Vec<SelectedCandidate> {
        self.candidates.iter().map(Candidate::selected).collect()
    }

    /// Rebuilds the stored set (`ScanSnapshot.candidates`) over the profiles `score` fetched.
    /// Problems no longer in the knowledge base are left out.
    pub fn from_selected(
        stored: &[SelectedCandidate],
        pool: &ProfilePool<'k>,
        classifier: &[ClassifierHit],
        weather: Option<&WeatherSummary>,
    ) -> Self {
        let candidates = stored
            .iter()
            .filter_map(|s| {
                let profile = pool.get(&s.problem_id)?;
                Some(Candidate {
                    profile,
                    source: s.source,
                    classifier: classifier
                        .iter()
                        .find(|h| h.problem == s.problem_id)
                        .map(|h| h.confidence),
                    head: classifier
                        .iter()
                        .find(|h| h.problem == s.problem_id)
                        .and_then(|h| h.head),
                    weather_fit: weather_fit(&profile.conditions, weather),
                })
            })
            .collect();
        Self { candidates }
    }

    /// Appends candidates (a `verify` job's additions) that aren't in the set yet.
    pub fn extend(&mut self, more: impl IntoIterator<Item = Candidate<'k>>) {
        for c in more {
            if !self.contains(&c.profile.id) {
                self.candidates.push(c);
            }
        }
    }
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

impl RankInput<'_, '_> {
    /// The production cap.
    pub const CAP: usize = MAX_CANDIDATES;
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
            head: input
                .classifier
                .iter()
                .find(|h| &h.problem == id)
                .and_then(|h| h.head),
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
            head: None,
            weather_fit: weather_fit(&profile.conditions, weather),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    fn ids(set: &CandidateSet<'_>) -> Vec<String> {
        set.as_slice()
            .iter()
            .map(|c| c.profile.id.to_string())
            .collect()
    }

    #[test]
    fn classifier_hits_are_three_distinct_problems_without_healthy() {
        let c = |v| Confidence::new(v).unwrap();
        let p = |s: &str| ProblemOrHealthy::Problem(s.parse().unwrap());
        let hits = classifier_hits([
            (ProblemOrHealthy::Healthy, c(0.9)),
            (p("a_one"), c(0.3)),
            (p("a_two"), c(0.6)),
            (p("a_one"), c(0.7)),
            (p("a_three"), c(0.2)),
            (p("a_four"), c(0.1)),
        ]);
        let names: Vec<_> = hits.iter().map(|h| h.problem.to_string()).collect();
        assert_eq!(names, ["a_one", "a_two", "a_three"]);
        assert!((hits[0].confidence.get() - 0.7).abs() < 1e-9);
    }

    #[test]
    fn ranks_classifier_then_weather_then_prevalence_and_fills_with_lookalikes() {
        let profiles = fixtures::problems();
        let pool = ProfilePool::new(&profiles);
        let classifier = [ClassifierHit {
            problem: "tomato_early_blight".parse().unwrap(),
            confidence: Confidence::new(0.71).unwrap(),
            head: None,
        }];
        let species: Vec<ProblemId> = [
            "tomato_early_blight",
            "tomato_late_blight",
            "tomato_flea_beetle_damage",
        ]
        .iter()
        .map(|s| s.parse().unwrap())
        .collect();
        let lookalikes = fixtures::lookalike_map(&profiles);
        let weather = fixtures::weather(26.0, 95.0, 40);
        let set = rank_candidates(&RankInput {
            pool: &pool,
            classifier: &classifier,
            species_problems: &species,
            lookalikes: &lookalikes,
            weather: Some(&weather),
            cap: 9,
        });
        let got = ids(&set);
        assert_eq!(got[0], "tomato_early_blight");
        assert_eq!(set.as_slice()[0].source, SelectionSource::Classifier);
        assert!(got.contains(&"tomato_septoria_leaf_spot".to_owned()));
        let septoria = set
            .as_slice()
            .iter()
            .find(|c| c.profile.id.as_str() == "tomato_septoria_leaf_spot")
            .unwrap();
        assert_eq!(septoria.source, SelectionSource::Lookalike);

        let tight = rank_candidates(&RankInput {
            pool: &pool,
            classifier: &classifier,
            species_problems: &species,
            lookalikes: &lookalikes,
            weather: Some(&weather),
            cap: 2,
        });
        assert_eq!(
            tight.as_slice().len(),
            2,
            "the cap holds before look-alikes"
        );
        assert!(!ids(&tight).contains(&"tomato_septoria_leaf_spot".to_owned()));
    }

    #[test]
    fn outside_candidates_skip_the_set() {
        let profiles = fixtures::problems();
        let pool = ProfilePool::new(&profiles);
        let blight = pool.get(&"tomato_early_blight".parse().unwrap()).unwrap();
        let flea = pool
            .get(&"tomato_flea_beetle_damage".parse().unwrap())
            .unwrap();
        let mut set = CandidateSet::default();
        set.extend([Candidate {
            profile: blight,
            source: SelectionSource::Classifier,
            classifier: None,
            head: None,
            weather_fit: WeatherFit::Neutral,
        }]);
        let added = outside_candidates(
            &set,
            &[
                (blight, SelectionSource::Damage),
                (flea, SelectionSource::Damage),
            ],
            None,
        );
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].profile.id, flea.id);
    }
}
