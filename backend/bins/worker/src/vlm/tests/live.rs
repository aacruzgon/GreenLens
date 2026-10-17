//! The hosted fallback's schemas against the real Anthropic API, ignored by default. No images
//! are sent: these check only that the API accepts each schema and answers inside it. The
//! indicators call asks 27 questions by default, the most a call can ask (9 candidates × 3
//! indicators); `LIVE_QUESTIONS` changes the count. One schema that large is refused, so this
//! also checks the backend's split into parts.
//!
//! Run with the key in `infra/dev/.env` (gitignored), from `backend/`:
//! `set -a; source ../infra/dev/.env; set +a; cargo nextest run -p worker --run-ignored only live_`
#![expect(
    clippy::print_stderr,
    reason = "a live test prints what the API answered, for whoever runs it by hand"
)]

use domain::{
    jobs::{Indicator, IndicatorWhere, OrganismOption, SpeciesOption},
    scan::ImageKind,
    vlm::BoxFormat,
};
use integrations::anthropic::{AnthropicClient, AnthropicConfig};
use serde_json::Value;

use crate::vlm::{
    backend::{AnthropicBackend, Stop, VlmBackend, VlmCall},
    schema::{self, BoxRange, Limits},
};

fn backend() -> AnthropicBackend {
    let key = std::env::var("LLM_API_KEY").expect("LLM_API_KEY is not set");
    let http = integrations::http::client(&integrations::http::HttpConfig::default())
        .expect("HTTP client");
    let config = AnthropicConfig::new(key.into());
    let model = config.models.vision_fallback.clone();
    AnthropicBackend::new(AnthropicClient::new(http, config), model)
}

async fn ask(name: &str, schema: &Value, instructions: &str) -> Value {
    let call = VlmCall {
        system: "You are a test. No images are attached; answer inside the schema anyway.",
        images: Vec::new(),
        instructions,
        schema_name: name,
        schema,
        max_output_tokens: 8_000,
    };
    let reply = backend().call(&call).await.expect("the call failed");
    eprintln!("{name}: {:?} {}", reply.stop, reply.text);
    assert_eq!(reply.stop, Stop::Complete);
    serde_json::from_str::<Value>(&reply.text).expect("the answer is not JSON")
}

#[tokio::test]
#[ignore = "calls the real Anthropic API"]
async fn live_indicators_worst_case() {
    let n: usize = std::env::var("LIVE_QUESTIONS").map_or(27, |v| v.parse().unwrap());
    let indicators: Vec<Indicator> = (1..=n)
        .map(|i| Indicator {
            indicator_id: format!("tomato_early_blight.sign_{i}").parse().unwrap(),
            question: format!("Is sign {i} visible?"),
            r#where: IndicatorWhere::LowerLeaves,
            image_kinds: vec![ImageKind::LeafCloseup],
        })
        .collect();
    let kinds = [
        ImageKind::WholePlant,
        ImageKind::LeafCloseup,
        ImageKind::LeafCloseup,
    ];
    let questions = schema::questions(&indicators, &kinds);
    let schema = schema::indicators(kinds.len(), &questions, Limits::CONTRACT);
    let answer = ask(
        "indicators",
        &schema,
        "Answer every question with unsure and no image.",
    )
    .await;
    assert_eq!(answer.as_object().map(serde_json::Map::len), Some(n));
}

#[tokio::test]
#[ignore = "calls the real Anthropic API"]
async fn live_findings() {
    let species = [SpeciesOption {
        id: "tomato".parse().unwrap(),
        name: "Tomato".to_owned(),
    }];
    let organisms = [OrganismOption {
        id: "aphids".parse().unwrap(),
        name: "Aphids".to_owned(),
    }];
    let schema = schema::findings(
        3,
        &species,
        &organisms,
        BoxRange::for_format(BoxFormat::AbsPxPrepared, 1_024),
        Limits::CONTRACT,
    );
    let _ = ask(
        "findings",
        &schema,
        "Report no organisms and no damage; guess tomato with low certainty.",
    )
    .await;
}
