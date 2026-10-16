//! `POST /v1/chat/completions` as served by vLLM 0.31 and llama.cpp's `llama-server`: the VLM
//! sidecar of the `vlm` workers ([contracts 6]). Non-streaming: a findings call is at most
//! 1,500 tokens and the worker validates the whole answer.
//!
//! Like [`crate::anthropic`], the request borrows (`'a` = the worker's prompt text, schema and
//! image bytes) and the response is owned. Images become `data:` URLs while serialising, so the
//! base64 text is written once, straight into the body.

use std::{borrow::Cow, fmt, sync::Arc, time::Duration};

use bytes::Bytes;
use domain::ports::{BoxError, ModelError};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

use crate::http::{self, Attempt, RetryPolicy, TransportError};

/// Which server answers. They accept the same body except for two fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerFlavour {
    /// vLLM: `max_completion_tokens` (`max_tokens` is deprecated in 0.31) and
    /// `repetition_penalty`.
    Vllm,
    /// llama.cpp: `max_tokens` and `repeat_penalty`.
    LlamaCpp,
}

/// Client settings.
#[derive(Debug, Clone)]
pub struct OpenAiCompatConfig {
    /// `http://vlm:8000`, without `/v1`.
    pub base_url: String,
    /// `Authorization: Bearer`, when the server was started with `--api-key`.
    pub api_key: Option<SecretString>,
    /// Total deadline of one call.
    pub timeout: Duration,
    /// The server.
    pub flavour: ServerFlavour,
    /// Retries of transport errors and 5xx. The worker sets [`RetryPolicy::NONE`]: it owns the
    /// job deadline and its one validation retry.
    pub retry: RetryPolicy,
}

/// Sampling parameters ([contracts 6]; `docs/vision` model decisions).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
pub struct Sampling {
    /// 0 for the VLM.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    /// 1; sent as `repetition_penalty` (vLLM) or `repeat_penalty` (llama.cpp).
    #[serde(skip)]
    pub repetition_penalty: Option<f32>,
    /// A fixed seed, for reproducible evaluation runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}

impl Sampling {
    /// Greedy decoding: temperature 0, `top_p` 1, no penalties. The VLM's setting.
    pub const DETERMINISTIC: Self = Self {
        temperature: Some(0.0),
        top_p: Some(1.0),
        presence_penalty: Some(0.0),
        repetition_penalty: Some(1.0),
        seed: None,
    };
}

/// One chat-completions request.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatCompletionRequest<'a> {
    /// The served model name (`--served-model-name`).
    pub model: Cow<'a, str>,
    /// System, then user.
    pub messages: Vec<ChatMessage<'a>>,
    /// Output budget: 1,500 for findings and indicators, 100 for guided checks.
    pub max_output_tokens: u32,
    /// Sampling.
    pub sampling: Sampling,
    /// Structured output.
    pub response_format: Option<ResponseFormat<'a>>,
}

/// A message's author.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    /// The system prompt.
    System,
    /// The user turn.
    User,
    /// A previous answer.
    Assistant,
}

/// One message.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChatMessage<'a> {
    /// Its author.
    pub role: ChatRole,
    /// Its content.
    pub content: ChatContent<'a>,
}

impl<'a> ChatMessage<'a> {
    /// A system message.
    pub fn system(text: impl Into<Cow<'a, str>>) -> Self {
        Self {
            role: ChatRole::System,
            content: ChatContent::Text(text.into()),
        }
    }

    /// A user message of parts.
    pub fn user(parts: Vec<ContentPart<'a>>) -> Self {
        Self {
            role: ChatRole::User,
            content: ChatContent::Parts(parts),
        }
    }
}

/// `content`: a string or parts.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ChatContent<'a> {
    /// Text.
    Text(Cow<'a, str>),
    /// Text and images.
    Parts(Vec<ContentPart<'a>>),
}

/// One part of a user message.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart<'a> {
    /// Text.
    Text {
        /// The text.
        text: Cow<'a, str>,
    },
    /// An image as a `data:` URL.
    ImageUrl {
        /// The URL object.
        image_url: ImageUrl<'a>,
    },
}

impl<'a> ContentPart<'a> {
    /// A text part.
    pub fn text(text: impl Into<Cow<'a, str>>) -> Self {
        Self::Text { text: text.into() }
    }

    /// An image part over borrowed bytes, sent as `data:{media_type};base64,…`.
    pub fn image(media_type: &'a str, bytes: &'a [u8]) -> Self {
        Self::ImageUrl {
            image_url: ImageUrl {
                url: DataUrl { media_type, bytes },
            },
        }
    }
}

