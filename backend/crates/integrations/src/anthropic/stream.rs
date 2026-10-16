//! Streaming: typed SSE events, the owned event stream, and the accumulator that folds events
//! into whole blocks.

use std::{
    collections::BTreeMap,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{Stream, StreamExt, stream::BoxStream};
use serde::Deserialize;
use serde_json::Value;

use super::{
    error::AnthropicError,
    response::{ApiErrorDetail, Message, ResponseBlock, StopReason, Usage, UsageUpdate},
};
use crate::sse::SseEvent;

/// One SSE `data:` payload; the SSE `event:` name equals its `type`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// The message with empty content, the input usage and a placeholder output count.
    MessageStart {
        /// The message so far.
        message: Message,
    },
    /// A block starts. A `tool_use` block's `input` is `{}` until its JSON deltas end.
    ContentBlockStart {
        /// The block's index in `content`.
        index: usize,
        /// The block, empty.
        content_block: ResponseBlock,
    },
    /// More of a block.
    ContentBlockDelta {
        /// The block.
        index: usize,
        /// The piece.
        delta: Delta,
    },
    /// A block is complete.
    ContentBlockStop {
        /// The block.
        index: usize,
    },
    /// The stop reason and cumulative usage; may come more than once.
    MessageDelta {
        /// The stop reason.
        delta: MessageDeltaBody,
        /// Cumulative counts.
        #[serde(default)]
        usage: Option<UsageUpdate>,
    },
    /// The end.
    MessageStop,
    /// Keep-alive.
    Ping,
    /// An error after HTTP 200 (`overloaded_error`). The client turns it into
    /// [`AnthropicError::Stream`] and ends the stream; callers never see this variant.
    Error {
        /// The error.
        error: ApiErrorDetail,
    },
    /// An event type added after this client was written. Ignored.
    #[serde(other)]
    Unknown,
}

/// `content_block_delta.delta`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Delta {
    /// Text.
    TextDelta {
        /// The text.
        text: String,
    },
    /// Part of a tool call's JSON; concatenate per index, parse at the block's stop.
    InputJsonDelta {
        /// The fragment.
        partial_json: String,
    },
    /// Thinking summary text.
    ThinkingDelta {
        /// The text.
        thinking: String,
    },
    /// A thinking block's signature, just before its stop.
    SignatureDelta {
        /// The signature.
        signature: String,
    },
    /// A delta type added later. Ignored.
    #[serde(other)]
    Unknown,
}

/// `message_delta.delta`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MessageDeltaBody {
    /// Why it stopped.
    pub stop_reason: Option<StopReason>,
    /// The refusal category.
    #[serde(default)]
    pub stop_details: Option<Value>,
}

impl StreamEvent {
    /// Decodes one SSE event. Unknown types decode as [`Self::Unknown`]; a malformed known
    /// type is an error.
    pub(crate) fn from_sse(event: &SseEvent) -> Result<Self, AnthropicError> {
        serde_json::from_str(&event.data).map_err(|e| AnthropicError::Decode(Box::new(e)))
    }
}

/// The owned event stream of one call. `'static` and `Send`: it owns the HTTP response, so the
/// SSE handler can move it into a spawned task or a response body. `Ping` and `Unknown` events
/// are already filtered out; an `error` event arrives as `Err(AnthropicError::Stream)` and is
/// the last item.
pub struct MessageStream {
    inner: BoxStream<'static, Result<StreamEvent, AnthropicError>>,
    request_id: Option<String>,
}

impl MessageStream {
    pub(crate) fn new(
        inner: BoxStream<'static, Result<StreamEvent, AnthropicError>>,
        request_id: Option<String>,
    ) -> Self {
        Self { inner, request_id }
    }

    /// The `request-id` header, for logs.
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    /// Reads the whole stream into a [`Message`].
    pub async fn collect_message(mut self) -> Result<Message, AnthropicError> {
        let mut acc = MessageAccumulator::default();
        while let Some(event) = self.inner.next().await {
            acc.apply(event?)?;
        }
        acc.finish()
    }
}

impl std::fmt::Debug for MessageStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessageStream")
            .field("request_id", &self.request_id)
            .finish_non_exhaustive()
    }
}

impl Stream for MessageStream {
    type Item = Result<StreamEvent, AnthropicError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_next_unpin(cx)
    }
}

/// What a stream event completed, for a caller that streams to the user: text as it comes,
/// whole tool calls and thinking blocks, usage, and the stop.
#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    /// More answer text.
    Text(String),
    /// A complete tool call (its JSON parsed).
    ToolUse {
        /// `toolu_…`.
        id: String,
        /// The tool.
        name: String,
        /// Its arguments.
        input: Value,
    },
    /// A complete thinking or redacted-thinking block, to replay in the tool loop.
    Reasoning(ResponseBlock),
    /// Cumulative usage, after `message_start` and each `message_delta`.
    Usage(Usage),
    /// The stop reason.
    Stop(StopReason),
}

/// Folds stream events into whole blocks and, at the end, a [`Message`]. Tool-call JSON is
/// concatenated per block index and parsed at `content_block_stop` (an empty accumulation is
/// `{}`); usage updates replace rather than add.
#[derive(Debug, Default)]
pub struct MessageAccumulator {
    message: Option<Message>,
    blocks: BTreeMap<usize, ResponseBlock>,
    json: BTreeMap<usize, String>,
}

