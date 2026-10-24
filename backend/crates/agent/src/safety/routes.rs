//! The router and safety gate (architecture 7, router and safety gate): turning the text
//! classifier's scores into a [`ChatRoute`], the poisoning route, and the fixed texts the
//! safety routes use.
//!
//! The scores are compared with each other, never read as probabilities: each label's score is
//! shifted by a calibration offset fitted on the labelled example messages, the highest wins,
//! and a near tie with `safety_sensitive` resolves to `safety_sensitive` (fail safe). A cheap
//! keyword check for a person or pet being poisoned runs whatever the classifier says, even
//! when `off_topic` wins, so the poison-control answer never depends on a model.
//!
//! **Farm records** (gap G10). When the garden has farm records, the classifier is also asked
//! for [`FARM_RECORDS_LABEL`], a question about what the garden's own records say (its Slack
//! channels and shared documents), and that label is a care question: the chat model answers it
//! with `search_farm_records`. Without farm records the classifier is asked exactly as before,
//! and a `farm_records` score counts as `off_topic`, since nothing could answer it.

use std::{collections::BTreeMap, sync::LazyLock};

use domain::{chat::ChatRoute, ports::LabelScore};
use regex::{Regex, RegexBuilder};

pub use domain::ports::{FARM_RECORDS_LABEL, FARM_ROUTE_LABELS, ROUTE_LABELS};

/// In a garden with farm records, when `farm_records` wins and `safety_sensitive` scores at
/// least this, the route is `safety_sensitive`: a question about the dose or the product the
/// records hold keeps the safety policy, however far `farm_records` is ahead.
pub const FARM_SAFETY_FLOOR: f64 = 0.5;

/// The route's wire name.
pub const fn route_label(route: ChatRoute) -> &'static str {
    match route {
        ChatRoute::DiagnosisQuestion => "diagnosis_question",
        ChatRoute::CareQuestion => "care_question",
        ChatRoute::TaskChange => "task_change",
        ChatRoute::SafetySensitive => "safety_sensitive",
        ChatRoute::OffTopic => "off_topic",
    }
}

/// The route a label counts for. `farm_records` is a care question in a garden with farm
/// records; in one without, the classifier wasn't asked for it, so it is unknown.
fn parse_route(label: &str, farm_records: bool) -> Option<ChatRoute> {
    Some(match label {
        "diagnosis_question" => ChatRoute::DiagnosisQuestion,
        "care_question" => ChatRoute::CareQuestion,
        "task_change" => ChatRoute::TaskChange,
        "safety_sensitive" => ChatRoute::SafetySensitive,
        // Unasked (no records), it is an unknown label like any other.
        FARM_RECORDS_LABEL if farm_records => ChatRoute::CareQuestion,
        "off_topic" => ChatRoute::OffTopic,
        _ => return None,
    })
}

/// Per-label offsets fitted on the labelled set, plus the near-tie margin.
#[derive(Debug, Clone, PartialEq)]
pub struct Calibration {
    /// Added to each label's raw score before comparing. Missing labels get 0.
    pub offsets: BTreeMap<String, f64>,
    /// When `safety_sensitive` is within this of the winner, it wins.
    pub safety_margin: f64,
    /// When the winner beats the runner-up by less than this and the winner is `off_topic`,
    /// the runner-up wins: a garden question is never brushed off on a coin toss.
    pub off_topic_margin: f64,
}

impl Default for Calibration {
    fn default() -> Self {
        Self {
            offsets: BTreeMap::new(),
            safety_margin: 0.15,
            off_topic_margin: 0.2,
        }
    }
}

impl Calibration {
    /// Jev's calibration, fitted on 2026-10-23 on `jev-1.14.0` with the 41 labelled messages of
    /// `tests/live_jev.rs`. Jev asks one yes/no question per label, and two of its answers run
    /// high where the LLM's don't: `off_topic` reached 0.40–0.67 on farm-records questions,
    /// 0.59 on "Add a task to water bed 2 tomorrow morning." and 0.85 on "I got some of the
    /// spray in my eyes, help" (above `safety_sensitive`'s 0.69), and `care_question` reached
    /// 0.70–0.88 on task changes. So `off_topic` is lowered by 0.25 (plainly off-topic messages
    /// still scored 0.97–0.99), `task_change` raised by 0.1, and `safety_sensitive` raised by
    /// 0.1 to fail safe (no message outside the safety set scored it above 0.21). The margins
    /// stay those of [`Calibration::default`]. A new Jev version is fitted again before
    /// `JEV_MODEL` moves to it.
    pub fn jev() -> Self {
        Self {
            offsets: BTreeMap::from([
                ("off_topic".to_owned(), -0.25),
                ("task_change".to_owned(), 0.1),
                ("safety_sensitive".to_owned(), 0.1),
            ]),
            ..Self::default()
        }
    }
}

