# Worker job errors, compiled

Part of [rust-errors](SKILL.md#worker-job-errors). A failed job is reported in the `complete` body with `status: "failed"` and `error: { code, message, retryable }`, answered `204` ([contracts 5, complete](../../../docs/architecture/contracts.md#post-internaljobsidcomplete)); a `v` the worker doesn't know fails the job with `unsupported_version` ([contracts 1, versioning](../../../docs/architecture/contracts.md#versioning)). The worker loop that sends it is [rust-workers](../rust-workers/SKILL.md)'s.

The claim body is read head first by the one implementation, the `JobHead` region of [rust-contract-types, the worker checks `v` first](../rust-contract-types/unions.md#3-the-worker-checks-v-first), copied below. Only a decoded `v` other than 1 becomes `unsupported_version`. A body whose head doesn't decode is `ClaimError::Head`: the worker can't name the job or prove its lease, so it logs the error and drops the job, and the lease expires ([contracts 5, leases](../../../docs/architecture/contracts.md#leases-runs-and-attempts)).

**Not in the contracts yet:** `unsupported_version` is sent with `retryable: false` ([contracts 12, question 19](../../../docs/architecture/contracts.md#12-open-questions)), and `model_error` is a placeholder code until contracts 5 lists the model errors ([contracts 12, question 18](../../../docs/architecture/contracts.md#12-open-questions)).

```rust
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error("unsupported job version {0}")]
    UnsupportedVersion(u64),
    #[error("{model} failed")]
    Model {
        model: &'static str,
        retryable: bool,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl JobError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnsupportedVersion(_) => "unsupported_version",
            // Not in the contracts yet: the model error codes (contracts 12, question 18).
            Self::Model { .. } => "model_error",
        }
    }

    pub const fn retryable(&self) -> bool {
        match self {
            // Not in the contracts yet: whether it is retryable (contracts 12, question 19).
            Self::UnsupportedVersion(_) => false,
            Self::Model { retryable, .. } => *retryable,
        }
    }
}

/// contracts 5 `CompleteJob.error`. `message` is for operators: the whole chain.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct JobFailure {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
}

impl From<&JobError> for JobFailure {
    fn from(error: &JobError) -> Self {
        Self {
            code: error.code(),
            message: Chain(error).to_string(),
            retryable: error.retryable(),
        }
    }
}

/// What the worker does with a claim body.
#[derive(Debug)]
pub enum Next<J> {
    Run(J),
    /// `complete` with `status: "failed"`, this error, and `X-Lease-Token: head.lease_token`.
    Fail {
        head: JobHead,
        failure: JobFailure,
    },
}

/// `Err` is a body the worker can't run: `Head` is logged and dropped, `Job` is a bug.
pub fn triage<J: serde::de::DeserializeOwned>(body: &[u8]) -> Result<Next<J>, ClaimError> {
    match decode_claim(body)? {
        Claimed::Job(job) => Ok(Next::Run(job)),
        Claimed::Unsupported(head) => {
            let failure = JobFailure::from(&JobError::UnsupportedVersion(head.v));
            Ok(Next::Fail { head, failure })
        }
    }
}

// copy of: rust-contract-types/unions.md JobHead
/// The fields needed to fail a claimed job, readable from any version: no
/// `deny_unknown_fields`, nothing but what `complete` needs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct JobHead {
    pub v: u64,
    pub job_id: String,
    pub lease_token: String,
}

/// A `200` claim body, read head first (contracts 1, versioning).
#[derive(Debug)]
pub enum Claimed<J> {
    /// `v` is 1 and the whole job decoded.
    Job(J),
    /// `v` isn't 1: complete the job as `failed` with `unsupported_version`.
    Unsupported(JobHead),
}

/// A claim body the worker can't run. Neither message includes the body.
#[derive(Debug, thiserror::Error)]
pub enum ClaimError {
    /// No numeric `v`, `job_id` or `lease_token`: the job can't be completed. Log it and
    /// let the lease expire (contracts 5, leases).
    #[error("claim body has no readable job head")]
    Head(#[source] serde_json::Error),
    /// `v` is 1 but the job doesn't decode: a bug, failed with `head`.
    // Not in the contracts yet: what `complete` reports for it (contracts 12, question 21).
    #[error("job {} does not decode as v1", .head.job_id)]
    Job {
        head: JobHead,
        #[source]
        source: serde_json::Error,
    },
}

/// Decodes a claim body: the head, then `v`, then the whole job.
pub fn decode_claim<J: serde::de::DeserializeOwned>(body: &[u8]) -> Result<Claimed<J>, ClaimError> {
    let head: JobHead = serde_json::from_slice(body).map_err(ClaimError::Head)?;
    if head.v != 1 {
        return Ok(Claimed::Unsupported(head));
    }
    serde_json::from_slice(body)
        .map(Claimed::Job)
        .map_err(|source| ClaimError::Job { head, source })
}
// end: JobHead

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
    }

    #[test]
    fn an_unknown_version_fails_the_job_in_the_complete_body() {
        let body = br#"{"v":2,"job_id":"01JAX","lease_token":"t","payload":{"new":"shape"}}"#;
        let Next::Fail { head, failure } = triage::<Job>(body).unwrap() else {
            panic!("a v2 job is failed, not run");
        };
        assert_eq!(head.lease_token, "t");
        assert_eq!(
            failure,
            JobFailure {
                code: "unsupported_version",
                message: "unsupported job version 2".to_owned(),
                retryable: false,
            }
        );
        let v1 = br#"{"v":1,"job_id":"01JAX","lease_token":"t"}"#;
        assert!(matches!(triage::<Job>(v1).unwrap(), Next::Run(job) if job.job_id == "01JAX"));
    }

    #[test]
    fn an_unreadable_head_is_not_unsupported_version() {
        let body = br#"{"job_id":"01JAX","lease_token":"t"}"#;
        let err = triage::<Job>(body).unwrap_err();
        assert!(matches!(err, ClaimError::Head(_)));
        assert_eq!(err.to_string(), "claim body has no readable job head");
    }

    #[test]
    fn the_failure_message_carries_the_chain() {
        let error = JobError::Model {
            model: "bioclip",
            retryable: true,
            source: "CUDA out of memory".into(),
        };
        let failure = JobFailure::from(&error);
        assert_eq!(failure.message, "bioclip failed: CUDA out of memory");
        assert!(failure.retryable);
    }
}
```
