//! The crate's error and its mapping to `domain::ports::ModelError`.

use domain::ports::{BoxError, ModelError};

use crate::{manifest::ManifestError, rle::RleError, runtime::OrtEp};

/// Everything that can go wrong in this crate outside the upload pipeline's own outcomes
/// (`upload::UploadError`) and the measurement stage's (`measure::MeasureError`).
#[derive(Debug, thiserror::Error)]
pub enum VisionError {
    /// ONNX Runtime refused to load, build or run a session.
    #[error("ONNX Runtime: {0}")]
    Ort(#[from] ort::Error),
    /// The model manifest is unreadable, or a file's hash doesn't match it.
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    /// The configured execution provider isn't compiled into this build.
    #[error("execution provider {0:?} is not compiled into this build")]
    EpNotCompiled(OrtEp),
    /// An fp16 model was given to ONNX Runtime's CPU provider, which crashes on fp16 graphs.
    #[error(
        "{entry} is an fp16 model, which the CPU provider can't run: fetch the fp32 set with \
         `python3 -m ml.models.fetch --for perception --cpu --dest DIR` and point MODEL_MANIFEST at \
         DIR/manifest.toml"
    )]
    Fp16OnCpu {
        /// The manifest entry.
        entry: String,
    },
    /// A loaded ONNX file's tensor names differ from its manifest entry's.
    #[error("{entry}: the ONNX file doesn't match the manifest: {detail}")]
    TensorMismatch {
        /// The manifest entry.
        entry: String,
        /// Which name, and what the file has.
        detail: String,
    },
    /// A model returned a tensor of an unexpected shape or name.
    #[error("{model}: unexpected output: {detail}")]
    Shape {
        /// The model.
        model: &'static str,
        /// What was wrong.
        detail: String,
    },
    /// An image couldn't be decoded or encoded.
    #[error("image: {0}")]
    Image(#[from] image::ImageError),
    /// A resize failed (a zero-sized source or target).
    #[error("resize: {0}")]
    Resize(String),
    /// The tokenizer failed.
    #[error("tokenizer: {0}")]
    Tokenizer(#[source] BoxError),
    /// A payload names a label list or classifier mapping this worker hasn't loaded.
    #[error("{what} {version} is not loaded")]
    UnknownLabelVersion {
        /// `label_set_version` or `classifier_labels_version`.
        what: &'static str,
        /// The version asked for.
        version: String,
    },
    /// A mask failed to encode or decode.
    #[error(transparent)]
    Rle(#[from] RleError),
    /// The job was cancelled while a model ran (`RunOptions::terminate`).
    #[error("cancelled")]
    Cancelled,
    /// The inference thread has stopped.
    #[error("the inference thread has stopped")]
    InferenceGone,
    /// A blocking task panicked.
    #[error("blocking task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

impl VisionError {
    /// The port error for `model`. A failed model is retryable ([contracts 5], `model_error`);
    /// an unknown label version isn't (`unknown_label_version`), which the worker reads from
    /// [`VisionError::is_unknown_label_version`] before it maps the error.
    pub fn into_model_error(self, model: &'static str) -> ModelError {
        ModelError::Failed {
            model,
            source: Box::new(self),
        }
    }

    /// Whether this is `unknown_label_version` ([contracts 5], `JobErrorCode`).
    pub const fn is_unknown_label_version(&self) -> bool {
        matches!(self, Self::UnknownLabelVersion { .. })
    }
}
