//! The worker API client ([contracts 5]): heartbeat, claim, events, complete and image fetch,
//! behind the `WorkerApi` trait so the loop is tested against a fake.

use std::{future::Future, time::Duration};

use bytes::Bytes;
use domain::{
    id::{JobId, LeaseToken},
    jobs::{
        ClaimRequest, ClaimedJob, CompleteJob, Heartbeat, HeartbeatResponse, Job, JobImage,
        WorkerEvents,
    },
};
use reqwest::{Client, Response, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

/// What a call about a job needs: its id and lease token. Owned and cheap to clone (the token
/// is an `Arc<str>`), so the job task, its progress sink and the registry each hold one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobLease {
    /// The job.
    pub job_id: JobId,
    /// `X-Lease-Token`.
    pub lease_token: LeaseToken,
}

impl From<&Job> for JobLease {
    fn from(job: &Job) -> Self {
        Self {
            job_id: job.job_id,
            lease_token: job.lease_token.clone(),
        }
    }
}

/// A failed call, classified by the error body's `retryable` ([contracts 2]): the server's
/// word on whether the same request may succeed later.
#[derive(Debug, thiserror::Error)]
pub enum CallError {
    /// `409` with `code: "lease_lost"`: the job was requeued, expired or cancelled. Drop it
    /// silently.
    #[error("lease lost")]
    LeaseLost,
    /// The same call may succeed later: an error body with `retryable: true`, a `429` or `5xx`
    /// without an error body (a proxy's own answer), a connect error, a timeout, or a body
    /// that couldn't be read. Retry with backoff, and not before `retry_after` when the body
    /// had `retry_after_s`.
    #[error("worker API unavailable: {reason}")]
    Unavailable {
        /// The status and body, or the transport error.
        reason: String,
        /// The body's `retry_after_s`.
        retry_after: Option<Duration>,
    },
    /// An error body with `retryable: false` (`500 internal` among them), or a `4xx` without
    /// one: the same call would fail again.
    #[error("worker API rejected the call: {0}")]
    Rejected(String),
}

impl CallError {
    /// `Unavailable` without a `retry_after`.
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
            retry_after: None,
        }
    }

    /// The wait the API asked for, if any.
    pub const fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::Unavailable { retry_after, .. } => *retry_after,
            Self::LeaseLost | Self::Rejected(_) => None,
        }
    }
}

/// The API boundary. Methods return `impl Future + Send`: the trait is used generically
/// (`Worker<A: WorkerApi>`), never as `dyn`, so no boxing is needed, and the explicit `Send`
/// lets job tasks that await these futures be spawned on the multi-thread runtime. Under
/// edition 2024 the returned future captures every lifetime in scope (`&self` and the borrowed
/// arguments), which is what a future that reads them needs.
pub trait WorkerApi: Send + Sync + 'static {
    /// `POST /internal/workers/heartbeat`.
    fn heartbeat(
        &self,
        hb: &Heartbeat,
    ) -> impl Future<Output = Result<HeartbeatResponse, CallError>> + Send;
    /// `POST /internal/jobs/claim`; `Ok(None)` is the `204` of a long poll that found nothing.
    fn claim(
        &self,
        req: &ClaimRequest,
    ) -> impl Future<Output = Result<Option<ClaimedJob>, CallError>> + Send;
    /// `POST /internal/jobs/{id}/events`.
    fn events(
        &self,
        lease: &JobLease,
        events: &WorkerEvents,
    ) -> impl Future<Output = Result<(), CallError>> + Send;
    /// `POST /internal/jobs/{id}/complete`; a repeat for a completed job is `204` too.
    fn complete(
        &self,
        lease: &JobLease,
        body: &CompleteJob,
    ) -> impl Future<Output = Result<(), CallError>> + Send;
    /// `GET /internal/jobs/{id}/images/{image_id}`: the WebP bytes.
    fn image(
        &self,
        lease: &JobLease,
        image: &JobImage,
    ) -> impl Future<Output = Result<Bytes, CallError>> + Send;
}

/// The reqwest implementation. One `Client` per process (clones share its pool), whose 600 s
/// read gap (`main`) is longer than every deadline here: each call sets its own, so the claim
/// long poll and large image downloads end at that deadline, never at a read gap.
#[derive(Debug, Clone)]
pub struct HttpWorkerApi {
    http: Client,
    base: String,
    token: SecretString,
    wait_s: u32,
}

impl HttpWorkerApi {
    /// A client for `base` (`https://api.example.org`).
    pub fn new(http: Client, base: &str, token: SecretString, wait_s: u32) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
            token,
            wait_s,
        }
    }

    fn post(&self, path: &str, timeout: Duration) -> reqwest::RequestBuilder {
        self.http
            .post(format!("{}{path}", self.base))
            .bearer_auth(self.token.expose_secret())
            .timeout(timeout)
    }

    fn leased(builder: reqwest::RequestBuilder, lease: &JobLease) -> reqwest::RequestBuilder {
        builder.header("X-Lease-Token", lease.lease_token.as_str())
    }
}