/// `image_url`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ImageUrl<'a> {
    /// The `data:` URL.
    pub url: DataUrl<'a>,
}

/// Borrowed image bytes that serialise as a `data:` URL without building the string first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataUrl<'a> {
    /// `image/webp`.
    pub media_type: &'a str,
    /// The encoded image.
    pub bytes: &'a [u8],
}

impl fmt::Display for DataUrl<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "data:{};base64,{}",
            self.media_type,
            data_encoding::BASE64.encode_display(self.bytes)
        )
    }
}

impl Serialize for DataUrl<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// `response_format`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat<'a> {
    /// Constrained to a schema (xgrammar on vLLM, a GBNF grammar on llama.cpp).
    JsonSchema {
        /// The schema.
        json_schema: JsonSchemaSpec<'a>,
    },
}

impl<'a> ResponseFormat<'a> {
    /// A strict schema format.
    pub fn strict(name: impl Into<Cow<'a, str>>, schema: Cow<'a, Value>) -> Self {
        Self::JsonSchema {
            json_schema: JsonSchemaSpec {
                name: name.into(),
                schema,
                strict: true,
            },
        }
    }
}

/// `response_format.json_schema`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JsonSchemaSpec<'a> {
    /// `findings`, `indicators`, `guided`.
    pub name: Cow<'a, str>,
    /// The schema, with its length and integer bounds: xgrammar enforces them.
    pub schema: Cow<'a, Value>,
    /// Always true for the VLM.
    pub strict: bool,
}

/// The body actually sent: the request with the flavour's field names. `'r` borrows the
/// request for serialising; `'a` is what the request borrows.
#[derive(Serialize)]
struct Body<'r, 'a> {
    model: &'r str,
    messages: &'r [ChatMessage<'a>],
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(flatten)]
    sampling: &'r Sampling,
    #[serde(skip_serializing_if = "Option::is_none")]
    repetition_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repeat_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<&'r ResponseFormat<'a>>,
    stream: bool,
}

impl<'r, 'a> Body<'r, 'a> {
    fn new(request: &'r ChatCompletionRequest<'a>, flavour: ServerFlavour) -> Self {
        let vllm = flavour == ServerFlavour::Vllm;
        let budget = request.max_output_tokens;
        let penalty = request.sampling.repetition_penalty;
        Self {
            model: &request.model,
            messages: &request.messages,
            max_completion_tokens: vllm.then_some(budget),
            max_tokens: (!vllm).then_some(budget),
            sampling: &request.sampling,
            repetition_penalty: if vllm { penalty } else { None },
            repeat_penalty: if vllm { None } else { penalty },
            response_format: request.response_format.as_ref(),
            stream: false,
        }
    }
}

/// The response.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ChatCompletion {
    /// `chatcmpl-…`.
    pub id: String,
    /// The served model.
    pub model: String,
    /// One choice (we never ask for `n` > 1).
    pub choices: Vec<Choice>,
    /// Token counts.
    #[serde(default)]
    pub usage: Option<CompletionUsage>,
}

impl ChatCompletion {
    /// The first choice's text and finish reason. `None` when the server sent no choice.
    pub fn first(&self) -> Option<(&str, FinishReason)> {
        let choice = self.choices.first()?;
        Some((
            choice.message.content.as_deref().unwrap_or(""),
            choice.finish_reason.unwrap_or(FinishReason::Other),
        ))
    }
}

/// One choice.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Choice {
    /// 0.
    pub index: u32,
    /// The answer.
    pub message: AssistantMessage,
    /// Why it stopped. `Length` means the output was cut: the worker treats it as a validation
    /// failure ([contracts 6]).
    pub finish_reason: Option<FinishReason>,
}

/// `choices[].message`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AssistantMessage {
    /// The text: the JSON, with structured output.
    #[serde(default)]
    pub content: Option<String>,
    /// A reasoning model's thinking (`reasoning_content`), when the server separates it.
    #[serde(default)]
    pub reasoning_content: Option<String>,
}

/// `finish_reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// Natural end or a stop string.
    Stop,
    /// `max_completion_tokens` reached.
    Length,
    /// A tool call (unused).
    ToolCalls,
    /// A content filter (unused by our servers).
    ContentFilter,
    /// Anything else, including a missing reason.
    #[serde(other)]
    Other,
}