/// How a message is answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteDecision {
    /// A normal turn on this route (`safety_sensitive` adds the safety policy to the prompt).
    Model(ChatRoute),
    /// A person or pet may have been poisoned: the fixed poison-control answer, no model.
    PoisonControl,
    /// A short redirect from the cheapest model, no tools.
    OffTopic,
}

impl RouteDecision {
    /// The route sent in `message_started`.
    pub const fn route(self) -> ChatRoute {
        match self {
            Self::Model(r) => r,
            Self::PoisonControl => ChatRoute::SafetySensitive,
            Self::OffTopic => ChatRoute::OffTopic,
        }
    }
}

/// "My dog ate…", "my son swallowed…", "is it poisonous to cats", "I got it in my eyes".
static POISONING: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"(?x)
        # a poison word and a person or pet anywhere in the message
        \b(?:poison(?:ed|ing|ous)?|toxic)\b.*?\b(?:me|us|i|we|someone|anyone|person|people|humans?|child|children|kids?|sons?|daughters?|bab(?:y|ies)|toddlers?|dogs?|pupp(?:y|ies)|cats?|kittens?|pets?|horses?|rabbits?|birds?|parrots?)\b
      | \b(?:me|us|i|we|someone|anyone|person|people|humans?|child|children|kids?|sons?|daughters?|bab(?:y|ies)|toddlers?|dogs?|pupp(?:y|ies)|cats?|kittens?|pets?|horses?|rabbits?|birds?|parrots?)\b.*?\b(?:poison(?:ed|ing|ous)?|toxic)\b
        # a person or pet as the subject of swallowing: at most one word between
      | \b(?:i|we|he|she|they|someone|child|children|kids?|sons?|daughters?|bab(?:y|ies)|toddlers?|dogs?|pupp(?:y|ies)|cats?|kittens?|pets?|horses?|rabbits?|birds?|parrots?)\b(?:\s+[\w']+)?\s+(?:swallowed|ingested|ate|eaten|drank|licked|chewed)\b
        # a product in the eyes or mouth
      | \b(?:in|into)\s+(?:my|his|her|their|its)\s+(?:eyes?|mouth)\b
        ",
    )
    .case_insensitive(true)
    .build()
    // reason: a constant pattern, covered by the tests below.
    .expect("poisoning pattern compiles")
});

/// Whether a message is about a person or pet being poisoned.
pub fn mentions_poisoning(text: &str) -> bool {
    POISONING.is_match(text)
}

