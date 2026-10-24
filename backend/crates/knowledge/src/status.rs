//! The diagnosis status ([contracts 7], diagnosis; [architecture 6.3], healthy only on
//! positive evidence): decided in the contract's order, the first row that holds wins.
//!
//! | Status | When |
//! |---|---|
//! | `healthy` | no candidate above zero, no pest or damage finding at medium or more, no non-healthy classifier candidate at medium or more, **and** positive evidence: the classifier's best result is `healthy` at medium or more, or every candidate has at least 2 of its 3 indicators answered `no` (a measured `no` only from a clean image) |
//! | `confirmed` | the top candidate above zero with at least one matched indicator, leading the second by at least the margin, and no question open |
//! | `uncertain` | everything else |
//!
//! `low_evidence` is `uncertain` plus the first three conditions of `healthy`.
//!
//! **Lifetimes.** `'a` in [`StatusInput<'a>`] borrows the step's scores and findings for the
//! call; `'k` in [`QuestionChoice<'k>`] is the profiles' lifetime, because the chosen indicator
//! is borrowed from its profile.

use std::collections::BTreeSet;

use domain::{
    id::{ImageId, ProblemId},
    knowledge::IndicatorSeed,
    records::diagnosis::{CandidateScore, DiagnosisStatus, EvidenceSource, IndicatorEvidence},
    rules::routing::FINDINGS_ORDER,
    scan::{Answer, ImageKind},
};

use crate::scoring::{AnswerSet, Scored};

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