/// `usage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub struct CompletionUsage {
    /// Prompt tokens, images included.
    #[serde(default)]
    pub prompt_tokens: u32,
    /// Output tokens.
    #[serde(default)]
    pub completion_tokens: u32,
}

/// Why a call failed.
#[derive(Debug, thiserror::Error)]
pub enum OpenAiCompatError {
    /// No response.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// An error status, with the start of the body.
    #[error("VLM server answered {status}: {body}")]
    Status {
        /// The HTTP status.
        status: u16,
        /// At most 512 bytes of the body.
        body: String,
        /// `Retry-After`.
        retry_after: Option<Duration>,
    },
    /// The body didn't parse.
    #[error("VLM server response did not parse")]
    Decode(#[source] BoxError),
    /// The request didn't serialise.
    #[error("request did not serialise")]
    Encode(#[source] BoxError),
}

impl OpenAiCompatError {
    /// Transport failures, 408, 429 and 5xx (a restarting server answers 503).
    pub fn retryable(&self) -> bool {
        match self {
            Self::Transport(_) => true,
            Self::Status { status, .. } => matches!(status, 408 | 429 | 500..=599),
            Self::Decode(_) | Self::Encode(_) => false,
        }
    }

    /// The server's `Retry-After`.
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::Status { retry_after, .. } => *retry_after,
            _ => None,
        }
    }

    /// The domain error: retryable failures are [`ModelError::Failed`] (the job is retried
    /// with `retryable: true`), a bad body is [`ModelError::InvalidOutput`], a 4xx (our request
    /// was refused: a schema the grammar compiler rejects, too many images) is also `Failed`.
    pub fn into_model_error(self, model: &'static str) -> ModelError {
        match self {
            Self::Decode(_) => ModelError::InvalidOutput {
                model,
                source: Box::new(self),
            },
            other => ModelError::Failed {
                model,
                source: Box::new(other),
            },
        }
    }
}

/// The client. Cheap to clone.
#[derive(Debug, Clone)]
pub struct OpenAiCompatClient {
    http: reqwest::Client,
    config: Arc<OpenAiCompatConfig>,
}

impl OpenAiCompatClient {
    /// A client over the shared HTTP client.
    pub fn new(http: reqwest::Client, config: OpenAiCompatConfig) -> Self {
        Self {
            http,
            config: Arc::new(config),
        }
    }

    /// The server flavour.
    pub fn flavour(&self) -> ServerFlavour {
        self.config.flavour
    }

    /// One chat completion. The request is serialised before the first `.await`; the response
    /// is owned.
    pub async fn chat(
        &self,
        request: &ChatCompletionRequest<'_>,
    ) -> Result<ChatCompletion, OpenAiCompatError> {
        let body = serde_json::to_vec(&Body::new(request, self.config.flavour))
            .map(Bytes::from)
            .map_err(|e| OpenAiCompatError::Encode(Box::new(e)))?;
        let url = self.url("/v1/chat/completions");
        let bytes = http::retry(self.config.retry, |_| {
            let builder = self
                .authorised(self.http.post(&url))
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.clone());
            async move { attempt(builder).await }
        })
        .await?;
        serde_json::from_slice(&bytes).map_err(|e| OpenAiCompatError::Decode(Box::new(e)))
    }

    /// `GET /v1/models`: the served names, for the start-up check against configuration.
    pub async fn models(&self) -> Result<Vec<String>, OpenAiCompatError> {
        #[derive(Deserialize)]
        struct List {
            data: Vec<Entry>,
        }
        #[derive(Deserialize)]
        struct Entry {
            id: String,
        }
        let url = self.url("/v1/models");
        let builder = self.authorised(self.http.get(&url));
        let bytes = match attempt(builder).await {
            Attempt::Done(b) => b,
            Attempt::Retry { error, .. } | Attempt::Fail(error) => return Err(error),
        };
        let list: List =
            serde_json::from_slice(&bytes).map_err(|e| OpenAiCompatError::Decode(Box::new(e)))?;
        Ok(list.data.into_iter().map(|e| e.id).collect())
    }

    /// `GET /health`: 200 once the model is loaded (both servers).
    pub async fn health(&self) -> Result<(), OpenAiCompatError> {
        let builder = self.authorised(self.http.get(self.url("/health")));
        match attempt(builder).await {
            Attempt::Done(_) => Ok(()),
            Attempt::Retry { error, .. } | Attempt::Fail(error) => Err(error),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.config.base_url.trim_end_matches('/'))
    }

    fn authorised(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let builder = builder.timeout(self.config.timeout);
        match &self.config.api_key {
            Some(key) => builder.bearer_auth(key.expose_secret()),
            None => builder,
        }
    }
}