impl MessageAccumulator {
    /// Applies one event; returns what it completed, if anything.
    pub fn apply(&mut self, event: StreamEvent) -> Result<Option<Output>, AnthropicError> {
        Ok(match event {
            StreamEvent::MessageStart { message } => {
                let usage = message.usage;
                self.message = Some(message);
                Some(Output::Usage(usage))
            }
            StreamEvent::ContentBlockStart {
                index,
                content_block,
            } => {
                self.blocks.insert(index, content_block);
                None
            }
            StreamEvent::ContentBlockDelta { index, delta } => {
                let block = self.blocks.get_mut(&index);
                match (delta, block) {
                    (Delta::TextDelta { text }, Some(ResponseBlock::Text { text: t })) => {
                        t.push_str(&text);
                        Some(Output::Text(text))
                    }
                    (Delta::InputJsonDelta { partial_json }, _) => {
                        self.json.entry(index).or_default().push_str(&partial_json);
                        None
                    }
                    (
                        Delta::ThinkingDelta { thinking },
                        Some(ResponseBlock::Thinking { thinking: t, .. }),
                    ) => {
                        t.push_str(&thinking);
                        None
                    }
                    (
                        Delta::SignatureDelta { signature },
                        Some(ResponseBlock::Thinking { signature: s, .. }),
                    ) => {
                        s.push_str(&signature);
                        None
                    }
                    _ => None,
                }
            }
            StreamEvent::ContentBlockStop { index } => {
                let json = self.json.remove(&index);
                match self.blocks.get_mut(&index) {
                    Some(ResponseBlock::ToolUse { id, name, input }) => {
                        let parsed = match json.as_deref() {
                            None | Some("") => Value::Object(serde_json::Map::new()),
                            Some(s) => {
                                serde_json::from_str(s).map_err(|e| AnthropicError::ToolInput {
                                    index,
                                    source: Box::new(e),
                                })?
                            }
                        };
                        input.clone_from(&parsed);
                        Some(Output::ToolUse {
                            id: id.clone(),
                            name: name.clone(),
                            input: parsed,
                        })
                    }
                    Some(
                        b @ (ResponseBlock::Thinking { .. }
                        | ResponseBlock::RedactedThinking { .. }),
                    ) => Some(Output::Reasoning(b.clone())),
                    _ => None,
                }
            }
            StreamEvent::MessageDelta { delta, usage } => {
                let message = self.message.get_or_insert_with(empty_message);
                if let Some(update) = usage {
                    message.usage.merge(&update);
                }
                if delta.stop_details.is_some() {
                    message.stop_details = delta.stop_details;
                }
                match delta.stop_reason {
                    Some(reason) => {
                        message.stop_reason = Some(reason);
                        Some(Output::Stop(reason))
                    }
                    None => Some(Output::Usage(message.usage)),
                }
            }
            StreamEvent::Error { error } => {
                return Err(AnthropicError::Stream {
                    kind: error.kind,
                    message: error.message,
                });
            }
            StreamEvent::MessageStop | StreamEvent::Ping | StreamEvent::Unknown => None,
        })
    }

    /// The usage so far.
    pub fn usage(&self) -> Usage {
        self.message.as_ref().map(|m| m.usage).unwrap_or_default()
    }

    /// The whole message. Fails if the stream ended before `message_start`.
    pub fn finish(self) -> Result<Message, AnthropicError> {
        let mut message = self.message.ok_or(AnthropicError::Truncated)?;
        message.content = self.blocks.into_values().collect();
        Ok(message)
    }
}

fn empty_message() -> Message {
    Message {
        id: String::new(),
        model: String::new(),
        role: super::Role::Assistant,
        content: Vec::new(),
        stop_reason: None,
        stop_details: None,
        usage: Usage::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sse::SseParser;

    /// A documented stream: thinking, text, a tool call split over two JSON deltas, a ping, an
    /// unknown event type, and cumulative usage.
    pub(crate) const STREAM: &str = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-opus-5-5\",\"content\":[],\"stop_reason\":null,\"usage\":{\"input_tokens\":20,\"output_tokens\":1}}}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\",\"signature\":\"\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"EqQB\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: ping\n",
        "data: {\"type\":\"ping\"}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"Water \"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"less.\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
        "event: new_future_event\n",
        "data: {\"type\":\"new_future_event\",\"x\":1}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"get_weather\",\"input\":{}}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"days\\\":\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"7}\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\":\"content_block_stop\",\"index\":2}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":40}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n",
    );

    #[test]
    fn folds_a_stream() {
        let mut parser = SseParser::default();
        let mut acc = MessageAccumulator::default();
        let mut outputs = Vec::new();
        for chunk in STREAM.as_bytes().chunks(7) {
            for ev in parser.push(chunk) {
                if let Some(o) = acc.apply(StreamEvent::from_sse(&ev).unwrap()).unwrap() {
                    outputs.push(o);
                }
            }
        }
        assert!(outputs.contains(&Output::Text("Water ".into())));
        assert!(outputs.contains(&Output::ToolUse {
            id: "toolu_1".into(),
            name: "get_weather".into(),
            input: serde_json::json!({"days": 7}),
        }));
        assert_eq!(outputs.last(), Some(&Output::Stop(StopReason::ToolUse)));
        let m = acc.finish().unwrap();
        assert_eq!(m.text(), "Water less.");
        assert_eq!(m.usage.output_tokens, 40);
        assert_eq!(m.usage.input_tokens, 20);
        assert_eq!(
            m.content[0],
            ResponseBlock::Thinking {
                thinking: String::new(),
                signature: "EqQB".into()
            }
        );
    }

    #[test]
    fn error_event_after_200() {
        let mut acc = MessageAccumulator::default();
        let ev: StreamEvent = serde_json::from_str(
            r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
        )
        .unwrap();
        let err = acc.apply(ev).unwrap_err();
        assert!(err.retryable());
    }
}