/// Decides the route from the classifier's scores and the poisoning check. With no usable
/// scores (the classifier failed or returned unknown labels) the turn is a care question: the
/// dose check still applies to every sentence. `farm_records` says whether the garden has farm
/// records, as the classifier was told: it decides what a `farm_records` score counts for. Two
/// labels for the same route count once, with the higher calibrated score. A `farm_records` win
/// with `safety_sensitive` at [`FARM_SAFETY_FLOOR`] or more is safety-sensitive. A message that
/// [`mentions_poisoning`] is [`RouteDecision::PoisonControl`] on any route, `off_topic`
/// included: Jev scored "I got some of the spray in my eyes, help" `off_topic` 0.85.
pub fn decide(
    text: &str,
    scores: Option<&[LabelScore]>,
    calibration: &Calibration,
    farm_records: bool,
) -> RouteDecision {
    // Each usable label's route, calibrated score, and whether it was `farm_records`.
    let mut ranked: Vec<(ChatRoute, f64, bool)> = scores
        .unwrap_or_default()
        .iter()
        .filter_map(|s| {
            let route = parse_route(&s.label, farm_records)?;
            let offset = calibration.offsets.get(&s.label).copied().unwrap_or(0.0);
            Some((route, s.score + offset, s.label == FARM_RECORDS_LABEL))
        })
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut seen = Vec::with_capacity(ranked.len());
    ranked.retain(|(route, _, _)| {
        let first = !seen.contains(route);
        seen.push(*route);
        first
    });

    let route = match ranked.as_slice() {
        [] => ChatRoute::CareQuestion,
        [(top, _, _)] => *top,
        [(top, top_score, farm_win), (second, second_score, _), ..] => {
            let safety = ranked
                .iter()
                .find(|(r, _, _)| *r == ChatRoute::SafetySensitive)
                .map(|(_, s, _)| *s);
            if safety.is_some_and(|s| {
                top_score - s <= calibration.safety_margin || (*farm_win && s >= FARM_SAFETY_FLOOR)
            }) {
                ChatRoute::SafetySensitive
            } else if *top == ChatRoute::OffTopic
                && top_score - second_score < calibration.off_topic_margin
            {
                *second
            } else {
                *top
            }
        }
    };

    if mentions_poisoning(text) {
        return RouteDecision::PoisonControl;
    }
    match route {
        ChatRoute::OffTopic => RouteDecision::OffTopic,
        r => RouteDecision::Model(r),
    }
}

/// The poison-control answer. Deliberately without phone numbers, which differ by country and
/// change; `AgentConfig::poison_control_text` replaces it with the local numbers at deployment.
pub const POISON_CONTROL_TEXT: &str = "If a person may have swallowed, breathed in or touched a \
garden product or a poisonous plant, call your local poison control centre or emergency \
services now, and have the product label or a photo of the plant with you. If it is a pet, \
call your vet or an animal poison control line straight away. Don't wait for symptoms, and \
don't make anyone vomit unless the poison centre tells you to.";

/// The safety policy added to the system prompt on the `safety_sensitive` route.
pub const SAFETY_POLICY: &str = "This question touches on pesticides or other products. Give \
integrated pest management and non-chemical options first. Never give a dose, rate, mixing \
ratio or percentage. If a product is appropriate, name the kind of product only and say to use \
one registered for this use where the user lives and to follow its label exactly.";

/// The text that replaces a reply sentence that fails the dose check ([contracts 7]: "a failing
/// sentence is replaced by the fixed safety note").
pub const REPLACED_SENTENCE: &str = domain::records::plan::SAFETY_NOTE;

#[cfg(test)]
mod tests {
    use super::*;

    fn scores(pairs: &[(&str, f64)]) -> Vec<LabelScore> {
        pairs
            .iter()
            .map(|(l, s)| LabelScore {
                label: (*l).to_owned(),
                score: *s,
            })
            .collect()
    }

    #[test]
    fn highest_calibrated_score_wins() {
        let c = Calibration::default();
        let s = scores(&[
            ("care_question", 0.7),
            ("diagnosis_question", 0.6),
            ("safety_sensitive", 0.1),
        ]);
        assert_eq!(
            decide("When should I water?", Some(&s), &c, false),
            RouteDecision::Model(ChatRoute::CareQuestion)
        );
        let mut c = Calibration::default();
        c.offsets.insert("diagnosis_question".to_owned(), 0.2);
        assert_eq!(
            decide("When should I water?", Some(&s), &c, false),
            RouteDecision::Model(ChatRoute::DiagnosisQuestion)
        );
    }

    #[test]
    fn a_near_tie_with_safety_sensitive_is_safety_sensitive() {
        let s = scores(&[("care_question", 0.62), ("safety_sensitive", 0.55)]);
        assert_eq!(
            decide(
                "Can I use copper on it?",
                Some(&s),
                &Calibration::default(),
                false
            ),
            RouteDecision::Model(ChatRoute::SafetySensitive)
        );
    }

    #[test]
    fn off_topic_needs_a_clear_win() {
        let c = Calibration::default();
        let close = scores(&[("off_topic", 0.5), ("care_question", 0.4)]);
        assert_eq!(
            decide("what about the weather", Some(&close), &c, false),
            RouteDecision::Model(ChatRoute::CareQuestion)
        );
        let clear = scores(&[("off_topic", 0.9), ("care_question", 0.1)]);
        assert_eq!(
            decide("write me a poem", Some(&clear), &c, false),
            RouteDecision::OffTopic
        );
    }

