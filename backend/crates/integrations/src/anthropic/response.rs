//! The response side: owned (`'static`) types, because a response outlives the request that
//! produced it and crosses tasks and channels.

use std::borrow::Cow;

use serde::Deserialize;
use serde_json::Value;

use super::request::{ContentBlock, Role};

/// A whole message: the non-streaming response, or what [`super::MessageAccumulator`] builds
/// from a stream.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Message {
    /// `msg_…`.
    pub id: String,
    /// The model that answered.
    pub model: String,
    /// Always `assistant`.
    pub role: Role,
    /// The blocks, in order.
    pub content: Vec<ResponseBlock>,
    /// Why it stopped; `None` only while streaming.
    pub stop_reason: Option<StopReason>,
    /// The refusal category and similar details.
    #[serde(default)]
    pub stop_details: Option<Value>,
    /// Token counts.
    pub usage: Usage,
}

impl Message {
    /// The text blocks joined: the answer, or the structured output's JSON.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| match b {
                ResponseBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    /// The tool calls, in order.
    pub fn tool_uses(&self) -> impl Iterator<Item = ToolUse<'_>> {
        self.content.iter().filter_map(|b| match b {
            ResponseBlock::ToolUse { id, name, input } => Some(ToolUse { id, name, input }),
            _ => None,
        })
    }

    /// The blocks as request blocks borrowing from this message, to replay it as the assistant
    /// turn of the next request in a tool loop (thinking blocks included, unchanged). Unknown
    /// blocks are dropped. The returned blocks borrow `self` (elided lifetime), so the message
    /// must outlive the next request; no text or signature is copied.
    pub fn replay(&self) -> Vec<ContentBlock<'_>> {
        self.content
            .iter()
            .filter_map(ResponseBlock::replay)
            .collect()
    }
}

/// A borrowed view of one tool call in a [`Message`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolUse<'m> {
    /// `toolu_…`.
    pub id: &'m str,
    /// The tool.
    pub name: &'m str,
    /// Its arguments.
    pub input: &'m Value,
}

/// One block of a response.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseBlock {
    /// Text.
    Text {
        /// The text.
        #[serde(default)]
        text: String,
    },
    /// A tool call. Streamed, `input` starts as `{}` and is filled at `content_block_stop`.
    ToolUse {
        /// `toolu_…`.
        id: String,
        /// The tool.
        name: String,
        /// Its arguments.
        #[serde(default)]
        input: Value,
    },
    /// Thinking: replay it unchanged on the next turn of a tool loop.
    Thinking {
        /// The summary, empty with `display: omitted`.
        #[serde(default)]
        thinking: String,
        /// The signature.
        #[serde(default)]
        signature: String,
    },
    /// Redacted thinking: replay it unchanged.
    RedactedThinking {
        /// The encrypted data.
        data: String,
    },
    /// A block type this client does not model (server tools, new types). Ignored.
    #[serde(other)]
    Unknown,
}

impl ResponseBlock {
    /// This block as a request block borrowing from it; `None` for [`Self::Unknown`].
    pub fn replay(&self) -> Option<ContentBlock<'_>> {
        Some(match self {
            Self::Text { text } => ContentBlock::text(text.as_str()),
            Self::ToolUse { id, name, input } => ContentBlock::ToolUse {
                id: Cow::Borrowed(id),
                name: Cow::Borrowed(name),
                input: Cow::Borrowed(input),
            },
            Self::Thinking {
                thinking,
                signature,
            } => ContentBlock::Thinking {
                thinking: Cow::Borrowed(thinking),
                signature: Cow::Borrowed(signature),
            },
            Self::RedactedThinking { data } => ContentBlock::RedactedThinking {
                data: Cow::Borrowed(data),
            },
            Self::Unknown => return None,
        })
    }
}

