//! Jev, `TypeSafe`'s System One model, as the fast [`TextClassifier`] (architecture 7, router and
//! safety gate; architecture 10).
//!
//! One `POST {base_url}/v1/systemone` per message, with the text as `state` and one question
//! per label (`TypeSafe`'s API reference, read and run on 2026-10-23):
//!
//! - **Route:** one `noul` (yes/no) question per route label, answered with the probability of
//!   yes. The labels are judged independently, as `agent`'s LLM classifier scores them, so a
//!   dose question about the farm records can score high on both `farm_records` and
//!   `safety_sensitive`, rather than split one probability between them; `agent` compares
//!   that score with `FARM_SAFETY_FLOOR` after its calibration offset.
//! - **Memory gate:** one `noul` for `remember`; `skip` is its complement.
//! - **Chunk topic:** one `choice` over the five topics, scored by its probabilities.
//!
//! The model is pinned (`JEV_MODEL`, default [`DEFAULT_MODEL`]) rather than `jev-latest`, an
//! alias that moves with each release: the router's calibration was fitted on one version.
//! `api` wires this adapter only when `JEV_*` is configured; otherwise it wires `agent`'s LLM
//! classifier.

use std::{borrow::Cow, collections::BTreeMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use domain::ports::{ClassifierTask, LabelScore, ModelError, PriorExchange, TextClassifier};
use reqwest::StatusCode;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::http::{TransportError, retry_after};

/// The model name in errors and logs.
pub const MODEL: &str = "jev";

/// The Jev version the router's calibration was fitted on (2026-10-23).
pub const DEFAULT_MODEL: &str = "jev-1.14.0";

/// The default deadline of one call, connect to last byte: within the router's
/// `classifier_timeout` (3 s), so a slow call fails here and is logged as Jev's.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

/// The chat router's labels ([contracts 8]), without and with farm records (gap G10), the
/// memory gate's and the chunk topics; shared with `agent`'s classifier.
pub use domain::ports::{FARM_ROUTE_LABELS, MEMORY_LABELS, ROUTE_LABELS, TOPIC_LABELS};

/// The name of the chunk-topic `choice` question.
const TOPIC_QUESTION: &str = "topic";

/// Settings.
#[derive(Debug, Clone)]
pub struct JevConfig {
    /// `JEV_URL`, such as `https://api.typesafe.ai`.
    pub base_url: String,
    /// `JEV_API_KEY`, sent as a bearer token.
    pub api_key: SecretString,
    /// `JEV_MODEL`: a versioned model id, [`DEFAULT_MODEL`] unless set.
    pub model: String,
    /// Deadline of one call: [`DEFAULT_TIMEOUT`].
    pub timeout: Duration,
}

impl JevConfig {
    /// The pinned model and the default deadline.
    pub fn new(base_url: String, api_key: SecretString) -> Self {
        Self {
            base_url,
            api_key,
            model: DEFAULT_MODEL.to_owned(),
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

/// The Jev client.
#[derive(Debug, Clone)]
pub struct JevClassifier {
    http: reqwest::Client,
    config: Arc<JevConfig>,
}

/// `SystemOneRequest`. `'a` borrows the configured model, the turn's text and its context.
#[derive(Debug, Serialize)]
struct SystemOneRequest<'a> {
    model: &'a str,
    state: State<'a>,
    questions: BTreeMap<&'static str, Question>,
}

/// The `state`: the text alone, or for a route with context, an object that names the message
/// to judge and the exchange before it.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum State<'a> {
    Text(&'a str),
    WithContext {
        earlier: Earlier<'a>,
        message: &'a str,
    },
}

/// The exchange before the message, under `earlier`.
#[derive(Debug, Serialize)]
struct Earlier<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    gardener: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assistant: Option<&'a str>,
}

/// Put before each route question when the state carries the earlier exchange. It goes in
/// every question because the request has no instructions shared by all questions, and
/// `TypeSafe`'s guidance keeps the judgment in the questions and the content in the state; it
/// is kept short (about 20 tokens a question).
const CONTEXT_PREFIX: &str = "Judge the gardener's newest `message`; `earlier` is the exchange \
                              before it, for context only. ";

/// A question, tagged by `type`.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Question {
    /// Yes or no, answered with the probability of yes.
    Noul { instructions: Cow<'static, str> },
    /// One of `criteria`, answered with a probability for each.
    Choice {
        instructions: &'static str,
        criteria: BTreeMap<&'static str, &'static str>,
    },
}

/// `SystemOneResponse`: the answers by question name, the version that answered and the usage.
#[derive(Debug, Deserialize)]
struct SystemOneResponse {
    model: String,
    answers: BTreeMap<String, Answer>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Answer {
    Noul {
        noul: f64,
    },
    Choice {
        probabilities: BTreeMap<String, f64>,
    },
    /// `score` and `box`, never asked.
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct Usage {
    input_tokens: u64,
}

/// A refusal status, with the server's `retry-after` on `429` and `529`. Never carries the
/// body: a `422` echoes the request, which holds the user's text.
#[derive(Debug, thiserror::Error)]
#[error("HTTP {status}{}", .retry_after.map(|d| format!(", retry after {} s", d.as_secs())).unwrap_or_default())]
struct StatusError {
    status: StatusCode,
    retry_after: Option<Duration>,
}

/// The route labels' questions. Each asks whether the message is that kind, as `agent`'s LLM
/// prompt describes the label.
fn route_questions(farm_records: bool) -> BTreeMap<&'static str, Question> {
    let noul = |instructions: &'static str| Question::Noul {
        instructions: Cow::Borrowed(instructions),
    };
    let mut q = BTreeMap::from([
        (
            "diagnosis_question",
            noul(
                "Does this gardener's message ask what is wrong with a plant: a symptom, \
                 damage, a pest or a disease it wants explained?",
            ),
        ),
        (
            "care_question",
            noul(
                "Does this gardener's message ask how to look after plants or the garden, \
                 such as watering, feeding, pruning, planting, soil, timing or harvest?",
            ),
        ),
        (
            "task_change",
            noul(
                "Does this message ask to add, finish, move, remove or change a garden task or \
                 the care plan?",
            ),
        ),
        (
            "safety_sensitive",
            noul(if farm_records {
                "Is this message about pesticides, fungicides, herbicides or other garden \
                 chemicals, their doses, rates or mixing, or about a person or pet being \
                 poisoned, also when it asks what the farm's records say about them?"
            } else {
                "Is this message about pesticides, fungicides, herbicides or other garden \
                 chemicals, their doses, rates or mixing, or about a person or pet being \
                 poisoned?"
            }),
        ),
        (
            "off_topic",
            noul(if farm_records {
                "Is this message about something other than plants, gardening, the garden or \
                 the farm's own records?"
            } else {
                "Is this message about something other than plants, gardening or the garden?"
            }),
        ),
    ]);
    if farm_records {
        q.insert(
            domain::ports::FARM_RECORDS_LABEL,
            noul(
                "Does this message ask what the farm's own records say: what someone in the \
                 crew saw, reported, did or posted in the farm's Slack channels or its shared \
                 documents and logs, or when or how often?",
            ),
        );
    }
    q
}

/// The route questions for a state with the earlier exchange: each says which part to judge.
fn with_context(questions: BTreeMap<&'static str, Question>) -> BTreeMap<&'static str, Question> {
    questions
        .into_iter()
        .map(|(name, q)| match q {
            Question::Noul { instructions } => (
                name,
                Question::Noul {
                    instructions: Cow::Owned(format!("{CONTEXT_PREFIX}{instructions}")),
                },
            ),
            other @ Question::Choice { .. } => (name, other),
        })
        .collect()
}

/// The questions of a task.
fn questions(task: ClassifierTask) -> BTreeMap<&'static str, Question> {
    match task {
        ClassifierTask::Route { farm_records } => route_questions(farm_records),
        ClassifierTask::MemoryWorthy => BTreeMap::from([(
            MEMORY_LABELS[0],
            Question::Noul {
                instructions: Cow::Borrowed(
                    "Does this exchange contain a fact the gardener stated that will still \
                     matter in later conversations: something done to a plant, or something \
                     true about a plant or the garden?",
                ),
            },
        )]),
        ClassifierTask::ChunkTopic => BTreeMap::from([(
            TOPIC_QUESTION,
            Question::Choice {
                instructions: "What is this passage from a gardening guide mostly about?",
                criteria: BTreeMap::from([
                    ("symptoms", "what the problem looks like on the plant"),
                    (
                        "conditions",
                        "the weather, soil or conditions that cause or favour it",
                    ),
                    ("management", "how to treat, control or prevent it"),
                    ("care", "general care of the plant"),
                    ("recognition", "how to recognise the plant or organism"),
                ]),
            },
        )]),
    }
}

/// A probability as a score: finite and within 0..=1, or 0.
fn score(p: f64) -> f64 {
    if p.is_finite() {
        p.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// The answers as per-label scores, highest first. Only the labels asked count, and every one
/// of them must be answered with the type asked: `None` otherwise, which the caller reports as
/// `InvalidOutput` and the router routes as a care question. Scoring a missing label 0 could
/// let a lone `off_topic` answer win and skip the safety route.
fn scores(task: ClassifierTask, answers: &BTreeMap<String, Answer>) -> Option<Vec<LabelScore>> {
    let noul = |label: &str| match answers.get(label) {
        Some(Answer::Noul { noul }) => Some(score(*noul)),
        _ => None,
    };
    let mut out: Vec<LabelScore> = match task {
        ClassifierTask::Route { farm_records } => {
            let labels: &[&str] = if farm_records {
                &FARM_ROUTE_LABELS
            } else {
                &ROUTE_LABELS
            };
            labels
                .iter()
                .map(|l| {
                    noul(l).map(|score| LabelScore {
                        label: (*l).to_owned(),
                        score,
                    })
                })
                .collect::<Option<_>>()?
        }
        ClassifierTask::MemoryWorthy => {
            let [remember, skip] = MEMORY_LABELS;
            let p = noul(remember)?;
            vec![
                LabelScore {
                    label: remember.to_owned(),
                    score: p,
                },
                LabelScore {
                    label: skip.to_owned(),
                    score: 1.0 - p,
                },
            ]
        }
        ClassifierTask::ChunkTopic => {
            let Some(Answer::Choice { probabilities }) = answers.get(TOPIC_QUESTION) else {
                return None;
            };
            TOPIC_LABELS
                .iter()
                .map(|l| {
                    probabilities.get(*l).map(|p| LabelScore {
                        label: (*l).to_owned(),
                        score: score(*p),
                    })
                })
                .collect::<Option<_>>()?
        }
    };
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    Some(out)
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
        self.classify_with_context(task, text, &PriorExchange::default())
            .await
    }

    /// For a route with an earlier exchange, the state is `{"earlier": {"gardener", "assistant"},
    /// "message"}` and each question says to judge `message`; otherwise the text alone, as
    /// `classify` sends it. The other tasks ignore the context.
    async fn classify_with_context(
        &self,
        task: ClassifierTask,
        text: &str,
        prior: &PriorExchange,
    ) -> Result<Vec<LabelScore>, ModelError> {
        let prior =
            Some(prior).filter(|p| matches!(task, ClassifierTask::Route { .. }) && !p.is_empty());
        let (state, questions) = match prior {
            Some(p) => (
                State::WithContext {
                    earlier: Earlier {
                        gardener: p.user.as_deref(),
                        assistant: p.assistant.as_deref(),
                    },
                    message: text,
                },
                with_context(questions(task)),
            ),
            None => (State::Text(text), questions(task)),
        };
        let unavailable = |source| ModelError::Unavailable {
            model: MODEL,
            source,
        };
        let invalid = |source| ModelError::InvalidOutput {
            model: MODEL,
            source,
        };
        let response = self
            .http
            .post(format!(
                "{}/v1/systemone",
                self.config.base_url.trim_end_matches('/')
            ))
            .bearer_auth(self.config.api_key.expose_secret())
            .timeout(self.config.timeout)
            .json(&SystemOneRequest {
                model: &self.config.model,
                state,
                questions,
            })
            .send()
            .await
            .map_err(|e| unavailable(Box::new(TransportError::from_reqwest(e))))?;
        let status = response.status();
        if !status.is_success() {
            // No retry: a chat turn can't wait out a rate limit, and the router already
            // routes a failed classification as a care question.
            return Err(unavailable(Box::new(StatusError {
                status,
                retry_after: retry_after(response.headers()),
            })));
        }
        // The deadline can run out while the body is read: that is the network, not the output.
        let body: SystemOneResponse = response.json().await.map_err(|e| {
            let timeout = e.is_timeout();
            let e = Box::new(TransportError::from_reqwest(e));
            if timeout { unavailable(e) } else { invalid(e) }
        })?;
        tracing::debug!(
            model = MODEL,
            version = %body.model,
            input_tokens = body.usage.as_ref().map(|u| u.input_tokens),
            "jev answered"
        );
        if body.model != self.config.model {
            // The calibration was fitted on the pinned version; another one may route differently.
            tracing::warn!(
                model = MODEL,
                asked = %self.config.model,
                answered = %body.model,
                "jev answered with another model version than JEV_MODEL"
            );
        }
        scores(task, &body.answers)
            .ok_or_else(|| invalid("no answer to the questions asked".into()))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, header, method, path},
    };

    use super::*;

    fn jev(server: &MockServer) -> JevClassifier {
        JevClassifier::new(
            reqwest::Client::new(),
            JevConfig::new(format!("{}/", server.uri()), SecretString::from("k")),
        )
    }

    fn nouls(pairs: &[(&str, f64)]) -> Value {
        let answers: serde_json::Map<String, Value> = pairs
            .iter()
            .map(|(l, p)| ((*l).to_owned(), json!({"type": "noul", "noul": p})))
            .collect();
        json!({
            "model": DEFAULT_MODEL,
            "answers": answers,
            "usage": {"input_tokens": 480, "output_tokens": 120},
        })
    }

    async fn sent_body(server: &MockServer) -> Value {
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        serde_json::from_slice(&requests[0].body).unwrap()
    }

    const ROUTE: ClassifierTask = ClassifierTask::Route {
        farm_records: false,
    };

    #[tokio::test]
    async fn the_route_asks_one_noul_per_label_on_the_pinned_model() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .and(header("authorization", "Bearer k"))
            .and(header("content-type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(nouls(&[
                ("diagnosis_question", 0.91),
                ("care_question", 0.4),
                ("task_change", 0.02),
                ("safety_sensitive", 0.05),
                ("off_topic", 0.03),
            ])))
            .expect(1)
            .mount(&server)
            .await;
        let s = jev(&server)
            .classify(ROUTE, "why are my leaves yellow?")
            .await
            .unwrap();
        let labels: Vec<&str> = s.iter().map(|l| l.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "diagnosis_question",
                "care_question",
                "safety_sensitive",
                "off_topic",
                "task_change"
            ]
        );
        assert!((s[0].score - 0.91).abs() < f64::EPSILON);

        let body = sent_body(&server).await;
        assert_eq!(body["model"], "jev-1.14.0");
        assert_eq!(body["state"], "why are my leaves yellow?");
        let questions = body["questions"].as_object().unwrap();
        let mut asked: Vec<&str> = questions.keys().map(String::as_str).collect();
        asked.sort_unstable();
        let mut want = ROUTE_LABELS.to_vec();
        want.sort_unstable();
        assert_eq!(asked, want);
        for q in questions.values() {
            assert_eq!(q["type"], "noul");
            assert!(q["instructions"].as_str().unwrap().ends_with('?'));
            assert_eq!(q.as_object().unwrap().len(), 2);
        }
        assert!(
            !questions["off_topic"]["instructions"]
                .as_str()
                .unwrap()
                .contains("records")
        );
    }

    #[tokio::test]
    async fn a_garden_with_farm_records_is_asked_the_farm_label_too() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(nouls(&[
                ("farm_records", 0.97),
                ("safety_sensitive", 0.1),
                ("off_topic", 0.2),
                ("care_question", 0.0),
                ("diagnosis_question", 0.0),
                ("task_change", 0.0),
            ])))
            .expect(1)
            .mount(&server)
            .await;
        let s = jev(&server)
            .classify(
                ClassifierTask::Route { farm_records: true },
                "What did Riley post in #field-reports?",
            )
            .await
            .unwrap();
        assert_eq!(s[0].label, "farm_records");
        assert_eq!(s.len(), FARM_ROUTE_LABELS.len());
        let body = sent_body(&server).await;
        let questions = &body["questions"];
        assert_eq!(questions.as_object().unwrap().len(), 6);
        assert!(
            questions["farm_records"]["instructions"]
                .as_str()
                .unwrap()
                .contains("Slack")
        );
        assert!(
            questions["off_topic"]["instructions"]
                .as_str()
                .unwrap()
                .contains("records")
        );
    }

    #[tokio::test]
    async fn the_memory_gate_is_one_noul_and_skip_its_complement() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .and(body_json(json!({
                "model": DEFAULT_MODEL,
                "state": "Gardener: I repotted it on Monday",
                "questions": {"remember": {
                    "type": "noul",
                    "instructions": "Does this exchange contain a fact the gardener stated that \
                                     will still matter in later conversations: something done \
                                     to a plant, or something true about a plant or the garden?",
                }},
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(nouls(&[("remember", 0.8)])))
            .expect(1)
            .mount(&server)
            .await;
        let s = jev(&server)
            .classify(
                ClassifierTask::MemoryWorthy,
                "Gardener: I repotted it on Monday",
            )
            .await
            .unwrap();
        assert_eq!(s[0].label, "remember");
        assert!((s[0].score - 0.8).abs() < 1e-9);
        assert_eq!(s[1].label, "skip");
        assert!((s[1].score - 0.2).abs() < 1e-9);
    }

    #[tokio::test]
    async fn the_chunk_topic_is_one_choice_scored_by_its_probabilities() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "model": DEFAULT_MODEL,
                "answers": {"topic": {
                    "type": "choice",
                    "choice": "management",
                    "confidence": 0.7,
                    "probabilities": {"symptoms": 0.1, "conditions": 0.0, "management": 0.8,
                                      "care": 0.1, "recognition": 0.0},
                }},
                "usage": {"input_tokens": 400, "output_tokens": 10},
            })))
            .mount(&server)
            .await;
        let s = jev(&server)
            .classify(ClassifierTask::ChunkTopic, "Remove infected leaves and…")
            .await
            .unwrap();
        assert_eq!(s.len(), TOPIC_LABELS.len());
        assert_eq!(s[0].label, "management");
        let body = sent_body(&server).await;
        let q = &body["questions"]["topic"];
        assert_eq!(q["type"], "choice");
        let mut criteria: Vec<&str> = q["criteria"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        criteria.sort_unstable();
        let mut want = TOPIC_LABELS.to_vec();
        want.sort_unstable();
        assert_eq!(criteria, want);
    }

    #[tokio::test]
    async fn unasked_answers_are_dropped_and_probabilities_clamped() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(nouls(&[
                ("farm_records", 0.99),
                ("care_question", 1.4),
                ("diagnosis_question", -0.2),
                ("task_change", 0.1),
                ("safety_sensitive", 0.1),
                ("off_topic", 0.1),
            ])))
            .mount(&server)
            .await;
        let s = jev(&server).classify(ROUTE, "any notes?").await.unwrap();
        assert!(s.iter().all(|l| l.label != "farm_records"));
        assert_eq!(s[0].label, "care_question");
        assert!((s[0].score - 1.0).abs() < f64::EPSILON);
        assert_eq!(s.last().unwrap().score, 0.0);
    }

    /// A partial answer must not route: scoring the missing labels 0 would send a poisoning
    /// message off-topic on a lone `off_topic` answer, skipping the safety route.
    #[tokio::test]
    async fn a_partial_or_mistyped_answer_is_invalid_output() {
        let mut full: Vec<(&str, f64)> = ROUTE_LABELS.iter().map(|l| (*l, 0.1)).collect();
        let score_typed = {
            let mut body = nouls(&full);
            body["answers"]["safety_sensitive"] = json!({"type": "score", "score": 1.0,
                "confidence": 1.0, "legend": {}, "probabilities": {}});
            body
        };
        full.retain(|(l, _)| *l == "off_topic");
        for body in [nouls(&full), score_typed] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .mount(&server)
                .await;
            let e = jev(&server)
                .classify(ROUTE, "My dog ate some slug pellets")
                .await
                .unwrap_err();
            assert!(
                matches!(e, ModelError::InvalidOutput { model: "jev", .. }),
                "{e:?}"
            );
        }
        // A topic choice missing a topic is invalid too.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "model": DEFAULT_MODEL,
                "answers": {"topic": {"type": "choice", "choice": "care", "confidence": 0.9,
                                      "probabilities": {"care": 0.9}}},
                "usage": {"input_tokens": 1, "output_tokens": 1},
            })))
            .mount(&server)
            .await;
        let e = jev(&server)
            .classify(ClassifierTask::ChunkTopic, "Water deeply.")
            .await
            .unwrap_err();
        assert!(matches!(e, ModelError::InvalidOutput { model: "jev", .. }));
    }

    #[tokio::test]
    async fn no_answer_to_the_questions_is_invalid_output() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(nouls(&[("other", 0.5)])))
            .mount(&server)
            .await;
        let e = jev(&server).classify(ROUTE, "hi").await.unwrap_err();
        assert!(matches!(e, ModelError::InvalidOutput { model: "jev", .. }));

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;
        let e = jev(&server)
            .classify(ClassifierTask::MemoryWorthy, "hi")
            .await
            .unwrap_err();
        assert!(matches!(e, ModelError::InvalidOutput { model: "jev", .. }));
    }

    #[tokio::test]
    async fn refusals_are_unavailable_with_the_status_and_retry_after() {
        for (status, retry, want) in [
            (
                429,
                Some("1"),
                "HTTP 429 Too Many Requests, retry after 1 s",
            ),
            (529, None, "HTTP 529 <unknown status code>"),
            (401, None, "HTTP 401 Unauthorized"),
            (422, None, "HTTP 422 Unprocessable Entity"),
        ] {
            let server = MockServer::start().await;
            let mut response = ResponseTemplate::new(status)
                .set_body_json(json!({"detail": [{"input": "the user's text"}]}));
            if let Some(r) = retry {
                response = response.insert_header("retry-after", r);
            }
            Mock::given(method("POST"))
                .respond_with(response)
                // No retry inside the router's deadline.
                .expect(1)
                .mount(&server)
                .await;
            let e = jev(&server).classify(ROUTE, "hi").await.unwrap_err();
            let ModelError::Unavailable { model, source } = &e else {
                panic!("{e:?}");
            };
            assert_eq!(*model, "jev");
            assert_eq!(source.to_string(), want);
            assert!(!format!("{e:?}").contains("the user's text"));
        }
    }

    #[tokio::test]
    async fn a_slow_answer_times_out_as_unavailable() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(nouls(&[("care_question", 0.9)]))
                    .set_delay(Duration::from_millis(500)),
            )
            .mount(&server)
            .await;
        let mut config = JevConfig::new(server.uri(), SecretString::from("k"));
        config.timeout = Duration::from_millis(100);
        let e = JevClassifier::new(reqwest::Client::new(), config)
            .classify(ROUTE, "hi")
            .await
            .unwrap_err();
        let ModelError::Unavailable { source, .. } = &e else {
            panic!("{e:?}");
        };
        let transport = source.downcast_ref::<TransportError>().unwrap();
        assert!(transport.timeout);
    }

    /// Headers on time, then a body that stalls past the deadline: `Unavailable`, like any
    /// timeout, not `InvalidOutput`.
    #[tokio::test]
    async fn a_body_that_stalls_past_the_deadline_is_unavailable() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 4096];
            let _ = socket.read(&mut request).await.unwrap();
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                      content-length: 200\r\n\r\n{\"model\":",
                )
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_secs(2)).await;
        });
        let mut config = JevConfig::new(format!("http://{addr}"), SecretString::from("k"));
        config.timeout = Duration::from_millis(200);
        let e = JevClassifier::new(reqwest::Client::new(), config)
            .classify(ROUTE, "hi")
            .await
            .unwrap_err();
        let ModelError::Unavailable { source, .. } = &e else {
            panic!("{e:?}");
        };
        assert!(source.downcast_ref::<TransportError>().unwrap().timeout);
        server.abort();
    }

    /// A follow-up's route names the message to judge and the exchange before it, and each
    /// question says to judge the message; an empty exchange and the other tasks send the text.
    #[tokio::test]
    async fn a_route_with_context_sends_the_earlier_exchange_and_marks_the_message() {
        let all: Vec<(&str, f64)> = ROUTE_LABELS.iter().map(|l| (*l, 0.1)).collect();
        let prior = PriorExchange {
            user: Some("Will the frost hurt my tomatoes?".to_owned()),
            assistant: Some("Frost is likely on Thursday night.".to_owned()),
        };
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(nouls(&all)))
            .mount(&server)
            .await;
        jev(&server)
            .classify_with_context(ROUTE, "What did you just tell me?", &prior)
            .await
            .unwrap();
        let body = sent_body(&server).await;
        assert_eq!(
            body["state"],
            json!({
                "earlier": {
                    "gardener": "Will the frost hurt my tomatoes?",
                    "assistant": "Frost is likely on Thursday night.",
                },
                "message": "What did you just tell me?",
            })
        );
        for q in body["questions"].as_object().unwrap().values() {
            let instructions = q["instructions"].as_str().unwrap();
            assert!(instructions.starts_with(CONTEXT_PREFIX), "{instructions}");
            assert!(instructions.ends_with('?'));
        }

        // Only the assistant half: no `gardener` key.
        let reply_only = PriorExchange {
            user: None,
            assistant: prior.assistant.clone(),
        };
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(nouls(&all)))
            .mount(&server)
            .await;
        jev(&server)
            .classify_with_context(ROUTE, "And the second one?", &reply_only)
            .await
            .unwrap();
        let body = sent_body(&server).await;
        assert_eq!(
            body["state"]["earlier"],
            json!({"assistant": "Frost is likely on Thursday night."})
        );

        // No exchange, or another task: the text alone, as `classify` sends it.
        for (task, answer) in [
            (ROUTE, nouls(&all)),
            (ClassifierTask::MemoryWorthy, nouls(&[("remember", 0.5)])),
        ] {
            for p in [PriorExchange::default(), prior.clone()] {
                if task == ROUTE && !p.is_empty() {
                    continue;
                }
                let server = MockServer::start().await;
                Mock::given(method("POST"))
                    .respond_with(ResponseTemplate::new(200).set_body_json(answer.clone()))
                    .mount(&server)
                    .await;
                jev(&server)
                    .classify_with_context(task, "hello", &p)
                    .await
                    .unwrap();
                let body = sent_body(&server).await;
                assert_eq!(body["state"], "hello");
                for q in body["questions"].as_object().unwrap().values() {
                    assert!(!q["instructions"].as_str().unwrap().contains("`earlier`"));
                }
            }
        }
    }

    #[test]
    fn the_default_deadline_fits_the_routers() {
        assert!(DEFAULT_TIMEOUT < Duration::from_secs(3));
        assert!(DEFAULT_MODEL.starts_with("jev-") && DEFAULT_MODEL != "jev-latest");
    }
}
