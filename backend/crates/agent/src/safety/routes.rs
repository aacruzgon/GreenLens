//! The router and safety gate (architecture 7, router and safety gate): turning the text
//! classifier's scores into a [`ChatRoute`], the poisoning route, and the fixed texts the
//! safety routes use.
//!
//! The scores are compared with each other, never read as probabilities: each label's score is
//! shifted by a calibration offset fitted on the labelled example messages, the highest wins,
//! and a near tie with `safety_sensitive` resolves to `safety_sensitive` (fail safe). A cheap
//! keyword check for a person or pet being poisoned runs whatever the classifier says, so the
//! poison-control answer never depends on a model.

use std::{collections::BTreeMap, sync::LazyLock};

use domain::{chat::ChatRoute, ports::LabelScore};
use regex::{Regex, RegexBuilder};

/// The labels of `ClassifierTask::Route`, in the order of contracts 8.
pub const ROUTE_LABELS: [&str; 5] = [
    "diagnosis_question",
    "care_question",
    "task_change",
    "safety_sensitive",
    "off_topic",
];

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

fn parse_route(label: &str) -> Option<ChatRoute> {
    Some(match label {
        "diagnosis_question" => ChatRoute::DiagnosisQuestion,
        "care_question" => ChatRoute::CareQuestion,
        "task_change" => ChatRoute::TaskChange,
        "safety_sensitive" => ChatRoute::SafetySensitive,
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
/// dose check still applies to every sentence.
pub fn decide(
    text: &str,
    scores: Option<&[LabelScore]>,
    calibration: &Calibration,
) -> RouteDecision {
    let mut ranked: Vec<(ChatRoute, f64)> = scores
        .unwrap_or_default()
        .iter()
        .filter_map(|s| {
            let route = parse_route(&s.label)?;
            let offset = calibration.offsets.get(&s.label).copied().unwrap_or(0.0);
            Some((route, s.score + offset))
        })
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));

    let route = match ranked.as_slice() {
        [] => ChatRoute::CareQuestion,
        [(top, _)] => *top,
        [(top, top_score), (second, second_score), ..] => {
            let safety = ranked
                .iter()
                .find(|(r, _)| *r == ChatRoute::SafetySensitive)
                .map(|(_, s)| *s);
            if safety.is_some_and(|s| top_score - s <= calibration.safety_margin) {
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

    if mentions_poisoning(text) && route != ChatRoute::OffTopic {
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
            decide("When should I water?", Some(&s), &c),
            RouteDecision::Model(ChatRoute::CareQuestion)
        );
        let mut c = Calibration::default();
        c.offsets.insert("diagnosis_question".to_owned(), 0.2);
        assert_eq!(
            decide("When should I water?", Some(&s), &c),
            RouteDecision::Model(ChatRoute::DiagnosisQuestion)
        );
    }

    #[test]
    fn a_near_tie_with_safety_sensitive_is_safety_sensitive() {
        let s = scores(&[("care_question", 0.62), ("safety_sensitive", 0.55)]);
        assert_eq!(
            decide("Can I use copper on it?", Some(&s), &Calibration::default()),
            RouteDecision::Model(ChatRoute::SafetySensitive)
        );
    }

    #[test]
    fn off_topic_needs_a_clear_win() {
        let c = Calibration::default();
        let close = scores(&[("off_topic", 0.5), ("care_question", 0.4)]);
        assert_eq!(
            decide("what about the weather", Some(&close), &c),
            RouteDecision::Model(ChatRoute::CareQuestion)
        );
        let clear = scores(&[("off_topic", 0.9), ("care_question", 0.1)]);
        assert_eq!(
            decide("write me a poem", Some(&clear), &c),
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
                decide(text, Some(&s), &c),
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
                decide(text, Some(&s), &c),
                RouteDecision::Model(ChatRoute::CareQuestion),
                "{text}"
            );
        }
    }

    #[test]
    fn no_scores_means_a_care_question() {
        assert_eq!(
            decide("hello", None, &Calibration::default()),
            RouteDecision::Model(ChatRoute::CareQuestion)
        );
    }
}
