//! The text classifier's LLM fallback (architecture 10: "LLM API, cheapest model, with
//! structured output"), used when Jev is not configured. One call per message: the model scores
//! every label of the task from 0 to 1 in a fixed JSON object. Scores are only compared with each
//! other and calibrated afterwards ([`crate::safety::Calibration`]).

use std::sync::Arc;

use domain::ports::{
    ChatMessage, ChatModel, ChatPurpose, ChatRequest, ChatRole, ClassifierTask, ContentPart,
    LabelScore, ModelError, StopReason, TextClassifier,
};
use serde_json::{Map, Value, json};

use crate::{
    prompt::{DataKind, classifier_system, data},
    safety::routes::{FARM_ROUTE_LABELS, ROUTE_LABELS},
};

/// The memory gate's labels and the chunk topics, shared with Jev.
pub use domain::ports::{MEMORY_LABELS, TOPIC_LABELS};

/// What the model is told, and the labels, per task.
pub const fn task_labels(task: ClassifierTask) -> (&'static str, &'static [&'static str]) {
    match task {
        ClassifierTask::Route {
            farm_records: false,
        } => (
            "Classify a gardener's chat message. diagnosis_question: about what is wrong with a \
             plant. care_question: about looking after plants. task_change: asks to add, finish \
             or change tasks or the plan. safety_sensitive: about pesticides, doses, or a person \
             or pet being poisoned. off_topic: not about plants or the garden.",
            &ROUTE_LABELS,
        ),
        // Gap G10: in a garden with farm records, a question about them is not off-topic.
        ClassifierTask::Route { farm_records: true } => (
            "Classify a gardener's chat message. The garden keeps farm records: its crew's \
             messages in the farm's Slack channels and its shared documents and logs. \
             diagnosis_question: about what is wrong with a plant. care_question: about looking \
             after plants. task_change: asks to add, finish or change tasks or the plan. \
             safety_sensitive: about pesticides, doses, or a person or pet being poisoned, \
             also when it asks what the farm records say about them. \
             farm_records: asks what the farm records say: what someone in the crew saw, \
             reported, did or posted, in a channel or a document, or when or how often. \
             off_topic: not about plants, the garden or its farm records.",
            &FARM_ROUTE_LABELS,
        ),
        ClassifierTask::MemoryWorthy => (
            "Decide whether this exchange contains a fact the gardener stated that will still \
             matter in later conversations (something done to the plant, or something true about \
             the plant or garden). remember: it does. skip: it does not.",
            &MEMORY_LABELS,
        ),
        ClassifierTask::ChunkTopic => (
            "Classify a passage from a gardening guide by its topic.",
            &TOPIC_LABELS,
        ),
    }
}

/// The strict output schema: one number per label, all required.
pub fn score_schema(labels: &[&str]) -> Value {
    let properties: Map<String, Value> = labels
        .iter()
        .map(|l| ((*l).to_owned(), json!({ "type": "number" })))
        .collect();
    json!({
        "type": "object",
        "properties": properties,
        "required": labels,
        "additionalProperties": false,
    })
}

/// The LLM fallback classifier.
pub struct LlmClassifier {
    model: Arc<dyn ChatModel>,
    max_tokens: u32,
}

impl std::fmt::Debug for LlmClassifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmClassifier")
            .field("model", &self.model.model_name(ChatPurpose::Classification))
            .finish_non_exhaustive()
    }
}

impl LlmClassifier {
    /// Over the configured `ChatModel`; `max_tokens` bounds the answer (thinking included).
    pub fn new(model: Arc<dyn ChatModel>, max_tokens: u32) -> Self {
        Self { model, max_tokens }
    }

    /// The request for one message.
    pub fn request(&self, task: ClassifierTask, text: &str) -> ChatRequest {
        let (description, labels) = task_labels(task);
        ChatRequest {
            purpose: ChatPurpose::Classification,
            system: classifier_system(description, labels),
            messages: vec![ChatMessage {
                role: ChatRole::User,
                content: vec![ContentPart::Text(data(DataKind::UserMessage, text))],
            }],
            tools: Vec::new(),
            response_schema: Some(score_schema(labels)),
            max_tokens: self.max_tokens,
        }
    }
}