async fn attempt(builder: reqwest::RequestBuilder) -> Attempt<Bytes, OpenAiCompatError> {
    let response = match builder.send().await {
        Ok(r) => r,
        Err(e) => {
            return Attempt::Retry {
                error: TransportError::from_reqwest(e).into(),
                retry_after: None,
            };
        }
    };
    let status = response.status();
    let retry_after = http::retry_after(response.headers());
    let bytes = match response.bytes().await {
        Ok(b) => b,
        Err(e) => {
            return Attempt::Retry {
                error: TransportError::from_reqwest(e).into(),
                retry_after: None,
            };
        }
    };
    if status.is_success() {
        return Attempt::Done(bytes);
    }
    let error = OpenAiCompatError::Status {
        status: status.as_u16(),
        body: http::snippet(&String::from_utf8_lossy(&bytes), 512),
        retry_after,
    };
    if http::retryable_status(status) {
        Attempt::Retry { error, retry_after }
    } else {
        Attempt::Fail(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_partial_json, method, path},
    };

    fn request<'a>(schema: &'a Value, image: &'a [u8]) -> ChatCompletionRequest<'a> {
        ChatCompletionRequest {
            model: "qwen3-vl-8b".into(),
            messages: vec![
                ChatMessage::system("You assess plants."),
                ChatMessage::user(vec![
                    ContentPart::image("image/webp", image),
                    ContentPart::text("Image 1: whole plant."),
                ]),
            ],
            max_output_tokens: 1500,
            sampling: Sampling::DETERMINISTIC,
            response_format: Some(ResponseFormat::strict("findings", Cow::Borrowed(schema))),
        }
    }

    #[test]
    fn body_per_flavour() {
        let schema = json!({"type": "object"});
        let req = request(&schema, b"hi");
        let vllm = serde_json::to_value(Body::new(&req, ServerFlavour::Vllm)).unwrap();
        assert_eq!(vllm["max_completion_tokens"], 1500);
        assert_eq!(vllm["repetition_penalty"], 1.0);
        assert!(vllm.get("max_tokens").is_none());
        assert_eq!(
            vllm["messages"][1]["content"][0]["image_url"]["url"],
            "data:image/webp;base64,aGk="
        );
        assert_eq!(vllm["response_format"]["json_schema"]["strict"], true);
        assert_eq!(vllm["temperature"], 0.0);
        let llama = serde_json::to_value(Body::new(&req, ServerFlavour::LlamaCpp)).unwrap();
        assert_eq!(llama["max_tokens"], 1500);
        assert_eq!(llama["repeat_penalty"], 1.0);
        assert!(llama.get("max_completion_tokens").is_none());
    }

    #[tokio::test]
    async fn chat_reports_length() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_partial_json(json!({"model": "qwen3-vl-8b"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl-1", "object": "chat.completion", "model": "qwen3-vl-8b",
                "choices": [{"index": 0, "message": {"role": "assistant", "content": "{\"a\":"},
                             "finish_reason": "length"}],
                "usage": {"prompt_tokens": 1200, "completion_tokens": 1500, "total_tokens": 2700}
            })))
            .mount(&server)
            .await;
        let client = OpenAiCompatClient::new(
            reqwest::Client::new(),
            OpenAiCompatConfig {
                base_url: server.uri(),
                api_key: None,
                timeout: Duration::from_secs(5),
                flavour: ServerFlavour::Vllm,
                retry: RetryPolicy::NONE,
            },
        );
        let schema = json!({"type": "object"});
        let out = client.chat(&request(&schema, b"x")).await.unwrap();
        assert_eq!(out.first(), Some(("{\"a\":", FinishReason::Length)));
    }

    #[tokio::test]
    async fn status_errors_are_classified() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503).set_body_string("loading"))
            .expect(1)
            .mount(&server)
            .await;
        let client = OpenAiCompatClient::new(
            reqwest::Client::new(),
            OpenAiCompatConfig {
                base_url: server.uri(),
                api_key: None,
                timeout: Duration::from_secs(5),
                flavour: ServerFlavour::LlamaCpp,
                retry: RetryPolicy::NONE,
            },
        );
        let schema = json!({});
        let err = client.chat(&request(&schema, b"x")).await.unwrap_err();
        assert!(err.retryable());
        assert!(matches!(err, OpenAiCompatError::Status { status: 503, .. }));
    }
}
