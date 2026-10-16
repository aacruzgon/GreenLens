//! Deterministic verification scoring ([architecture 6.3], step 3; [contracts 7],
//! `CandidateScore`): the sum of the weights of matched indicators minus the weights of
//! contradicted ones; `unsure` counts for nothing. Each indicator keeps one answer, ranked
//! `user`, then `measurement`, then `vlm` ([contracts 7], measurable indicators).
//!
//! **Lifetimes.** `'e` is the lifetime of the stored evidence (`ScanSnapshot.evidence`):
//! [`AnswerSet<'e>`] only points into it. Scores are owned (`CandidateScore`), because they
//! are stored in the diagnosis.

use std::collections::BTreeMap;

use domain::{
    id::IndicatorId,
    knowledge::ProblemSeed,
    records::diagnosis::{CandidateScore, EvidenceSource, IndicatorEvidence},
    scan::Answer,
};

use crate::{
    candidates::{Candidate, rank_order},
    weather_fit::fit_rank,
};

/// One answer per indicator, borrowed from the stored evidence for `'e`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnswerSet<'e> {
    by_indicator: BTreeMap<&'e IndicatorId, &'e IndicatorEvidence>,
}

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
    CandidateScore {
        problem_id: profile.id.clone(),
        name: profile.name.clone(),
        kind: profile.kind,
        score: sum(&matched) - sum(&contradicted),
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

/// Whether the weather argues for a candidate: used in fusion's drivers.
pub fn favoured(score: &CandidateScore) -> bool {
    fit_rank(score.weather_fit) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    #[test]
    fn user_outranks_measurement_outranks_vlm() {
        let id: IndicatorId = "tomato_early_blight.concentric_rings".parse().unwrap();
        let evidence = vec![
            fixtures::evidence(&id, 3, Answer::Yes, EvidenceSource::User),
            fixtures::evidence(&id, 3, Answer::No, EvidenceSource::Vlm),
            fixtures::evidence(&id, 3, Answer::Unsure, EvidenceSource::Measurement),
        ];
        let set = AnswerSet::new(&evidence);
        assert_eq!(set.get(&id).unwrap().source, EvidenceSource::User);
        assert!(set.settled(&id));
    }

    #[test]
    fn matched_minus_contradicted_and_unsure_counts_nothing() {
        let profiles = fixtures::problems();
        let blight = fixtures::candidate(&profiles, "tomato_early_blight");
        let ids: Vec<_> = blight
            .profile
            .discriminating()
            .map(|i| i.id.clone())
            .collect();
        let evidence = vec![
            fixtures::evidence(&ids[0], 3, Answer::Yes, EvidenceSource::Vlm),
            fixtures::evidence(&ids[1], 2, Answer::No, EvidenceSource::Vlm),
            fixtures::evidence(&ids[2], 1, Answer::Unsure, EvidenceSource::Vlm),
        ];
        let s = score_candidate(&blight, &AnswerSet::new(&evidence));
        assert_eq!(s.score, 1);
        assert_eq!(
            (s.matched.len(), s.contradicted.len(), s.unsure.len()),
            (1, 1, 1)
        );
    }
}
