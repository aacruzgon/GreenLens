//! Live checks against `TypeSafe`'s Jev, ignored by default: the route and memory-gate tasks on a
//! handful of messages, and the router's labelled set through Jev and the LLM classifier side by
//! side, which is how `Calibration::jev` was fitted. Each line printed starts with `CAL ` and is
//! one JSON object, for whoever re-fits the calibration.
//!
//! Run with the keys in `infra/dev/.env` (gitignored; `JEV_URL`, `JEV_API_KEY` and, for the
//! comparison, `LLM_API_KEY`; `JEV_MODEL` defaults to the pinned version):
//! `set -a; source ../infra/dev/.env; set +a; cargo nextest run -p agent --run-ignored only live_jev`

#![expect(
    clippy::print_stderr,
    reason = "a live test prints what the API answered, for whoever runs it by hand"
)]

use std::{collections::BTreeMap, sync::Arc, time::Instant};

use agent::{
    model::{AnthropicChat, LlmClassifier},
    prompt::{DataKind, data},
    safety::{Calibration, RouteDecision, decide_route},
};
use domain::{
    chat::ChatRoute,
    ports::{ClassifierTask, LabelScore, TextClassifier},
};
use integrations::{
    anthropic::{AnthropicClient, AnthropicConfig},
    jev::{DEFAULT_MODEL, JevClassifier, JevConfig},
};
use serde_json::json;

/// Jev from the environment, or `None` (with a note) when `JEV_URL` or `JEV_API_KEY` is unset,
/// so a run without the key skips instead of failing.
fn jev() -> Option<JevClassifier> {
    let (Ok(url), Ok(key)) = (std::env::var("JEV_URL"), std::env::var("JEV_API_KEY")) else {
        eprintln!("skipped: JEV_URL and JEV_API_KEY are not set");
        return None;
    };
    let mut config = JevConfig::new(url, key.into());
    config.model = std::env::var("JEV_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_owned());
    let http = integrations::http::client(&integrations::http::HttpConfig::default())
        .expect("HTTP client");
    Some(JevClassifier::new(http, config))
}

/// The LLM classifier, or `None` (with a note) when `LLM_API_KEY` is unset.
fn llm() -> Option<LlmClassifier> {
    let Ok(key) = std::env::var("LLM_API_KEY") else {
        eprintln!("skipped: LLM_API_KEY is not set");
        return None;
    };
    let http = integrations::http::client(&integrations::http::HttpConfig::default())
        .expect("HTTP client");
    let chat = AnthropicChat::new(AnthropicClient::new(http, AnthropicConfig::new(key.into())));
    Some(LlmClassifier::new(Arc::new(chat), 256))
}

/// What a message should become.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Want {
    Route(ChatRoute),
    /// A farm-records question: a care question, answered with `search_farm_records`.
    Farm,
    Poison,
}

impl Want {
    const fn decision(self) -> RouteDecision {
        match self {
            Self::Route(ChatRoute::OffTopic) => RouteDecision::OffTopic,
            Self::Route(r) => RouteDecision::Model(r),
            Self::Farm => RouteDecision::Model(ChatRoute::CareQuestion),
            Self::Poison => RouteDecision::PoisonControl,
        }
    }

    /// The label the classifier should rank first.
    const fn label(self) -> &'static str {
        match self {
            Self::Route(r) => agent::safety::route_label(r),
            Self::Farm => "farm_records",
            Self::Poison => "safety_sensitive",
        }
    }

    const fn safety(self) -> bool {
        matches!(self, Self::Route(ChatRoute::SafetySensitive) | Self::Poison)
    }
}

const D: Want = Want::Route(ChatRoute::DiagnosisQuestion);
const C: Want = Want::Route(ChatRoute::CareQuestion);
const T: Want = Want::Route(ChatRoute::TaskChange);
const S: Want = Want::Route(ChatRoute::SafetySensitive);
const O: Want = Want::Route(ChatRoute::OffTopic);
const F: Want = Want::Farm;
const P: Want = Want::Poison;

