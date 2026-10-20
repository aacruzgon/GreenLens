//! Tokens and cost: a [`ChatModel`] that records every call's model and usage (the chat turn's
//! tool loop, the router's classifier, memory extraction), and the price table.

use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use futures::{StreamExt as _, stream::BoxStream};
use serde::Serialize;

use domain::ports::{ChatChunk, ChatCompletion, ChatModel, ChatPurpose, ChatRequest, ModelError};

/// US dollars per million tokens, input and output, on Anthropic's first-party API. Source:
/// Anthropic's model pricing (platform.claude.com, "Pricing"), as listed on 2026-10-20. Haiku
/// 5.5's prices hold for prompts up to 100,000 tokens; longer ones cost 5 times as much.
/// Every input token is priced as uncached: the domain's `Usage` counts cache reads and writes
/// with the rest, so the cost is an estimate.
pub const PRICES: [Price; 3] = [
    Price {
        model: "claude-opus-5-5",
        input: 4.00,
        output: 20.00,
        long_prompt: None,
    },
    Price {
        model: "claude-sonnet-5-5",
        input: 2.00,
        output: 10.00,
        long_prompt: None,
    },
    Price {
        model: "claude-haiku-5-5",
        input: 0.10,
        output: 0.50,
        long_prompt: Some(LongPrompt {
            above: 100_000,
            input: 0.50,
            output: 2.50,
        }),
    },
];

/// One model's prices.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Price {
    /// The model id.
    pub model: &'static str,
    /// Per million input tokens.
    pub input: f64,
    /// Per million output tokens.
    pub output: f64,
    /// Higher prices for a long prompt.
    pub long_prompt: Option<LongPrompt>,
}

/// The prices of a prompt longer than `above` tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LongPrompt {
    /// Input tokens.
    pub above: u32,
    /// Per million input tokens.
    pub input: f64,
    /// Per million output tokens.
    pub output: f64,
}

/// The price of one call, or `None` for a model not in [`PRICES`].
pub fn cost(model: &str, tokens_in: u32, tokens_out: u32) -> Option<f64> {
    let p = PRICES.iter().find(|p| p.model == model)?;
    let (input, output) = match p.long_prompt {
        Some(long) if tokens_in > long.above => (long.input, long.output),
        _ => (p.input, p.output),
    };
    Some((f64::from(tokens_in) * input + f64::from(tokens_out) * output) / 1_000_000.0)
}

/// One model call.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Call {
    /// The model id.
    pub model: String,
    /// What the call was for.
    pub purpose: &'static str,
    /// Input tokens, cached ones included.
    pub tokens_in: u32,
    /// Output tokens, thinking included.
    pub tokens_out: u32,
}

/// Records every call to the model it wraps.
pub struct Meter {
    inner: Arc<dyn ChatModel>,
    calls: Mutex<Vec<Call>>,
}

impl Meter {
    /// Wraps `inner`.
    pub fn new(inner: Arc<dyn ChatModel>) -> Self {
        Self {
            inner,
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Calls recorded so far.
    pub fn len(&self) -> usize {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// The calls since `from` (a [`Meter::len`]).
    pub fn since(&self, from: usize) -> Vec<Call> {
        let calls = self.calls.lock().unwrap_or_else(PoisonError::into_inner);
        calls.get(from..).map(<[Call]>::to_vec).unwrap_or_default()
    }

    fn record(&self, purpose: ChatPurpose, usage: domain::chat::Usage) {
        let call = Call {
            model: self.inner.model_name(purpose),
            purpose: agent::model::anthropic::purpose_label(purpose),
            tokens_in: usage.tokens_in,
            tokens_out: usage.tokens_out,
        };
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(call);
    }
}

#[async_trait]
impl ChatModel for Meter {
    fn model_name(&self, purpose: ChatPurpose) -> String {
        self.inner.model_name(purpose)
    }

    fn stream<'a>(
        &'a self,
        request: &'a ChatRequest,
    ) -> BoxStream<'a, Result<ChatChunk, ModelError>> {
        // Usage is cumulative within a call, and the adapter sends the final counts just
        // before `Done`: record the latest once the call is done.
        let mut latest = None;
        self.inner
            .stream(request)
            .inspect(move |chunk| match chunk {
                Ok(ChatChunk::Usage(usage)) => latest = Some(*usage),
                Ok(ChatChunk::Done(_)) => {
                    if let Some(usage) = latest.take() {
                        self.record(request.purpose, usage);
                    }
                }
                _ => {}
            })
            .boxed()
    }

    async fn complete(&self, request: &ChatRequest) -> Result<ChatCompletion, ModelError> {
        let completion = self.inner.complete(request).await?;
        self.record(request.purpose, completion.usage);
        Ok(completion)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_costs_its_tokens_at_the_models_prices() {
        let c = cost("claude-sonnet-5-5", 1_000_000, 100_000).unwrap();
        assert!((c - 3.0).abs() < 1e-9);
        let c = cost("claude-opus-5-5", 20_000, 1_000).unwrap();
        assert!((c - 0.1).abs() < 1e-9);
        assert!(cost("claude-unknown", 1, 1).is_none());
    }

    #[test]
    fn a_long_haiku_prompt_costs_more() {
        let short = cost("claude-haiku-5-5", 100_000, 0).unwrap();
        let long = cost("claude-haiku-5-5", 100_001, 0).unwrap();
        assert!((short - 0.01).abs() < 1e-9);
        assert!(long > 4.0 * short);
    }
}
