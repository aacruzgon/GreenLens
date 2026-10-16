//! The agent's settings, read by `api` from the environment at start-up. Model ids live in the
//! LLM client's configuration (`integrations::anthropic::ModelIds`), not here.

use std::time::Duration;

use crate::safety::{Calibration, POISON_CONTROL_TEXT};

/// Limits and texts for chat, the plan writer and memory.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentConfig {
    /// Model calls per chat turn, the first included. A turn that still wants tools after the
    /// last call ends with [`crate::prompt::LOOP_CAP_TEXT`].
    pub max_model_calls: u32,
    /// Stored messages put back into the context, newest kept (architecture 7: "the most recent
    /// messages").
    pub recent_messages: u32,
    /// Memory hits added to the context.
    pub memory_hits: usize,
    /// Knowledge hits `search_knowledge` returns.
    pub knowledge_hits: usize,
    /// The daily chat token budget per user, input and output together (architecture 11).
    pub daily_chat_tokens: u64,
    /// `max_tokens` of one chat model call.
    pub chat_max_tokens: u32,
    /// `max_tokens` of one plan-writer call.
    pub plan_max_tokens: u32,
    /// `max_tokens` of an extraction, summary or off-topic call.
    pub cheap_max_tokens: u32,
    /// How long the router waits for the classifier before routing as a care question.
    pub classifier_timeout: Duration,
    /// A conversation is summarised again after this many messages beyond the recent window.
    pub summarise_every: u32,
    /// The poison-control answer, with the local numbers filled in at deployment.
    pub poison_control_text: String,
    /// The router's calibration, fitted on the labelled example messages.
    pub calibration: Calibration,
    /// The memory gate's minimum calibrated margin of `remember` over `skip`.
    pub memory_gate_margin: f64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_model_calls: 6,
            recent_messages: 12,
            memory_hits: 5,
            knowledge_hits: 4,
            daily_chat_tokens: 300_000,
            chat_max_tokens: 4_096,
            plan_max_tokens: 8_192,
            cheap_max_tokens: 1_024,
            classifier_timeout: Duration::from_secs(3),
            summarise_every: 20,
            poison_control_text: POISON_CONTROL_TEXT.to_owned(),
            calibration: Calibration::default(),
            memory_gate_margin: 0.1,
        }
    }
}
