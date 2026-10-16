---
name: rust-axum-api
description: "The PDM api binary on axum 0.8, tower-http 0.7, governor and utoipa 6: routing, State, extractors and rejections, ApiError to ErrorBody, Clerk and worker auth, SSE with resume, layers, CORS, body limits, timeouts, rate limits, shutdown, OpenAPI. Use for any route, handler or middleware."
---

# The `api` HTTP layer (axum)

This skill covers the `api` binary's HTTP surface: routes, handlers, extractors, middleware, the SSE endpoint and the OpenAPI spec, on axum 0.8.9, tower 0.5.3, tower-http 0.7.1, governor 0.10.4, utoipa 6.0.0 and utoipa-axum 0.3.0 ([versions](../rust/versions.md)). **The project documents win** over this skill and over any crate default: every shape and status comes from [contracts 1](../../../docs/architecture/contracts.md#1-conventions) (wire format, auth headers, idempotency, roles), [contracts 2](../../../docs/architecture/contracts.md#2-errors) (`ErrorBody` and the codes), [contracts 4](../../../docs/architecture/contracts.md#4-scan-events-sse) (SSE), [contracts 5](../../../docs/architecture/contracts.md#5-worker-api-and-jobs) (the worker API), [architecture 9](../../../docs/architecture/README.md#9-api-surface), [11](../../../docs/architecture/README.md#11-security-privacy-and-safety) and [12](../../../docs/architecture/README.md#12-deployment), and [frontend 8](../../../docs/frontend/README.md#8-authentication-clerk), [9](../../../docs/frontend/README.md#9-api-layer), [10](../../../docs/frontend/README.md#10-live-streams) and [17](../../../docs/frontend/README.md#17-builds-environments-and-delivery). Where this skill restates a shape, it is copied from there. How the `api` binary is actually built is [backend api](../../../docs/backend/api.md); this skill follows it, and where the two disagree the document wins ([backend api 1](../../../docs/backend/api.md#1-purpose-and-boundaries), "The project skills").

Every example in this skill and its reference files is compiled, linted and, where it holds tests, run by `python3 .claude/skills/rust/scripts/check_examples.py --skill rust-axum-api`.

| Topic | Reference |
|---|---|
| `ApiError` → `ErrorBody`, `request_id`, rejections, panics, body limits, 404 vs 403 | [error-responses.md](error-responses.md) |
| Clerk session tokens, the access extractors and their `Membership<R>` proofs, worker tokens and `X-Lease-Token`, the Svix webhook | [auth.md](auth.md) |
| `GET /scans/{id}/events`: subscribe, replay, live, lag, keepalive, close | [sse.md](sse.md) |
| Layer order, CORS, compression, timeouts, rate limits, graceful shutdown | [layers.md](layers.md) |
| `OpenApiRouter`, `routes!`, `#[utoipa::path]`, `backend/openapi.json` | [openapi.md](openapi.md) |

## Routing and state