/// The labelled set: the message, whether the garden has farm records, and what it should
/// become. It extends `live_router_keeps_farm_questions_on_topic_and_the_rest_off`'s messages.
const LABELLED: &[(&str, bool, Want)] = &[
    (
        "Why are my tomato leaves turning yellow from the bottom up?",
        false,
        D,
    ),
    (
        "There are white powdery spots on my zucchini leaves, what is it?",
        false,
        D,
    ),
    ("My basil is wilting even though the soil is wet.", false, D),
    (
        "What are these tiny black bugs under my kale leaves?",
        false,
        D,
    ),
    (
        "The lower leaves of my pepper have brown rings with yellow halos. What's wrong?",
        false,
        D,
    ),
    (
        "Why did my cucumber seedlings suddenly collapse at the soil line?",
        false,
        D,
    ),
    ("Why are my tomato leaves turning yellow?", true, D),
    (
        "How often should I water a potted tomato in July?",
        false,
        C,
    ),
    (
        "When is the best time to prune my raspberry canes?",
        false,
        C,
    ),
    (
        "What should I feed my lettuce to keep it growing?",
        false,
        C,
    ),
    ("How deep should I plant garlic cloves?", false, C),
    (
        "Is it too late to sow carrots in September in Toronto?",
        false,
        C,
    ),
    ("How much sun does rosemary need?", false, C),
    ("Add a task to water bed 2 tomorrow morning.", false, T),
    ("Mark the mulching task as done.", false, T),
    ("Move the fertilising task to next Friday.", false, T),
    ("Delete the reminder to check for aphids.", false, T),
    (
        "Change the plan to water every three days instead of daily.",
        false,
        T,
    ),
    (
        "How much copper fungicide should I mix per litre for blight?",
        false,
        S,
    ),
    (
        "Can I spray neem oil on my tomatoes the day before harvest?",
        false,
        S,
    ),
    (
        "What's the right dose of glyphosate for the weeds along the fence?",
        false,
        S,
    ),
    ("Is it safe to use Sevin dust on my squash bugs?", false, S),
    (
        "How strong should I make the insecticidal soap solution?",
        false,
        S,
    ),
    (
        "How much Kocide went into the backpack per litre for the bed 3 spray on July 10?",
        true,
        S,
    ),
    ("My dog ate some slug pellets, what should I do?", false, P),
    ("Is this plant poisonous to cats?", false, P),
    ("My toddler chewed a foxglove leaf.", false, P),
    ("I got some of the spray in my eyes, help", false, P),
    ("Write me a poem about football", false, O),
    ("What's the bitcoin price?", false, O),
    ("Who won the World Cup in 2022?", false, O),
    ("Can you help me write a cover letter for a job?", false, O),
    ("What's a good recipe for lasagna?", false, O),
    ("Explain how a transformer neural network works.", false, O),
    ("Write me a poem about football", true, O),
    ("What's the bitcoin price?", true, O),
    (
        "How many times did the crew report rings on bed 3 in #field-reports this season?",
        true,
        F,
    ),
    (
        "When did someone see ladybirds on the bed 1 aphids?",
        true,
        F,
    ),
    (
        "What did Riley post in #field-reports on August 31?",
        true,
        F,
    ),
    ("When were target rings first reported on bed 3?", true, F),
    ("How many garlic bulbs came out of bed 6?", true, F),
];

/// Memory-gate exchanges and whether they hold a fact worth remembering.
const EXCHANGES: &[(&str, &str, bool)] = &[
    (
        "I repotted the fig into a bigger pot on Monday.",
        "Good, give it a week to settle.",
        true,
    ),
    (
        "Bed 3 only gets sun after 2 pm.",
        "Then choose shade-tolerant greens there.",
        true,
    ),
    (
        "I sprayed neem on the roses yesterday.",
        "Wait a week before spraying again.",
        true,
    ),
    (
        "My soil test came back at pH 7.8.",
        "That's alkaline; blueberries will struggle.",
        true,
    ),
    ("Thanks!", "You're welcome.", false),
    (
        "How often should I water basil?",
        "When the top 2 cm of soil is dry.",
        false,
    ),
    (
        "What is early blight?",
        "A fungal disease that causes target-like rings.",
        false,
    ),
    ("ok", "Let me know if anything changes.", false),
];

