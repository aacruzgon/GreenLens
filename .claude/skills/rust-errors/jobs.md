# Worker job errors, compiled

Part of [rust-errors](SKILL.md#worker-job-errors). A failed job is reported in the `complete` body with `status: "failed"` and `error: { code, message, retryable }`, answered `204` ([contracts 5, complete](../../../docs/architecture/contracts.md#post-internaljobsidcomplete)). In `domain` that is `domain::jobs::JobError`, whose `code` is a `domain::jobs::JobErrorCode` ([backend domain-types 5, events and completion](../../../docs/backend/domain-types.md#events-and-completion)); the worker loop that sends it is [rust-workers](../rust-workers/SKILL.md)'s, and the worker's own mapping is [backend worker 14](../../../docs/backend/worker.md#14-errors).

## The codes

Settled in [contracts 12, questions 18, 19 and 21](../../../docs/architecture/contracts.md#12-open-questions) and stated in [contracts 5, complete](../../../docs/architecture/contracts.md#post-internaljobsidcomplete) ([backend worker 16](../../../docs/backend/worker.md#16-decisions-on-the-contracts-open-questions), [pipeline 17](../../../docs/backend/pipeline.md#17-open-questions-and-decisions)):

| `JobErrorCode` | `retryable` | When |
|---|---|---|
| `unsupported_version` | `false` | A job `v` other than 1, or a `v: 1` job whose `kind` or payload this build can't read: `decode_claim` keeps its head as `ClaimedJob::Unsupported` ([rust-contract-types, unions 3](../rust-contract-types/unions.md#3-the-worker-checks-v-first)). The same worker can't succeed on a retry; a newer one gets the job only after a requeue |
| `unknown_label_version` | `false` | The payload names a `label_set_version` or `classifier_labels_version` the worker hasn't loaded: `vision::VisionError::UnknownLabelVersion`, found by downcasting the model error's source |
| `model_error` | `true`; `false` when the adapter can't do the kind (`ModelError::Unsupported`) | A model failure, a VLM server or LLM API error, output that failed validation twice. The API also stores a `done` result that doesn't decode for the job's kind as `model_error`, `retryable: true` ([rust-contract-types, unions 4](../rust-contract-types/unions.md#4-complete-tagged-by-status-result-decoded-by-the-jobs-kind)) |
| `image_fetch_failed` | `true` | An image fetch that fails after its retries |
| `max_runtime_exceeded` | `true` | The worker stopped the job itself at its soft deadline ([rust-workers, rule 11](../rust-workers/SKILL.md#2-the-loop)) |
| any other string | the sender's flag | Kept as `JobErrorCode::Other(String)` and stored as written, so an older API reads a newer worker's code |

`JobErrorCode` serialises as its bare string and decodes any string; the canonical definition, with `JobError` and `JobError::new`, is the `JobErrorCode` region of [rust-contract-types, unions 4](../rust-contract-types/unions.md#4-complete-tagged-by-status-result-decoded-by-the-jobs-kind), copied below.

**The message is the error and its `source()` chain, `": "`-joined** ([backend worker 14](../../../docs/backend/worker.md#14-errors)): `Chain(&error).to_string()` ([Display and source](SKILL.md#display-and-source)). It is for operators; the user sees the API's `ScanFailure`, never this text.

**A claim body without a readable head** (`UnreadableHead`) can't be completed at all: there is no `job_id` or `lease_token` to send. The worker logs it and drops the job, and the lease expires ([contracts 5, leases](../../../docs/architecture/contracts.md#leases-runs-and-attempts)). It is never reported as `unsupported_version`.

## The code

`ModelError` below is `domain::ports::ModelError` as [backend domain 6](../../../docs/backend/domain.md#port-errors) writes it, with the source as an inline box; `VisionError` stands in for `vision::VisionError` with only the variant this needs ([backend vision](../../../docs/backend/vision.md)). `job_error` is the worker's ([backend worker 14](../../../docs/backend/worker.md#14-errors)), and `unsupported` is what `run_job` sends for `ClaimedJob::Unsupported` ([backend worker 6](../../../docs/backend/worker.md#6-leases-heartbeats-and-completion)).

```rust
/// `domain::ports::ModelError`: a model's error.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    /// The adapter doesn't do this (a perception adapter asked for a `vlm` job).
    #[error("{model} does not support {what}")]
    Unsupported {
        model: &'static str,
        what: &'static str,
    },
    /// The model's output failed validation after its one retry.
    #[error("{model} returned invalid output")]
    InvalidOutput {
        model: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// Out of memory, a crashed session, a 5xx from the sidecar: worth another attempt.
    #[error("{model} failed")]
    Failed {
        model: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// The hosted API is down or rate limited.
    #[error("{model} unavailable")]
    Unavailable {
        model: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl ModelError {
    /// Whether a job that failed with this error may be retried (contracts 5, `retryable`).
    pub const fn retryable(&self) -> bool {
        !matches!(self, Self::Unsupported { .. })
    }
}

/// A stand-in for `vision::VisionError`, with the one variant `job_error` looks for.
#[derive(Debug, thiserror::Error)]
pub enum VisionError {
    /// A payload names a label list or classifier mapping this worker hasn't loaded.
    #[error("{what} {version} is not loaded")]
    UnknownLabelVersion { what: &'static str, version: String },
}

impl VisionError {
    pub const fn is_unknown_label_version(&self) -> bool {
        matches!(self, Self::UnknownLabelVersion { .. })
    }
}

/// Maps a model's error to the `complete` error (contracts 5, `JobErrorCode`):
/// `unknown_label_version` (not retryable) when the source is a `vision` label-version error,
/// `model_error` otherwise, retryable unless the adapter doesn't do the job at all.
pub fn job_error(e: &ModelError) -> JobError {
    let message = Chain(e).to_string();
    let source = std::error::Error::source(e).and_then(|s| s.downcast_ref::<VisionError>());
    if source.is_some_and(VisionError::is_unknown_label_version) {
        return JobError::new(JobErrorCode::UnknownLabelVersion, message);
    }
    JobError {
        code: JobErrorCode::ModelError,
        message,
        retryable: e.retryable(),
    }
}

/// The error for a claimed job this build can't run (`ClaimedJob::Unsupported`), sent with
/// `X-Lease-Token: head.lease_token`.
pub fn unsupported(head: &JobHead) -> JobError {
    JobError::new(
        JobErrorCode::UnsupportedVersion,
        format!(
            "job v {} (or its kind or payload) is not supported by this worker",
            head.v
        ),
    )
}

// copy of: rust-contract-types/unions.md JobHead
/// The fields needed to fail a claimed job, readable from any version: no
/// `deny_unknown_fields`, nothing but what `complete` needs (`domain::jobs::JobHead`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct JobHead {
    pub v: u64,
    pub job_id: String,
    pub lease_token: String,
}

/// What a worker makes of a `200` claim body (`domain::jobs::ClaimedJob`).
#[derive(Debug, Clone, PartialEq)]
pub enum ClaimedJob<J> {
    /// `v` is 1 and the whole job decoded.
    Job(Box<J>),
    /// Complete it as `failed` with `unsupported_version`, `retryable: false`: another `v`, or
    /// a `v: 1` job whose `kind` or payload this build can't read (contracts 5, leases).
    Unsupported(JobHead),
}

/// A claim body without a numeric `v`, a `job_id` or a `lease_token`: the job can't be
/// completed. Log it and let the lease expire (contracts 5, leases). The message never
/// includes the body.
#[derive(Debug, thiserror::Error)]
#[error("claim body has no readable job head")]
pub struct UnreadableHead(#[source] pub serde_json::Error);

/// Decodes a claim body: the head, then `v`, then the whole job. Only a body without even the
/// head is an error.
pub fn decode_claim<J: serde::de::DeserializeOwned>(
    body: &[u8],
) -> Result<ClaimedJob<J>, UnreadableHead> {
    let head: JobHead = serde_json::from_slice(body).map_err(UnreadableHead)?;
    if head.v != 1 {
        return Ok(ClaimedJob::Unsupported(head));
    }
    match serde_json::from_slice(body) {
        Ok(job) => Ok(ClaimedJob::Job(Box::new(job))),
        Err(error) => {
            tracing::error!(%error, job_id = %head.job_id, "unreadable job");
            Ok(ClaimedJob::Unsupported(head))
        }
    }
}
// end: JobHead

// copy of: rust-contract-types/unions.md JobErrorCode
/// `JobErrorCode` (contracts 5, complete): the codes a worker sends in `CompleteJob.error`,
/// typed `JobErrorCode | string`, so a code this build doesn't know (from a newer worker)
/// still decodes, as `Other`, and is stored as written (`domain::jobs::JobErrorCode`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum JobErrorCode {
    /// A payload or job `v` the worker doesn't support, a kind it doesn't know, or a payload
    /// it can't read; `retryable: false`.
    UnsupportedVersion,
    /// A `label_set_version` or `classifier_labels_version` the worker hasn't loaded;
    /// `retryable: false`.
    UnknownLabelVersion,
    /// Inference failed, or its output failed validation twice; `retryable: true`, except
    /// `false` when the adapter can't do the job's kind at all.
    ModelError,
    /// An image URL failed after retries; `retryable: true`.
    ImageFetchFailed,
    /// The worker stopped the job itself at its soft deadline; `retryable: true`.
    MaxRuntimeExceeded,
    /// Any other code, kept verbatim.
    Other(String),
}

impl JobErrorCode {
    /// The wire string.
    pub fn as_str(&self) -> &str {
        match self {
            Self::UnsupportedVersion => "unsupported_version",
            Self::UnknownLabelVersion => "unknown_label_version",
            Self::ModelError => "model_error",
            Self::ImageFetchFailed => "image_fetch_failed",
            Self::MaxRuntimeExceeded => "max_runtime_exceeded",
            Self::Other(code) => code,
        }
    }

    /// Whether the contract says a failure with this code may be retried; `None` for
    /// `Other`, whose `retryable` flag alone decides. `model_error` says `Some(true)`, the
    /// usual case: the worker sends `false` when the adapter can't do the kind.
    pub const fn retryable(&self) -> Option<bool> {
        match self {
            Self::UnsupportedVersion | Self::UnknownLabelVersion => Some(false),
            Self::ModelError | Self::ImageFetchFailed | Self::MaxRuntimeExceeded => Some(true),
            Self::Other(_) => None,
        }
    }
}

impl serde::Serialize for JobErrorCode {
    /// The bare string, `Other` as written.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for JobErrorCode {
    /// Any string: the five known codes, everything else as `Other`.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = <String as serde::Deserialize>::deserialize(deserializer)?;
        Ok(match code.as_str() {
            "unsupported_version" => Self::UnsupportedVersion,
            "unknown_label_version" => Self::UnknownLabelVersion,
            "model_error" => Self::ModelError,
            "image_fetch_failed" => Self::ImageFetchFailed,
            "max_runtime_exceeded" => Self::MaxRuntimeExceeded,
            _ => Self::Other(code),
        })
    }
}

/// `CompleteJob.error` (contracts 5): job errors travel here, never as HTTP status codes
/// (`domain::jobs::JobError`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct JobError {
    /// A `JobErrorCode`, or another string.
    pub code: JobErrorCode,
    /// Detail for the log: the error and its `source()` chain.
    pub message: String,
    /// Counts toward `attempts` when true.
    pub retryable: bool,
}

impl JobError {
    /// A failure with the contract's `retryable` for `code`. `Other` is never sent by this
    /// build, so it gets `false`.
    pub fn new(code: JobErrorCode, message: String) -> Self {
        let retryable = code.retryable().unwrap_or(false);
        Self {
            code,
            message,
            retryable,
        }
    }
}
// end: JobErrorCode

// copy of: rust-errors/SKILL.md Chain
/// The log form of an error: its message, then each source's, joined by ": ".
/// Log it with `error = %Chain(&error)`; store it with `Chain(&error).to_string()`.
pub struct Chain<'a>(pub &'a (dyn std::error::Error + 'static));

impl std::fmt::Display for Chain<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)?;
        let mut source = self.0.source();
        while let Some(error) = source {
            write!(f, ": {error}")?;
            source = error.source();
        }
        Ok(())
    }
}
// end: Chain

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, serde::Deserialize)]
    struct Job {
        job_id: String,
        kind: Kind,
    }

    #[derive(Debug, serde::Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum Kind {
        Guided,
    }

    /// What the worker sends for a claim body, or `None` when it runs the job.
    fn triage(body: &[u8]) -> Result<Option<(String, JobError)>, UnreadableHead> {
        Ok(match decode_claim::<Job>(body)? {
            ClaimedJob::Job(job) => {
                assert!(matches!(job.kind, Kind::Guided), "{}", job.job_id);
                None
            }
            ClaimedJob::Unsupported(head) => Some((head.lease_token.clone(), unsupported(&head))),
        })
    }

    #[test]
    fn another_version_and_an_unreadable_v1_job_are_unsupported_version() {
        for body in [
            br#"{"v":2,"job_id":"01JAX","lease_token":"t","payload":{"new":"shape"}}"#.as_slice(),
            br#"{"v":1,"job_id":"01JAX","lease_token":"t","kind":"soil_probe","payload":{}}"#,
        ] {
            let (lease_token, error) = triage(body).unwrap().expect("failed, not run");
            assert_eq!(lease_token, "t");
            assert_eq!(error.code, JobErrorCode::UnsupportedVersion);
            assert!(!error.retryable);
        }
        let v1 = br#"{"v":1,"job_id":"01JAX","lease_token":"t","kind":"guided"}"#;
        assert!(triage(v1).unwrap().is_none());
    }

    #[test]
    fn an_unreadable_head_is_not_unsupported_version() {
        let body = br#"{"job_id":"01JAX","lease_token":"t"}"#;
        let err = triage(body).unwrap_err();
        assert_eq!(err.to_string(), "claim body has no readable job head");
    }

    #[test]
    fn a_model_failure_is_a_retryable_model_error_with_the_chain() {
        let error = job_error(&ModelError::Failed {
            model: "bioclip",
            source: "CUDA out of memory".into(),
        });
        assert_eq!(error.code, JobErrorCode::ModelError);
        assert_eq!(error.message, "bioclip failed: CUDA out of memory");
        assert!(error.retryable);
    }

    #[test]
    fn an_adapter_that_cannot_do_the_kind_is_not_retried() {
        let error = job_error(&ModelError::Unsupported {
            model: "perception",
            what: "vlm jobs",
        });
        assert_eq!(
            (error.code, error.retryable),
            (JobErrorCode::ModelError, false)
        );
    }

    #[test]
    fn a_label_list_not_loaded_is_unknown_label_version() {
        let error = job_error(&ModelError::Failed {
            model: "bioclip",
            source: Box::new(VisionError::UnknownLabelVersion {
                what: "label_set_version",
                version: "2026-10-30".to_owned(),
            }),
        });
        assert_eq!(error.code, JobErrorCode::UnknownLabelVersion);
        assert!(!error.retryable);
        assert_eq!(
            error.message,
            "bioclip failed: label_set_version 2026-10-30 is not loaded"
        );
    }

    #[test]
    fn the_codes_are_the_contract_strings() {
        let soft_deadline = JobError::new(JobErrorCode::MaxRuntimeExceeded, String::new());
        let fetch = JobError::new(JobErrorCode::ImageFetchFailed, String::new());
        assert!(soft_deadline.retryable && fetch.retryable);
        assert_eq!(
            serde_json::to_value(&fetch).unwrap(),
            serde_json::json!({ "code": "image_fetch_failed", "message": "", "retryable": true })
        );
    }
}
```