/// Reads the model's JSON into scores, highest first. Unknown keys are ignored, missing labels
/// score 0, and scores are clamped to 0..=1 (a non-number counts as 0).
pub fn parse_scores(text: &str, labels: &[&str]) -> Option<Vec<LabelScore>> {
    let value: Value = serde_json::from_str(text.trim()).ok()?;
    let object = value.as_object()?;
    let mut scores: Vec<LabelScore> = labels
        .iter()
        .map(|l| LabelScore {
            label: (*l).to_owned(),
            score: object
                .get(*l)
                .and_then(Value::as_f64)
                .filter(|s| s.is_finite())
                .map_or(0.0, |s| s.clamp(0.0, 1.0)),
        })
        .collect();
    scores.sort_by(|a, b| b.score.total_cmp(&a.score));
    Some(scores)
}

/// What a refused classification counts as: the safe label scores 1.
fn refused(task: ClassifierTask, labels: &[&str]) -> Vec<LabelScore> {
    let safe = match task {
        ClassifierTask::Route { .. } => "safety_sensitive",
        ClassifierTask::MemoryWorthy => "skip",
        ClassifierTask::ChunkTopic => "management",
    };
    let mut scores: Vec<LabelScore> = labels
        .iter()
        .map(|l| LabelScore {
            label: (*l).to_owned(),
            score: if *l == safe { 1.0 } else { 0.0 },
        })
        .collect();
    scores.sort_by(|a, b| b.score.total_cmp(&a.score));
    scores
}

#[async_trait::async_trait]
impl TextClassifier for LlmClassifier {
    async fn classify(
        &self,
        task: ClassifierTask,
        text: &str,
    ) -> Result<Vec<LabelScore>, ModelError> {
        let (_, labels) = task_labels(task);
        let request = self.request(task, text);
        let completion = self.model.complete(&request).await?;
        if completion.stop == StopReason::Refusal {
            return Ok(refused(task, labels));
        }
        parse_scores(&completion.text, labels).ok_or_else(|| ModelError::InvalidOutput {
            model: "anthropic/classification",
            source: "classifier output is not a JSON object of scores".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_are_parsed_clamped_and_sorted() {
        let s = parse_scores(
            r#"{"remember": 0.2, "skip": 1.7, "extra": 9}"#,
            &MEMORY_LABELS,
        )
        .unwrap();
        assert_eq!(s[0].label, "skip");
        assert!((s[0].score - 1.0).abs() < f64::EPSILON);
        assert!(parse_scores("not json", &MEMORY_LABELS).is_none());
        let missing = parse_scores("{}", &MEMORY_LABELS).unwrap();
        assert!(missing.iter().all(|l| l.score == 0.0));
    }

    #[test]
    fn the_schema_is_strict() {
        let s = score_schema(&ROUTE_LABELS);
        assert_eq!(s["additionalProperties"], false);
        assert_eq!(s["required"].as_array().unwrap().len(), 5);
    }

    /// Gap G10: a garden without farm records is asked exactly as before; one with them gets
    /// the `farm_records` label, and `off_topic` no longer covers its records.
    #[test]
    fn the_route_task_adds_the_farm_label_only_with_farm_records() {
        let (without, labels) = task_labels(ClassifierTask::Route {
            farm_records: false,
        });
        assert_eq!(labels, ROUTE_LABELS);
        assert!(!without.contains("farm_records"));
        assert!(without.ends_with("off_topic: not about plants or the garden."));

        let (with, labels) = task_labels(ClassifierTask::Route { farm_records: true });
        assert_eq!(labels, FARM_ROUTE_LABELS);
        assert!(with.contains("Slack channels"));
        assert!(with.contains("farm_records: asks what the farm records say"));
        assert!(with.ends_with("off_topic: not about plants, the garden or its farm records."));
        let schema = score_schema(labels);
        assert_eq!(schema["required"].as_array().unwrap().len(), 6);
        assert_eq!(
            crate::prompt::CLASSIFIER_PROMPT_VERSION,
            "classify/2026-10-20.2"
        );

        let c = LlmClassifier::new(
            Arc::new(crate::testing::ScriptedModel::completing(
                "{}",
                StopReason::EndTurn,
            )),
            256,
        );
        let system = c
            .request(ClassifierTask::Route { farm_records: true }, "hi")
            .system;
        assert!(system.contains(
            "Labels: diagnosis_question care_question task_change safety_sensitive \
             farm_records off_topic.\n\n"
        ));
    }

    #[tokio::test]
    async fn a_refusal_routes_as_safety_sensitive() {
        for farm_records in [false, true] {
            let model = Arc::new(crate::testing::ScriptedModel::completing(
                "",
                StopReason::Refusal,
            ));
            let s = LlmClassifier::new(model, 256)
                .classify(ClassifierTask::Route { farm_records }, "how much poison")
                .await
                .unwrap();
            assert_eq!(s[0].label, "safety_sensitive");
        }
    }
}
