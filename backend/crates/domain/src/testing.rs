//! Test doubles for the synchronous ports and the one async port `mockall` can't mock, behind
//! the `test-util` feature: `domain = { workspace = true, features = ["test-util", "mock"] }`
//! under another crate's `[dev-dependencies]`.

use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

use futures::stream::{self, BoxStream};
use ulid::Ulid;

use crate::{
    garden::species::SpeciesPhoto,
    id::SpeciesId,
    ports::{
        ChatChunk, ChatCompletion, ChatModel, ChatPurpose, ChatRequest, Clock, IdGen, ModelError,
        SpeciesPhotos, StopReason, UrlSigner,
    },
    url::SigningInput,
    wire::Timestamp,
};

/// A clock that returns a set time until moved.
#[derive(Debug)]
pub struct FixedClock(Mutex<Timestamp>);

impl FixedClock {
    /// A clock at `t`.
    pub fn at(t: Timestamp) -> Self {
        Self(Mutex::new(t))
    }

    /// Moves the clock forward.
    pub fn advance(&self, by: std::time::Duration) {
        if let Ok(mut t) = self.0.lock() {
            *t = t.plus(by);
        }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        // reason: a poisoned lock means a test already panicked; the time is still valid.
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Ids 1, 2, 3, … (as ULIDs), so tests can predict them.
#[derive(Debug, Default)]
pub struct SequentialIds(AtomicU64);

impl IdGen for SequentialIds {
    fn ulid(&self) -> Ulid {
        Ulid(u128::from(self.0.fetch_add(1, Ordering::Relaxed) + 1))
    }
}

/// A signer whose signature is the canonical input's length: deterministic and obviously fake.
#[derive(Debug, Default)]
pub struct FakeSigner;

impl UrlSigner for FakeSigner {
    fn signature(&self, input: &SigningInput) -> String {
        format!("{:x}", input.canonical().len())
    }
    fn verify(&self, input: &SigningInput, sig: &str) -> bool {
        self.signature(input) == sig
    }
    fn public_base(&self) -> &'static str {
        "https://api.test"
    }
}

/// A species photo index with fixed contents.
#[derive(Debug, Default)]
pub struct FixedPhotos(pub Vec<(SpeciesId, Vec<SpeciesPhoto>)>);

impl SpeciesPhotos for FixedPhotos {
    fn photos(&self, species: &SpeciesId) -> Vec<SpeciesPhoto> {
        self.0
            .iter()
            .find(|(s, _)| s == species)
            .map(|(_, p)| p.clone())
            .unwrap_or_default()
    }
}

/// A chat model that replays a script of chunks for every request. `mockall` can't mock
/// `ChatModel::stream`, whose stream borrows the request, so tests use this.
#[derive(Debug, Default)]
pub struct ScriptedChat {
    /// The chunks every `stream` yields.
    pub chunks: Vec<ChatChunk>,
}

#[async_trait::async_trait]
impl ChatModel for ScriptedChat {
    fn model_name(&self, _: ChatPurpose) -> String {
        "scripted".to_owned()
    }

    fn stream<'a>(&'a self, _: &'a ChatRequest) -> BoxStream<'a, Result<ChatChunk, ModelError>> {
        Box::pin(stream::iter(self.chunks.iter().cloned().map(Ok)))
    }

    async fn complete(&self, _: &ChatRequest) -> Result<ChatCompletion, ModelError> {
        let text = self
            .chunks
            .iter()
            .filter_map(|c| match c {
                ChatChunk::TextDelta(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        Ok(ChatCompletion {
            text,
            tool_calls: Vec::new(),
            usage: crate::chat::Usage::default(),
            stop: StopReason::EndTurn,
        })
    }
}
