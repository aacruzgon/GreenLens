# Request IDs and the request span

Part of [rust-tracing](SKILL.md). The shared decision: header `x-request-id`, value `req_<ULID>` (the shape of the `request_id` in the [contracts 2 example](../../../docs/architecture/contracts.md#2-errors)), made by tower-http's `SetRequestIdLayer`, stored on the request as `RequestId`, recorded on the request span, copied to the response, and written into `ErrorBody.error.request_id` by [rust-axum-api](../rust-axum-api/error-responses.md). This file owns making the ID and the span. Versions: tower-http 0.7.1 (`trace`, `request-id`), axum 0.8.9, tower 0.5.3.

## Contents

1. [The layer stack](#1-the-layer-stack)
2. [The code and its test](#2-the-code-and-its-test)
3. [Notes](#3-notes)

## 1. The layer stack

Inside one `ServiceBuilder`, the first layer is the outermost and sees the request first ([axum, ordering](https://docs.rs/axum/0.8.9/axum/middleware/index.html#ordering)). The whole stack is added with `Router::layer`, which wraps each route *after* routing, so `MatchedPath` is already in the request's extensions ([`MatchedPath`, accessing via extensions](https://docs.rs/axum/0.8.9/axum/extract/struct.MatchedPath.html#accessing-matchedpath-via-extensions); [`Router::layer`](https://docs.rs/axum/0.8.9/axum/struct.Router.html#method.layer): add routes and the fallback first).

| # | Layer | Why here |
|---|---|---|
| 1 | `map_request` removing `x-request-id` | `SetRequestId` keeps an ID the client sent and only makes one when the header is missing (`SetRequestId::call` in `request_id.rs`). Removing it first makes every ID ours: a fixed shape, unique, and never attacker-chosen text in our logs and error bodies |
| 2 | `SetRequestIdLayer::x_request_id(MakeReqId)` | Makes `req_<ULID>`, sets the header and inserts [`RequestId`](https://docs.rs/tower-http/0.7.1/tower_http/request_id/struct.RequestId.html) into the extensions |
| 3 | `PropagateRequestIdLayer::x_request_id()` | Copies the header to the response ([`PropagateRequestIdLayer`](https://docs.rs/tower-http/0.7.1/tower_http/request_id/struct.PropagateRequestIdLayer.html)) |
| 4 | `TraceLayer` with our `make_span_with` | Reads `RequestId` and `MatchedPath`, so it must be inside 2 |
| 5 | `render_errors`, `CatchPanicLayer` | Owned by [rust-axum-api](../rust-axum-api/error-responses.md). Inside the span, so their log lines carry the `request_id`, and a panic's `500` is seen by `TraceLayer` |

## 2. The code and its test

The span records only the matched route template (`/scans/{id}/images/{image_id}`), never `uri()`. tower-http's `DefaultMakeSpan` records `uri = %request.uri()`, which includes the query string, and image URLs carry `?exp=&sig=` ([contracts 1, wire format](../../../docs/architecture/contracts.md#wire-format)); its `include_headers(true)` would record `Authorization` and `X-Lease-Token` (`trace/make_span.rs`). Field names follow the OpenTelemetry HTTP server span conventions ([below](#3-notes)).

```rust
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::{MatchedPath, Request};
use axum::http::Response;
use axum::routing::get;
use tower::ServiceBuilder;
use tower_http::request_id::{PropagateRequestIdLayer, RequestId, SetRequestIdLayer};
use tower_http::trace::TraceLayer;
use tracing::Span;
use tracing::field::Empty;
use tracing_subscriber::fmt::MakeWriter;

// canonical: rust-tracing/request-id.md MakeReqId
/// `req_<ULID>`: 30 ASCII characters, always a valid header value.
#[derive(Clone, Copy, Debug, Default)]
pub struct MakeReqId;

impl tower_http::request_id::MakeRequestId for MakeReqId {
    fn make_request_id<B>(
        &mut self,
        _: &axum::http::Request<B>,
    ) -> Option<tower_http::request_id::RequestId> {
        let id = format!("req_{}", ulid::Ulid::generate());
        axum::http::HeaderValue::try_from(id)
            .ok()
            .map(tower_http::request_id::RequestId::new)
    }
}
// end: MakeReqId

fn request_span(req: &Request) -> Span {
    let request_id = req
        .extensions()
        .get::<RequestId>()
        .and_then(|id| id.header_value().to_str().ok())
        .unwrap_or_default();
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(MatchedPath::as_str);
    tracing::info_span!(
        "request",
        request_id,
        http.request.method = %req.method(),
        http.route = route,
        http.response.status_code = Empty,
    )
}

fn on_response(res: &Response<Body>, latency: Duration, span: &Span) {
    span.record("http.response.status_code", res.status().as_u16());
    let latency_ms = u64::try_from(latency.as_millis()).unwrap_or(u64::MAX);
    tracing::info!(latency_ms, "response");
}

/// Add every route and the fallback before calling this.
pub fn with_request_layers(router: Router) -> Router {
    router.layer(
        ServiceBuilder::new()
            .map_request(|mut req: Request| {
                req.headers_mut().remove("x-request-id");
                req
            })
            .layer(SetRequestIdLayer::x_request_id(MakeReqId))
            .layer(PropagateRequestIdLayer::x_request_id())
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(request_span)
                    .on_request(())
                    .on_response(on_response),
            ),
    )
}

#[derive(Clone, Default)]
pub struct Captured(Arc<Mutex<Vec<u8>>>);

impl io::Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut buf = self.0.lock().map_err(|_| io::Error::other("poisoned"))?;
        buf.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'w> MakeWriter<'w> for Captured {
    type Writer = Self;

    fn make_writer(&'w self) -> Self::Writer {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt as _;

    fn app() -> Router {
        let routes = Router::new().route(
            "/scans/{id}/images/{image_id}",
            get(|| async {
                tracing::info!("serving image");
                "webp bytes"
            }),
        );
        with_request_layers(routes)
    }

    #[tokio::test]
    async fn id_is_ours_and_on_every_line() {
        let out = Captured::default();
        let subscriber = tracing_subscriber::fmt()
            .json()
            .flatten_event(true)
            .with_current_span(false)
            .with_writer(out.clone())
            .finish();
        // Thread-local; the default test runtime runs everything on this thread.
        let _guard = tracing::subscriber::set_default(subscriber);

        let request = axum::http::Request::get(
            "/scans/01JAX3K9T6R2B8V0QZ4M7D1E5F/images/01JAX6BBBBBBBBBBBBBBBBBBBB?exp=1760000000&sig=c2VjcmV0",
        )
        .header("x-request-id", "client-chosen")
        .body(Body::empty())
        .unwrap();
        let response = app().oneshot(request).await.unwrap();

        let id = response.headers()["x-request-id"]
            .to_str()
            .unwrap()
            .to_owned();
        assert!(id.starts_with("req_") && id.len() == 30, "{id}");

        let text = String::from_utf8(out.0.lock().unwrap().clone()).unwrap();
        assert!(!text.contains("sig="), "no query string in the logs");
        assert!(!text.contains("client-chosen"));
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        for line in &lines {
            let span = &line["spans"][0];
            assert_eq!(span["request_id"], id.as_str());
            assert_eq!(span["http.route"], "/scans/{id}/images/{image_id}");
            assert_eq!(span["http.request.method"], "GET");
        }
        assert_eq!(lines[1]["message"], "response");
        assert_eq!(lines[1]["spans"][0]["http.response.status_code"], 200);
        assert!(lines[1]["latency_ms"].is_u64());
    }
}
```

## 3. Notes

- **The ID's length.** A ULID is 26 characters, so the ID is 30. The contracts example `req_01JAX4B2Q8` is shortened; the prefix and alphabet match.
- **Recording later.** A field set after the span is created (the status code) must be declared up front with `field::Empty`: "you cannot record a value for a field that was not specified when the span was created" ([`Span::record`](https://docs.rs/tracing/0.1.44/tracing/struct.Span.html#method.record)).
- **Unmatched requests.** With no `MatchedPath`, `http.route` is left out rather than filled with the raw path; the convention says "the URI path can NOT substitute it". The layers only wrap a fallback that was added before `Router::layer` ([`Router::layer`](https://docs.rs/axum/0.8.9/axum/struct.Router.html#method.layer)), so add the fallback first.
- **Failures.** `TraceLayer`'s default `on_failure` logs `5xx` responses at `ERROR` with the latency, inside the span ([`DefaultOnFailure`](https://docs.rs/tower-http/0.7.1/tower_http/trace/struct.DefaultOnFailure.html)). The error's chain is logged once, by `render_errors` ([rust-axum-api](../rust-axum-api/error-responses.md)), which also records `request_id` as a field of its own event; the two lines don't repeat the chain.
- **The filter keeps `info`.** The request span is `INFO`. Under `RUST_LOG=warn` it is disabled, and every line of the request, `ERROR` ones included, loses `spans` and with it the `request_id`; only an event that records `request_id` itself keeps it ([json-logs.md 1](json-logs.md#1-setup-in-main), `warn_drops_the_request_span_and_its_id`).
- **SSE.** `on_response` fires when the headers are sent, so an event stream's `latency_ms` is the time to open it. The response body is polled inside the span (`trace/body.rs` enters it on each poll), so lines logged while producing events carry the `request_id`; a task spawned for the stream needs `.in_current_span()` ([json-logs.md 4](json-logs.md#4-spans-across-tokiospawn-and-spawn_blocking)).
- **OpenTelemetry names.** `http.request.method`, `http.route` and `http.response.status_code` are the attribute names of the HTTP server span in semantic conventions 1.44.0, all three marked Stable ([HTTP server span](https://opentelemetry.io/docs/specs/semconv/http/http-spans/#http-server-span)). `url.path` is Stable too, but the conventions say sensitive content in it "SHOULD be scrubbed", and `http.route` carries the same information without IDs. Dotted names are plain `tracing` field names (`http.route = route`), so the JSON keys are the dotted strings. The project ships **no OpenTelemetry exporter**: logs are JSON on stdout ([architecture 12, operations](../../../docs/architecture/README.md#operations)). The names only keep that option open.
