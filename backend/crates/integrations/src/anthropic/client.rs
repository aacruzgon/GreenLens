//! The Messages client: configuration, model ids, `create` and `stream`.

use std::{sync::Arc, time::Duration};

use bytes::Bytes;
use domain::ports::ChatPurpose;
use futures::{StreamExt, TryStreamExt};
use reqwest::{StatusCode, header::HeaderMap};
use secrecy::{ExposeSecret, SecretString};

use super::{
    error::AnthropicError,
    request::{Body, MessagesRequest},
    response::{ErrorBody, Message},
    stream::{MessageStream, StreamEvent},
};
use crate::{
    http::{self, Attempt, RetryPolicy, TransportError},
    sse,
};

/// The API version header every request sends.
pub const API_VERSION: &str = "2023-06-01";

/// Model ids per caller. Ids are complete as written (no date suffix) and come from
/// configuration, so a model change is a deploy, not a code change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelIds {
    /// Chat turns: the strongest model.
    pub chat: String,
    /// The plan writer.
    pub plan_writer: String,
    /// Memory extraction: a cheap model.
    pub memory_extraction: String,
    /// Off-topic redirects: the cheapest model.
    pub off_topic: String,
    /// The fallback worker's vision calls.
    pub vision_fallback: String,
    /// The router and safety-gate classifier when Jev is not configured.
    pub classifier: String,
}

impl Default for ModelIds {
    fn default() -> Self {
        let strong = "claude-opus-5-5".to_owned();
        let cheap = "claude-haiku-5-5".to_owned();
        Self {
            chat: strong.clone(),
            plan_writer: strong.clone(),
            memory_extraction: cheap.clone(),
            off_topic: cheap.clone(),
            vision_fallback: strong,
            classifier: cheap,
        }
    }
}

impl ModelIds {
    /// The model for a domain purpose.
    pub fn for_purpose(&self, purpose: ChatPurpose) -> &str {
        match purpose {
            ChatPurpose::Chat => &self.chat,
            ChatPurpose::PlanWriter => &self.plan_writer,
            ChatPurpose::MemoryExtraction => &self.memory_extraction,
            ChatPurpose::OffTopic => &self.off_topic,
            ChatPurpose::VisionFallback => &self.vision_fallback,
            ChatPurpose::Classification => &self.classifier,
        }
    }
}

/// Client settings.
#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    /// `x-api-key`. Never logged: `SecretString`'s `Debug` prints a placeholder.
    pub api_key: SecretString,
    /// `https://api.anthropic.com`; a test server in tests.
    pub base_url: String,
    /// Model ids.
    pub models: ModelIds,
    /// Retries before the first response byte. The worker sets [`RetryPolicy::NONE`].
    pub retry: RetryPolicy,
    /// Total deadline of a non-streaming call. Streams have only the shared client's read gap.
    pub request_timeout: Duration,
    /// `anthropic-beta` values, empty for none.
    pub betas: Vec<String>,
}

impl AnthropicConfig {
    /// The defaults with this key: the public endpoint, the default models, standard retries,
    /// a 10-minute deadline for non-streaming calls.
    pub fn new(api_key: SecretString) -> Self {
        Self {
            api_key,
            base_url: "https://api.anthropic.com".to_owned(),
            models: ModelIds::default(),
            retry: RetryPolicy::STANDARD,
            request_timeout: Duration::from_mins(10),
            betas: Vec::new(),
        }
    }
}

/// The Messages API client. Cheap to clone (an `Arc` and the shared `reqwest::Client`), and
/// `Send + Sync + 'static`, so `agent` and the worker hold it by value.
#[derive(Debug, Clone)]
pub struct AnthropicClient {
    http: reqwest::Client,
    config: Arc<AnthropicConfig>,
}

impl AnthropicClient {
    /// A client over the shared HTTP client.
    pub fn new(http: reqwest::Client, config: AnthropicConfig) -> Self {
        Self {
            http,
            config: Arc::new(config),
        }
    }

    /// The configured model ids.
    pub fn models(&self) -> &ModelIds {
        &self.config.models
    }

    /// One non-streaming call. Retries per the configured policy.
    ///
    /// The request is serialised before the first `.await` into an owned body, so the borrow
    /// of `request` (and of everything it borrows) ends there in practice; the future still
    /// names both lifetimes because it holds the reference until it is first polled.
    pub async fn create(&self, request: &MessagesRequest<'_>) -> Result<Message, AnthropicError> {
        let body = encode(request, false)?;
        let response = self.send(body, false).await?;
        let bytes = response
            .bytes()
            .await
            .map_err(TransportError::from_reqwest)?;
        serde_json::from_slice(&bytes).map_err(|e| AnthropicError::Decode(Box::new(e)))
    }

