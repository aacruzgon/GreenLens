---
name: rust-axum-api
description: "The PDM api binary on axum 0.8, tower-http 0.7, governor and utoipa 6: routing, State, extractors and rejections, ApiError to ErrorBody, Clerk and worker auth, SSE with resume, layers, CORS, body limits, timeouts, rate limits, shutdown, OpenAPI. Use for any route, handler or middleware."
---

# The `api` HTTP layer (axum)

This skill covers the `api` binary's HTTP surface: routes, handlers, extractors, middleware, the SSE endpoint and the OpenAPI spec, on axum 0.8.9, tower 0.5.3, tower-http 0.7.1, governor 0.10.4, utoipa 6.0.0 and utoipa-axum 0.3.0 ([versions](../rust/versions.md)). **The project documents win** over this skill and over any crate default: every shape and status comes from [contracts 1](../../../docs/architecture/contracts.md#1-conventions) (wire format, auth headers, idempotency, roles), [contracts 2](../../../docs/architecture/contracts.md#2-errors) (`ErrorBody` and the codes), [contracts 4](../../../docs/architecture/contracts.md#4-scan-events-sse) (SSE), [contracts 5](../../../docs/architecture/contracts.md#5-worker-api-and-jobs) (the worker API), [architecture 9](../../../docs/architecture/README.md#9-api-surface), [11](../../../docs/architecture/README.md#11-security-privacy-and-safety) and [12](../../../docs/architecture/README.md#12-deployment), and [frontend 8](../../../docs/frontend/README.md#8-authentication-clerk), [9](../../../docs/frontend/README.md#9-api-layer), [10](../../../docs/frontend/README.md#10-live-streams) and [17](../../../docs/frontend/README.md#17-builds-environments-and-delivery). Where this skill restates a shape, it is copied from there.

Every example in this skill and its reference files is compiled, linted and, where it holds tests, run by `python3 .claude/skills/rust/scripts/check_examples.py --skill rust-axum-api`.

| Topic | Reference |
|---|---|
| `ApiError` → `ErrorBody`, `request_id`, rejections, panics, body limits, 404 vs 403 | [error-responses.md](error-responses.md) |
| Clerk session tokens, worker tokens and `X-Lease-Token`, the Svix webhook | [auth.md](auth.md) |
| `GET /scans/{id}/events`: subscribe, replay, live, lag, keepalive, close | [sse.md](sse.md) |
| Layer order, CORS, timeouts, rate limits, graceful shutdown | [layers.md](layers.md) |
| `OpenApiRouter`, `routes!`, `#[utoipa::path]`, `backend/openapi.json` | [openapi.md](openapi.md) |

## Routing and state