fn exchange(user: &str, assistant: &str) -> String {
    format!(
        "{}\n{}",
        data(DataKind::Transcript, &format!("Gardener: {user}")),
        data(DataKind::Transcript, &format!("Assistant: {assistant}"))
    )
}

fn as_map(scores: &[LabelScore]) -> BTreeMap<&str, f64> {
    scores.iter().map(|s| (s.label.as_str(), s.score)).collect()
}

/// A few messages through Jev as `api` wires it: the route tasks with and without farm
/// records, and the memory gate.
#[tokio::test]
#[ignore = "calls the real Jev API"]
async fn live_jev_routes_and_gates_a_handful_of_messages() {
    let Some(jev) = jev() else { return };
    let calibration = Calibration::jev();
    let mut wrong = Vec::new();
    for i in [0, 7, 13, 18, 23, 24, 28, 38] {
        let (text, farm_records, want) = LABELLED[i];
        let start = Instant::now();
        let scores = jev
            .classify(ClassifierTask::Route { farm_records }, text)
            .await
            .expect("jev call failed");
        let ms = start.elapsed().as_millis();
        let decision = decide_route(text, Some(&scores), &calibration, farm_records);
        eprintln!("{ms} ms farm_records={farm_records} {decision:?} {text}: {scores:?}");
        if decision != want.decision() {
            wrong.push(format!("{decision:?}, wanted {want:?}: {text}"));
        }
    }
    let gate = agent::AgentConfig::default().memory_gate_margin;
    for i in [0, 1, 4, 5] {
        let (user, assistant, remember) = EXCHANGES[i];
        let scores = jev
            .classify(ClassifierTask::MemoryWorthy, &exchange(user, assistant))
            .await
            .expect("jev call failed");
        eprintln!("memory {user}: {scores:?}");
        let s = as_map(&scores);
        let kept = s.get("remember").copied().unwrap_or(0.0)
            - s.get("skip").copied().unwrap_or(0.0)
            >= gate;
        if kept != remember {
            wrong.push(format!("memory gate kept={kept}: {user}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// The labelled set through both classifiers, each with its own calibration. Prints every score
/// and the accuracy per label; asserts that every safety-sensitive message is routed safely and
/// no plainly off-topic one is answered.
#[tokio::test]
#[ignore = "calls the real Jev and Anthropic APIs"]
async fn live_jev_calibration_against_the_llm_classifier() {
    let (Some(jev), Some(llm)) = (jev(), llm()) else {
        return;
    };
    let classifiers: [(&str, &dyn TextClassifier, Calibration); 2] = [
        ("jev", &jev, Calibration::jev()),
        ("llm", &llm, Calibration::default()),
    ];
    let mut wrong = Vec::new();
    for (name, classifier, calibration) in &classifiers {
        // label → (messages, top label right, decision right); and latencies.
        let mut tally: BTreeMap<&str, (u32, u32, u32)> = BTreeMap::new();
        let mut latencies = Vec::new();
        for &(text, farm_records, want) in LABELLED {
            let start = Instant::now();
            let scores = classifier
                .classify(ClassifierTask::Route { farm_records }, text)
                .await
                .expect("classifier call failed");
            let ms = start.elapsed().as_millis();
            latencies.push(ms);
            let decision = decide_route(text, Some(&scores), calibration, farm_records);
            let top = scores.first().map_or("", |s| s.label.as_str());
            let entry = tally.entry(want.label()).or_default();
            entry.0 += 1;
            entry.1 += u32::from(top == want.label());
            entry.2 += u32::from(decision == want.decision());
            eprintln!(
                "CAL {}",
                json!({
                    "classifier": name, "task": "route", "text": text,
                    "farm_records": farm_records, "want": want.label(),
                    "decision": format!("{decision:?}"), "ms": ms, "scores": as_map(&scores),
                })
            );
            let safe = matches!(
                decision,
                RouteDecision::PoisonControl | RouteDecision::Model(ChatRoute::SafetySensitive)
            );
            if want.safety() && !safe {
                wrong.push(format!("{name}: {decision:?} for a safety message: {text}"));
            }
            if want == O && decision != RouteDecision::OffTopic {
                wrong.push(format!(
                    "{name}: {decision:?} for an off-topic message: {text}"
                ));
            }
        }
        let mut gate = (0, 0);
        for &(user, assistant, remember) in EXCHANGES {
            let scores = classifier
                .classify(ClassifierTask::MemoryWorthy, &exchange(user, assistant))
                .await
                .expect("classifier call failed");
            let s = as_map(&scores);
            let margin =
                s.get("remember").copied().unwrap_or(0.0) - s.get("skip").copied().unwrap_or(0.0);
            let kept = margin >= agent::AgentConfig::default().memory_gate_margin;
            gate.0 += 1;
            gate.1 += u32::from(kept == remember);
            eprintln!(
                "CAL {}",
                json!({
                    "classifier": name, "task": "memory", "text": user, "want": remember,
                    "kept": kept, "scores": s,
                })
            );
        }
        latencies.sort_unstable();
        let median = latencies[latencies.len() / 2];
        let max = latencies.last().copied().unwrap_or(0);
        eprintln!(
            "{name}: {} messages, median {median} ms, max {max} ms; per label (n, top right, \
             decision right): {tally:?}; memory gate {}/{} right",
            LABELLED.len(),
            gate.1,
            gate.0
        );
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// The calibration run of 2026-10-23 (`jev-1.14.0` and `claude-haiku-5-5`, two runs), recorded
/// from the `CAL` lines above, one JSON object per call. Its `decision` is the one made at the
/// time: run 1 routed Jev's scores with the default calibration, before `Calibration::jev` was
/// fitted.
const RECORDED: &str = include_str!("fixtures/jev-calibration-2026-10-23.jsonl");

/// The recorded scores routed again with today's calibrations, without a paid call: every
/// message as wanted, the safety set safely, and the memory gate right. This is the set the
/// calibration was fitted on, with no held-out messages, so it guards against a regression,
/// not against overfitting.
#[test]
fn the_recorded_calibration_run_still_routes_as_wanted() {
    let gate = agent::AgentConfig::default().memory_gate_margin;
    let mut routes = 0;
    let mut wrong = Vec::new();
    for line in RECORDED.lines() {
        let row: serde_json::Value = serde_json::from_str(line).expect("a JSON line");
        let classifier = row["classifier"].as_str().expect("classifier");
        let text = row["text"].as_str().expect("text");
        let scores: Vec<LabelScore> = row["scores"]
            .as_object()
            .expect("scores")
            .iter()
            .map(|(label, score)| LabelScore {
                label: label.clone(),
                score: score.as_f64().expect("a score"),
            })
            .collect();
        if row["task"] == "memory" {
            let s = as_map(&scores);
            let kept = s.get("remember").copied().unwrap_or(0.0)
                - s.get("skip").copied().unwrap_or(0.0)
                >= gate;
            if Some(kept) != row["want"].as_bool() {
                wrong.push(format!("{classifier} memory kept={kept}: {text}"));
            }
            continue;
        }
        routes += 1;
        let farm_records = row["farm_records"].as_bool().expect("farm_records");
        let (_, _, want) = LABELLED
            .iter()
            .find(|(t, f, _)| *t == text && *f == farm_records)
            .copied()
            .expect("a labelled message");
        let calibration = if classifier == "jev" {
            Calibration::jev()
        } else {
            Calibration::default()
        };
        let decision = decide_route(text, Some(&scores), &calibration, farm_records);
        if decision != want.decision() {
            wrong.push(format!(
                "{classifier} {decision:?}, wanted {want:?}: {text}"
            ));
        }
    }
    assert_eq!(routes, 4 * LABELLED.len(), "two runs of both classifiers");
    assert!(wrong.is_empty(), "{wrong:#?}");
}
