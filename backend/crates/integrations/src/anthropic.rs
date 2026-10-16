//! The Anthropic Messages API (`POST /v1/messages`), over raw HTTP: there is no official Rust
//! SDK. `agent` builds `domain::ports::ChatModel` on [`AnthropicClient`]; the fallback worker's
//! vision adapter calls [`AnthropicClient::create`] with image blocks and structured output.
//!
//! **Borrowed requests, owned responses.** [`MessagesRequest<'a>`] borrows the caller's system
//! prompt, conversation, tool schemas and image bytes; the client serialises it once into an
//! owned body and never holds it across the network. Responses ([`Message`]) and the event
//! stream ([`MessageStream`]) own their data and are `'static`, so they can cross tasks.

mod client;
mod error;
mod request;
mod response;
pub mod schema;
mod stream;

pub use client::{API_VERSION, AnthropicClient, AnthropicConfig, ModelIds};
pub use error::AnthropicError;
pub use request::{
    Base64, CacheControl, CacheTtl, ContentBlock, Effort, ImageSource, InputMessage,
    MessagesRequest, Metadata, OutputConfig, OutputFormat, Role, System, TextBlock, Thinking,
    ThinkingDisplay, Tool, ToolChoice, ToolResultContent,
};
pub use response::{
    ApiErrorDetail, Message, ResponseBlock, StopReason, ToolUse, Usage, UsageUpdate,
};
pub use schema::{Sanitized, Stripped, sanitize_in_place, sanitize_schema};
pub use stream::{Delta, MessageAccumulator, MessageDeltaBody, MessageStream, Output, StreamEvent};

#[cfg(test)]
mod tests;
