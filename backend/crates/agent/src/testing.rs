//! Test doubles for the agent's unit tests. `domain`'s `ScriptedChat` replays one script for
//! every call; the tool loop and the plan writer's retry need a different answer per call, so
//! [`ScriptedModel`] pops one script per call and records every request it was sent.

use std::{collections::VecDeque, sync::Mutex};

use domain::{
    chat::Usage,
    ports::{
        ChatChunk, ChatCompletion, ChatModel, ChatPurpose, ChatRequest, ModelError, StopReason,
    },
};
use futures::stream::{self, BoxStream, StreamExt as _};

/// A chat model with one scripted answer per call.
#[derive(Debug, Default)]
pub struct ScriptedModel {
    streams: Mutex<VecDeque<Vec<ChatChunk>>>,
    completions: Mutex<VecDeque<ChatCompletion>>,
    /// Every request, in call order.
    pub requests: Mutex<Vec<ChatRequest>>,
}

impl ScriptedModel {
    /// A model whose `stream` calls answer with these scripts, in order; an empty script fails
    /// with `ModelError::Unavailable`.
    pub fn streaming(scripts: Vec<Vec<ChatChunk>>) -> Self {
        Self {
            streams: Mutex::new(scripts.into()),
            ..Self::default()
        }
    }

    /// A model whose one `complete` call answers with `text`.
    pub fn completing(text: &str, stop: StopReason) -> Self {
        Self::completions(vec![(text.to_owned(), stop)])
    }

    /// A model whose `complete` calls answer in order.
    pub fn completions(answers: Vec<(String, StopReason)>) -> Self {
        Self {
            completions: Mutex::new(
                answers
                    .into_iter()
                    .map(|(text, stop)| ChatCompletion {
                        text,
                        tool_calls: Vec::new(),
                        usage: Usage {
                            tokens_in: 100,
                            tokens_out: 20,
                        },
                        stop,
                    })
                    .collect(),
            ),
            ..Self::default()
        }
    }

    /// The requests so far.
    pub fn sent(&self) -> Vec<ChatRequest> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl ChatModel for ScriptedModel {
    fn model_name(&self, _: ChatPurpose) -> String {
        "scripted".to_owned()
    }

    fn stream<'a>(
        &'a self,
        request: &'a ChatRequest,
    ) -> BoxStream<'a, Result<ChatChunk, ModelError>> {
        self.requests.lock().unwrap().push(request.clone());
        let script = self
            .streams
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| vec![ChatChunk::Done(StopReason::EndTurn)]);
        if script.is_empty() {
            // An explicitly empty script stands for a provider failure.
            return stream::iter([Err(ModelError::Unavailable {
                model: "scripted",
                source: "overloaded".into(),
            })])
            .boxed();
        }
        stream::iter(script.into_iter().map(Ok)).boxed()
    }

    async fn complete(&self, request: &ChatRequest) -> Result<ChatCompletion, ModelError> {
        self.requests.lock().unwrap().push(request.clone());
        self.completions
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| ModelError::Unavailable {
                model: "scripted",
                source: "no scripted completion left".into(),
            })
    }
}