    #[test]
    fn poisoning_is_routed_to_poison_control_whatever_the_scores() {
        let s = scores(&[("care_question", 0.9), ("safety_sensitive", 0.1)]);
        let c = Calibration::default();
        for text in [
            "My dog ate some slug pellets",
            "the baby has eaten a leaf",
            "my son swallowed a berry from the plant",
            "Is this plant poisonous to cats?",
            "I got neem spray in my eyes",
        ] {
            assert_eq!(
                decide(text, Some(&s), &c, false),
                RouteDecision::PoisonControl,
                "{text}"
            );
        }
        for text in [
            "Aphids ate the new leaves",
            "I think slugs ate my seedlings",
            "Is the sap toxic?",
        ] {
            assert_eq!(
                decide(text, Some(&s), &c, false),
                RouteDecision::Model(ChatRoute::CareQuestion),
                "{text}"
            );
        }
    }

    #[test]
    fn a_farm_records_question_is_a_care_question_only_with_farm_records() {
        let c = Calibration::default();
        // Q19 of the Maple Row evaluation: the classifier's farm label wins clearly.
        let text = "What did Riley post in #field-reports on August 31?";
        let s = scores(&[
            ("farm_records", 0.9),
            ("off_topic", 0.5),
            ("care_question", 0.1),
        ]);
        assert_eq!(
            decide(text, Some(&s), &c, true),
            RouteDecision::Model(ChatRoute::CareQuestion)
        );
        // Without farm records the label wasn't asked for and counts for nothing: off_topic
        // beats care_question by the margin, as before G10.
        assert_eq!(decide(text, Some(&s), &c, false), RouteDecision::OffTopic);
        // A safety-sensitive farm question stays safety-sensitive: in a near tie, and when
        // farm_records wins clearly but safety_sensitive reaches the floor.
        let text = "How much Kocide went in per litre?";
        for pairs in [
            [("farm_records", 0.8), ("safety_sensitive", 0.7)],
            [("farm_records", 1.0), ("safety_sensitive", 0.6)],
            [
                ("farm_records", 1.0),
                ("safety_sensitive", FARM_SAFETY_FLOOR),
            ],
        ] {
            assert_eq!(
                decide(text, Some(&scores(&pairs)), &c, true),
                RouteDecision::Model(ChatRoute::SafetySensitive),
                "{pairs:?}"
            );
        }
        // Below the floor, the records question is a care question.
        let s = scores(&[("farm_records", 1.0), ("safety_sensitive", 0.3)]);
        assert_eq!(
            decide(text, Some(&s), &c, true),
            RouteDecision::Model(ChatRoute::CareQuestion)
        );
        // The floor is for a farm_records win only: care_question still needs the margin.
        let s = scores(&[("care_question", 1.0), ("safety_sensitive", 0.6)]);
        assert_eq!(
            decide("When should I water?", Some(&s), &c, true),
            RouteDecision::Model(ChatRoute::CareQuestion)
        );
    }

    #[test]
    fn poisoning_wins_over_a_clear_off_topic_win() {
        let s = scores(&[("off_topic", 0.95), ("safety_sensitive", 0.1)]);
        for c in [Calibration::default(), Calibration::jev()] {
            for text in [
                "My dog ate some slug pellets, what should I do?",
                "I got some of the spray in my eyes, help",
            ] {
                assert_eq!(
                    decide(text, Some(&s), &c, false),
                    RouteDecision::PoisonControl,
                    "{text}"
                );
            }
            assert_eq!(
                decide("write me a poem about football", Some(&s), &c, false),
                RouteDecision::OffTopic
            );
        }
    }

    #[test]
    fn farm_records_never_rescue_a_clearly_off_topic_message() {
        let c = Calibration::default();
        let s = scores(&[
            ("off_topic", 0.95),
            ("farm_records", 0.1),
            ("care_question", 0.05),
        ]);
        for farm_records in [true, false] {
            for text in ["write me a poem about football", "what's the bitcoin price"] {
                assert_eq!(
                    decide(text, Some(&s), &c, farm_records),
                    RouteDecision::OffTopic,
                    "{text}, farm_records={farm_records}"
                );
            }
        }
    }

