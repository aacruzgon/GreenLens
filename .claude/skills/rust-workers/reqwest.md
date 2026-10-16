# HTTP from the worker: reqwest 0.13.5

Part of [rust-workers](SKILL.md). Versions: reqwest 0.13.5 with the `json` feature ([versions.md](../rust/versions.md)), axum 0.8.9 for the test server. reqwest 0.13 is a new major release: code written for 0.12 may not compile or may behave differently ([CHANGELOG](https://github.com/seanmonstar/reqwest/blob/v0.13.5/CHANGELOG.md)). The calls themselves are fixed by [contracts 5](../../../docs/architecture/contracts.md#5-worker-api-and-jobs).

## Contents

1. [The client](#1-the-client)
2. [Timeouts](#2-timeouts)
3. [The worker API calls](#3-the-worker-api-calls)
4. [Reading errors](#4-reading-errors)
5. [The VLM sidecar](#5-the-vlm-sidecar)

## 1. The client

- **One `Client` per process.** It holds the connection pool, and it is an `Arc` inside: clone it into tasks, don't wrap it in another `Arc` and don't build one per call. [Client](https://docs.rs/reqwest/0.13.5/reqwest/struct.Client.html)
- **Build it with `Client::builder()...build()`**, which returns an error. `Client::new()` panics if the TLS backend can't be initialised. [Client::new](https://docs.rs/reqwest/0.13.5/reqwest/struct.Client.html#method.new), [ClientBuilder::build](https://docs.rs/reqwest/0.13.5/reqwest/struct.ClientBuilder.html#method.build)
- **TLS in 0.13**: the default backend is rustls with the aws-lc crypto provider, and certificates are checked by `rustls-platform-verifier` (0.7.1 in the lock file). On Linux that verifier loads the system's CA certificates when the client is built, and `build()` fails with "No CA certificates were loaded from the system" if there are none. So every worker image needs the distribution's CA certificates, even a rig worker that only calls the API. *From the source, not run*: `reqwest-0.13.5/src/async_impl/client.rs` (`rustls_platform_verifier::Verifier::new` in `build`) and `rustls-platform-verifier-0.7.1/src/verification/others.rs`. [CHANGELOG, v0.13.0](https://github.com/seanmonstar/reqwest/blob/v0.13.5/CHANGELOG.md)
- **`json` is a crate feature** (on in versions.md): `RequestBuilder::json` sets `Content-Type: application/json` unless already set, and `Response::json` decodes the body. [RequestBuilder::json](https://docs.rs/reqwest/0.13.5/reqwest/struct.RequestBuilder.html#method.json), [Response::json](https://docs.rs/reqwest/0.13.5/reqwest/struct.Response.html#method.json)
- **`bearer_auth`** writes `Authorization: Bearer <token>` and marks the header sensitive. [RequestBuilder::bearer_auth](https://docs.rs/reqwest/0.13.5/reqwest/struct.RequestBuilder.html#method.bearer_auth)

## 2. Timeouts

| Setting | Covers | Default | In the worker |
|---|---|---|---|
| `ClientBuilder::timeout` | The whole request: from connecting until the body has been read | None | 30 s, as a backstop only (*this skill's choice*) |
| `RequestBuilder::timeout` | The same, for one request; overrides the client's | None | Every call sets its own |
| `ClientBuilder::connect_timeout` | The connect phase only | None | 5 s |
| `ClientBuilder::read_timeout` | Each read, reset after each successful read, **including the wait for the response headers** | None | Not set |

- The claim gets `wait_s + 10 s`, longer than the long poll. Heartbeat 5 s, `events` and `complete` 10 s, image fetch 20 s: *design choices*, each far below the 30 s heartbeat limit or the job's `max_runtime`.
- **No `read_timeout` on the shared client.** Its timer starts when the request is sent and is checked before the headers arrive, so a claim that waits 25 s for a job fails at the read timeout. A vLLM call without streaming is silent until the answer is complete, too. The test `read_timeout_also_bounds_the_wait_for_headers` below shows it, and the source agrees: `reqwest-0.13.5/src/async_impl/client.rs`, `Pending::poll` checks `read_timeout_fut` before the response future. There is no per-request `read_timeout`. [ClientBuilder::read_timeout](https://docs.rs/reqwest/0.13.5/reqwest/struct.ClientBuilder.html#method.read_timeout), [RequestBuilder](https://docs.rs/reqwest/0.13.5/reqwest/struct.RequestBuilder.html)
- `Error::is_timeout()` walks the error's source chain, so it is also true for a timeout while reading the body. [Error::is_timeout](https://docs.rs/reqwest/0.13.5/reqwest/struct.Error.html#method.is_timeout)
- **`is_decode()` alone doesn't mean a bad body.** `Response::json` and `bytes` report a body that couldn't be *read* (a timeout, a cut connection) with the same `Decode` kind as JSON that doesn't parse (`reqwest-0.13.5/src/async_impl/response.rs`, `do_bytes` and `json` both call `error::decode`). Tell them apart by the source: only a `serde_json::Error` source is a bad body; the test `a_bad_body_is_rejected_a_stalled_body_is_unavailable` proves both. [Error::is_decode](https://docs.rs/reqwest/0.13.5/reqwest/struct.Error.html#method.is_decode), [Response::json](https://docs.rs/reqwest/0.13.5/reqwest/struct.Response.html#method.json)

Sources: [ClientBuilder::timeout](https://docs.rs/reqwest/0.13.5/reqwest/struct.ClientBuilder.html#method.timeout), [ClientBuilder::read_timeout](https://docs.rs/reqwest/0.13.5/reqwest/struct.ClientBuilder.html#method.read_timeout), [ClientBuilder::connect_timeout](https://docs.rs/reqwest/0.13.5/reqwest/struct.ClientBuilder.html#method.connect_timeout), [RequestBuilder::timeout](https://docs.rs/reqwest/0.13.5/reqwest/struct.RequestBuilder.html#method.timeout).

## 3. The worker API calls

`HttpApi` is what implements `WorkerApi` from [loop.md](loop.md) (the trait isn't repeated here, so this block compiles alone; `CallError` is the same as there). The tests run against an axum server on a local port, so they need no network.

```rust
use std::error::Error as _;
use std::time::Duration;

use reqwest::{Client, Response, StatusCode, Url};
use serde::Deserialize;
use serde::de::DeserializeOwned;

pub const LEASE_TOKEN_HEADER: &str = "x-lease-token"; // contracts §1: `X-Lease-Token`

#[derive(Debug, thiserror::Error)]
pub enum CallError {
    #[error("lease lost")]
    LeaseLost,
    #[error("worker API unavailable: {reason}")]
    Unavailable {
        reason: String,
        retry_after: Option<Duration>,
    },
    #[error("worker API rejected the call: {0}")]
    Rejected(String),
}

// Only the fields the worker reads; serde ignores the others (contracts §2 has the full shape).
#[derive(Deserialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Deserialize)]
struct ErrorDetail {
    code: String,
    retryable: bool,
    retry_after_s: Option<u64>,
}

/// Sketch of the `domain` job type (contracts §5), with plain types and no `payload`.
#[derive(Debug, Deserialize)]
pub struct Job {
    pub v: u64,
    pub job_id: String,
    pub kind: String,
    pub scan_id: String,
    pub run: Option<u32>,
    pub attempt: u32,
    pub lease_token: String,
    pub lease_until: String, // a `Timestamp` in `domain`; never compared with the local clock
    pub max_runtime_s: u64,
}

/// contracts §5 `JobImage`, with plain types.
#[derive(Debug, Deserialize)]
pub struct JobImage {
    pub image_id: String,
    pub kind: String, // `ImageKind` in `domain`
    pub width: u32,
    pub height: u32,
    pub url: String, // `GET /internal/jobs/{job_id}/images/{image_id}`
}

/// One per process. `Client` is an `Arc` inside, so clones share one connection pool.
pub fn build_client() -> reqwest::Result<Client> {
    Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30)) // a backstop; each call below sets its own
        .build()
}

#[derive(Clone)]
pub struct HttpApi {
    client: Client,
    base: Url, // e.g. "https://api.example.com"
    worker_token: String,
}

impl HttpApi {
    /// Returns `cancel_job_ids`. 5 s, far below the 30 s after which the broker requeues.
    pub async fn heartbeat(&self, body: &serde_json::Value) -> Result<Vec<String>, CallError> {
        #[derive(Deserialize)]
        struct HeartbeatResponse {
            cancel_job_ids: Vec<String>,
        }
        let resp = self
            .client
            .post(self.url("/internal/workers/heartbeat")?)
            .bearer_auth(&self.worker_token)
            .json(body)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| transport(&e))?;
        let resp: HeartbeatResponse = json(check(resp).await?).await?;
        Ok(resp.cancel_job_ids)
    }

    pub async fn claim(
        &self,
        body: &serde_json::Value,
        wait_s: u64,
    ) -> Result<Option<Job>, CallError> {
        let resp = self
            .client
            .post(self.url("/internal/jobs/claim")?)
            .bearer_auth(&self.worker_token)
            .json(body)
            .timeout(Duration::from_secs(wait_s + 10)) // longer than the long poll
            .send()
            .await
            .map_err(|e| transport(&e))?;
        if resp.status() == StatusCode::NO_CONTENT {
            return Ok(None); // nothing arrived within wait_s
        }
        // Production decodes head first with rust-contract-types' `decode_claim`, so a
        // `v != 1` body still yields what `complete` needs (unions.md §3).
        json(check(resp).await?).await.map(Some)
    }

    pub async fn events(&self, job: &Job, body: &serde_json::Value) -> Result<(), CallError> {
        self.job_call(job, "events", body).await
    }

    pub async fn complete(&self, job: &Job, body: &serde_json::Value) -> Result<(), CallError> {
        self.job_call(job, "complete", body).await
    }

    async fn job_call(
        &self,
        job: &Job,
        call: &str,
        body: &serde_json::Value,
    ) -> Result<(), CallError> {
        let resp = self
            .client
            .post(self.url(&format!("/internal/jobs/{}/{call}", job.job_id))?)
            .bearer_auth(&self.worker_token)
            .header(LEASE_TOKEN_HEADER, job.lease_token.as_str())
            .json(body)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| transport(&e))?;
        check(resp).await.map(drop) // 204
    }

    /// A plant or soil image of the job, from the `url` of its `JobImage` (contracts §5,
    /// storage §3). Payloads never list depth maps, and no route serves one.
    pub async fn image(&self, job: &Job, image: &JobImage) -> Result<Vec<u8>, CallError> {
        let url = self.url(&image.url)?;
        let resp = self
            .client
            .get(url)
            .bearer_auth(&self.worker_token)
            .header(LEASE_TOKEN_HEADER, job.lease_token.as_str())
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .map_err(|e| transport(&e))?;
        let bytes = check(resp)
            .await?
            .bytes()
            .await
            .map_err(|e| transport(&e))?;
        Ok(Vec::from(bytes))
    }

    /// Resolves a path (or a full URL) against the API's base. A URL on another origin is
    /// refused: the bearer and lease tokens only ever go to the API.
    fn url(&self, path: &str) -> Result<Url, CallError> {
        let url = self
            .base
            .join(path)
            .map_err(|e| CallError::Rejected(format!("bad url {path}: {e}")))?;
        if url.origin() == self.base.origin() {
            Ok(url)
        } else {
            Err(CallError::Rejected(format!("url {path} is not on the API")))
        }
    }
}

/// Turns a non-2xx answer into a `CallError`. The body is read first: `error_for_status`
/// would consume the response, and only the body says `lease_lost`, `retryable` and
/// `retry_after_s` (contracts §2).
async fn check(resp: Response) -> Result<Response, CallError> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    // A proxy's own 502 has no `ErrorBody`: fall back to the status.
    let body = resp.json::<ErrorBody>().await.ok().map(|b| b.error);
    Err(match body {
        Some(error) if error.code == "lease_lost" => CallError::LeaseLost,
        Some(error) if error.retryable => CallError::Unavailable {
            reason: format!("{status} {}", error.code),
            retry_after: error.retry_after_s.map(Duration::from_secs),
        },
        Some(error) => CallError::Rejected(format!("{status} {}", error.code)),
        None if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() => {
            CallError::Unavailable {
                reason: status.to_string(),
                retry_after: None,
            }
        }
        None => CallError::Rejected(status.to_string()),
    })
}

/// Decodes a success body. Only JSON that doesn't parse is a bug; a body that couldn't be
/// read (a timeout, a cut connection) is retried like any network error.
async fn json<T: DeserializeOwned>(resp: Response) -> Result<T, CallError> {
    resp.json().await.map_err(|e| {
        if matches!(e.source(), Some(source) if source.is::<serde_json::Error>()) {
            CallError::Rejected(format!("bad body: {e}"))
        } else {
            transport(&e)
        }
    })
}

fn transport(e: &reqwest::Error) -> CallError {
    // is_timeout() also covers a timeout while reading the body
    let reason = if e.is_timeout() {
        format!("timed out: {e}")
    } else {
        e.to_string()
    };
    CallError::Unavailable {
        reason,
        retry_after: None,
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use axum::Router;
    use axum::body::{Body, Bytes};
    use axum::extract::Path;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::{get, post};
    use futures::StreamExt;

    use super::*;

    fn error_body(code: &str, retryable: bool, retry_after_s: Option<u64>) -> String {
        serde_json::json!({ "error": {
            "code": code, "message": "", "retryable": retryable,
            "retry_after_s": retry_after_s, "request_id": "req_1"
        }})
        .to_string()
    }

    async fn complete(Path(id): Path<String>) -> (StatusCode, String) {
        match id.as_str() {
            "busy" => (
                StatusCode::TOO_MANY_REQUESTS,
                error_body("rate_limited", true, Some(7)),
            ),
            "bug" => (
                StatusCode::INTERNAL_SERVER_ERROR,
                error_body("internal", false, None),
            ),
            _ => (StatusCode::BAD_GATEWAY, String::new()), // a proxy's own answer
        }
    }

    async fn serve() -> Url {
        let app = Router::new()
            .route(
                "/internal/jobs/claim",
                post(|| async { StatusCode::NO_CONTENT }),
            )
            .route(
                "/internal/jobs/{id}/events",
                post(|headers: HeaderMap| async move {
                    assert_eq!(headers["x-lease-token"], "lease1");
                    assert_eq!(headers["authorization"], "Bearer worker-secret");
                    (StatusCode::CONFLICT, error_body("lease_lost", false, None))
                }),
            )
            .route("/internal/jobs/{id}/complete", post(complete))
            .route(
                "/internal/jobs/{id}/images/{image_id}",
                get(|| async { vec![0xff_u8, 0xd8] }),
            )
            .route("/bad-json", get(|| async { r#"{"cancel_job_ids": 7}"# }))
            .route(
                "/stalls",
                get(|| async {
                    // The headers and one byte, then nothing: the body never ends.
                    let first = futures::stream::once(async {
                        Ok::<_, Infallible>(Bytes::from_static(b"{"))
                    });
                    Body::from_stream(first.chain(futures::stream::pending()))
                }),
            )
            .route(
                "/slow",
                post(|| async {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    StatusCode::NO_CONTENT
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Url::parse(&format!("http://{addr}")).unwrap()
    }

    fn api(base: Url) -> HttpApi {
        HttpApi {
            client: build_client().unwrap(),
            base,
            worker_token: "worker-secret".into(),
        }
    }

    fn job(job_id: &str) -> Job {
        Job {
            v: 1,
            job_id: job_id.into(),
            kind: "perception".into(),
            scan_id: "scan1".into(),
            run: Some(1),
            attempt: 1,
            lease_token: "lease1".into(),
            lease_until: "2026-10-18T14:05:22Z".into(),
            max_runtime_s: 120,
        }
    }

    fn image(url: &str) -> JobImage {
        JobImage {
            image_id: "img1".into(),
            kind: "whole_plant".into(),
            width: 1024,
            height: 768,
            url: url.into(),
        }
    }

    #[tokio::test]
    async fn no_content_means_no_job() {
        let api = api(serve().await);
        let claimed = api.claim(&serde_json::json!({}), 25).await.unwrap();
        assert!(claimed.is_none());
    }

    #[tokio::test]
    async fn conflict_with_lease_lost_is_lease_lost() {
        let api = api(serve().await);
        let err = api
            .events(&job("job1"), &serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, CallError::LeaseLost), "{err}");
    }

    #[tokio::test]
    async fn a_retryable_error_keeps_its_retry_after() {
        let api = api(serve().await);
        let err = api.complete(&job("busy"), &serde_json::json!({})).await;
        let Err(CallError::Unavailable { retry_after, .. }) = err else {
            panic!("{err:?}");
        };
        assert_eq!(retry_after, Some(Duration::from_secs(7)));
    }

    #[tokio::test]
    async fn a_5xx_the_body_calls_not_retryable_is_rejected() {
        let api = api(serve().await);
        let err = api.complete(&job("bug"), &serde_json::json!({})).await;
        assert!(matches!(err, Err(CallError::Rejected(_))), "{err:?}");
    }

    #[tokio::test]
    async fn a_5xx_without_an_error_body_is_unavailable() {
        let api = api(serve().await);
        let err = api.complete(&job("proxy"), &serde_json::json!({})).await;
        let Err(CallError::Unavailable { retry_after, .. }) = err else {
            panic!("{err:?}");
        };
        assert_eq!(retry_after, None);
    }

    #[tokio::test]
    async fn a_bad_body_is_rejected_a_stalled_body_is_unavailable() {
        let base = serve().await;
        let client = build_client().unwrap();
        let resp = client
            .get(base.join("/bad-json").unwrap())
            .send()
            .await
            .unwrap();
        // `{"cancel_job_ids": 7}` is JSON, but not the list of strings asked for.
        let err = json::<Vec<String>>(resp).await.unwrap_err();
        assert!(matches!(err, CallError::Rejected(_)), "{err}");

        let resp = client
            .get(base.join("/stalls").unwrap())
            .timeout(Duration::from_secs(1))
            .send()
            .await
            .unwrap();
        let err = json::<serde_json::Value>(resp).await.unwrap_err();
        assert!(matches!(err, CallError::Unavailable { .. }), "{err}");
    }

    #[tokio::test]
    async fn images_come_from_the_payload_url_on_the_api_only() {
        let api = api(serve().await);
        let bytes = api
            .image(&job("job1"), &image("/internal/jobs/job1/images/img1"))
            .await
            .unwrap();
        assert_eq!(bytes, [0xff, 0xd8]);
        let elsewhere = image("https://example.com/internal/jobs/job1/images/img1");
        let err = api.image(&job("job1"), &elsewhere).await.unwrap_err();
        assert!(matches!(err, CallError::Rejected(_)), "{err}");
    }

    #[tokio::test]
    async fn read_timeout_also_bounds_the_wait_for_headers() {
        let base = serve().await;
        let client = Client::builder()
            .read_timeout(Duration::from_secs(1))
            .build()
            .unwrap();
        let err = client
            .post(base.join("/slow").unwrap())
            .send()
            .await
            .unwrap_err();
        assert!(err.is_timeout()); // a long poll would die the same way
    }

    #[tokio::test]
    async fn a_request_timeout_overrides_the_client_timeout() {
        let base = serve().await;
        let client = Client::builder()
            .timeout(Duration::from_secs(1))
            .build()
            .unwrap();
        let slow = client.post(base.join("/slow").unwrap());
        let resp = slow.timeout(Duration::from_secs(3)).send().await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }
}
```

## 4. Reading errors

- **`error_for_status` consumes the `Response`** and returns an error that keeps the URL, the status and the reason phrase (`reqwest-0.13.5/src/async_impl/response.rs`). The body is gone, and the body is the only place that says `lease_lost` rather than another `409` code, whether the error is `retryable`, and its `retry_after_s` ([contracts 2](../../../docs/architecture/contracts.md#2-errors)). So `check` never calls it: it decodes the `ErrorBody` of every non-2xx answer. [Response::error_for_status](https://docs.rs/reqwest/0.13.5/reqwest/struct.Response.html#method.error_for_status), [Response::status](https://docs.rs/reqwest/0.13.5/reqwest/struct.Response.html#method.status)
- `204` is a success status; the claim checks for it before decoding a `Job`. [contracts 5, claim](../../../docs/architecture/contracts.md#post-internaljobsclaim)
- **What each outcome means to the loop** ([loop.md](loop.md)):

  | Answer | `CallError` | The loop |
  |---|---|---|
  | `409` with `code: "lease_lost"` | `LeaseLost` | Drops the job, sends nothing more |
  | Any error body with `retryable: true` (a `429 rate_limited`, a `503`) | `Unavailable`, with `retry_after_s` when sent | Retries with backoff, and not before `retry_after_s` |
  | A `429` or `5xx` without an error body (a proxy's own answer), a connect error, a timeout, a body that couldn't be read | `Unavailable`, no `retry_after` | Retries with backoff |
  | Any other error body (`retryable: false`, a `500 internal` among them), or a success body that doesn't parse | `Rejected` | Logs it; a bug on one side, so not retried in a loop |

  Sources: `retryable` and `retry_after_s` are fields of every error body ([contracts 2](../../../docs/architecture/contracts.md#2-errors)); `429` asks the client to wait before retrying ([MDN: 429](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Status/429)). The tests above prove each row.
- The `ErrorBody` is decoded with only `code`, `retryable` and `retry_after_s`; serde ignores the other fields unless a type says `deny_unknown_fields`, and worker → API traffic stays lenient. [serde: deny_unknown_fields](https://serde.rs/container-attrs.html#deny_unknown_fields)
- **Images come from `JobImage.url`**, resolved against the API's base with `Url::join`, which takes a path and keeps a full URL as it is. A URL on another origin is refused, so the bearer and lease tokens never leave the API. [contracts 5, payloads](../../../docs/architecture/contracts.md#payloads), [Url::join](https://docs.rs/reqwest/0.13.5/reqwest/struct.Url.html#method.join), [Url::origin](https://docs.rs/reqwest/0.13.5/reqwest/struct.Url.html#method.origin)

## 5. The VLM sidecar

The vlm worker talks to vLLM or llama.cpp over their OpenAI-compatible API ([architecture 3](../../../docs/architecture/README.md#3-components)). Take the endpoint URL from configuration rather than building it here. The body (images first, then the prompt, with the per-job JSON Schema for structured output) is defined by [contracts 6](../../../docs/architecture/contracts.md#per-job-schema).

- **The call's timeout is what is left of the job, minus time for `complete`, capped.** A `vlm` job has 5 minutes for two calls ([contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)); the second call gets whatever the first left. When less than the reserve remains, don't start the call: fail the job as retryable instead.
- **Use `RequestBuilder::timeout`, not `read_timeout`**: without streaming, the sidecar sends nothing until the answer is complete.
- Dropping the future (the job's token was cancelled) abandons the request on the worker's side. Whether the sidecar then stops generating is *unverified*; check it on day 2 with the benchmark.

```rust
use std::time::Duration;

use tokio::time::Instant;

/// Time one VLM call may take: what is left before the job's soft deadline, minus what
/// `complete` needs, and never more than `cap`. `None` means: don't start the call.
pub fn call_budget(now: Instant, soft_deadline: Instant, cap: Duration) -> Option<Duration> {
    const RESERVE_FOR_COMPLETE: Duration = Duration::from_secs(5);
    let left = soft_deadline.saturating_duration_since(now);
    let budget = left.checked_sub(RESERVE_FOR_COMPLETE)?.min(cap);
    (!budget.is_zero()).then_some(budget)
}

/// One call to the sidecar's OpenAI-compatible chat endpoint. `body` carries the messages
/// and the per-job JSON Schema of contracts §6; building it is not this function's job.
pub async fn vlm_call(
    client: &reqwest::Client,
    chat_url: &str,
    body: &serde_json::Value,
    budget: Duration,
) -> reqwest::Result<serde_json::Value> {
    client
        .post(chat_url)
        .json(body)
        .timeout(budget) // the whole call, including the wait for the first byte
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
}

#[test]
fn the_budget_shrinks_to_the_deadline() {
    let now = Instant::now();
    let cap = Duration::from_secs(120);
    let in_secs = |s| now + Duration::from_secs(s);
    assert_eq!(call_budget(now, in_secs(285), cap), Some(cap));
    assert_eq!(
        call_budget(now, in_secs(65), cap),
        Some(Duration::from_secs(60))
    );
    assert_eq!(call_budget(now, in_secs(5), cap), None);
}
```
