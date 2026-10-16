//! The two VLM backends behind one small trait: an OpenAI-compatible server (vLLM, llama.cpp)
//! and the hosted LLM API. Both are thin layers over the `integrations` clients.

use std::{borrow::Cow, future::Future};

use domain::ports::ModelError;
use integrations::{
    anthropic::{
        self, AnthropicClient, ContentBlock, Effort, InputMessage, MessagesRequest, OutputConfig,
        OutputFormat, Role, System,
    },
    openai_compat::{
        ChatCompletionRequest, ChatMessage, ContentPart, FinishReason, OpenAiCompatClient,
        ResponseFormat, Sampling,
    },
};
use serde_json::Value;

/// One image of a call: its label line (`img1: whole plant`) and its JPEG bytes. `'a` borrows
/// the prepared image the adapter keeps for the whole job.
#[derive(Debug, Clone, Copy)]
pub struct VlmImage<'a> {
    /// `img1: whole plant`.
    pub label: &'a str,
    /// JPEG.
    pub jpeg: &'a [u8],
}

/// One structured-output call. Everything is borrowed for `'a`: the prompts and schema the
/// adapter built for this call and the images it prepared for the job; the backend serialises
/// the request once and keeps none of it.
#[derive(Debug, Clone)]
pub struct VlmCall<'a> {
    /// The system prompt.
    pub system: &'a str,
    /// The images, first in the user message.
    pub images: Vec<VlmImage<'a>>,
    /// The call's instructions, after the images.
    pub instructions: &'a str,
    /// The schema's name (`findings`, `indicators`, `guided`).
    pub schema_name: &'a str,
    /// The per-call JSON Schema, with its limits.
    pub schema: &'a Value,
    /// The output budget.
    pub max_output_tokens: u32,
}

/// Why the model stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// A complete answer.
    Complete,
    /// The token limit: a validation failure, never a truncated success.
    Length,
    /// A safety refusal (HTTP 200 with `stop_reason: refusal`): a validation failure.
    Refusal,
}

/// The answer's text (JSON) and how it ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VlmReply {
    /// The JSON text.
    pub text: String,
    /// Why it stopped.
    pub stop: Stop,
}

/// A VLM backend. The adapter is generic over it (`VlmAdapter<B>`), so the trait is never a
/// `dyn` and can return `impl Future`. The named lifetime `'a` ties the future to both `&self`
/// and the borrowed call; `+ Send` is spelled out so the adapter's own `#[async_trait]` future,
/// which awaits this one, stays `Send`.
pub trait VlmBackend: Send + Sync + 'static {
    /// The model name in `ModelError`s.
    fn name(&self) -> &'static str;
    /// One call.
    fn call<'a>(
        &'a self,
        call: &'a VlmCall<'a>,
    ) -> impl Future<Output = Result<VlmReply, ModelError>> + Send + 'a;
}

/// vLLM or llama.cpp through `integrations::openai_compat`.
#[derive(Debug, Clone)]
pub struct OpenAiBackend {
    client: OpenAiCompatClient,
    served_name: String,
}

impl OpenAiBackend {
    /// A backend calling `served_name` (`vlm`).
    pub const fn new(client: OpenAiCompatClient, served_name: String) -> Self {
        Self {
            client,
            served_name,
        }
    }
}

impl VlmBackend for OpenAiBackend {
    fn name(&self) -> &'static str {
        "vlm"
    }

    async fn call<'a>(&'a self, call: &'a VlmCall<'a>) -> Result<VlmReply, ModelError> {
        let mut parts = Vec::with_capacity(call.images.len() * 2 + 1);
        for image in &call.images {
            parts.push(ContentPart::text(image.label));
            parts.push(ContentPart::image("image/jpeg", image.jpeg));
        }
        parts.push(ContentPart::text(call.instructions));
        let request = ChatCompletionRequest {
            model: Cow::Borrowed(self.served_name.as_str()),
            messages: vec![ChatMessage::system(call.system), ChatMessage::user(parts)],
            max_output_tokens: call.max_output_tokens,
            sampling: Sampling {
                seed: Some(0),
                ..Sampling::DETERMINISTIC
            },
            response_format: Some(ResponseFormat::strict(
                call.schema_name,
                Cow::Borrowed(call.schema),
            )),
        };
        let completion = self
            .client
            .chat(&request)
            .await
            .map_err(|e| e.into_model_error("vlm"))?;
        let (text, finish) = completion
            .first()
            .ok_or_else(|| ModelError::InvalidOutput {
                model: "vlm",
                source: "no choice in the answer".into(),
            })?;
        Ok(VlmReply {
            text: text.to_owned(),
            stop: match finish {
                FinishReason::Length => Stop::Length,
                FinishReason::ContentFilter => Stop::Refusal,
                FinishReason::Stop | FinishReason::ToolCalls | FinishReason::Other => {
                    Stop::Complete
                }
            },
        })
    }
}

/// The hosted LLM through `integrations::anthropic`. The schema is sanitised first: the API
/// rejects length, item-count and numeric-bound keywords with a 400 and needs
/// `additionalProperties: false` on every object (contracts 6); the adapter's decoding truncates
/// and clamps afterwards for every backend.
#[derive(Debug, Clone)]
pub struct AnthropicBackend {
    client: AnthropicClient,
    model: String,
}

impl AnthropicBackend {
    /// A backend calling `model` (`claude-opus-5-5`).
    pub const fn new(client: AnthropicClient, model: String) -> Self {
        Self { client, model }
    }
}

impl VlmBackend for AnthropicBackend {
    fn name(&self) -> &'static str {
        "llm_fallback"
    }

    async fn call<'a>(&'a self, call: &'a VlmCall<'a>) -> Result<VlmReply, ModelError> {
        let sanitized = anthropic::sanitize_schema(call.schema);
        let mut content = Vec::with_capacity(call.images.len() * 2 + 1);
        for image in &call.images {
            content.push(ContentBlock::text(image.label));
            content.push(ContentBlock::image("image/jpeg", image.jpeg));
        }
        content.push(ContentBlock::text(call.instructions));
        let mut request = MessagesRequest::new(
            self.model.as_str(),
            call.max_output_tokens,
            vec![InputMessage {
                role: Role::User,
                content,
            }],
        );
        request.system = Some(System::Text(Cow::Borrowed(call.system)));
        request.output_config = Some(OutputConfig {
            effort: Some(Effort::Low),
            format: Some(OutputFormat::JsonSchema {
                schema: sanitized.schema,
            }),
        });
        let message = self
            .client
            .create(&request)
            .await
            .map_err(|e| e.into_model_error("llm_fallback"))?;
        Ok(VlmReply {
            text: message.text(),
            stop: match message.stop_reason {
                Some(anthropic::StopReason::MaxTokens) => Stop::Length,
                Some(anthropic::StopReason::Refusal) => Stop::Refusal,
                _ => Stop::Complete,
            },
        })
    }
}