    #[test]
    fn an_unasked_farm_label_counts_for_nothing() {
        let c = Calibration::default();
        // Without farm records the classifier wasn't offered farm_records, so a score for it
        // (a stub or a model that ignored the label list) is unknown, not off-topic.
        let s = scores(&[("farm_records", 0.9), ("care_question", 0.1)]);
        assert_eq!(
            decide("any notes on bed 3?", Some(&s), &c, false),
            RouteDecision::Model(ChatRoute::CareQuestion)
        );
        // So it can't turn a poisoning message off-topic and skip poison control.
        assert_eq!(
            decide(
                "My dog ate the slug pellets, what did the crew log?",
                Some(&s),
                &c,
                false
            ),
            RouteDecision::PoisonControl
        );
    }

    /// Jev's live scores of 2026-10-23 (`jev-1.14.0`, run 1 of
    /// `tests/fixtures/jev-calibration-2026-10-23.jsonl`, the top labels only) for the labelled
    /// messages at their closest: a task change 0.08 ahead of `care_question`, a farm question
    /// 0.10 ahead of `off_topic`, and a splash of spray in the eyes where `off_topic` outscores
    /// `safety_sensitive`.
    #[test]
    fn jevs_calibration_routes_its_close_calls() {
        /// A message, whether the garden has farm records, Jev's scores, and the decision.
        type Case<'a> = (&'a str, bool, &'a [(&'a str, f64)], RouteDecision);
        let jev = Calibration::jev();
        let cases: [Case<'_>; 5] = [
            (
                "Change the plan to water every three days instead of daily.",
                false,
                &[
                    ("task_change", 0.96),
                    ("care_question", 0.88),
                    ("off_topic", 0.20),
                ],
                RouteDecision::Model(ChatRoute::TaskChange),
            ),
            (
                "I got some of the spray in my eyes, help",
                false,
                &[
                    ("off_topic", 0.85),
                    ("safety_sensitive", 0.69),
                    ("task_change", 0.03),
                ],
                RouteDecision::PoisonControl,
            ),
            (
                "When were target rings first reported on bed 3?",
                true,
                &[
                    ("farm_records", 0.77),
                    ("off_topic", 0.67),
                    ("safety_sensitive", 0.21),
                ],
                RouteDecision::Model(ChatRoute::CareQuestion),
            ),
            (
                "Can I spray neem oil on my tomatoes the day before harvest?",
                false,
                &[("care_question", 0.95), ("safety_sensitive", 0.92)],
                RouteDecision::Model(ChatRoute::SafetySensitive),
            ),
            (
                "What's the bitcoin price?",
                true,
                &[
                    ("off_topic", 0.98),
                    ("farm_records", 0.02),
                    ("safety_sensitive", 0.01),
                    ("task_change", 0.01),
                ],
                RouteDecision::OffTopic,
            ),
        ];
        for (text, farm_records, pairs, want) in cases {
            assert_eq!(
                decide(text, Some(&scores(pairs)), &jev, farm_records),
                want,
                "{text}"
            );
        }
        // The eye splash's scores on a message without a poisoning keyword route safely with
        // either calibration. With `safety_sensitive` lowered to 0.60 (invented, not
        // measured), only Jev's offsets keep it off the off-topic reply.
        let s = scores(&[("off_topic", 0.85), ("safety_sensitive", 0.69)]);
        for c in [Calibration::default(), jev.clone()] {
            assert_eq!(
                decide("Help, the spray got on my skin", Some(&s), &c, false),
                RouteDecision::Model(ChatRoute::SafetySensitive)
            );
        }
        let s = scores(&[("off_topic", 0.85), ("safety_sensitive", 0.60)]);
        assert_eq!(
            decide(
                "Help, the spray got on my skin",
                Some(&s),
                &Calibration::default(),
                false
            ),
            RouteDecision::OffTopic
        );
        assert_eq!(
            decide("Help, the spray got on my skin", Some(&s), &jev, false),
            RouteDecision::Model(ChatRoute::SafetySensitive)
        );
    }

    #[test]
    fn no_scores_means_a_care_question() {
        assert_eq!(
            decide("hello", None, &Calibration::default(), false),
            RouteDecision::Model(ChatRoute::CareQuestion)
        );
    }
}
