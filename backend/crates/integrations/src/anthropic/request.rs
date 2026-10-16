//! The request side of `POST /v1/messages`: every type borrows from the caller.
//!
//! **Lifetime `'a`** is the caller's data: the system prompt, the conversation, the tool
//! schemas and the image bytes. A request is built, serialised once into an owned body, and
//! dropped before the first network byte comes back, so nothing in it has to be cloned or be
//! `'static`. Text fields are `Cow<'a, str>`, so a caller with an owned `String` (a sanitised
//! schema, a formatted prompt) moves it in as `Cow::Owned` and one with a borrowed `&str` pays
//! nothing. Image bytes are base64-encoded while serialising, straight into the body
//! ([`Base64`]), never into an intermediate `String`.

use std::borrow::Cow;

use serde::{Serialize, Serializer};
use serde_json::Value;

/// One `POST /v1/messages` body, without `stream` (the client sets it).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MessagesRequest<'a> {
    /// A model id from configuration, such as `claude-opus-5-5`.
    pub model: Cow<'a, str>,
    /// Output budget, thinking included.
    pub max_tokens: u32,
    /// The system prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<System<'a>>,
    /// The conversation, oldest first, alternating user and assistant.
    pub messages: Vec<InputMessage<'a>>,
    /// Tools the model may call.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Tool<'a>>,
    /// Forced choices (`any`, `tool`) are a 400 on Opus 5.5 and Sonnet 5.5; send `auto` or
    /// nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice<'a>>,
    /// Thinking. Opus 5.5 always thinks adaptively, so `None` (the default) is right for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<Thinking>,
    /// Effort and structured output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig<'a>>,
    /// Top-level automatic prompt caching (the last cacheable block).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    /// An opaque user id (a hash, never an email), for abuse detection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata<'a>>,
}

impl<'a> MessagesRequest<'a> {
    /// A request with only the required fields.
    pub fn new(
        model: impl Into<Cow<'a, str>>,
        max_tokens: u32,
        messages: Vec<InputMessage<'a>>,
    ) -> Self {
        Self {
            model: model.into(),
            max_tokens,
            system: None,
            messages,
            tools: Vec::new(),
            tool_choice: None,
            thinking: None,
            output_config: None,
            cache_control: None,
            metadata: None,
        }
    }
}

/// `system`: a string, or text blocks when a block needs `cache_control`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum System<'a> {
    /// Plain text.
    Text(Cow<'a, str>),
    /// Blocks.
    Blocks(Vec<TextBlock<'a>>),
}

/// A `{"type":"text"}` block of `system`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename = "text")]
pub struct TextBlock<'a> {
    /// The text.
    pub text: Cow<'a, str>,
    /// A cache breakpoint after this block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// `cache_control`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename = "ephemeral")]
pub struct CacheControl {
    /// 5 minutes (default) or 1 hour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<CacheTtl>,
}

impl CacheControl {
    /// The 5-minute breakpoint.
    pub const EPHEMERAL: Self = Self { ttl: None };
}

/// `cache_control.ttl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CacheTtl {
    /// `"5m"`.
    #[serde(rename = "5m")]
    FiveMinutes,
    /// `"1h"`.
    #[serde(rename = "1h")]
    OneHour,
}

/// A message's author.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// The user, and tool results.
    User,
    /// The model.
    Assistant,
}

/// One message.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InputMessage<'a> {
    /// Its author.
    pub role: Role,
    /// Its blocks. Results of parallel tool calls go in one user message.
    pub content: Vec<ContentBlock<'a>>,
}

/// One content block of a request.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock<'a> {
    /// Text.
    Text {
        /// The text.
        text: Cow<'a, str>,
        /// A cache breakpoint.
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// An image; put images before the text that refers to them.
    Image {
        /// The bytes or a URL.
        source: ImageSource<'a>,
        /// A cache breakpoint.
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// A tool call the model made, replayed.
    ToolUse {
        /// `toolu_…`.
        id: Cow<'a, str>,
        /// The tool.
        name: Cow<'a, str>,
        /// Its arguments.
        input: Cow<'a, Value>,
    },
    /// A tool's result.
    ToolResult {
        /// The call it answers.
        tool_use_id: Cow<'a, str>,
        /// The result.
        content: ToolResultContent<'a>,
        /// The tool failed.
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        is_error: bool,
        /// A cache breakpoint.
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// A thinking block, replayed byte for byte (preserved thinking).
    Thinking {
        /// The (possibly empty) summary.
        thinking: Cow<'a, str>,
        /// The signature binding it to this conversation.
        signature: Cow<'a, str>,
    },
    /// A redacted thinking block, replayed byte for byte.
    RedactedThinking {
        /// The encrypted data.
        data: Cow<'a, str>,
    },
}

impl<'a> ContentBlock<'a> {
    /// A text block without a breakpoint.
    pub fn text(text: impl Into<Cow<'a, str>>) -> Self {
        Self::Text {
            text: text.into(),
            cache_control: None,
        }
    }

    /// A base64 image block over borrowed bytes.
    pub fn image(media_type: &'a str, bytes: &'a [u8]) -> Self {
        Self::Image {
            source: ImageSource::Base64 {
                media_type: Cow::Borrowed(media_type),
                data: Base64(bytes),
            },
            cache_control: None,
        }
    }
}

/// `tool_result.content`: a string or blocks (text and images).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ToolResultContent<'a> {
    /// Text, usually JSON.
    Text(Cow<'a, str>),
    /// Blocks.
    Blocks(Vec<ContentBlock<'a>>),
}

/// `image.source`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource<'a> {
    /// Inline bytes: JPEG, PNG, GIF or WebP, at most 10 MB encoded.
    Base64 {
        /// `image/webp`.
        media_type: Cow<'a, str>,
        /// The raw bytes, encoded on the way out.
        data: Base64<'a>,
    },
    /// A URL the API fetches; it must be reachable from the internet.
    Url {
        /// The URL.
        url: Cow<'a, str>,
    },
}

/// Raw bytes that serialise as standard base64 without an intermediate `String`:
/// `collect_str` hands `data_encoding`'s `Display` adapter to the serializer, which writes it
/// into the body as it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Base64<'a>(pub &'a [u8]);

impl Serialize for Base64<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&data_encoding::BASE64.encode_display(self.0))
    }
}

