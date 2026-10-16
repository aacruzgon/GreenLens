//! The text embedder ([architecture 10]; recipe in [vision models 10]): snowflake-arctic-embed-xs
//! in fp32 on the control plane's CPU, with our own `ort` + `tokenizers` code (fastembed 7.1.1
//! pins `ort` rc.13, which can't share a build with rc.14). CLS pooling, then L2 to unit length;
//! the query prefix before queries, nothing before documents.
//!
//! [architecture 10]: ../../../docs/architecture/README.md#10-models-and-gpu-placement
//! [vision models 10]: ../../../docs/vision/models.md#10-text-embedder

use std::sync::Arc;

use async_trait::async_trait;
pub use domain::ports::EmbedMode;
use domain::ports::{Embedding, EmbeddingModel, ModelError, TextEmbedder};
use ndarray::Array2;
use ort::{inputs, value::TensorRef};
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};

use crate::{
    VisionError,
    manifest::Manifest,
    runtime::{OrtRuntime, SessionPool, SessionSettings},
};

/// Put before every chat question and search query ([vision models 10]).
pub const QUERY_PREFIX: &str = "Represent this sentence for searching relevant passages: ";

/// The embedder's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbedderConfig {
    /// Sessions in the pool: how many embeddings run at once.
    pub sessions: usize,
    /// Truncation, tokens.
    pub max_tokens: usize,
}

impl Default for EmbedderConfig {
    fn default() -> Self {
        Self {
            sessions: 2,
            max_tokens: 512,
        }
    }
}

struct Inner {
    sessions: SessionPool,
    tokenizer: Tokenizer,
    model: EmbeddingModel,
}

/// The embedder. Cloning shares the sessions. One instance serves both sides of a retrieval:
/// the caller passes [`EmbedMode`] on every `TextEmbedder::embed` call, and only
/// [`EmbedMode::Query`] gets [`QUERY_PREFIX`].
#[derive(Clone)]
pub struct ArcticEmbedder {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for ArcticEmbedder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArcticEmbedder")
            .field("model", &self.inner.model)
            .finish_non_exhaustive()
    }
}

impl ArcticEmbedder {
    /// The model's manifest entry.
    pub const MODEL: &'static str = "text_embedder";
    /// The tokenizer's manifest entry.
    pub const TOKENIZER: &'static str = "text_embedder_tokenizer";
    /// The output width; the `chunk_vec` index's `DIMENSION`.
    pub const DIMENSION: usize = 384;

    /// Loads the pinned files (both hashes checked).
    pub fn load(
        runtime: &OrtRuntime,
        manifest: &Manifest,
        config: EmbedderConfig,
    ) -> Result<Self, VisionError> {
        manifest.verify(&[Self::MODEL, Self::TOKENIZER])?;
        let entry = manifest.entry(Self::MODEL)?;
        let path = manifest.path(entry);
        let sessions = (0..config.sessions.max(1))
            .map(|_| runtime.session(&path, SessionSettings::default()))
            .collect::<Result<Vec<_>, _>>()?;
        let mut tokenizer = Tokenizer::from_file(manifest.path(manifest.entry(Self::TOKENIZER)?))
            .map_err(VisionError::Tokenizer)?;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: config.max_tokens,
                ..TruncationParams::default()
            }))
            .map_err(VisionError::Tokenizer)?;
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::BatchLongest,
            pad_id: 0,
            pad_token: "[PAD]".to_owned(),
            ..PaddingParams::default()
        }));
        Ok(Self {
            inner: Arc::new(Inner {
                sessions: SessionPool::new(Self::MODEL, sessions),
                tokenizer,
                model: EmbeddingModel {
                    name: "snowflake-arctic-embed-xs".to_owned(),
                    version: entry.version.clone(),
                    dimension: Self::DIMENSION,
                },
            }),
        })
    }

    /// `embedding_model` as stored beside every vector: `snowflake-arctic-embed-xs@d8c86521`.
    pub fn stored_name(&self) -> String {
        format!("{}@{}", self.inner.model.name, self.inner.model.version)
    }

    /// Embeds on the calling thread. Blocking.
    pub fn embed_blocking(
        &self,
        mode: EmbedMode,
        texts: &[String],
    ) -> Result<Vec<Embedding>, VisionError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let prefixed = model_inputs(mode, texts);
        let encodings = self
            .inner
            .tokenizer
            .encode_batch(prefixed, true)
            .map_err(VisionError::Tokenizer)?;
        let batch = encodings.len();
        let seq = encodings.first().map_or(0, |e| e.get_ids().len());
        let mut ids = Array2::<i64>::zeros((batch, seq));
        let mut mask = Array2::<i64>::zeros((batch, seq));
        for (row, e) in encodings.iter().enumerate() {
            for (col, (&id, &m)) in e.get_ids().iter().zip(e.get_attention_mask()).enumerate() {
                ids[[row, col]] = i64::from(id);
                mask[[row, col]] = i64::from(m);
            }
        }
        let types = Array2::<i64>::zeros((batch, seq));
        let mut session = self.inner.sessions.checkout();
        let outputs = session.run(inputs![
            "input_ids" => TensorRef::from_array_view(&ids)?,
            "attention_mask" => TensorRef::from_array_view(&mask)?,
            "token_type_ids" => TensorRef::from_array_view(&types)?,
        ])?;
        let (shape, hidden) = outputs["last_hidden_state"].try_extract_tensor::<f32>()?;
        let dim = shape.get(2).copied().unwrap_or(0) as usize;
        if dim != Self::DIMENSION {
            return Err(VisionError::Shape {
                model: "arctic-embed-xs",
                detail: format!("hidden size {dim}"),
            });
        }
        Ok(cls_pool_normalise(hidden, batch, seq, dim)
            .into_iter()
            .map(Embedding)
            .collect())
    }
}

