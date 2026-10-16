---
name: rust-tracing
description: "Logging for the PDM backend: the JSON tracing subscriber and RUST_LOG, #[instrument] and spans, req_<ULID> request IDs on the request span, spans across spawn, error chains, worker job spans, and what must never be logged. Use when adding logs or spans, or reviewing them."
---

# Tracing and logs

Both binaries log with `tracing`, as JSON on stdout, with no exporter ([architecture 12, operations](../../../docs/architecture/README.md#operations)). **The project documents win**: what may not appear in a log follows from [architecture 11, security, privacy and safety](../../../docs/architecture/README.md#11-security-privacy-and-safety) and the credentials in [contracts 1, authentication headers](../../../docs/architecture/contracts.md#authentication-headers); the request ID's shape from [contracts 2](../../../docs/architecture/contracts.md#2-errors); the job fields from [contracts 5](../../../docs/architecture/contracts.md#5-worker-api-and-jobs). Report any disagreement rather than resolving it silently.

Versions: tracing 0.1.44, tracing-subscriber 0.3.23 (`env-filter`, `json`), tower-http 0.7.1 (`trace`, `request-id`) ([versions.md](../rust/versions.md)). The proofs are in two reference files:

| File | What it holds |
|---|---|
| [json-logs.md](json-logs.md) | Subscriber setup, the shape of a line, error chains, spans across `spawn` and `spawn_blocking`, secret types |
| [request-id.md](request-id.md) | The layer stack, `req_<ULID>`, the request span and its test |

## 1. One subscriber, set once in `main`

- Call `init_tracing()` first in `main`, in `api` and `worker` ([json-logs.md 1](json-logs.md#1-setup-in-main)). It builds `fmt().json().flatten_event(true).with_current_span(false).with_span_list(true)` with an `EnvFilter` from `RUST_LOG`, default `info`, and fails start-up on an invalid `RUST_LOG`.
- A line is one JSON object: `timestamp` (RFC 3339 UTC, microseconds), `level`, the event's fields and `message` at the top level, `target`, and `spans`, a list of every entered span's fields, root first ([json-logs.md 2](json-logs.md#2-what-a-line-looks-like)). The request span is the root, so its `request_id` is on every line of the request.
- Never `println!` or `eprintln!` (the lints `print_stdout`, `print_stderr` are on, [rust-cargo-lints](../rust-cargo-lints/SKILL.md)).
- Record numbers as `u64`/`i64`/`f64`: a `u128` (from `Duration::as_millis`) is written as a JSON string ([json-logs.md 2](json-logs.md#2-what-a-line-looks-like)).
- A span below the filter's level is disabled and records nothing ([`Span::is_disabled`](https://docs.rs/tracing/0.1.44/tracing/struct.Span.html#method.is_disabled); [`EnvFilter` directives](https://docs.rs/tracing-subscriber/0.3.23/tracing_subscriber/filter/struct.EnvFilter.html#directives)), so spans that carry IDs are `INFO`, and **the filter must keep `info`** for `api` and `worker`. Under `RUST_LOG=warn` the request span is disabled, and an `ERROR` line inside it has no `spans` and no `request_id`. Quiet a noisy crate with `RUST_LOG=info,tower_http=warn`, never by lowering the default. Error events that must be found by request also record `request_id` as their own field, as `render_errors` does ([rust-axum-api](../rust-axum-api/error-responses.md#1-the-design-one-renderer)). Both are proved in [json-logs.md 1](json-logs.md#1-setup-in-main).
- A directive list replaces the default: `RUST_LOG=api=debug` alone turns every other target off; write `RUST_LOG=info,api=debug` ([json-logs.md 1](json-logs.md#1-setup-in-main)).

## 2. `#[instrument]` and spans

- `#[instrument]` makes an `INFO` span named after the function and **records every argument with `Debug`** unless skipped ([`#[instrument]`](https://docs.rs/tracing/0.1.44/tracing/attr.instrument.html)). Request bodies, `State<Arc<AppState>>`, tokens and image bytes would all be printed. So always write `#[instrument(skip_all, fields(...))]` and name the fields you want.
- Record IDs with `%` (their `Display`, the bare ULID): `fields(scan_id = %scan_id)`. Declare fields known later as `field::Empty` and fill them with `Span::current().record(...)`; a field not declared at creation can't be added ([`Span::record`](https://docs.rs/tracing/0.1.44/tracing/struct.Span.html#method.record)).
- `err` logs the `Err` with its `Display`, at `ERROR` by default, and only the outer message ([`#[instrument]`, `err`](https://docs.rs/tracing/0.1.44/tracing/attr.instrument.html); [json-logs.md 3](json-logs.md#3-logging-errors-with-their-chain)). Use it only on a function that is the last to see the error; elsewhere the caller logs it, once ([rust-errors, display and source](../rust-errors/SKILL.md#display-and-source)).
- Don't use `ret`: it logs the `Ok` value with `Debug` at the span's level ([`#[instrument]`, `ret`](https://docs.rs/tracing/0.1.44/tracing/attr.instrument.html)), which for a handler is the response body.
- Field names are `snake_case`, the same words as the contracts (`scan_id`, `job_id`, `run`; [contracts 1, wire format](../../../docs/architecture/contracts.md#wire-format)), so a log search uses the names the API sends. HTTP fields use the OpenTelemetry names ([request-id.md 3](request-id.md#3-notes)).

```rust
use std::sync::Arc;

use axum::extract::{Path, State};
use tracing::field::Empty;
use tracing::instrument;

pub struct AppState;

#[derive(serde::Deserialize)]
pub struct AnswerRequest {
    pub question_id: String,
    pub option_id: String,
}

#[instrument(skip_all, fields(scan_id = %scan_id, garden_id = Empty))]
pub async fn answer(
    State(_state): State<Arc<AppState>>,
    Path(scan_id): Path<String>,
    axum::Json(body): axum::Json<AnswerRequest>,
) -> &'static str {
    // Looked up from the scan; recorded once known.
    tracing::Span::current().record("garden_id", "01JAX2GGGGGGGGGGGGGGGGGGGG");
    tracing::info!(question_id = %body.question_id, "answer stored");
    "ok"
}
```

## 3. Never in a log

[Architecture 11](../../../docs/architecture/README.md#11-security-privacy-and-safety) keeps images, locations and private chats to their owners and keeps images behind signed URLs; the tokens in [contracts 1](../../../docs/architecture/contracts.md#authentication-headers) are credentials. None of these may reach a log line, a span field or an error message that is logged.

| Never log | Why | How it stays out |
|---|---|---|
| Request and response bodies | They hold everything below | `skip_all`; no `ret`; no `?body` |
| `Authorization` (the Clerk JWT, the worker token), `X-Lease-Token`, Svix signatures | Credentials | No `DefaultMakeSpan::include_headers(true)`; token types with a redacting `Debug` and no `Display` ([json-logs.md 5](json-logs.md#5-secrets-cant-be-logged-by-accident)) |
| Signed URLs, their `sig` and `exp` | A URL is a 15-minute credential ([contracts 1](../../../docs/architecture/contracts.md#wire-format)) | The request span records `http.route`, never `uri()` ([request-id.md 2](request-id.md#2-the-code-and-its-test)); log image IDs, not URLs |
| Image and depth-map bytes, masks | Photos may show people; depth maps are never served ([architecture 11](../../../docs/architecture/README.md#11-security-privacy-and-safety)) | Log the image ID, `kind`, `width`, `height`, byte count |
| Embeddings and model inputs | Large and derived from the photos | Log the model name and vector length |
| Location: a garden's centroid, postal code or city | Location requires consent and is stored at reduced precision | Log the `garden_id` |
| Chat messages, memories, plan text written for a user | Memories are private to their author until shared | Log IDs, token counts, the route |
| Display names and the auth provider's user ID | Personal data | Log the user's ULID |
| Server secrets: the SurrealDB root and `api` passwords, the RustFS access and secret keys, the URL-signing secret, the LLM API key, Clerk's secret key, the Svix webhook signing secret, worker tokens | Each opens every garden's data or lets a caller pose as the API, a worker or Clerk ([AGENTS.md, secrets](../../../AGENTS.md#secrets); [database 1, users](../../../docs/database/README.md#1-version-and-setup); [storage 1](../../../docs/storage/README.md#1-version-and-setup) and [3](../../../docs/storage/README.md#3-how-images-are-served); [architecture 11, infrastructure](../../../docs/architecture/README.md#11-security-privacy-and-safety)) | Read into secret types with a redacting `Debug` and no `Display` ([json-logs.md 5](json-logs.md#5-secrets-cant-be-logged-by-accident)); never log the config struct or the environment; `skip_all` on start-up functions that take them |
| A `reqwest::Error`'s URL | Its `Display` appends ` for url (…)` with the whole URL, query string included (`reqwest-0.13.5/src/error.rs`), so a signed `?exp=&sig=` URL or a key in a query string would be logged with the error | Log `error.without_url()` ([`Error::without_url`](https://docs.rs/reqwest/0.13.5/reqwest/struct.Error.html#method.without_url)) and the route as its own field ([json-logs.md 5](json-logs.md#5-secrets-cant-be-logged-by-accident)) |

## 4. Async: spawned tasks and span guards

- A task from `tokio::spawn` doesn't inherit the current span. Spawn `fut.instrument(span)` or `fut.in_current_span()`; for `spawn_blocking`, capture `Span::current()` and call `span.in_scope(...)` inside the closure. The test in [json-logs.md 4](json-logs.md#4-spans-across-tokiospawn-and-spawn_blocking) shows a plain spawn losing the span ([`Instrument`](https://docs.rs/tracing/0.1.44/tracing/trait.Instrument.html)).
- Never hold `span.enter()` across `.await` ([`Span::enter`, in asynchronous code](https://docs.rs/tracing/0.1.44/tracing/struct.Span.html#in-asynchronous-code)). Clippy's `await_holding_invalid_type` can enforce it with two lines of `clippy.toml` ([json-logs.md 4](json-logs.md#4-spans-across-tokiospawn-and-spawn_blocking)).

## 5. Request IDs

The request span is made by `TraceLayer` inside the request-ID layers, all added with `Router::layer` so `MatchedPath` is known ([request-id.md 1](request-id.md#1-the-layer-stack)):

1. an incoming `x-request-id` is removed (tower-http would otherwise keep the client's value);
2. `SetRequestIdLayer` makes `req_<ULID>` and stores `RequestId` in the extensions;
3. `PropagateRequestIdLayer` copies it to the response;
4. `TraceLayer::make_span_with` records `request_id`, `http.request.method`, `http.route`, and on response `http.response.status_code` and `latency_ms`.

The test in [request-id.md 2](request-id.md#2-the-code-and-its-test) checks that the response header and every log line carry the same ID, that a client-sent ID is replaced, and that the query string never appears. [rust-axum-api](../rust-axum-api/error-responses.md) puts the same `RequestId` into `ErrorBody.error.request_id`.

## 6. Errors

[rust-errors](../rust-errors/SKILL.md) owns the error types and the rule that a wrapped error is in `source()` or in `Display`, never both. When logging:

- an `anyhow::Error`: `error = format!("{err:#}")`, the whole chain on one line;
- a typed error: `error = %Chain(&err)`, a small adapter that walks `source()`;
- not `%err` or `&err as &dyn Error` alone: the JSON formatter writes only the outer message;
- a `reqwest::Error` after `without_url()` ([never in a log](#3-never-in-a-log));
- once, where the error is handled ([rust-errors, display and source](../rust-errors/SKILL.md#display-and-source)).

The forms are compared in [json-logs.md 3](json-logs.md#3-logging-errors-with-their-chain). `Chain` is the canonical adapter of [rust-errors](../rust-errors/SKILL.md#display-and-source); blocks here carry exact copies.

## 7. Worker job spans

Each job runs in one span with the fields of the claim response ([contracts 5, claim](../../../docs/architecture/contracts.md#post-internaljobsclaim)): `job_id`, `kind`, `scan_id`, `run` (absent for `guided` jobs; an `Option` field is recorded only when `Some`, [`Value` for `Option<T>`](https://docs.rs/tracing/0.1.44/tracing/trait.Value.html#impl-Value-for-Option%3CT%3E)) and `attempt`. Never the `lease_token`. Everything the job spawns is instrumented with it, so every line of the job, from the VLM call to `complete`, can be found by `job_id` ([rust-workers](../rust-workers/SKILL.md)).

```rust
use tracing::Instrument as _;

#[derive(Clone, Copy, Debug)]
pub enum JobKind {
    Perception,
    Vlm,
    Verify,
    Guided,
}

impl JobKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Perception => "perception",
            Self::Vlm => "vlm",
            Self::Verify => "verify",
            Self::Guided => "guided",
        }
    }
}

pub struct Job {
    pub job_id: String,
    pub kind: JobKind,
    pub scan_id: String,
    pub run: Option<u32>,
    pub attempt: u32,
    pub lease_token: String,
}

fn infer(image_count: usize) -> usize {
    image_count * 3
}

#[derive(Debug, thiserror::Error)]
#[error("complete was refused")]
pub struct CompleteError;

/// Stand-in for `POST /internal/jobs/{id}/complete`, which sends the token as `X-Lease-Token`.
async fn complete(lease_token: &str) -> Result<(), CompleteError> {
    tokio::task::yield_now().await;
    if lease_token.is_empty() {
        Err(CompleteError)
    } else {
        Ok(())
    }
}

pub async fn run_job(job: Job) {
    let span = tracing::info_span!(
        "job",
        job_id = %job.job_id,
        kind = job.kind.as_str(),
        scan_id = %job.scan_id,
        run = job.run,
        attempt = job.attempt,
    );
    async move {
        tracing::info!("started");
        let span = tracing::Span::current();
        let regions = tokio::task::spawn_blocking(move || span.in_scope(|| infer(4))).await;
        match regions {
            Ok(n) => tracing::info!(regions = n, "perception done"),
            Err(err) => tracing::error!(error = %err, "inference task failed"),
        }
        // The token is sent as `X-Lease-Token`, never recorded as a field.
        if let Err(error) = complete(&job.lease_token).await {
            tracing::warn!(error = %error, "complete failed");
        }
    }
    .instrument(span)
    .await;
}
```

## Review checklist

- [ ] `init_tracing` is the first call in each `main`; no other subscriber is set; no `println!`; `RUST_LOG` keeps `info` ([section 1](#1-one-subscriber-set-once-in-main)).
- [ ] Every `#[instrument]` has `skip_all` and explicit `fields(...)`; no `ret`; `err` only where the error ends.
- [ ] Nothing from [the never-log table](#3-never-in-a-log) in any field, message or logged error text; token and secret types have a redacting `Debug` and no `Display`; `reqwest::Error`s are logged `without_url()`.
- [ ] The request span records `http.route`, not `uri()`; the request-ID layers are in the order of [request-id.md 1](request-id.md#1-the-layer-stack).
- [ ] Every `tokio::spawn` and `TaskTracker::spawn` of request or job work carries a span; `spawn_blocking` uses `in_scope`; no `enter()` guard across `.await`.
- [ ] Errors logged once, with the chain (`{:#}` or `Chain`).
- [ ] Numeric fields are `u64`/`i64`/`f64`, not `u128`.
- [ ] Worker jobs run in a `job` span with `job_id`, `kind`, `scan_id`, `run`, `attempt`.

## Related skills

- [rust](../rust/SKILL.md): versions and the example checker.
- [rust-axum-api](../rust-axum-api/SKILL.md): the rest of the layer stack and `request_id` in error bodies.
- [rust-errors](../rust-errors/SKILL.md): error types, `source()` and `Display`.
- [rust-async](../rust-async/SKILL.md): `TaskTracker`, cancellation and `spawn_blocking`.
- [rust-workers](../rust-workers/SKILL.md): the job loop that runs inside the job span.
- [rust-contract-types](../rust-contract-types/SKILL.md): the ID and token types that are logged or redacted.
- [rust-cargo-lints](../rust-cargo-lints/SKILL.md): `clippy.toml`, where the `await-holding-invalid-types` list would go.
