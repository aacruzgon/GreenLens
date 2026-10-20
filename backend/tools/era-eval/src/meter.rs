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

/// Whether `model` is in [`PRICES`]. The cost cap needs every model that is called priced.
pub fn is_priced(model: &str) -> bool {
    PRICES.iter().any(|p| p.model == model)
}

/// Refuses a model id that isn't in [`PRICES`], with the ids that are.
pub fn ensure_priced<'a>(models: impl IntoIterator<Item = &'a str>) -> anyhow::Result<()> {
    for model in models {
        anyhow::ensure!(
            is_priced(model),
            "the model \"{model}\" has no price in the table, so --max-cost-usd couldn't hold; \
             priced models: {}",
            PRICES.map(|p| p.model).join(", ")
        );
    }
    Ok(())
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

/// One tool call the model made: its name and the arguments it gave (EON gap G18).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolCall {
    /// The tool.
    pub name: String,
    /// The arguments, as the model sent them.
    pub arguments: serde_json::Value,
}

/// Records every call to the model it wraps, and the tool calls the model makes.
pub struct Meter {
    inner: Arc<dyn ChatModel>,
    calls: Mutex<Vec<Call>>,
    tool_calls: Mutex<Vec<ToolCall>>,
}

impl Meter {
    /// Wraps `inner`.
    pub fn new(inner: Arc<dyn ChatModel>) -> Self {
        Self {
            inner,
            calls: Mutex::new(Vec::new()),
            tool_calls: Mutex::new(Vec::new()),
        }
    }

    /// Tool calls recorded so far.
    pub fn tool_calls_len(&self) -> usize {
        self.tool_calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// The tool calls since `from` (a [`Meter::tool_calls_len`]).
    pub fn tool_calls_since(&self, from: usize) -> Vec<ToolCall> {
        let calls = self
            .tool_calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        calls
            .get(from..)
            .map(<[ToolCall]>::to_vec)
            .unwrap_or_default()
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

    /// Watches one streamed call.
    fn recorder(&self, purpose: ChatPurpose) -> Recorder<'_> {
        Recorder {
            meter: self,
            purpose,
            latest: None,
        }
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

/// Watches one streamed call and records its latest usage once: when the call is done, when it
/// ends in an error, or when the stream is dropped before either (a cancelled turn), so a call
/// that spent tokens and failed is still costed.
struct Recorder<'a> {
    meter: &'a Meter,
    purpose: ChatPurpose,
    /// Usage is cumulative within a call: the latest replaces the earlier ones.
    latest: Option<domain::chat::Usage>,
}

impl Recorder<'_> {
    fn see(&mut self, chunk: &Result<ChatChunk, ModelError>) {
        match chunk {
            Ok(ChatChunk::Usage(usage)) => self.latest = Some(*usage),
            Ok(ChatChunk::Done(_)) | Err(_) => self.flush(),
            Ok(ChatChunk::ToolCall {
                name, arguments, ..
            }) => self
                .meter
                .tool_calls
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(ToolCall {
                    name: name.clone(),
                    arguments: arguments.clone(),
                }),
            Ok(_) => {}
        }
    }

    fn flush(&mut self) {
        if let Some(usage) = self.latest.take() {
            self.meter.record(self.purpose, usage);
        }
    }
}

impl Drop for Recorder<'_> {
    fn drop(&mut self) {
        self.flush();
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
        // The adapter sends the call's cumulative counts, the final ones just before `Done`.
        let mut recorder = self.recorder(request.purpose);
        self.inner
            .stream(request)
            .inspect(move |chunk| recorder.see(chunk))
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

    #[test]
    fn an_unpriced_model_is_refused() {
        assert!(ensure_priced(["claude-opus-5-5", "claude-haiku-5-5"]).is_ok());
        let e = ensure_priced(["claude-opus-5-5", "claude-opus-9"]).unwrap_err();
        assert!(e.to_string().contains("claude-opus-9"), "{e}");
    }

    struct Silent;

    #[async_trait]
    impl ChatModel for Silent {
        fn model_name(&self, _: ChatPurpose) -> String {
            "claude-haiku-5-5".to_owned()
        }

        fn stream<'a>(
            &'a self,
            _: &'a ChatRequest,
        ) -> BoxStream<'a, Result<ChatChunk, ModelError>> {
            futures::stream::empty().boxed()
        }

        async fn complete(&self, _: &ChatRequest) -> Result<ChatCompletion, ModelError> {
            Err(ModelError::Unsupported {
                model: "silent",
                what: "complete",
            })
        }
    }

    fn usage(tokens_in: u32, tokens_out: u32) -> ChatChunk {
        ChatChunk::Usage(domain::chat::Usage {
            tokens_in,
            tokens_out,
        })
    }

    #[test]
    fn a_call_is_costed_once_with_its_latest_usage_whatever_its_end() {
        let meter = Meter::new(Arc::new(Silent));
        // Done: the latest usage replaces the earlier one, recorded once.
        let mut r = meter.recorder(ChatPurpose::Chat);
        r.see(&Ok(usage(10, 1)));
        r.see(&Ok(usage(10, 40)));
        r.see(&Ok(ChatChunk::Done(domain::ports::StopReason::EndTurn)));
        drop(r);
        // An error after usage: still recorded.
        let mut r = meter.recorder(ChatPurpose::Chat);
        r.see(&Ok(usage(20, 5)));
        r.see(&Err(ModelError::Unsupported {
            model: "silent",
            what: "stream",
        }));
        drop(r);
        // Dropped mid-stream: recorded on drop.
        let mut r = meter.recorder(ChatPurpose::Chat);
        r.see(&Ok(usage(30, 7)));
        drop(r);
        // A tool call is kept with its arguments.
        let mut r = meter.recorder(ChatPurpose::Chat);
        r.see(&Ok(ChatChunk::ToolCall {
            call_id: "c1".to_owned(),
            name: "search_farm_records".to_owned(),
            arguments: serde_json::json!({"query": "spray", "until": "2026-07-28"}),
        }));
        drop(r);
        let tool_calls = meter.tool_calls_since(0);
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "search_farm_records");
        assert_eq!(tool_calls[0].arguments["until"], "2026-07-28");
        // No usage, no call.
        drop(meter.recorder(ChatPurpose::Chat));
        let calls = meter.since(0);
        let counts: Vec<(u32, u32)> = calls.iter().map(|c| (c.tokens_in, c.tokens_out)).collect();
        assert_eq!(counts, [(10, 40), (20, 5), (30, 7)]);
    }
}
