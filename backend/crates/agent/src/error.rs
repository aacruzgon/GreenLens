//! The agent's error enums. Ports' errors (`StoreError`, `ModelError`, `BlobError`) are wrapped,
//! never stringified, so `api` maps them with its existing tables.

use domain::{
    error::{DomainError, ErrorCode},
    id::MemoryId,
    ports::{BlobError, ModelError, ProviderError, StoreError},
};

/// Why a chat turn could not start. Returned before the stream exists, so `api` answers with an
/// ordinary HTTP error ([contracts 8], sending a message).
#[derive(Debug, thiserror::Error)]
pub enum TurnError {
    /// A rule: `invalid_request` (text too long, too many images, an image of another
    /// conversation), `budget_exceeded` (the daily chat token budget), `not_found`.
    #[error(transparent)]
    Domain(#[from] DomainError),
    /// The database failed.
    #[error("store failed before the turn started")]
    Store(#[from] StoreError),
}

/// Why the plan writer produced no plan. Validation failures are not errors: failing steps are
/// dropped and reported in `WrittenPlan::dropped`.
#[derive(Debug, thiserror::Error)]
pub enum PlanWriterError {
    /// The LLM call failed.
    #[error("plan writer model call failed")]
    Model(#[from] ModelError),
    /// The answer did not match the schema twice, or stopped at `max_tokens`.
    #[error("plan writer output unusable: {0}")]
    Output(String),
    /// The provider's safety classifier declined.
    #[error("plan writer request refused by the provider")]
    Refused,
}

/// A chat memory id that is private: the plan writer reads only shared memories
/// (architecture 7, memory).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("memory {id} is a private chat memory and cannot reach the plan writer")]
pub struct PrivateMemory {
    /// The memory.
    pub id: MemoryId,
}

/// How a tool call failed.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// The call was refused: a dose in its arguments, arguments that don't parse, a record
    /// outside the garden, a preview that fails plan validation. The reason goes back to the
    /// model as an `is_error` tool result so it can rewrite the call.
    #[error("{0}")]
    Rejected(String),
    /// The store failed. The model is told the tool is unavailable; the cause is logged.
    #[error("store failed in a tool")]
    Store(#[from] StoreError),
    /// Object storage failed (`view_image`).
    #[error("object storage failed in a tool")]
    Blob(#[from] BlobError),
    /// A model port failed (the query embedding).
    #[error("model failed in a tool")]
    Model(#[from] ModelError),
}

impl ToolError {
    /// The text the model reads as the tool result.
    pub fn for_model(&self) -> String {
        match self {
            Self::Rejected(reason) => reason.clone(),
            Self::Store(StoreError::NotFound) => "Not found in this garden.".to_owned(),
            Self::Store(StoreError::Rejected(e)) => format!("Refused: {e}."),
            Self::Store(_) | Self::Blob(_) | Self::Model(_) => {
                "The tool is unavailable right now. Answer without it.".to_owned()
            }
        }
    }
}

/// Why memory extraction or summarisation failed. Never shown to the user: the turn has already
/// been stored, so a failure only means nothing was remembered.
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    /// The classifier or the cheap model failed.
    #[error("memory model call failed")]
    Model(#[from] ModelError),
    /// The memory store or chat store failed.
    #[error("memory store failed")]
    Store(#[from] StoreError),
    /// The cheap model's answer did not parse.
    #[error("memory extraction output unusable")]
    Output,
}

/// Why the belief sweep ([`crate::beliefs::sync_pending`]) stopped. Nothing is lost: the
/// memories not yet recorded stay unsynced for the next run.
#[derive(Debug, thiserror::Error)]
pub enum BeliefSyncError {
    /// Reading the unsynced memories or marking one synced failed.
    #[error("memory store failed in the belief sweep")]
    Store(#[from] StoreError),
    /// Agent Memory refused or missed a record; the sweep stopped there to keep the order.
    #[error("agent memory failed after {synced} records")]
    Record {
        /// Memories recorded before the failure.
        synced: usize,
        /// The provider's error.
        #[source]
        source: ProviderError,
    },
}

/// The `error` event's code and text for a model failure after the stream started
/// ([contracts 8]). A provider that is down, overloaded or too slow (a transport timeout) is
/// `upstream_unavailable`; anything else is our bug, `internal`. `retryable` is the code's
/// default from contracts 2 (`ErrorCode::default_retryable`).
pub(crate) const fn stream_error(e: &ModelError) -> (ErrorCode, &'static str) {
    match e {
        ModelError::Unavailable { .. } => (
            ErrorCode::UpstreamUnavailable,
            "The assistant is unavailable right now. Try again in a moment.",
        ),
        ModelError::Unsupported { .. }
        | ModelError::InvalidOutput { .. }
        | ModelError::Failed { .. } => {
            (ErrorCode::Internal, "Something went wrong while answering.")
        }
    }
}
