//! `ChatModel` over the Anthropic Messages API (`integrations::anthropic`): the strongest model
//! for chat and the plan writer, the cheapest for extraction, summaries, off-topic redirects and
//! classification (architecture 10), chosen per [`ChatPurpose`] by the client's `ModelIds`.
//!
//! The request is built **borrowing** the domain `ChatRequest` (`MessagesRequest<'a>` holds
//! `Cow::Borrowed` text, tool schemas and image bytes), serialised once by the client, and
//! dropped. Responses are owned. Thinking is left at the models' default (adaptive, always on for
//! Opus 5.5); depth is set with `effort` per purpose. Forced `tool_choice` is never sent (a 400
//! on Opus 5.5); structured output uses `output_config.format`.

use std::borrow::Cow;

use domain::ports::{
    ChatChunk, ChatCompletion, ChatMessage, ChatModel, ChatPurpose, ChatRequest, ChatRole,
    ContentPart, ModelError, ReasoningBlock, StopReason,
};
use futures::{StreamExt as _, stream::BoxStream};
use integrations::anthropic::{
    self as api, AnthropicClient, CacheControl, ContentBlock, Effort, InputMessage,
    MessageAccumulator, MessagesRequest, Output, OutputConfig, OutputFormat, ResponseBlock, Role,
    System, TextBlock, Tool, ToolResultContent, sanitize_schema,
};

/// The Anthropic adapter. Cheap to clone (the client is an `Arc` inside).
#[derive(Clone)]
pub struct AnthropicChat {
    client: AnthropicClient,
}

impl std::fmt::Debug for AnthropicChat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnthropicChat")
            .field("models", self.client.models())
            .finish_non_exhaustive()
    }
}

/// The label put in `ModelError`, which takes a `&'static str`: the configured model id is a
/// `String`, so errors name the tier instead.
pub const fn purpose_label(purpose: ChatPurpose) -> &'static str {
    match purpose {
        ChatPurpose::Chat => "anthropic/chat",
        ChatPurpose::PlanWriter => "anthropic/plan_writer",
        ChatPurpose::MemoryExtraction => "anthropic/memory_extraction",
        ChatPurpose::OffTopic => "anthropic/off_topic",
        ChatPurpose::VisionFallback => "anthropic/vision_fallback",
        ChatPurpose::Classification => "anthropic/classification",
    }
}

/// The effort per purpose: chat at the default, the plan writer higher, the cheap calls low.
pub const fn effort_for(purpose: ChatPurpose) -> Effort {
    match purpose {
        ChatPurpose::Chat | ChatPurpose::VisionFallback => Effort::Medium,
        ChatPurpose::PlanWriter => Effort::High,
        ChatPurpose::MemoryExtraction | ChatPurpose::OffTopic | ChatPurpose::Classification => {
            Effort::Low
        }
    }
}

impl AnthropicChat {
    /// Wraps a configured client.
    pub fn new(client: AnthropicClient) -> Self {
        Self { client }
    }

    /// The API request for a domain request, borrowing it.
    ///
    /// `'a` is shared by `self` (the model id comes from the client's config) and `request`
    /// (everything else); the result lives no longer than either. Callers serialise it at once.
    pub fn build<'a>(&'a self, request: &'a ChatRequest) -> MessagesRequest<'a> {
        let model = self.client.models().for_purpose(request.purpose);
        let mut api_request =
            MessagesRequest::new(model, request.max_tokens, messages(&request.messages));
        api_request.system = Some(System::Blocks(vec![TextBlock {
            text: Cow::Borrowed(&request.system),
            // Tools render before the system prompt, so this one breakpoint caches both.
            cache_control: Some(CacheControl::EPHEMERAL),
        }]));
        let mut tools: Vec<&domain::ports::ToolSpec> = request.tools.iter().collect();
        tools.sort_by(|a, b| a.name.cmp(&b.name));
        api_request.tools = tools
            .into_iter()
            .map(|t| Tool {
                name: Cow::Borrowed(&t.name),
                description: Cow::Borrowed(&t.description),
                input_schema: sanitize_schema(&t.parameters).schema,
                strict: true,
                cache_control: None,
            })
            .collect();
        api_request.output_config = Some(OutputConfig {
            effort: Some(effort_for(request.purpose)),
            format: request
                .response_schema
                .as_ref()
                .map(|s| OutputFormat::JsonSchema {
                    schema: sanitize_schema(s).schema,
                }),
        });
        if request.purpose == ChatPurpose::Chat {
            // Automatic caching of the growing conversation inside one turn's tool loop.
            api_request.cache_control = Some(CacheControl::EPHEMERAL);
        }
        api_request
    }
}