/// A tool definition.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tool<'a> {
    /// The name.
    pub name: Cow<'a, str>,
    /// What it does.
    pub description: Cow<'a, str>,
    /// JSON Schema of the input; sanitise it first when `strict` is set
    /// ([`super::sanitize_schema`]).
    pub input_schema: Cow<'a, Value>,
    /// Guarantees the input validates; needs `additionalProperties: false`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub strict: bool,
    /// A cache breakpoint after the tool list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// `tool_choice`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolChoice<'a> {
    /// The model decides.
    Auto {
        /// One tool call per turn at most.
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        disable_parallel_tool_use: bool,
    },
    /// Some tool must be called (400 on Opus 5.5 and Sonnet 5.5).
    Any,
    /// This tool must be called (400 on Opus 5.5 and Sonnet 5.5).
    Tool {
        /// The tool.
        name: Cow<'a, str>,
    },
    /// No tool may be called.
    None,
}

/// `thinking`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Thinking {
    /// The model decides how much to think.
    Adaptive {
        /// What comes back in thinking blocks.
        #[serde(skip_serializing_if = "Option::is_none")]
        display: Option<ThinkingDisplay>,
    },
    /// No thinking: Haiku 5.5 at effort `high` or lower; a 400 on Opus 5.5.
    Disabled,
}

/// `thinking.display`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingDisplay {
    /// A summary.
    Summarized,
    /// Nothing but the signature (the default).
    Omitted,
}

/// `output_config`.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct OutputConfig<'a> {
    /// How hard the model works.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    /// Structured output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<OutputFormat<'a>>,
}

/// `output_config.effort`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    /// `low`.
    Low,
    /// `medium`, Opus 5.5's default.
    Medium,
    /// `high`.
    High,
    /// `xhigh`.
    Xhigh,
    /// `max`.
    Max,
}

/// `output_config.format`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputFormat<'a> {
    /// The reply is one text block holding JSON that matches the schema. The schema must be
    /// sanitised first ([`super::sanitize_schema`]): unsupported keywords are a 400.
    JsonSchema {
        /// The schema.
        schema: Cow<'a, Value>,
    },
}

/// `metadata`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Metadata<'a> {
    /// An opaque id.
    pub user_id: Cow<'a, str>,
}

/// The serialised body: the caller's request plus `stream`. Two lifetimes: `'r` is the borrow
/// of the request for the moment of serialising, `'a` the data the request itself borrows.
/// They differ because the client only holds the request for one call while its data lives as
/// long as the caller's context.
#[derive(Serialize)]
pub(crate) struct Body<'r, 'a> {
    #[serde(flatten)]
    pub request: &'r MessagesRequest<'a>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub stream: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serialises_the_documented_shape() {
        let schema = json!({"type": "object"});
        let bytes = [0xffu8, 0x00, 0x10];
        let mut req = MessagesRequest::new(
            "claude-opus-5-5",
            1500,
            vec![InputMessage {
                role: Role::User,
                content: vec![
                    ContentBlock::image("image/webp", &bytes),
                    ContentBlock::text("Image 1: the leaf."),
                ],
            }],
        );
        req.system = Some(System::Text("Be brief.".into()));
        req.output_config = Some(OutputConfig {
            effort: Some(Effort::Medium),
            format: Some(OutputFormat::JsonSchema {
                schema: Cow::Borrowed(&schema),
            }),
        });
        let body = serde_json::to_value(Body {
            request: &req,
            stream: true,
        })
        .unwrap();
        assert_eq!(
            body,
            json!({
                "model": "claude-opus-5-5",
                "max_tokens": 1500,
                "system": "Be brief.",
                "messages": [{"role": "user", "content": [
                    {"type": "image", "source": {"type": "base64", "media_type": "image/webp", "data": "/wAQ"}},
                    {"type": "text", "text": "Image 1: the leaf."}
                ]}],
                "output_config": {"effort": "medium", "format": {"type": "json_schema", "schema": {"type": "object"}}},
                "stream": true
            })
        );
    }

    #[test]
    fn tool_result_and_cache() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "toolu_1".into(),
            content: ToolResultContent::Text("{}".into()),
            is_error: false,
            cache_control: Some(CacheControl {
                ttl: Some(CacheTtl::OneHour),
            }),
        };
        assert_eq!(
            serde_json::to_value(&block).unwrap(),
            json!({"type": "tool_result", "tool_use_id": "toolu_1", "content": "{}",
                   "cache_control": {"type": "ephemeral", "ttl": "1h"}})
        );
    }
}