    /// One streaming call. Retries per the policy until the API answers 200; from then on
    /// errors arrive in the stream and are not retried (the caller may restart the call if it
    /// has sent nothing to the user yet).
    ///
    /// The returned [`MessageStream`] is `'static`: it owns the response and borrows neither
    /// `self` nor `request`. That is the point of serialising first: the borrowed request
    /// lives only as long as this `async fn`'s first poll, while the stream can be moved into a
    /// spawned task or an SSE body. `ChatModel::stream<'a>` in `agent` can still return a
    /// `BoxStream<'a, _>`, since a `'static` stream outlives any `'a`.
    pub async fn stream(
        &self,
        request: &MessagesRequest<'_>,
    ) -> Result<MessageStream, AnthropicError> {
        let body = encode(request, true)?;
        let response = self.send(body, true).await?;
        let request_id = header(response.headers(), "request-id");
        let events = sse::events(
            response
                .bytes_stream()
                .map_err(TransportError::from_reqwest),
        )
        .map_err(AnthropicError::from)
        .and_then(|ev| async move { StreamEvent::from_sse(&ev) })
        .try_filter(|ev| {
            std::future::ready(!matches!(ev, StreamEvent::Ping | StreamEvent::Unknown))
        })
        .map(|item| match item {
            Ok(StreamEvent::Error { error }) => Err(AnthropicError::Stream {
                kind: error.kind,
                message: error.message,
            }),
            other => other,
        })
        .scan(false, |failed, item| {
            // An error is the last item: stop after yielding it.
            if *failed {
                return std::future::ready(None);
            }
            *failed = item.is_err();
            std::future::ready(Some(item))
        });
        Ok(MessageStream::new(events.boxed(), request_id))
    }

    /// POSTs the body, retrying statuses and transport errors the policy allows. `Bytes`
    /// clones are reference counts, so each attempt re-sends the same buffer.
    async fn send(
        &self,
        body: Bytes,
        streaming: bool,
    ) -> Result<reqwest::Response, AnthropicError> {
        let url = format!("{}/v1/messages", self.config.base_url.trim_end_matches('/'));
        http::retry(self.config.retry, |_| {
            let mut builder = self
                .http
                .post(&url)
                .header("x-api-key", self.config.api_key.expose_secret())
                .header("anthropic-version", API_VERSION)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.clone());
            if !self.config.betas.is_empty() {
                builder = builder.header("anthropic-beta", self.config.betas.join(","));
            }
            if streaming {
                builder = builder.header(reqwest::header::ACCEPT, "text/event-stream");
            } else {
                builder = builder.timeout(self.config.request_timeout);
            }
            async move {
                let response = match builder.send().await {
                    Ok(r) => r,
                    Err(e) => {
                        let error = AnthropicError::from(TransportError::from_reqwest(e));
                        return Attempt::Retry {
                            error,
                            retry_after: None,
                        };
                    }
                };
                if response.status().is_success() {
                    return Attempt::Done(response);
                }
                let error = api_error(response).await;
                if error.retryable() {
                    let retry_after = error.retry_after();
                    Attempt::Retry { error, retry_after }
                } else {
                    Attempt::Fail(error)
                }
            }
        })
        .await
    }
}

fn encode(request: &MessagesRequest<'_>, stream: bool) -> Result<Bytes, AnthropicError> {
    serde_json::to_vec(&Body { request, stream })
        .map(Bytes::from)
        .map_err(|e| AnthropicError::Encode(Box::new(e)))
}

fn header(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get(name)?.to_str().ok().map(str::to_owned)
}

/// Reads an error response into [`AnthropicError`], recognising the spend cap.
async fn api_error(response: reqwest::Response) -> AnthropicError {
    let status = response.status();
    let retry_after = http::retry_after(response.headers());
    let request_id = header(response.headers(), "request-id");
    let text = response.text().await.unwrap_or_default();
    let parsed: Option<ErrorBody> = serde_json::from_str(&text).ok();
    let spend_cap = status == StatusCode::TOO_MANY_REQUESTS
        && parsed
            .as_ref()
            .and_then(|b| b.error.details.as_ref())
            .and_then(|d| d.get("error_code"))
            .and_then(serde_json::Value::as_str)
            == Some("enforced_spend_limit_reached");
    if spend_cap {
        return AnthropicError::SpendCap { request_id };
    }
    let (kind, message, body_id) = match parsed {
        Some(b) => (b.error.kind, b.error.message, b.request_id),
        None => (String::from("unknown"), http::snippet(&text, 512), None),
    };
    AnthropicError::Api {
        status: status.as_u16(),
        kind,
        message,
        request_id: request_id.or(body_id),
        retry_after,
    }
}