- **State is `State<Arc<AppState>>`, and `AppState` holds ports, not clients.** Every dependency is an `Arc<dyn Port>` from `domain::ports`, grouped in `Ports` (the store, memory, knowledge, the blob store, the auth and webhook verifiers, weather, soil, the geocoder, the notifier, the text embedder, the URL signer, species photos, the clock and ID generator). Beside them sit the three **seams**, traits `api` defines over the library crates' entry points so a test can swap them like a port (`PipelineApi` for `pipeline::Pipeline`, `ChatTurns` for `agent::ChatAgent`, `ImageProcessing` for the upload pipeline), the configuration, and `api`'s own in-process pieces: the event hub, the scan writers, the job broker, the rate limiters, the upload permits, the worker tokens, the deletion runner, the `DeleteQueue`, the shutdown token and the `TaskTracker`. Never a `Surreal<Any>` or an `aws_sdk_s3::Client`: SQL lives only in `store`, S3 calls only in `blob`, outbound HTTP only in `integrations`, and only `wiring.rs` names a concrete adapter ([backend README 5](../../../docs/backend/README.md#5-crate-graph-and-dependency-rules); [backend api 6](../../../docs/backend/api.md#6-application-state-wiring-and-the-seams)). The adapters keep the one-per-process rules inside them: one `Surreal<Any>` ([database 6](../../../docs/database/README.md#6-rust-sdk): in 3.x a clone opens a new session), one S3 client ([storage 5](../../../docs/storage/README.md#5-rust-client)), one `reqwest::Client` shared by every `integrations` adapter ([`reqwest::Client`](https://docs.rs/reqwest/0.13.5/reqwest/struct.Client.html) holds a connection pool). The ports use `#[async_trait]` so they stay dyn compatible ([rust-async traits 3](../rust-async/traits.md#3-what-the-project-does-with-its-boundary-traits)). Attach the state once with `Router::with_state` ([`State`](https://docs.rs/axum/0.8.9/axum/extract/struct.State.html)).
- **Paths use `{name}`.** axum 0.8 replaced `/:id` and `/*rest` with `/{id}` and `/{*rest}`; the old syntax panics when the route is added (the test below; [CHANGELOG 0.8.0](https://docs.rs/crate/axum/0.8.9/source/CHANGELOG.md)). The route string has no types: a capture is text, and the extractor's type parses it (`ApiPath<ScanId>`, [route captures](https://docs.rs/axum/0.8.9/axum/struct.Router.html#captures); [error-responses 3](error-responses.md#3-extractor-rejections)).
- **No version prefix**: REST paths are as in architecture 9, and changes are additive ([contracts 1, versioning](../../../docs/architecture/contracts.md#versioning)).
- **Route groups** (app, uploads, claim, worker) are separate routers, each with its own `TimeoutLayer` and `DefaultBodyLimit`, merged at the end; auth is not a group layer but an extractor on each handler ([layers 2](layers.md#2-the-stack-in-order), [auth 4](auth.md#4-which-routes-get-which-check); [backend api 7](../../../docs/backend/api.md#7-the-router-and-the-layer-stack)).
- **A fallback** returns `ApiError::NotFound`, so an unknown path also gets an `ErrorBody`, and the finished router is wrapped in `without_allow`, so a wrong method is a `404` without `Allow` ([error-responses 2](error-responses.md#2-the-compiled-example)).

The example below shows the state and one route. The two ports are cut down to one method each (the real ones are `domain::ports::Store` and `BlobStore`), and `healthz`'s failure is a stand-in (`Unhealthy`) to keep the block short; in the `api` crate the port errors convert into `ApiError::UpstreamUnavailable { upstream: "database" | "storage", .. }`, rendered as in [error-responses 2](error-responses.md#2-the-compiled-example). `GET /healthz` answers `200 { ok: true }` when the API can reach SurrealDB and RustFS ([contracts 9, ops](../../../docs/architecture/contracts.md#ops); [backend api 11](../../../docs/backend/api.md#11-endpoint-index), `healthz`).

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
use tokio_util::{sync::CancellationToken, task::TaskTracker};

pub type PortError = Box<dyn std::error::Error + Send + Sync>;

/// Cut down from `domain::ports::Store`: the store answers for `SurrealDB`.
#[async_trait::async_trait]
pub trait Store: Send + Sync {
    async fn status(&self) -> Result<(), PortError>;
}

/// Cut down from `domain::ports::BlobStore`: the blob store answers for `RustFS`.
#[async_trait::async_trait]
pub trait BlobStore: Send + Sync {
    async fn list_empty_prefix(&self) -> Result<(), PortError>;
}

/// Ports and in-process pieces only: no `Surreal<Any>`, no S3 client (backend api 6).
pub struct AppState {
    pub store: Arc<dyn Store>,
    pub blob: Arc<dyn BlobStore>,
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

/// `GET /healthz` (contracts 9, ops).
async fn healthz(State(state): State<Arc<AppState>>) -> Result<Json<Value>, Unhealthy> {
    state
        .store
        .status()
        .await
        .map_err(|_| Unhealthy("database"))?;
    state
        .blob
        .list_empty_prefix()
        .await
        .map_err(|_| Unhealthy("storage"))?;
    Ok(Json(json!({ "ok": true })))
}

pub fn ops_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;

    /// A test fake: the same `Arc<dyn Port>` slot the real adapter fills in `wiring.rs`.
    struct Up(bool);

    #[async_trait::async_trait]
    impl Store for Up {
        async fn status(&self) -> Result<(), PortError> {
            if self.0 {
                Ok(())
            } else {
                Err("connection refused".into())
            }
        }
    }

    #[async_trait::async_trait]
    impl BlobStore for Up {
        async fn list_empty_prefix(&self) -> Result<(), PortError> {
            if self.0 {
                Ok(())
            } else {
                Err("connection refused".into())
            }
        }
    }

    async fn healthz_status(store: bool, blob: bool) -> StatusCode {
        let state = Arc::new(AppState {
            store: Arc::new(Up(store)),
            blob: Arc::new(Up(blob)),
            shutdown: CancellationToken::new(),
            tasks: TaskTracker::new(),
        });
        let request = axum::http::Request::get("/healthz")
            .body(axum::body::Body::empty())
            .unwrap();
        ops_routes(state).oneshot(request).await.unwrap().status()
    }

    #[tokio::test]
    async fn healthz_needs_the_store_and_the_blob_store() {
        assert_eq!(healthz_status(true, true).await, StatusCode::OK);
        assert_eq!(
            healthz_status(false, true).await,
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            healthz_status(true, false).await,
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[test]
    #[should_panic(expected = "Path segments must not start with `:`")]
    fn axum_0_8_rejects_the_old_capture_syntax() {
        let _ = Router::<()>::new().route("/scans/:id", get(|| async {}));
    }
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
- **IDs are typed** (`ApiPath<ScanId>`, where `ScanId` is an alias of the one generic `Id<T>`, [rust-contract-types](../rust-contract-types/primitives.md#1-the-code); [backend domain 4](../../../docs/backend/domain.md#4-ids-and-primitives)) and travel as strings without a table prefix ([contracts 1, wire format](../../../docs/architecture/contracts.md#wire-format)). A path value that isn't a ULID is `404 not_found`, the same as a well-formed ID that names no record or one the caller may not see; a malformed ID in a body or query is `400 invalid_request` naming the field ([contracts 2](../../../docs/architecture/contracts.md#2-errors); [contracts 12, question 14](../../../docs/architecture/contracts.md#12-open-questions); [backend api 22](../../../docs/backend/api.md#22-decisions-on-the-contracts-open-questions); [error-responses 3](error-responses.md#3-extractor-rejections)).
- **App request bodies reject unknown fields** (`#[serde(deny_unknown_fields)]`), so a misspelt field is `400 invalid_request` naming its JSON path, and the API is deployed before an app that sends a new field; worker request bodies stay lenient ([contracts 1, versioning](../../../docs/architecture/contracts.md#versioning); [contracts 12, question 13](../../../docs/architecture/contracts.md#12-open-questions); [error-responses 3](error-responses.md#3-extractor-rejections)).
- **Headers** (`Idempotency-Key`, `Last-Event-ID`, `X-Lease-Token`) come from `HeaderMap` and are parsed by hand; a missing required one or a bad value is `invalid_request` with the header's name as `field`, so a worker call without `X-Lease-Token` is `400` with `field: "X-Lease-Token"`, while a wrong or stale token stays `409 lease_lost` ([contracts 1, authentication headers](../../../docs/architecture/contracts.md#authentication-headers); [contracts 12, question 23](../../../docs/architecture/contracts.md#12-open-questions); [error-responses 3](error-responses.md#3-extractor-rejections)). `TypedHeader` moved to `axum-extra` in axum 0.7 ([CHANGELOG 0.7.0](https://docs.rs/crate/axum/0.8.9/source/CHANGELOG.md)), which isn't pinned ([versions](../rust/versions.md)).
- **Uploads** (`multipart/form-data`) use `Multipart` in their own group, with a `DefaultBodyLimit` of 12 MiB + 2 × 4 MiB + 128 KiB and per-part caps: the photo at 12 MiB (`upload_too_large`), the depth map at 4 MiB (dropped, never failing the upload) ([error-responses 4](error-responses.md#4-body-limits), [contracts 3](../../../docs/architecture/contracts.md#post-scansidimages), [backend api 7](../../../docs/backend/api.md#7-the-router-and-the-layer-stack)).
- **Idempotency** (`Idempotency-Key` on the create calls, the stored replay, `409 request_in_progress` with `retry_after_s: 2` ([contracts 2](../../../docs/architecture/contracts.md#2-errors), "`retry_after_s`"), `422 idempotency_conflict`) follows [contracts 1, idempotency](../../../docs/architecture/contracts.md#idempotency) with the key record of [database 4](../../../docs/database/README.md#idempotency-keys-). The multipart fingerprint hashes the fields and file bytes, never the raw body. In the `api` crate it is a body extractor (`IdemJson<T>`) whose rejection is the replay, and a record dropped unfinished (a panic, a timeout, a lost client) releases its key from `Drop` ([backend api 10](../../../docs/backend/api.md#10-idempotency)).

## Errors

- `ApiError` lives in the `api` crate; each variant maps to one contracts 2 code and status. The variants and the full mapping table are in [rust-errors](../rust-errors/SKILL.md#the-mapping-table). The backend boxes it (`ApiError(Box<Inner>)` over `domain::error::ErrorKind`); the body is the same ([error-responses 1](error-responses.md#1-the-design-one-renderer)).
- `IntoResponse for ApiError` leaves the parts on the response; one `render_errors` middleware writes the `ErrorBody` with the `request_id` taken from the request's `RequestId`, and a `Retry-After` header whenever `retry_after_s` is set. The same middleware gives axum's and tower-http's own error responses a contract body ([error-responses 1](error-responses.md#1-the-design-one-renderer)).
- A panic becomes `500 internal` with an `ErrorBody` through `CatchPanicLayer::custom(panic_response)`. It is still a bug ([error-responses 2](error-responses.md#2-the-compiled-example), [rust-errors](../rust-errors/SKILL.md#panics-and-expect)).
- Non-members get `404`, never `403` ([error-responses 5](error-responses.md#5-404-not-403)).
- Worker job failures are not HTTP errors: they travel in the `complete` body ([rust-errors](../rust-errors/SKILL.md#worker-job-errors)).

## Auth, SSE, layers, OpenAPI

- **Clerk**: verify against the JWKS with `jsonwebtoken` 11, requiring `exp`, `nbf`, `iss`, `sub`, and checking `azp` against the web origins when present. Everything fails as `401 unauthenticated`, and so does a still-valid token of a deleted account, whose tombstone the store checks: it never creates the user again ([auth 1](auth.md#1-clerk-session-tokens)).
- **Access checks are extractors**, not middleware: `AuthUser`, then `Access<R, T>`, which returns a `Membership<R>` proof the repository call requires, so the role a route needs is in its signature and checked by the compiler ([auth 4](auth.md#4-which-routes-get-which-check); [backend domain 8](../../../docs/backend/domain.md#8-authorization-proofs)).
- **Workers**: a bearer token per worker, hashed with SHA-256 and compared in constant time against every configured token (a deliberate difference from backend api 9, which keeps each raw token as a `SecretString`: hashing first makes every comparison the same length, [auth 2](auth.md#2-workers-bearer-token-and-x-lease-token)); a missing `X-Lease-Token` is `400 invalid_request`, and the token is checked by the store inside its write, mismatch `409 lease_lost` ([auth 2](auth.md#2-workers-bearer-token-and-x-lease-token)).
- **The auth webhook**: a bad Svix signature or a timestamp more than 5 minutes away is `401`; everything else is `204`, `user.created` changes nothing, `user.deleted` starts the account cascade ([auth 3](auth.md#3-the-auth-webhook)).
- **SSE**: subscribe, replay after `Last-Event-ID`, forward live dropping sent `seq`s, replay again on `Lagged`, a gap or 15 s without an event, `: keepalive` after 15 idle seconds, end at the resting point (also one reached without an event), when the scan is deleted, or on shutdown ([sse](sse.md); [contracts 4, when the stream closes](../../../docs/architecture/contracts.md#when-the-stream-closes)). The chat stream runs its turn in a spawned task feeding an `mpsc`, and notices a disconnect through `Sender::closed()` or a failed `send` ([sse 4](sse.md#4-chat-streams)).
- **Layers**, top of the `ServiceBuilder` first: drop a client's `x-request-id`, set `req_<ULID>`, propagate it, trace, CORS, compression (never SSE), error renderer, catch panic, body limit; then per-group timeout and body limit; `without_allow` around the finished router ([layers 2](layers.md#2-the-stack-in-order)).
- **CORS** allows the web origins and exactly `Authorization`, `Content-Type`, `Idempotency-Key`, `Last-Event-ID`, and exposes `x-request-id` and `retry-after` ([layers 3](layers.md#3-cors)).
- **Timeouts** bound the response head only, so SSE bodies are safe; app 10 s, uploads and worker 60 s, the claim group 35 s for its 30 s long poll. The layer answers a bare `503`, which the renderer turns into `503 upstream_unavailable` with `upstream: "api"`, `retryable: true`, `retry_after_s: 5`; `408` and `504` are never produced, and dropping the handler releases its idempotency key ([layers 4](layers.md#4-timeouts); [contracts 2](../../../docs/architecture/contracts.md#2-errors), "Timeouts"; [contracts 12, questions 16 and 17](../../../docs/architecture/contracts.md#12-open-questions)).
- **Rate limits**: governor keyed by user, checked in the `AuthUser` extractor after the token; invites at most 10 attempts per user per minute ([contracts 9, members and invites](../../../docs/architecture/contracts.md#members-and-invites)): a burst of 5, then one every 12 s (`Quota::per_minute(5)`), as [backend api 19](../../../docs/backend/api.md#19-rate-limits-and-budgets) builds it; `Quota::per_minute(10)` would let 19 through in the first minute ([layers 5](layers.md#5-rate-limits)). `429 rate_limited`, `retryable: true`, `retry_after_s` rounded up. **Not in the contracts yet:** contracts 2 gives the general 120 a minute only as a placeholder, and the 1 MiB JSON limit is one too ([contracts 12, question 34](../../../docs/architecture/contracts.md#12-open-questions)); a JSON body over it is `400 invalid_request` with `field: "body"`, and `413` stays for images ([contracts 12, question 15](../../../docs/architecture/contracts.md#12-open-questions); [error-responses 4](error-responses.md#4-body-limits)).
- **Shutdown**: `with_graceful_shutdown(token.cancelled_owned())` waits for open connections, so every stream must end on the token ([layers 6](layers.md#6-graceful-shutdown), [rust-async shutdown 5](../rust-async/shutdown.md#5-api)).
- **OpenAPI**: every route through `OpenApiRouter` and `routes!` (worker, webhook, media and ops routes too, with their own security), error statuses with `body = ErrorBody`, OpenAPI 3.1.0, written to `backend/openapi.json` by `api openapi`. The one plain `.route` is `/openapi.json` itself, added to the `axum::Router` after `split_for_parts` because it serves the document that split produced ([openapi](openapi.md); [backend api 7](../../../docs/backend/api.md#7-the-router-and-the-layer-stack)).

## Review checklist

- [ ] Every handler returns `Result<_, ApiError>`; no `axum::Json`/`Query`/`Path` extractor, no plain-text error ([handlers and extractors](#handlers-and-extractors)).
- [ ] Body extractor last; `State<Arc<AppState>>` holding `Arc<dyn Port>`s, never a `Surreal<Any>`, an S3 client or a cloned client per request.
- [ ] Paths use `{id}`; IDs are typed (`ApiPath<ScanId>`): a malformed, unknown or hidden path ID is `404`; a malformed ID in a body or query is `400 invalid_request` naming the field ([handlers and extractors](#handlers-and-extractors)).
- [ ] Garden-scoped calls take an access extractor (`Access<R, T>`) and pass its `Membership<R>` to the store; unknown and not-a-member return the same `404`; the role check comes after membership ([auth 4](auth.md#4-which-routes-get-which-check)).
- [ ] App request types have `deny_unknown_fields`; worker request types don't.
- [ ] Error bodies match contracts 2: `retry_after_s` only with `request_in_progress`, `rate_limited` and the 503 codes; `details` only as the table lists.
- [ ] New routes are in the right group (app, uploads, claim, worker) and registered through `OpenApiRouter` with their error responses.
- [ ] Uploads have a route `DefaultBodyLimit` and per-part caps; depth problems never fail the upload.
- [ ] SSE: subscribe before replay, dedupe by `seq` (a duplicate resting event still closes), `Lagged`, a gap and 15 s without an event (`REST_CHECK`) → state read and replay, `Closed` and a gone scan (`NotFound`) → end, keepalive `"keepalive"` at 15 s, ends at rest, on shutdown and on a store error, no body timeout layer, not compressed ([sse](sse.md)).
- [ ] Long polls and streams watch the shutdown token ([layers 6](layers.md#6-graceful-shutdown)).
- [ ] CORS lists exactly the four request headers and exposes `x-request-id` and `retry-after`; no `Any` origin ([layers 3](layers.md#3-cors)).
- [ ] Timeouts are `TimeoutLayer::with_status_code(StatusCode::SERVICE_UNAVAILABLE, …)`, rendered as `upstream_unavailable` from `api` ([layers 4](layers.md#4-timeouts)).
- [ ] A synchronous extractor returns `std::future::ready(..)`, not an `async fn` without `.await` (`clippy::unused_async_trait_impl`, [auth 4](auth.md#4-which-routes-get-which-check)).
- [ ] No `unwrap()` outside tests; `expect` says why it can't fail ([rust-errors](../rust-errors/SKILL.md#panics-and-expect)).

## Sibling skills

[rust](../rust/SKILL.md) (entry, versions, checker) · [rust-errors](../rust-errors/SKILL.md) (the error design and the code table) · [rust-tracing](../rust-tracing/SKILL.md) (request IDs, `TraceLayer`, JSON logs) · [rust-async](../rust-async/SKILL.md) (cancellation, `TaskTracker`, broadcast) · [rust-contract-types](../rust-contract-types/SKILL.md) (the `domain` types and their serde and schema derives) · [rust-surrealdb](../rust-surrealdb/SKILL.md) (store queries, the event replay read) · [rust-workers](../rust-workers/SKILL.md) (the worker side of the job API) · [rust-testing](../rust-testing/SKILL.md) · [rust-cargo-lints](../rust-cargo-lints/SKILL.md).
