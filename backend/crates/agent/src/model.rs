//! The model adapters `agent` provides for `domain` ports: [`AnthropicChat`] (`ChatModel`) and
//! [`LlmClassifier`] (`TextClassifier`'s fallback when Jev is not configured; Jev's adapter is
//! `integrations::jev::JevClassifier`).

pub mod anthropic;
pub mod classifier;

pub use anthropic::{AnthropicChat, effort_for, purpose_label};
pub use classifier::{LlmClassifier, MEMORY_LABELS, TOPIC_LABELS, parse_scores, score_schema};