/// Domain messages to API messages. Tool results travel in a user message; consecutive messages
/// with the same API role are merged, as the API expects alternating turns.
fn messages(messages: &[ChatMessage]) -> Vec<InputMessage<'_>> {
    let mut out: Vec<InputMessage<'_>> = Vec::with_capacity(messages.len());
    for m in messages {
        let role = match m.role {
            ChatRole::Assistant => Role::Assistant,
            ChatRole::User | ChatRole::Tool => Role::User,
        };
        let blocks = m.content.iter().map(block);
        match out.last_mut() {
            Some(last) if last.role == role => last.content.extend(blocks),
            _ => out.push(InputMessage {
                role,
                content: blocks.collect(),
            }),
        }
    }
    out
}

fn block(part: &ContentPart) -> ContentBlock<'_> {
    match part {
        ContentPart::Text(t) => ContentBlock::text(t.as_str()),
        ContentPart::Image { media_type, data } => ContentBlock::image(media_type, data),
        ContentPart::ToolCall {
            call_id,
            name,
            arguments,
        } => ContentBlock::ToolUse {
            id: Cow::Borrowed(call_id),
            name: Cow::Borrowed(name),
            input: Cow::Borrowed(arguments),
        },
        ContentPart::ToolResult {
            call_id,
            content,
            is_error,
        } => ContentBlock::ToolResult {
            tool_use_id: Cow::Borrowed(call_id),
            content: ToolResultContent::Text(match content {
                serde_json::Value::String(s) => Cow::Borrowed(s.as_str()),
                other => Cow::Owned(other.to_string()),
            }),
            is_error: *is_error,
            cache_control: None,
        },
        ContentPart::Reasoning(ReasoningBlock::Thinking { text, signature }) => {
            ContentBlock::Thinking {
                thinking: Cow::Borrowed(text),
                signature: Cow::Borrowed(signature),
            }
        }
        ContentPart::Reasoning(ReasoningBlock::Redacted { data }) => {
            ContentBlock::RedactedThinking {
                data: Cow::Borrowed(data),
            }
        }
    }
}

const fn stop(reason: Option<api::StopReason>) -> StopReason {
    match reason {
        Some(api::StopReason::ToolUse) => StopReason::ToolUse,
        Some(api::StopReason::MaxTokens) => StopReason::MaxTokens,
        Some(api::StopReason::Refusal) => StopReason::Refusal,
        Some(
            api::StopReason::EndTurn
            | api::StopReason::StopSequence
            | api::StopReason::PauseTurn
            | api::StopReason::Other,
        )
        | None => StopReason::EndTurn,
    }
}

/// One accumulator output as a domain chunk; `None` for blocks the domain doesn't model.
fn chunk(output: Output) -> Option<ChatChunk> {
    Some(match output {
        Output::Text(t) => ChatChunk::TextDelta(t),
        Output::ToolUse { id, name, input } => ChatChunk::ToolCall {
            call_id: id,
            name,
            arguments: input,
        },
        Output::Reasoning(ResponseBlock::Thinking {
            thinking,
            signature,
        }) => ChatChunk::Reasoning(ReasoningBlock::Thinking {
            text: thinking,
            signature,
        }),
        Output::Reasoning(ResponseBlock::RedactedThinking { data }) => {
            ChatChunk::Reasoning(ReasoningBlock::Redacted { data })
        }
        Output::Reasoning(_) => return None,
        Output::Usage(u) => ChatChunk::Usage(u.to_domain()),
        Output::Stop(s) => ChatChunk::Done(stop(Some(s))),
    })
}

#[async_trait::async_trait]
impl ChatModel for AnthropicChat {
    fn model_name(&self, purpose: ChatPurpose) -> String {
        self.client.models().for_purpose(purpose).to_owned()
    }

