//! Deterministic verification scoring ([architecture 6.3], step 3; [contracts 7],
//! `CandidateScore`): the sum of the weights of matched indicators minus the weights of
//! contradicted ones, plus the disease classifier's weight for the problem; `unsure` counts
//! for nothing. Each indicator keeps one answer, ranked `user`, then `measurement`, then
//! `vlm` ([contracts 7], measurable indicators).
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
    wire::{Confidence, ConfidenceBand},
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

/// What the disease classifier adds to a candidate it named ([contracts 7],
/// `CandidateScore.classifier_weight`): as much as the strongest indicator at high confidence,
/// as an ordinary one at medium, and nothing at low, as `unsure` counts for nothing. The
/// classifier never confirms a diagnosis alone: [`crate::status::decide`] also needs a
/// matched indicator.
pub fn classifier_weight(candidate: &Candidate<'_>) -> u8 {
    match candidate.classifier.map(Confidence::band) {
        Some(ConfidenceBand::High) => 3,
        Some(ConfidenceBand::Medium) => 2,
        Some(ConfidenceBand::Low) | None => 0,
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
        assert_eq!(s.classifier_weight, 0);
    }

    #[test]
    fn the_classifier_adds_by_band() {
        let profiles = fixtures::problems();
        let mut blight = fixtures::candidate(&profiles, "tomato_early_blight");
        let weight = |c: Option<f64>, blight: &mut Candidate<'_>| {
            blight.classifier = c.map(Confidence::saturating);
            score_candidate(blight, &AnswerSet::default())
        };
        let high = weight(Some(0.8), &mut blight);
        assert_eq!((high.classifier_weight, high.score), (3, 3));
        let medium = weight(Some(0.575), &mut blight);
        assert_eq!((medium.classifier_weight, medium.score), (2, 2));
        assert_eq!(weight(Some(0.49), &mut blight).score, 0);
        assert_eq!(weight(None, &mut blight).score, 0);
    }

    /// The S5 rig run (backend roadmap section 9): the head called early blight at 0.575,
    /// the VLM answered its angular-spots indicator `no`, and blossom end rot, which the
    /// photos couldn't show, won at 0. With the head's weight, early blight ties at 0 and
    /// ranks first on the classifier's score.
    #[test]
    fn a_medium_head_call_offsets_one_contradicted_indicator() {
        let profiles = fixtures::problems();
        let mut blight = fixtures::candidate(&profiles, "tomato_early_blight");
        blight.classifier = Some(Confidence::saturating(0.575));
        let other = fixtures::candidate(&profiles, "tomato_late_blight");
        let angular = blight.profile.discriminating().nth(1).unwrap().id.clone();
        let evidence = vec![fixtures::evidence(
            &angular,
            2,
            Answer::No,
            EvidenceSource::Vlm,
        )];
        let answers = AnswerSet::new(&evidence);
        let ranked = score_all(&[other, blight], &answers);
        assert_eq!(ranked[0].score.problem_id.as_str(), "tomato_early_blight");
        assert_eq!(ranked[0].score.score, 0);
    }
}
