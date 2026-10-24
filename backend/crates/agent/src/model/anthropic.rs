//! `ChatModel` over the Anthropic Messages API (`integrations::anthropic`): the strongest model
//! for chat and the plan writer, the cheapest for extraction, summaries, off-topic redirects and
//! classification (architecture 10), chosen per [`ChatPurpose`] by the client's `ModelIds`.
//!
//! The request is built **borrowing** the domain `ChatRequest` (`MessagesRequest<'a>` holds
//! `Cow::Borrowed` text, tool schemas and image bytes), serialised once by the client, and
//! dropped. Responses are owned. Thinking is left at the models' default (adaptive, always on for
//! Opus 5.5); depth is set with `effort` per purpose. Forced `tool_choice` is never sent (a 400
//! on Opus 5.5); structured output uses `output_config.format`. A tool is `strict` only when its
//! spec says so: nineteen strict tools make a grammar the API refuses as too large.

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
                strict: t.strict,
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
        Some(api::StopReason::MaxTokens | api::StopReason::ModelContextWindowExceeded) => {
            StopReason::MaxTokens
        }
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
        Output::Usage(u) => ChatChunk::Usage(u.to_call()),
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
            let mut stopped = false;
            while let Some(event) = events.next().await {
                let event = event.map_err(|e| e.into_model_error(label))?;
                let output = acc.apply(event).map_err(|e| e.into_model_error(label))?;
                if matches!(output, Some(Output::Stop(_))) {
                    // The `message_delta` with the stop reason also carries the final output
                    // count, which the accumulator merges without emitting: send it before
                    // `Done`, or the call's usage stays at an earlier, smaller count.
                    stopped = true;
                    yield ChatChunk::Usage(acc.usage().to_call());
                }
                if let Some(c) = output.and_then(chunk) {
                    yield c;
                }
            }
            // A connection closed cleanly mid-answer ends the byte stream without an error;
            // without a stop reason the answer is incomplete, not finished.
            if !stopped {
                Err(api::AnthropicError::Truncated.into_model_error(label))?;
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
                    strict: true,
                },
                ToolSpec {
                    name: "a_tool".to_owned(),
                    description: "a".to_owned(),
                    parameters: json!({"type": "object", "properties": {}}),
                    strict: false,
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
            body["tools"][0].get("strict").is_none(),
            "a non-strict tool sends no flag"
        );
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

    #[tokio::test]
    async fn a_stream_cut_off_before_its_stop_reason_ends_in_an_error() {
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

        let server = MockServer::start().await;
        let body = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-opus-5-5\",\"content\":[],\"stop_reason\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":1}}}\n\n",
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Water the\"}}\n\n",
        );
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(body),
            )
            .mount(&server)
            .await;
        let mut config = AnthropicConfig::new("test-key".to_owned().into());
        config.base_url = server.uri();
        config.retry = integrations::http::RetryPolicy::NONE;
        let a = AnthropicChat::new(AnthropicClient::new(
            integrations::http::client(&integrations::http::HttpConfig::default()).unwrap(),
            config,
        ));
        let request = ChatRequest {
            purpose: ChatPurpose::Chat,
            system: "s".to_owned(),
            messages: vec![ChatMessage {
                role: ChatRole::User,
                content: vec![ContentPart::Text("hi".to_owned())],
            }],
            tools: vec![],
            response_schema: None,
            max_tokens: 100,
        };
        let items: Vec<_> = a.stream(&request).collect().await;
        assert!(matches!(items[0], Ok(ChatChunk::Usage(_))));
        assert!(matches!(&items[1], Ok(ChatChunk::TextDelta(t)) if t == "Water the"));
        assert!(matches!(
            items.last(),
            Some(Err(ModelError::Unavailable { .. }))
        ));
    }

    /// The final output count comes in the `message_delta` that carries the stop reason; the
    /// last `Usage` before `Done` must hold it (found by the era-eval harness, docs/EON E8.1).
    /// It also carries the cache split, for the daily chat budget.
    #[tokio::test]
    async fn the_last_usage_before_done_has_the_final_output_count() {
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

        let server = MockServer::start().await;
        let body = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-opus-5-5\",\"content\":[],\"stop_reason\":null,\"usage\":{\"input_tokens\":5,\"cache_read_input_tokens\":100,\"cache_creation_input_tokens\":20,\"output_tokens\":1}}}\n\n",
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Water the bed.\"}}\n\n",
            "event: content_block_stop\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":42}}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        );
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(body),
            )
            .mount(&server)
            .await;
        let mut config = AnthropicConfig::new("test-key".to_owned().into());
        config.base_url = server.uri();
        config.retry = integrations::http::RetryPolicy::NONE;
        let a = AnthropicChat::new(AnthropicClient::new(
            integrations::http::client(&integrations::http::HttpConfig::default()).unwrap(),
            config,
        ));
        let request = ChatRequest {
            purpose: ChatPurpose::Chat,
            system: "s".to_owned(),
            messages: vec![ChatMessage {
                role: ChatRole::User,
                content: vec![ContentPart::Text("hi".to_owned())],
            }],
            tools: vec![],
            response_schema: None,
            max_tokens: 100,
        };
        let items: Vec<_> = a.stream(&request).map(Result::unwrap).collect().await;
        let last_usage = items
            .iter()
            .rev()
            .find_map(|c| match c {
                ChatChunk::Usage(u) => Some(*u),
                _ => None,
            })
            .unwrap();
        assert_eq!(last_usage.usage.tokens_in, 125, "every input token");
        assert_eq!(last_usage.usage.tokens_out, 42);
        assert_eq!((last_usage.cache_read, last_usage.cache_write), (100, 20));
        assert!(matches!(
            items.last(),
            Some(ChatChunk::Done(StopReason::EndTurn))
        ));
    }
}