    /// The generator owns nothing but borrows `self` and `request` for `'a`; the API request it
    /// builds borrows them too and is dropped once the client has serialised it. The client's
    /// `MessageStream` is `'static`, so the only borrows the returned stream holds are the two
    /// the signature names.
    fn stream<'a>(
        &'a self,
        request: &'a ChatRequest,
    ) -> BoxStream<'a, Result<ChatChunk, ModelError>> {
        let label = purpose_label(request.purpose);
        async_stream::try_stream! {
            let api_request = self.build(request);
            let mut events = self
                .client
                .stream(&api_request)
                .await
                .map_err(|e| e.into_model_error(label))?;
            drop(api_request);
            let mut acc = MessageAccumulator::default();
            while let Some(event) = events.next().await {
                let event = event.map_err(|e| e.into_model_error(label))?;
                let output = acc.apply(event).map_err(|e| e.into_model_error(label))?;
                if let Some(c) = output.and_then(chunk) {
                    yield c;
                }
            }
        }
        .boxed()
    }

    async fn complete(&self, request: &ChatRequest) -> Result<ChatCompletion, ModelError> {
        let label = purpose_label(request.purpose);
        let message = self
            .client
            .create(&self.build(request))
            .await
            .map_err(|e| e.into_model_error(label))?;
        Ok(ChatCompletion {
            text: message.text(),
            tool_calls: message
                .tool_uses()
                .map(|t| ChatChunk::ToolCall {
                    call_id: t.id.to_owned(),
                    name: t.name.to_owned(),
                    arguments: t.input.clone(),
                })
                .collect(),
            usage: message.usage.to_domain(),
            stop: stop(message.stop_reason),
        })
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use domain::ports::ToolSpec;
    use integrations::anthropic::{AnthropicConfig, ModelIds};
    use serde_json::json;

    use super::*;

    fn adapter() -> AnthropicChat {
        let http = integrations::http::client(&integrations::http::HttpConfig::default()).unwrap();
        let mut config = AnthropicConfig::new("test-key".to_owned().into());
        config.models = ModelIds::default();
        AnthropicChat::new(AnthropicClient::new(http, config))
    }

    #[test]
    fn requests_borrow_the_domain_request_and_follow_the_api_rules() {
        let a = adapter();
        let request = ChatRequest {
            purpose: ChatPurpose::Chat,
            system: "system".to_owned(),
            messages: vec![
                ChatMessage {
                    role: ChatRole::User,
                    content: vec![ContentPart::Text("hi".to_owned())],
                },
                ChatMessage {
                    role: ChatRole::Assistant,
                    content: vec![
                        ContentPart::Reasoning(ReasoningBlock::Thinking {
                            text: String::new(),
                            signature: "sig".to_owned(),
                        }),
                        ContentPart::ToolCall {
                            call_id: "toolu_1".to_owned(),
                            name: "get_forecast".to_owned(),
                            arguments: json!({"days": 3}),
                        },
                    ],
                },
                ChatMessage {
                    role: ChatRole::Tool,
                    content: vec![
                        ContentPart::ToolResult {
                            call_id: "toolu_1".to_owned(),
                            content: json!({"error": "no"}),
                            is_error: true,
                        },
                        ContentPart::Image {
                            media_type: "image/webp",
                            data: Bytes::from_static(b"RIFF"),
                        },
                    ],
                },
            ],
            tools: vec![
                ToolSpec {
                    name: "z_tool".to_owned(),
                    description: "z".to_owned(),
                    parameters: json!({"type": "object", "properties": {"n": {"type": "integer", "minimum": 1}}}),
                },
                ToolSpec {
                    name: "a_tool".to_owned(),
                    description: "a".to_owned(),
                    parameters: json!({"type": "object", "properties": {}}),
                },
            ],
            response_schema: None,
            max_tokens: 100,
        };
        let built = a.build(&request);
        let body = serde_json::to_value(&built).unwrap();
        assert_eq!(body["model"], "claude-opus-5-5");
        assert_eq!(body["tools"][0]["name"], "a_tool", "tools are sorted");
        assert!(
            body["tools"][1]["input_schema"]["properties"]["n"]
                .get("minimum")
                .is_none()
        );
        assert_eq!(body["tools"][1]["strict"], true);
        assert!(
            body.get("tool_choice").is_none(),
            "forced tool_choice is a 400 on Opus 5.5"
        );
        assert!(body.get("thinking").is_none(), "adaptive by default");
        assert_eq!(body["output_config"]["effort"], "medium");
        assert_eq!(body["messages"].as_array().unwrap().len(), 3);
        assert_eq!(body["messages"][1]["content"][0]["type"], "thinking");
        assert_eq!(body["messages"][2]["role"], "user");
        assert_eq!(body["messages"][2]["content"][0]["type"], "tool_result");
        assert_eq!(body["messages"][2]["content"][0]["is_error"], true);
        assert_eq!(body["messages"][2]["content"][1]["type"], "image");
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn cheap_purposes_use_the_cheap_model_and_low_effort() {
        let a = adapter();
        assert_eq!(
            a.model_name(ChatPurpose::Classification),
            "claude-haiku-5-5"
        );
        assert_eq!(a.model_name(ChatPurpose::PlanWriter), "claude-opus-5-5");
        let request = ChatRequest {
            purpose: ChatPurpose::MemoryExtraction,
            system: String::new(),
            messages: vec![],
            tools: vec![],
            response_schema: Some(
                json!({"type": "object", "properties": {"t": {"type": "string", "maxLength": 9}}}),
            ),
            max_tokens: 10,
        };
        let body = serde_json::to_value(a.build(&request)).unwrap();
        assert_eq!(body["output_config"]["effort"], "low");
        let schema = &body["output_config"]["format"]["schema"];
        assert_eq!(schema["additionalProperties"], false);
        assert!(schema["properties"]["t"].get("maxLength").is_none());
        assert!(body.get("cache_control").is_none());
    }
}