/// The texts as the model reads them: [`QUERY_PREFIX`] before each query, documents unchanged.
pub fn model_inputs(mode: EmbedMode, texts: &[String]) -> Vec<String> {
    texts
        .iter()
        .map(|t| match mode {
            EmbedMode::Query => format!("{QUERY_PREFIX}{t}"),
            EmbedMode::Document => t.clone(),
        })
        .collect()
}

/// CLS pooling (each row's first token) and L2 normalisation, dividing by max(‖v‖, 1e-12).
pub fn cls_pool_normalise(hidden: &[f32], batch: usize, seq: usize, dim: usize) -> Vec<Vec<f32>> {
    (0..batch)
        .map(|b| {
            let start = b * seq * dim;
            let cls = &hidden[start..start + dim];
            let norm = cls.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-12);
            cls.iter().map(|v| v / norm).collect()
        })
        .collect()
}

#[async_trait]
impl TextEmbedder for ArcticEmbedder {
    fn model(&self) -> EmbeddingModel {
        self.inner.model.clone()
    }

    /// Moves a clone of `self` (an `Arc`), the `Copy` mode and an owned copy of the texts into
    /// `spawn_blocking`, whose closure must be `'static`: it can't borrow `&self` or `texts`,
    /// which only live as long as this call.
    async fn embed(&self, mode: EmbedMode, texts: &[String]) -> Result<Vec<Embedding>, ModelError> {
        let this = self.clone();
        let texts = texts.to_vec();
        tokio::task::spawn_blocking(move || this.embed_blocking(mode, &texts))
            .await
            .map_err(VisionError::from)
            .and_then(|r| r)
            .map_err(|e| e.into_model_error("arctic-embed-xs"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cls_pooling_takes_the_first_token_and_normalises() {
        // batch 2, seq 2, dim 2
        let hidden = [3.0, 4.0, 9.0, 9.0, 0.0, 2.0, 1.0, 1.0];
        let out = cls_pool_normalise(&hidden, 2, 2, 2);
        assert_eq!(out[0], vec![0.6, 0.8]);
        assert_eq!(out[1], vec![0.0, 1.0]);
        let zero = cls_pool_normalise(&[0.0, 0.0], 1, 1, 2);
        assert_eq!(zero[0], vec![0.0, 0.0]);
    }

    #[test]
    fn only_queries_get_the_prefix() {
        let texts = vec!["yellow leaves on tomato".to_owned()];
        assert_eq!(
            model_inputs(EmbedMode::Query, &texts),
            vec![format!("{QUERY_PREFIX}yellow leaves on tomato")]
        );
        assert_eq!(model_inputs(EmbedMode::Document, &texts), texts);
    }
}
