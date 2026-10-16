//! The pipeline's error ([`PipelineError`]): the ports' errors wrapped with `#[from]`, never
//! stringified, so `api` maps them with its existing tables ([contracts 2]).

use domain::{
    error::DomainError,
    ports::{BlobError, Conflict, ModelError, ProviderError, StoreError},
};

/// Why an entry point or a step stopped.
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    /// The database. `Conflict(ClaimLost | QuestionStepLost)` inside a step means another
    /// owner (the sweep, a newer run) has the work; the step stops quietly ([`Self::lost`]).
    #[error("store failed")]
    Store(#[from] StoreError),
    /// A rule refused the request (`invalid_state`, `not_found`, …).
    #[error(transparent)]
    Domain(#[from] DomainError),
    /// Object storage (photos and depth maps for measurement).
    #[error("object storage failed")]
    Blob(#[from] BlobError),
    /// A model port (the text embedder for the pipeline's memory).
    #[error("model failed")]
    Model(#[from] ModelError),
    /// The weather provider; a run's `retrieve` fails the scan with `upstream_unavailable`.
    #[error("provider failed")]
    Provider(#[from] ProviderError),
    /// The plan writer.
    #[error("plan writer failed")]
    PlanWriter(#[from] agent::PlanWriterError),
    /// A blocking task panicked or was cancelled.
    #[error("blocking task failed")]
    Join(#[from] tokio::task::JoinError),
    /// No live perception or fallback worker has loaded the knowledge base's label versions
    /// ([contracts 5], versions a worker hasn't loaded): `api` answers `503
    /// upstream_unavailable`.
    #[error(
        "no live worker has loaded label set {label_set_version} / classifier labels {classifier_labels_version}"
    )]
    LabelsNotLoaded {
        /// The label set the job would name.
        label_set_version: String,
        /// The classifier labels it would name.
        classifier_labels_version: String,
    },
    /// The stored data breaks an invariant the pipeline relies on (a private memory offered to
    /// the plan writer, a question without its indicator).
    #[error("inconsistent data: {0}")]
    Inconsistent(&'static str),
}

impl PipelineError {
    /// Another owner has the step: the sweep re-claimed it, a new run started, or the plant or
    /// garden is being deleted. Not an error for the caller.
    pub const fn lost(&self) -> bool {
        matches!(
            self,
            Self::Store(StoreError::Conflict(
                Conflict::ClaimLost | Conflict::QuestionStepLost
            ))
        )
    }
}