- **State is `State<Arc<AppState>>`.** One `Surreal<Any>`, one `aws_sdk_s3::Client`, one `reqwest::Client` per process, built at start-up and shared through the `Arc` ([database 6](../../../docs/database/README.md#6-rust-sdk): don't clone `Surreal<Any>` per request; in 3.x a clone opens a new session; [storage 5](../../../docs/storage/README.md#5-rust-client) for the S3 client; [`reqwest::Client`](https://docs.rs/reqwest/0.13.5/reqwest/struct.Client.html) holds a connection pool, so create one and reuse it). Attach it once with `Router::with_state` ([`State`](https://docs.rs/axum/0.8.9/axum/extract/struct.State.html)).
- **Paths use `{name}`.** axum 0.8 replaced `/:id` and `/*rest` with `/{id}` and `/{*rest}`; the old syntax panics when the route is added (the test below; [CHANGELOG 0.8.0](https://docs.rs/crate/axum/0.8.9/source/CHANGELOG.md)). The route string has no types: a capture is text, and the extractor's type parses it (`ApiPath<ScanId>`, [route captures](https://docs.rs/axum/0.8.9/axum/struct.Router.html#captures); [error-responses 3](error-responses.md#3-extractor-rejections)).
- **No version prefix**: REST paths are as in architecture 9, and changes are additive ([contracts 1, versioning](../../../docs/architecture/contracts.md#versioning)).
- **Route groups** (app, worker, webhook, ops) are separate `Router`s with their own layers, merged at the end ([auth 4](auth.md#4-which-routes-get-which-check), [layers 2](layers.md#2-the-stack-in-order)).
- **A fallback** returns `ApiError::NotFound`, so an unknown path also gets an `ErrorBody`, and the finished router is wrapped in `without_allow`, so a wrong method is a `404` without `Allow` ([error-responses 2](error-responses.md#2-the-compiled-example)).

The example below shows the state and one route. `healthz`'s failure is a stand-in (`Unhealthy`) to keep the block short; in the `api` crate it is `ApiError::UpstreamUnavailable { upstream: "database", .. }`, rendered as in [error-responses 2](error-responses.md#2-the-compiled-example). `GET /healthz` answers `200 { ok: true }` when the API can reach SurrealDB and RustFS ([contracts 9, ops](../../../docs/architecture/contracts.md#ops)).

```rust
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::{Value, json};
use surrealdb::{Surreal, engine::any::Any};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

pub struct AppState {
    pub db: Surreal<Any>,
    pub s3: aws_sdk_s3::Client,
    pub http: reqwest::Client,
    pub shutdown: CancellationToken,
    pub tasks: TaskTracker,
}

/// Stand-in for `ApiError::UpstreamUnavailable` (error-responses.md has the real one).
pub struct Unhealthy(pub &'static str);

impl IntoResponse for Unhealthy {
    fn into_response(self) -> Response {
        StatusCode::SERVICE_UNAVAILABLE.into_response()
    }
}

/// `GET /healthz` (contracts 9, ops). The object-store check is the `blob` crate's.
async fn healthz(State(state): State<Arc<AppState>>) -> Result<Json<Value>, Unhealthy> {
    state.db.health().await.map_err(|_| Unhealthy("database"))?;
    Ok(Json(json!({ "ok": true })))
}

pub fn ops_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .with_state(state)
}

#[test]
#[should_panic(expected = "Path segments must not start with `:`")]
fn axum_0_8_rejects_the_old_capture_syntax() {
    let _ = Router::<()>::new().route("/scans/:id", get(|| async {}));
}
```

## Handlers and extractors

- **The body extractor is last**, and there is only one; everything before it implements `FromRequestParts` ([the order of extractors](https://docs.rs/axum/0.8.9/axum/extract/index.html#the-order-of-extractors)). Putting a body extractor earlier doesn't compile:

```rust,compile_fail,E0277
use axum::{Router, extract::Path, routing::post};

async fn answer(body: String, Path(id): Path<String>) -> String {
    format!("{id}: {body}")
}

fn main() {
    let _: Router = Router::new().route("/scans/{id}/answers", post(answer));
}
```

- **Never take `axum::Json`, `Query` or `Path` directly.** Their rejections are plain text with axum's statuses (a JSON body of the wrong shape is `422`). Take `ApiJson`, `ApiQuery` and `ApiPath`, whose rejection is `ApiError`, so the client gets `400 invalid_request` with `{ field, problem }` ([error-responses 3](error-responses.md#3-extractor-rejections)).
- **Return `Result<T, ApiError>`** from every handler. Never `(StatusCode, String)`, `anyhow::Error` or a bare `StatusCode` for an error ([rust-errors](../rust-errors/SKILL.md)).
- **IDs are typed** (`ApiPath<ScanId>`, the newtypes of [rust-contract-types](../rust-contract-types/primitives.md#1-the-code)) and travel as strings without a table prefix ([contracts 1, wire format](../../../docs/architecture/contracts.md#wire-format)). A value that isn't a ULID fails extraction with `400 invalid_request` naming the capture; a well-formed ID that names no record, or one the caller may not see, is `404` ([contracts 2](../../../docs/architecture/contracts.md#2-errors)). **Not in the contracts yet:** contracts 2 doesn't say whether a malformed ID is `400` or `404` ([contracts 12, question 14](../../../docs/architecture/contracts.md#12-open-questions)).
- **App request bodies reject unknown fields** (`#[serde(deny_unknown_fields)]`), so a misspelt field is `400 invalid_request` naming it; worker request bodies stay lenient ([error-responses 3](error-responses.md#3-extractor-rejections)). **Not in the contracts yet:** contracts 2 doesn't list unknown fields as `invalid_request` ([contracts 12, question 13](../../../docs/architecture/contracts.md#12-open-questions)).
- **Headers** (`Idempotency-Key`, `Last-Event-ID`, `X-Lease-Token`) come from `HeaderMap` and are parsed by hand; a bad value is `invalid_request` with the header's name as `field` ([error-responses 3](error-responses.md#3-extractor-rejections)). `TypedHeader` moved to `axum-extra` in axum 0.7 ([CHANGELOG 0.7.0](https://docs.rs/crate/axum/0.8.9/source/CHANGELOG.md)), which isn't pinned ([versions](../rust/versions.md)).
- **Uploads** (`multipart/form-data`) use `Multipart` with a per-route `DefaultBodyLimit` and per-part caps: the photo at 12 MiB (`upload_too_large`), the depth map at 4 MiB (dropped, never failing the upload) ([error-responses 4](error-responses.md#4-body-limits), [contracts 3](../../../docs/architecture/contracts.md#post-scansidimages)).
- **Idempotency** (`Idempotency-Key` on the create calls, the stored replay, `409 request_in_progress`, `422 idempotency_conflict`) follows [contracts 1, idempotency](../../../docs/architecture/contracts.md#idempotency) with the key record of [database 4](../../../docs/database/README.md#idempotency-keys-). The multipart fingerprint hashes the fields and file bytes, never the raw body.

## Errors

- `ApiError` lives in the `api` crate; each variant maps to one contracts 2 code and status. The variants and the full mapping table are in [rust-errors](../rust-errors/SKILL.md#the-mapping-table).
- `IntoResponse for ApiError` leaves the parts on the response; one `render_errors` middleware writes the `ErrorBody` with the `request_id` taken from the request's `RequestId`. The same middleware gives axum's and tower-http's own error responses a contract body ([error-responses 1](error-responses.md#1-the-design-one-renderer)).
- A panic becomes `500 internal` with an `ErrorBody` through `CatchPanicLayer::custom(panic_response)`. It is still a bug ([error-responses 2](error-responses.md#2-the-compiled-example), [rust-errors](../rust-errors/SKILL.md#panics-and-expect)).
- Non-members get `404`, never `403` ([error-responses 5](error-responses.md#5-404-not-403)).
- Worker job failures are not HTTP errors: they travel in the `complete` body ([rust-errors](../rust-errors/SKILL.md#worker-job-errors)).

## Auth, SSE, layers, OpenAPI

- **Clerk**: verify against the JWKS with `jsonwebtoken` 11, requiring `exp`, `nbf`, `iss`, `sub`, turning on `validate_nbf`, and checking `azp` against the web origins when present. Everything fails as `401 unauthenticated` ([auth 1](auth.md#1-clerk-session-tokens)).
- **Workers**: a bearer token per worker, hashed with SHA-256 and compared in constant time against every configured token; `X-Lease-Token` checked by the store inside its write, mismatch `409 lease_lost` ([auth 2](auth.md#2-workers-bearer-token-and-x-lease-token)).
- **SSE**: subscribe, replay after `Last-Event-ID`, forward live dropping sent `seq`s, replay again on `Lagged` or a gap, `: keepalive` after 15 idle seconds, end at the resting point or on shutdown ([sse](sse.md)).
- **Layers**, top of the `ServiceBuilder` first: drop a client's `x-request-id`, set `req_<ULID>`, propagate it, trace, CORS, error renderer, catch panic, body limit; then per-group auth, rate limit, timeout; `without_allow` around the finished router ([layers 2](layers.md#2-the-stack-in-order)).
- **CORS** allows the web origins and exactly `Authorization`, `Content-Type`, `Idempotency-Key`, `Last-Event-ID` ([layers 3](layers.md#3-cors)).
- **Timeouts** bound the response head only, so SSE bodies are safe; the claim group gets 35 s for its 30 s long poll. The layer answers `504`, which the renderer turns into `500 internal` ([layers 4](layers.md#4-timeouts)). **Not in the contracts yet:** contracts 2 has no timeout code, and `internal` is `retryable: false` ([contracts 12, questions 16 and 17](../../../docs/architecture/contracts.md#12-open-questions)).
- **Rate limits**: governor keyed by user; invites at most 10 a minute per user (a burst of 5, then one every 12 s; `Quota::per_minute(10)` would let 19 through); `429 rate_limited`, `retryable: true`, `retry_after_s` rounded up ([layers 5](layers.md#5-rate-limits)).
- **Shutdown**: `with_graceful_shutdown(token.cancelled_owned())` waits for open connections, so every stream must end on the token ([layers 6](layers.md#6-graceful-shutdown), [rust-async shutdown 5](../rust-async/shutdown.md#5-api)).
- **OpenAPI**: public routes through `OpenApiRouter` and `routes!`, error statuses with `body = ErrorBody`, OpenAPI 3.1, written to `backend/openapi.json` by `api openapi` ([openapi](openapi.md)).

## Review checklist

- [ ] Every handler returns `Result<_, ApiError>`; no `axum::Json`/`Query`/`Path` extractor, no plain-text error ([handlers and extractors](#handlers-and-extractors)).
- [ ] Body extractor last; `State<Arc<AppState>>`, not a cloned client per request.
- [ ] Paths use `{id}`; IDs are typed (`ApiPath<ScanId>`): a malformed one is `400 invalid_request`, a well-formed unknown or hidden one `404` ([handlers and extractors](#handlers-and-extractors)).
- [ ] Garden-scoped reads return the same `404` for unknown and not-a-member; the role check comes after membership.
- [ ] App request types have `deny_unknown_fields`; worker request types don't.
- [ ] Error bodies match contracts 2: `retry_after_s` only with `request_in_progress`, `rate_limited` and the 503 codes; `details` only as the table lists.
- [ ] New routes are in the right group (app, worker, webhook, ops) and, if public, registered through `OpenApiRouter` with their error responses.
- [ ] Uploads have a route `DefaultBodyLimit` and per-part caps; depth problems never fail the upload.
- [ ] SSE: subscribe before replay, dedupe by `seq` (a duplicate resting event still closes), `Lagged`, a gap and `Closed` → replay, keepalive `"keepalive"` at 15 s, ends at rest, on shutdown and on a store error, no body timeout layer ([sse](sse.md)).
- [ ] Long polls and streams watch the shutdown token ([layers 6](layers.md#6-graceful-shutdown)).
- [ ] CORS lists exactly the four request headers; no `Any` origin ([layers 3](layers.md#3-cors)).
- [ ] No `unwrap()` outside tests; `expect` says why it can't fail ([rust-errors](../rust-errors/SKILL.md#panics-and-expect)).

## Sibling skills

[rust](../rust/SKILL.md) (entry, versions, checker) · [rust-errors](../rust-errors/SKILL.md) (the error design and the code table) · [rust-tracing](../rust-tracing/SKILL.md) (request IDs, `TraceLayer`, JSON logs) · [rust-async](../rust-async/SKILL.md) (cancellation, `TaskTracker`, broadcast) · [rust-contract-types](../rust-contract-types/SKILL.md) (the `domain` types and their serde and schema derives) · [rust-surrealdb](../rust-surrealdb/SKILL.md) (store queries, the event replay read) · [rust-workers](../rust-workers/SKILL.md) (the worker side of the job API) · [rust-testing](../rust-testing/SKILL.md) · [rust-cargo-lints](../rust-cargo-lints/SKILL.md).