/// `stop_reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Complete.
    EndTurn,
    /// `max_tokens` reached: structured output may be cut off.
    MaxTokens,
    /// A stop sequence matched.
    StopSequence,
    /// It wants tool results.
    ToolUse,
    /// A long server-tool turn paused; send it back to continue.
    PauseTurn,
    /// A safety classifier declined, with HTTP 200; `stop_details.category` says why. The
    /// output may not match the schema.
    Refusal,
    /// A reason added after this client was written.
    #[serde(other)]
    Other,
}

/// Token counts. In a stream, `message_start` carries the input counts and a placeholder
/// `output_tokens`, and each `message_delta` carries **cumulative** counts: the accumulator
/// replaces, never adds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub struct Usage {
    /// Uncached input tokens.
    #[serde(default)]
    pub input_tokens: u32,
    /// Output tokens, thinking included.
    #[serde(default)]
    pub output_tokens: u32,
    /// Input tokens written to the cache.
    #[serde(default)]
    pub cache_creation_input_tokens: u32,
    /// Input tokens read from the cache.
    #[serde(default)]
    pub cache_read_input_tokens: u32,
}

impl Usage {
    /// Every input token, cached or not: `chat_message.tokens_in`.
    pub const fn total_input(&self) -> u32 {
        self.input_tokens
            .saturating_add(self.cache_creation_input_tokens)
            .saturating_add(self.cache_read_input_tokens)
    }

    /// The domain's counts.
    pub const fn to_domain(&self) -> domain::chat::Usage {
        domain::chat::Usage {
            tokens_in: self.total_input(),
            tokens_out: self.output_tokens,
        }
    }

    /// Merges a cumulative update: fields the update carries replace ours.
    pub(crate) fn merge(&mut self, update: &UsageUpdate) {
        if let Some(v) = update.input_tokens {
            self.input_tokens = v;
        }
        if let Some(v) = update.output_tokens {
            self.output_tokens = v;
        }
        if let Some(v) = update.cache_creation_input_tokens {
            self.cache_creation_input_tokens = v;
        }
        if let Some(v) = update.cache_read_input_tokens {
            self.cache_read_input_tokens = v;
        }
    }
}

/// `message_delta.usage`: cumulative, and any field may be missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub struct UsageUpdate {
    /// Uncached input tokens.
    pub input_tokens: Option<u32>,
    /// Output tokens so far.
    pub output_tokens: Option<u32>,
    /// Cache writes.
    pub cache_creation_input_tokens: Option<u32>,
    /// Cache reads.
    pub cache_read_input_tokens: Option<u32>,
}

/// An error body, or the `error` of a stream's `error` event.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ApiErrorDetail {
    /// `invalid_request_error`, `rate_limit_error`, `overloaded_error`, …
    #[serde(rename = "type")]
    pub kind: String,
    /// A human-readable message.
    #[serde(default)]
    pub message: String,
    /// `error_code` such as `enforced_spend_limit_reached`.
    #[serde(default)]
    pub details: Option<Value>,
}

/// A non-streaming error body.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct ErrorBody {
    pub error: ApiErrorDetail,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_a_message_with_unknown_blocks() {
        let m: Message = serde_json::from_value(json!({
            "id": "msg_1", "type": "message", "role": "assistant", "model": "claude-opus-5-5",
            "content": [
                {"type": "thinking", "thinking": "", "signature": "sig"},
                {"type": "server_tool_use", "id": "x", "name": "web_search", "input": {}},
                {"type": "text", "text": "{\"a\":1}"}
            ],
            "stop_reason": "refusal", "stop_details": {"category": "cyber"},
            "usage": {"input_tokens": 10, "output_tokens": 3, "cache_read_input_tokens": 5}
        }))
        .unwrap();
        assert_eq!(m.text(), "{\"a\":1}");
        assert_eq!(m.stop_reason, Some(StopReason::Refusal));
        assert_eq!(m.usage.total_input(), 15);
        assert_eq!(m.replay().len(), 2);
    }
}