/// A measured `no` counts as evidence of health only from a clean image; any other `no`
/// counts.
fn counts_as_health(e: &IndicatorEvidence, clean: &BTreeSet<ImageId>) -> bool {
    e.answer == Answer::No
        && (e.source != EvidenceSource::Measurement
            || e.measured
                .as_ref()
                .is_some_and(|m| clean.contains(&m.image_id)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{candidates::Candidate, fixtures, scoring::score_all};
    use domain::records::diagnosis::MeasuredValue;
    use domain::records::measurement::MeasureQuantity;
    use domain::wire::Confidence;

    fn input<'a>(scores: &'a [CandidateScore], clean: &'a BTreeSet<ImageId>) -> StatusInput<'a> {
        StatusInput {
            scores,
            findings: FindingSummary::default(),
            clean_images: clean,
            question_open: false,
            margin: 2,
        }
    }

    fn scored<'k>(cands: &[Candidate<'k>], evidence: &[IndicatorEvidence]) -> Vec<Scored<'k>> {
        score_all(cands, &AnswerSet::new(evidence))
    }

    #[test]
    fn confirmed_needs_the_margin_and_no_open_question() {
        let profiles = fixtures::problems();
        let cands = fixtures::candidates(
            &profiles,
            &["tomato_early_blight", "tomato_septoria_leaf_spot"],
        );
        let blight: Vec<_> = cands[0].indicators().map(|i| i.id.clone()).collect();
        let ev = vec![
            fixtures::evidence(&blight[0], 3, Answer::Yes, EvidenceSource::Vlm),
            fixtures::evidence(&blight[1], 2, Answer::Yes, EvidenceSource::Vlm),
        ];
        let s: Vec<_> = scored(&cands, &ev).into_iter().map(|s| s.score).collect();
        let clean = BTreeSet::new();
        assert_eq!(
            decide(&input(&s, &clean)).status,
            DiagnosisStatus::Confirmed
        );
        let mut open = input(&s, &clean);
        open.question_open = true;
        assert_eq!(decide(&open).status, DiagnosisStatus::Uncertain);
    }

    /// Knowledge open question 3, decided in S4: a `label_only` species with no classifier
    /// result has no candidates. It is `uncertain` with `low_evidence`, never `healthy`
    /// without the classifier's own `healthy`, and no question is asked, since a photo can't
    /// add a candidate the knowledge base doesn't hold.
    #[test]
    fn an_empty_candidate_set_is_low_evidence_and_asks_nothing() {
        let empty: Vec<CandidateScore> = Vec::new();
        let clean = BTreeSet::new();
        let d = decide(&input(&empty, &clean));
        assert_eq!(d.status, DiagnosisStatus::Uncertain);
        assert!(d.low_evidence);
        let ambiguity = d.ambiguity.unwrap();
        assert!(choose_question(ambiguity, &[], &AnswerSet::default()).is_none());
    }

    #[test]
    fn the_classifier_alone_ranks_but_never_confirms() {
        let profiles = fixtures::problems();
        let mut cands = fixtures::candidates(
            &profiles,
            &["tomato_early_blight", "tomato_septoria_leaf_spot"],
        );
        cands[0].classifier = Some(Confidence::saturating(0.9));
        let ranked = scored(&cands, &[]);
        let s: Vec<_> = ranked.iter().map(|s| s.score.clone()).collect();
        assert_eq!((s[0].score, s[0].classifier_weight), (3, 3));
        let clean = BTreeSet::new();
        let d = decide(&input(&s, &clean));
        assert_eq!(d.status, DiagnosisStatus::Uncertain);
        assert_eq!(d.ambiguity, Some(Ambiguity::Undecided));
        // One matched indicator then confirms it.
        let rings = cands[0].indicators().next().unwrap().id.clone();
        let ev = vec![fixtures::evidence(
            &rings,
            3,
            Answer::Yes,
            EvidenceSource::User,
        )];
        let s: Vec<_> = scored(&cands, &ev).into_iter().map(|s| s.score).collect();
        assert_eq!(s[0].score, 6);
        assert_eq!(
            decide(&input(&s, &clean)).status,
            DiagnosisStatus::Confirmed
        );
    }

    /// Contracts 7's example: the classifier's early blight at 0.71, every indicator `unsure`.
    /// Its weight makes a score, so the run asks the user early blight's target rings (the
    /// heaviest indicator nobody settled) instead of a close-up the classifier already read.
    #[test]
    fn a_head_call_with_unsure_indicators_asks_its_heaviest_indicator() {
        let profiles = fixtures::problems();
        let mut cands = fixtures::candidates(
            &profiles,
            &["tomato_early_blight", "tomato_septoria_leaf_spot"],
        );
        cands[0].classifier = Some(Confidence::saturating(0.71));
        let ev: Vec<_> = cands
            .iter()
            .flat_map(Candidate::indicators)
            .map(|i| fixtures::evidence(&i.id, i.weight.get(), Answer::Unsure, EvidenceSource::Vlm))
            .collect();
        let ranked = scored(&cands, &ev);
        let s: Vec<_> = ranked.iter().map(|s| s.score.clone()).collect();
        let clean = BTreeSet::new();
        let mut i = input(&s, &clean);
        i.findings.classifier_problem = true;
        let d = decide(&i);
        assert_eq!(d.status, DiagnosisStatus::Uncertain);
        assert!(!d.low_evidence);
        assert_eq!(d.ambiguity, Some(Ambiguity::Undecided));
        let q = choose_question(Ambiguity::Undecided, &ranked, &AnswerSet::new(&ev)).unwrap();
        let QuestionChoice::Indicator { indicator, .. } = q else {
            panic!("expected an indicator question, got {q:?}");
        };
        let heaviest = cands[0].indicators().max_by_key(|i| i.weight).unwrap();
        assert_eq!(indicator.id, heaviest.id);
    }

    #[test]
    fn all_unsure_is_ambiguous_not_healthy_and_asks_for_a_close_up() {
        let profiles = fixtures::problems();
        let cands = fixtures::candidates(
            &profiles,
            &["tomato_early_blight", "tomato_septoria_leaf_spot"],
        );
        let ev: Vec<_> = cands
            .iter()
            .flat_map(Candidate::indicators)
            .map(|i| fixtures::evidence(&i.id, i.weight.get(), Answer::Unsure, EvidenceSource::Vlm))
            .collect();
        let ranked = scored(&cands, &ev);
        let s: Vec<_> = ranked.iter().map(|s| s.score.clone()).collect();
        let clean = BTreeSet::new();
        let d = decide(&input(&s, &clean));
        assert_eq!(d.status, DiagnosisStatus::Uncertain);
        assert!(d.low_evidence);
        assert_eq!(d.ambiguity, Some(Ambiguity::AllUnsure));
        let q = choose_question(Ambiguity::AllUnsure, &ranked, &AnswerSet::new(&ev)).unwrap();
        assert!(matches!(
            q,
            QuestionChoice::Photo {
                kind: ImageKind::LeafCloseup,
                ..
            }
        ));
    }

    #[test]
    fn classifier_early_blight_with_unsure_indicators_is_not_low_evidence() {
        let scores: Vec<CandidateScore> = Vec::new();
        let clean = BTreeSet::new();
        let mut i = input(&scores, &clean);
        i.findings.classifier_problem = true;
        let d = decide(&i);
        assert_eq!(d.status, DiagnosisStatus::Uncertain);
        assert!(!d.low_evidence);
    }

    #[test]
    fn healthy_from_the_classifier_or_from_no_answers() {
        let clean = BTreeSet::new();
        let empty: Vec<CandidateScore> = Vec::new();
        let mut i = input(&empty, &clean);
        assert_eq!(
            decide(&i).status,
            DiagnosisStatus::Uncertain,
            "no candidates, no positive evidence"
        );
        i.findings.classifier_healthy = true;
        assert_eq!(decide(&i).status, DiagnosisStatus::Healthy);

        let profiles = fixtures::problems();
        let cands = fixtures::candidates(&profiles, &["tomato_early_blight"]);
        let ids: Vec<_> = cands[0].indicators().map(|i| i.id.clone()).collect();
        let ev = vec![
            fixtures::evidence(&ids[0], 3, Answer::No, EvidenceSource::Vlm),
            fixtures::evidence(&ids[1], 2, Answer::No, EvidenceSource::Vlm),
        ];
        let s: Vec<_> = scored(&cands, &ev).into_iter().map(|s| s.score).collect();
        assert_eq!(decide(&input(&s, &clean)).status, DiagnosisStatus::Healthy);
    }

    #[test]
    fn a_measured_no_counts_for_health_only_from_a_clean_image() {
        let profiles = fixtures::problems();
        let cands = fixtures::candidates(&profiles, &["tomato_early_blight"]);
        let ids: Vec<_> = cands[0].indicators().map(|i| i.id.clone()).collect();
        let image: ImageId = "01JAX3K9T6R2B8V0QZ4M7D1E5F".parse().unwrap();
        let mut measured = fixtures::evidence(&ids[0], 3, Answer::No, EvidenceSource::Measurement);
        measured.measured = Some(MeasuredValue {
            quantity: MeasureQuantity::LesionDiameterMm,
            value: 9.0,
            image_id: image,
        });
        let ev = vec![
            measured,
            fixtures::evidence(&ids[1], 2, Answer::No, EvidenceSource::Vlm),
        ];
        let s: Vec<_> = scored(&cands, &ev).into_iter().map(|s| s.score).collect();
        let dirty = BTreeSet::new();
        assert_eq!(
            decide(&input(&s, &dirty)).status,
            DiagnosisStatus::Uncertain
        );
        let clean = BTreeSet::from([image]);
        assert_eq!(decide(&input(&s, &clean)).status, DiagnosisStatus::Healthy);
    }

    #[test]
    fn indicator_question_skips_settled_indicators() {
        let profiles = fixtures::problems();
        let cands = fixtures::candidates(
            &profiles,
            &["tomato_early_blight", "tomato_septoria_leaf_spot"],
        );
        let blight: Vec<_> = cands[0].indicators().map(|i| i.id.clone()).collect();
        let ev = vec![
            fixtures::evidence(&blight[0], 3, Answer::Unsure, EvidenceSource::User),
            fixtures::evidence(&blight[1], 2, Answer::Yes, EvidenceSource::Vlm),
        ];
        let ranked = scored(&cands, &ev);
        let q = choose_question(Ambiguity::Undecided, &ranked, &AnswerSet::new(&ev)).unwrap();
        let QuestionChoice::Indicator {
            indicator,
            candidates,
        } = q
        else {
            panic!("indicator")
        };
        assert_ne!(indicator.id, blight[0], "the user already answered it");
        assert_eq!(
            indicator.weight.get(),
            3,
            "septoria's small_spots, weight 3"
        );
        assert_eq!(candidates.len(), 2);
    }
}

#[cfg(test)]
mod props {
    use super::*;
    use crate::{fixtures, scoring::score_all};
    use domain::records::diagnosis::EvidenceSource;
    use domain::wire::Confidence;
    use proptest::prelude::*;

    fn answer() -> impl Strategy<Value = Option<(Answer, EvidenceSource)>> {
        let a = prop_oneof![Just(Answer::Yes), Just(Answer::No), Just(Answer::Unsure)];
        let s = prop_oneof![
            Just(EvidenceSource::Vlm),
            Just(EvidenceSource::Measurement),
            Just(EvidenceSource::User)
        ];
        proptest::option::of((a, s))
    }

    proptest! {
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
    }
}
