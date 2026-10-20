//! Live checks against the real Anthropic API, ignored by default. Each sends one short request
//! through the same adapter `api` uses, to settle what the docs leave open: whether the tool and
//! plan schemas (with `oneOf` and integer `format`s) are accepted, whether chat streams in pieces,
//! and whether the classifier's 256-token budget survives Haiku 5.5's thinking.
//!
//! Run with the key in `infra/dev/.env` (gitignored):
//! `set -a; source ../infra/dev/.env; set +a; cargo nextest run -p agent --run-ignored only live_`

#![expect(
    clippy::print_stderr,
    reason = "a live test prints what the API answered, for whoever runs it by hand"
)]

use std::{sync::Arc, time::Instant};

use agent::{
    model::AnthropicChat, model::LlmClassifier, plan_writer::PLAN_SCHEMA, tools::ToolRegistry,
};
use domain::ports::{
    ChatChunk, ChatMessage, ChatModel, ChatPurpose, ChatRequest, ChatRole, ClassifierTask,
    ContentPart, TextClassifier,
};
use futures::StreamExt;
use integrations::anthropic::{AnthropicClient, AnthropicConfig};

fn adapter() -> AnthropicChat {
    let key = std::env::var("LLM_API_KEY").expect("LLM_API_KEY is not set");
    let http = integrations::http::client(&integrations::http::HttpConfig::default())
        .expect("HTTP client");
    AnthropicChat::new(AnthropicClient::new(http, AnthropicConfig::new(key.into())))
}

fn user(text: &str) -> Vec<ChatMessage> {
    vec![ChatMessage {
        role: ChatRole::User,
        content: vec![ContentPart::Text(text.to_owned())],
    }]
}

/// A chat call with all twenty tools (three strict), streamed, on the chat model. Prints when
/// each text delta arrived, to show the answer streams in pieces rather than at the end.
#[tokio::test]
#[ignore = "calls the real Anthropic API"]
async fn live_chat_accepts_every_tool_schema_and_streams() {
    let model = adapter();
    let request = ChatRequest {
        purpose: ChatPurpose::Chat,
        system: "You are a test. Call no tools.".to_owned(),
        messages: user("In three short sentences, say how often to water a potted tomato."),
        tools: ToolRegistry::standard()
            .specs_for(agent::tools::Offer::everything(domain::wire::Role::Owner)),
        response_schema: None,
        max_tokens: 4_096,
    };
    let start = Instant::now();
    let mut stream = model.stream(&request);
    let mut text = String::new();
    let mut arrivals = Vec::new();
    let mut done = None;
    while let Some(chunk) = stream.next().await {
        match chunk.expect("chat call failed") {
            ChatChunk::TextDelta(t) => {
                arrivals.push(start.elapsed().as_millis());
                text.push_str(&t);
            }
            ChatChunk::Done(s) => done = Some(s),
            _ => {}
        }
    }
    eprintln!("chat: {done:?} {text:?}");
    eprintln!(
        "{} text deltas, first at {:?} ms, last at {:?} ms: {arrivals:?}",
        arrivals.len(),
        arrivals.first(),
        arrivals.last()
    );
    assert!(done.is_some(), "the stream ended without a stop reason");
    assert!(arrivals.len() > 1, "the answer arrived in one piece");
}

/// A plan-writer call with the plan schema as structured output.
#[tokio::test]
#[ignore = "calls the real Anthropic API"]
async fn live_plan_schema_is_accepted() {
    let model = adapter();
    let request = ChatRequest {
        purpose: ChatPurpose::PlanWriter,
        system: "You are a test. Write the smallest valid plan the schema allows.".to_owned(),
        messages: user("A tomato plant is slightly underwatered. Plan: water it."),
        tools: Vec::new(),
        response_schema: Some(PLAN_SCHEMA.clone()),
        max_tokens: 8_192,
    };
    let reply = model.complete(&request).await.expect("plan call failed");
    eprintln!("plan: {:?} {}", reply.stop, reply.text);
    serde_json::from_str::<serde_json::Value>(&reply.text).expect("the plan is not JSON");
}

/// The router's classifier as `api` wires it: the cheap model, 256 tokens.
#[tokio::test]
#[ignore = "calls the real Anthropic API"]
async fn live_classifier_fits_in_256_tokens() {
    let classifier = LlmClassifier::new(Arc::new(adapter()), 256);
    let scores = classifier
        .classify(
            ClassifierTask::Route {
                farm_records: false,
            },
            "Why are my tomato leaves turning yellow?",
        )
        .await
        .expect("classifier call failed");
    eprintln!("classifier: {scores:?}");
}

/// Gap G10 on the real classifier: in a garden with farm records, the evaluation's questions
/// about its Slack channel are on topic, and plainly off-topic messages stay off-topic with or
/// without farm records. Prints every score, for calibration.
#[tokio::test]
#[ignore = "calls the real Anthropic API"]
async fn live_router_keeps_farm_questions_on_topic_and_the_rest_off() {
    use agent::safety::{Calibration, RouteDecision, decide_route};

    let classifier = LlmClassifier::new(Arc::new(adapter()), 256);
    let calibration = Calibration::default();
    let on_topic = [
        "How many times did the crew report rings on bed 3 in #field-reports this season?",
        "When did someone see ladybirds on the bed 1 aphids?",
        "What did Riley post in #field-reports on August 31?",
        "When were target rings first reported on bed 3?",
        "How many garlic bulbs came out of bed 6?",
        "Why are my tomato leaves turning yellow?",
    ];
    let off_topic = [
        "Write me a poem about football",
        "What's the bitcoin price?",
        "Who won the World Cup in 2022?",
        "Can you help me write a cover letter for a job?",
    ];
    let mut wrong = Vec::new();
    for (text, want_on_topic) in on_topic
        .iter()
        .map(|t| (*t, true))
        .chain(off_topic.iter().map(|t| (*t, false)))
    {
        for farm_records in [true, false] {
            let scores = classifier
                .classify(ClassifierTask::Route { farm_records }, text)
                .await
                .expect("classifier call failed");
            let decision = decide_route(text, Some(&scores), &calibration, farm_records);
            eprintln!("farm_records={farm_records} {decision:?} {text}: {scores:?}");
            let off = decision == RouteDecision::OffTopic;
            // Without farm records only the plainly off-topic set is asserted: the farm
            // questions are routed as they were before G10.
            if (farm_records && off == want_on_topic) || (!want_on_topic && !off) {
                wrong.push(format!("farm_records={farm_records} {decision:?}: {text}"));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}