/// The fields of [contracts 2]'s error body the worker reads; serde ignores the others.
#[derive(Deserialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Deserialize)]
struct ErrorDetail {
    code: String,
    retryable: bool,
    #[serde(default)]
    retry_after_s: Option<u32>,
}

/// Reads a non-2xx response. Only the body says `lease_lost`, `retryable` and
/// `retry_after_s`, so it is read before anything consumes the response; a body that can't be
/// read is a transport error.
async fn classify(response: Response) -> CallError {
    let status = response.status();
    match response.text().await {
        Ok(body) => classify_body(status, &body),
        Err(e) => transport(&e),
    }
}

/// Classifies an error answer by its body's `retryable`, the server's word ([contracts 2]).
/// `lease_lost` is the one code with its own meaning. Without an error body (a proxy's `502`),
/// a `429` or `5xx` is worth another try and anything else is not.
pub fn classify_body(status: StatusCode, body: &str) -> CallError {
    let what = format!("{status}: {body}");
    match serde_json::from_str::<ErrorBody>(body)
        .ok()
        .map(|b| b.error)
    {
        Some(e) if status == StatusCode::CONFLICT && e.code == "lease_lost" => CallError::LeaseLost,
        Some(e) if e.retryable => CallError::Unavailable {
            reason: what,
            retry_after: e.retry_after_s.map(|s| Duration::from_secs(u64::from(s))),
        },
        None if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS => {
            CallError::unavailable(what)
        }
        // `retryable: false` (`500 internal` among them), or a 4xx without an error body.
        Some(_) | None => CallError::Rejected(what),
    }
}

/// Decodes a claimed job. `ClaimedJob::decode` reads `v`, `job_id` and `lease_token` first and
/// keeps a job of another `v` as `Unsupported`; a `v: 1` job whose `kind` or payload this
/// build can't read (an older worker against a newer API, contracts open question 21) is kept
/// as `Unsupported` too, so it is failed with `unsupported_version` instead of being left to
/// expire. Only a body without even the head is an error.
pub fn decode_claim(body: &[u8]) -> Result<ClaimedJob, CallError> {
    match ClaimedJob::decode(body) {
        Ok(job) => Ok(job),
        Err(e) => match serde_json::from_slice::<domain::jobs::JobHead>(body) {
            Ok(head) => {
                tracing::error!(error = %e, job_id = %head.job_id, "unreadable job");
                Ok(ClaimedJob::Unsupported(head))
            }
            Err(_) => Err(CallError::Rejected(format!("undecodable job: {e}"))),
        },
    }
}

fn transport(e: &reqwest::Error) -> CallError {
    CallError::unavailable(e.to_string())
}

const CALL_TIMEOUT: Duration = Duration::from_secs(10);

impl WorkerApi for HttpWorkerApi {
    async fn heartbeat(&self, hb: &Heartbeat) -> Result<HeartbeatResponse, CallError> {
        let response = self
            .post("/internal/workers/heartbeat", Duration::from_secs(5))
            .json(hb)
            .send()
            .await
            .map_err(|e| transport(&e))?;
        if !response.status().is_success() {
            return Err(classify(response).await);
        }
        response.json().await.map_err(|e| transport(&e))
    }

    async fn claim(&self, req: &ClaimRequest) -> Result<Option<ClaimedJob>, CallError> {
        let timeout = Duration::from_secs(u64::from(self.wait_s) + 10);
        let response = self
            .post("/internal/jobs/claim", timeout)
            .json(req)
            .send()
            .await
            .map_err(|e| transport(&e))?;
        match response.status() {
            StatusCode::NO_CONTENT => Ok(None),
            s if s.is_success() => {
                let body = response.bytes().await.map_err(|e| transport(&e))?;
                decode_claim(&body).map(Some)
            }
            _ => Err(classify(response).await),
        }
    }

    async fn events(&self, lease: &JobLease, events: &WorkerEvents) -> Result<(), CallError> {
        let path = format!("/internal/jobs/{}/events", lease.job_id);
        let response = Self::leased(self.post(&path, CALL_TIMEOUT), lease)
            .json(events)
            .send()
            .await
            .map_err(|e| transport(&e))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(classify(response).await)
        }
    }

    async fn complete(&self, lease: &JobLease, body: &CompleteJob) -> Result<(), CallError> {
        let path = format!("/internal/jobs/{}/complete", lease.job_id);
        let response = Self::leased(self.post(&path, CALL_TIMEOUT), lease)
            .json(body)
            .send()
            .await
            .map_err(|e| transport(&e))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(classify(response).await)
        }
    }

    async fn image(&self, lease: &JobLease, image: &JobImage) -> Result<Bytes, CallError> {
        let request = self
            .http
            .get(format!("{}{}", self.base, image.url.as_str()))
            .bearer_auth(self.token.expose_secret())
            .timeout(Duration::from_secs(30));
        let response = Self::leased(request, lease)
            .send()
            .await
            .map_err(|e| transport(&e))?;
        if response.status().is_success() {
            response.bytes().await.map_err(|e| transport(&e))
        } else {
            Err(classify(response).await)
        }
    }
}

#[cfg(test)]
mod tests;
