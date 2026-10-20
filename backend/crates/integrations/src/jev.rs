//! Jev, the fast text classifier: a [`TextClassifier`] stub behind configuration
//! (architecture 7, router and safety gate).
//!
//! Jev is in early access and its API is not public, so the request and response shapes here
//! are placeholders (*unverified*): one `POST {base_url}/v1/classify` with the task, the text and
//! the candidate labels, answered with scores. `api` wires this adapter only when `JEV_*` is
//! configured; otherwise it wires `agent`'s LLM classifier, which is the real fallback.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use domain::ports::{ClassifierTask, LabelScore, ModelError, TextClassifier};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::http::TransportError;

/// The model name in errors and logs.
pub const MODEL: &str = "jev";

/// The chat router's labels ([contracts 8]), without and with farm records (gap G10); shared
/// with `agent`'s classifier.
pub use domain::ports::{FARM_ROUTE_LABELS, ROUTE_LABELS};

/// The memory gate's labels.
pub const MEMORY_LABELS: &[&str] = &["memory_worthy", "not_memory_worthy"];

/// Settings.
#[derive(Debug, Clone)]
pub struct JevConfig {
    /// The early-access endpoint.
    pub base_url: String,
    /// The API key.
    pub api_key: SecretString,
    /// Deadline: the router is on every chat turn's critical path, so 2 s.
    pub timeout: Duration,
}

/// The Jev client.
#[derive(Debug, Clone)]
pub struct JevClassifier {
    http: reqwest::Client,
    config: Arc<JevConfig>,
}

/// The placeholder request. `'a` borrows the turn's text and the static labels.
#[derive(Debug, Serialize)]
struct ClassifyRequest<'a> {
    task: &'static str,
    text: &'a str,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    labels: &'a [&'a str],
}

#[derive(Debug, Deserialize)]
struct ClassifyResponse {
    scores: Vec<Score>,
}

#[derive(Debug, Deserialize)]
struct Score {
    label: String,
    score: f64,
}

impl JevClassifier {
    /// A client over the shared HTTP client.
    pub fn new(http: reqwest::Client, config: JevConfig) -> Self {
        Self {
            http,
            config: Arc::new(config),
        }
    }
}

#[async_trait]
impl TextClassifier for JevClassifier {
    async fn classify(
        &self,
        task: ClassifierTask,
        text: &str,
    ) -> Result<Vec<LabelScore>, ModelError> {
        let (name, labels): (&'static str, &[&str]) = match task {
            ClassifierTask::Route {
                farm_records: false,
            } => ("route", &ROUTE_LABELS),
            ClassifierTask::Route { farm_records: true } => ("route", &FARM_ROUTE_LABELS),
            ClassifierTask::MemoryWorthy => ("memory_worthy", MEMORY_LABELS),
            ClassifierTask::ChunkTopic => ("chunk_topic", &[]),
        };
        let unavailable = |source| ModelError::Unavailable {
            model: MODEL,
            source,
        };
        let response = self
            .http
            .post(format!(
                "{}/v1/classify",
                self.config.base_url.trim_end_matches('/')
            ))
            .bearer_auth(self.config.api_key.expose_secret())
            .timeout(self.config.timeout)
            .json(&ClassifyRequest {
                task: name,
                text,
                labels,
            })
            .send()
            .await
            .map_err(|e| unavailable(Box::new(TransportError::from_reqwest(e))))?;
        let status = response.status();
        if !status.is_success() {
            return Err(unavailable(format!("HTTP {status}").into()));
        }
        let body: ClassifyResponse =
            response
                .json()
                .await
                .map_err(|e| ModelError::InvalidOutput {
                    model: MODEL,
                    source: Box::new(TransportError::from_reqwest(e)),
                })?;
        // Only the labels asked: a label the router wasn't offered must not count.
        let mut scores: Vec<LabelScore> = body
            .scores
            .into_iter()
            .filter(|s| labels.contains(&s.label.as_str()))
            .map(|s| LabelScore {
                label: s.label,
                score: s.score,
            })
            .collect();
        scores.sort_by(|a, b| b.score.total_cmp(&a.score));
        Ok(scores)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_partial_json, method, path},
    };

    use super::*;

    #[tokio::test]
    async fn scores_come_back_highest_first() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/classify"))
            .and(body_partial_json(
                json!({"task": "route", "text": "why are my leaves yellow?"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"scores": [
                {"label": "care_question", "score": 0.2},
                {"label": "diagnosis_question", "score": 0.7}
            ]})))
            .mount(&server)
            .await;
        let jev = JevClassifier::new(
            reqwest::Client::new(),
            JevConfig {
                base_url: server.uri(),
                api_key: SecretString::from("k"),
                timeout: Duration::from_secs(2),
            },
        );
        let s = jev
            .classify(
                ClassifierTask::Route {
                    farm_records: false,
                },
                "why are my leaves yellow?",
            )
            .await
            .unwrap();
        assert_eq!(s[0].label, "diagnosis_question");
    }

    #[tokio::test]
    async fn a_garden_with_farm_records_is_asked_the_farm_label_too() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/classify"))
            .and(body_partial_json(json!({
                "task": "route",
                "labels": FARM_ROUTE_LABELS,
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"scores": [
                {"label": "farm_records", "score": 0.8}
            ]})))
            .expect(1)
            .mount(&server)
            .await;
        let jev = JevClassifier::new(
            reqwest::Client::new(),
            JevConfig {
                base_url: server.uri(),
                api_key: SecretString::from("k"),
                timeout: Duration::from_secs(2),
            },
        );
        let s = jev
            .classify(
                ClassifierTask::Route { farm_records: true },
                "What did Riley post in #field-reports?",
            )
            .await
            .unwrap();
        assert_eq!(s[0].label, "farm_records");
    }
}
