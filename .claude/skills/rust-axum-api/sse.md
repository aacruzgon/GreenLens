# `GET /scans/{id}/events`: the scan event stream

Part of [rust-axum-api](SKILL.md). The behaviour is fixed by [contracts 4](../../../docs/architecture/contracts.md#4-scan-events-sse) (transport, resume, keepalive, when the stream closes) and [architecture 12](../../../docs/architecture/README.md#12-deployment) (`Cache-Control: no-cache` behind a tunnel, one in-process broadcast, one serialised writer per scan). Broadcast semantics in general are in [rust-async channels 4](../rust-async/channels.md#4-broadcast-the-sse-hub); the replay query (`scan_event:[$scan, $after + 1]..=[$scan, ..]`) is in [rust-surrealdb](../rust-surrealdb/SKILL.md) and [database 4](../../../docs/database/README.md#scan-events-a-gap-free-seq-).

## Contents

1. [What the endpoint does](#1-what-the-endpoint-does)
2. [What axum writes](#2-what-axum-writes)
3. [The compiled example](#3-the-compiled-example)
4. [Chat streams](#4-chat-streams)

## 1. What the endpoint does

In this order:

1. **Before any byte is sent:** authenticate, take the scan ID as `ApiPath<ScanId>` (a value that isn't a ULID is `400 invalid_request` naming `id`; [error-responses 3](error-responses.md#3-extractor-rejections)), check membership (an unknown scan and a non-member both get `404`, [error-responses 5](error-responses.md#5-404-not-403)), and parse `Last-Event-ID` (absent means 0: replay the whole history). Errors here, a store error included (`500 internal`), are ordinary `ErrorBody` responses, because headers haven't gone out yet. **Not in the contracts yet:** contracts 2 doesn't say whether a malformed ID is `400` or `404` ([contracts 12, question 14](../../../docs/architecture/contracts.md#12-open-questions)).
2. **Subscribe** to the scan's broadcast. This comes first, so an event stored while the replay runs is either in the replay or in the receiver, never in neither ([contracts 4, transport](../../../docs/architecture/contracts.md#transport): "first subscribes … then replays"; [`Sender::subscribe`](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/struct.Sender.html#method.subscribe) sees only values sent after the call).
3. **Read the scan's state, then replay** every stored event with `seq` > the last one sent. State first: the writer commits an event together with the state it leads to ([contracts 4, transport](../../../docs/architecture/contracts.md#transport): one serialised writer stores, then broadcasts), so if the scan was at rest when its state was read, the replay that follows holds every event up to the resting point. Reading the events first and the state second loses the last events when they arrive in between.
4. **If that state was at rest** (the [resting rule](../../../docs/architecture/contracts.md#when-the-stream-closes)), end the stream after the replay.
5. **Forward live events**, dropping any whose `seq` was already sent ([contracts 4, transport](../../../docs/architecture/contracts.md#transport)). The writer marks, in the broadcast message, the event that brings the scan to rest; after sending it, end the stream. The mark isn't stored (`scan_event` has `run`, `type`, `payload` and `ts`; [schema.surql](../../../docs/database/schema.surql)), so the replay can't see it. That is why the drop rule has one exception: a dropped duplicate that carries the mark also ends the stream. It happens when the writer commits the resting event after step 3 read the state but before the replay read the events: the replay already sent it, and without the exception the stream would wait for an event that never comes (the `a_resting_event_committed_during_the_replay_closes_the_stream` test).
6. **On `RecvError::Lagged`** (the receiver fell behind and the channel overwrote events), go back to step 3. A live event whose `seq` skips ahead (`seq > last + 1`) is treated the same way, so the client never sees a gap ([`Receiver::recv`](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/struct.Receiver.html#method.recv): after `Lagged`, the next `recv` returns the oldest value still held).
7. **On `RecvError::Closed`** (the hub dropped the scan's sender while this stream was subscribed), subscribe again and go back to step 3. `Closed` is not a resting point: the stream ends only on the resting rules ([rust-async channels 4](../rust-async/channels.md#4-broadcast-the-sse-hub)). The new receiver comes with a sender the hub keeps, so this doesn't spin; the hub retires a sender only when it has no receivers ([rust-async channels 3](../rust-async/channels.md#3-oneshot-and-the-actor-pattern-the-per-scan-event-writer); not fixed by the contracts, [contracts 12, question 22](../../../docs/architecture/contracts.md#12-open-questions)).
8. **On shutdown** (the process `CancellationToken`), end the stream; `axum::serve`'s graceful shutdown waits for every open connection, so a stream that never ends would hold the process up ([rust-async shutdown 5](../rust-async/shutdown.md#5-api), [layers 6](layers.md#6-graceful-shutdown)). The app resumes with `Last-Event-ID`.
9. **On a store error mid-stream** (the state read or the replay), log it at `warn` and end the stream. The status is already 200; the app reconnects with `Last-Event-ID` ([frontend 10](../../../docs/frontend/README.md#10-live-streams)). Never turn the error into `false` or an empty replay: that would close a running scan's stream as if it were at rest, or leave it waiting.

Sources: [`async_stream::stream!`](https://docs.rs/async-stream/0.3.6/async_stream/macro.stream.html), [`tokio::select!`](https://docs.rs/tokio/1.53.3/tokio/macro.select.html), [`CancellationToken::cancelled`](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html#method.cancelled), [`RecvError`](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/error/enum.RecvError.html).

## 2. What axum writes

All checked in `axum-0.8.9/src/response/sse.rs` ([source](https://docs.rs/crate/axum/0.8.9/source/src/response/sse.rs)) and by the tests below.

| Need (contracts 4, architecture 12) | axum 0.8.9 |
|---|---|
| `id: 17`, `event: finding`, `data: {...}`, blank line | `Event::default().id("17").event("finding").data(json)`. Each setter writes `name: value\n` in call order, and the event ends with `\n`. So call `id`, `event`, `data` in that order to match the contract's layout. `id`, `event` and `retry` panic if called twice on one event; every value panics if it contains `\r` or `\n` (`data` splits on newlines into several `data:` lines instead) ([`Event`](https://docs.rs/axum/0.8.9/axum/response/sse/struct.Event.html)) |
| JSON in `data` | `.json_data(&value)` serialises straight into the event and returns `Result`; `.data(&str)` for JSON already serialised (the stored payload with fresh signed URLs) ([`Event::json_data`](https://docs.rs/axum/0.8.9/axum/response/sse/struct.Event.html#method.json_data)) |
| `: keepalive` every 15 s | `Sse::keep_alive(KeepAlive::new().text("keepalive"))`. `KeepAlive::new` sets `max_interval: Duration::from_secs(15)` and `event: Event::DEFAULT_KEEP_ALIVE`, which is `b":\n\n"`, an empty comment; `.text("keepalive")` gives `: keepalive\n\n` ([`KeepAlive::interval`](https://docs.rs/axum/0.8.9/axum/response/sse/struct.KeepAlive.html#method.interval) and [`KeepAlive::text`](https://docs.rs/axum/0.8.9/axum/response/sse/struct.KeepAlive.html#method.text) document both defaults). `KeepAliveStream::poll_next` resets the timer after every event, so a keepalive goes out after 15 s **without** an event, not on a fixed clock |
| `Cache-Control: no-cache` | Set by `Sse`'s `IntoResponse`, with `Content-Type: text/event-stream`. Nothing to add ([`Sse`](https://docs.rs/axum/0.8.9/axum/response/sse/struct.Sse.html); the `event_wire_format_matches_contracts_4` test) |
| Close the stream | End the `Stream` (`return` inside `stream!`). An `Err` item becomes a body error, which aborts the response instead of ending it cleanly (`Sse`'s body, `poll_frame`) |

**Timeouts.** tower-http's `TimeoutLayer` limits the time until the response *head* is ready ([layers 4](layers.md#4-timeouts)), so it never cuts an SSE body; the test in layers.md proves it. Don't put `ResponseBodyTimeoutLayer` or `ResponseBodyDeadlineLayer` on this route: the first is an idle timeout between body frames, which aborts an idle stream with an error if it is shorter than the 15 s keepalive and never fires if it is longer; the second cuts every stream at a fixed age (layers 4 has the details and a test).

## 3. The compiled example

`Hub` stands for the SSE hub (one `broadcast::Sender` per scan) and `Store` for the `store` crate. `publish` in the tests plays the serialised writer: store, then broadcast. The block carries copies of `ApiError` and `ApiPath` from [error-responses](error-responses.md#2-the-compiled-example), so the handler's errors are the real ones; the tests check the `ErrorParts` each leaves on its response.

```rust
use std::{
    collections::HashMap,
    convert::Infallible,
    fmt,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use async_stream::stream;
use axum::{
    extract::State,
    http::HeaderMap,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::Stream;
use serde::Deserialize;
use tokio::sync::broadcast::{self, error::RecvError};
use tokio_util::sync::CancellationToken;

/// A stored event (one `scan_event` row), with its data line already built (fresh
/// signed URLs included).
#[derive(Debug, Clone)]
pub struct ScanEvent {
    pub seq: u64,
    pub kind: &'static str,
    pub data: String,
}

impl ScanEvent {
    fn to_sse(&self) -> Event {
        Event::default()
            .id(self.seq.to_string())
            .event(self.kind)
            .data(&self.data)
    }
}

/// What the writer broadcasts. `rests` marks the event that brings the scan to rest
/// (contracts 4, when the stream closes); it isn't stored, so only live subscribers see it.
#[derive(Debug)]
pub struct Live {
    pub event: ScanEvent,
    pub rests: bool,
}

/// A typed path ID; the real newtypes are in rust-contract-types. It parses inside
/// `visit_str`, so a bad value keeps the capture's name (error-responses 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanId(ulid::Ulid);

impl<'de> Deserialize<'de> for ScanId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl serde::de::Visitor<'_> for Visitor {
            type Value = ScanId;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a 26-character ULID")
            }
            fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<ScanId, E> {
                ulid::Ulid::from_string(s).map(ScanId).map_err(E::custom)
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}

impl fmt::Display for ScanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// The stand-in store's failure.
#[derive(Debug, thiserror::Error)]
#[error("store unavailable")]
pub struct StoreDown;

type Write = Box<dyn FnOnce() + Send>;

/// Stand-in for the `store` crate, for one scan.
#[derive(Default)]
pub struct Store {
    events: Mutex<Vec<ScanEvent>>,
    at_rest: AtomicBool,
    readable: Mutex<Vec<String>>,
    down: AtomicBool,
    /// A write the writer commits after the stream read the state, before it reads the
    /// events: the window in which the resting event can land in the replay.
    racing_write: Mutex<Option<Write>>,
}

impl Store {
    pub async fn events_after(&self, after: u64) -> Result<Vec<ScanEvent>, StoreDown> {
        tokio::task::yield_now().await; // the real query is a network round trip
        let racing = self.racing_write.lock().map_err(|_| StoreDown)?.take();
        if let Some(write) = racing {
            write();
        }
        let events = self.events.lock().map_err(|_| StoreDown)?;
        Ok(events.iter().filter(|e| e.seq > after).cloned().collect())
    }

    /// The garden-scoped read: `false` for an unknown scan *and* for a non-member.
    pub async fn caller_can_read(&self, scan_id: &str) -> Result<bool, StoreDown> {
        tokio::task::yield_now().await;
        if self.down.load(Ordering::SeqCst) {
            return Err(StoreDown);
        }
        let readable = self.readable.lock().map_err(|_| StoreDown)?;
        Ok(readable.iter().any(|id| id == scan_id))
    }

    pub async fn at_rest(&self) -> Result<bool, StoreDown> {
        tokio::task::yield_now().await;
        if self.down.load(Ordering::SeqCst) {
            return Err(StoreDown);
        }
        Ok(self.at_rest.load(Ordering::SeqCst))
    }
}

/// One channel per scan. 256 is this skill's choice: a run sends dozens of events
/// (contracts 4, event catalogue), and a client that falls further behind replays.
#[derive(Default)]
pub struct Hub {
    channels: Mutex<HashMap<String, broadcast::Sender<Arc<Live>>>>,
}

impl Hub {
    const CAPACITY: usize = 256;

    pub fn subscribe(&self, scan_id: &str) -> broadcast::Receiver<Arc<Live>> {
        self.sender(scan_id, Self::CAPACITY).subscribe()
    }

    fn sender(&self, scan_id: &str, capacity: usize) -> broadcast::Sender<Arc<Live>> {
        let mut channels = self.channels.lock().unwrap_or_else(PoisonError::into_inner);
        channels
            .entry(scan_id.to_owned())
            .or_insert_with(|| broadcast::channel(capacity).0)
            .clone()
    }
}

pub struct AppState {
    pub store: Store,
    pub hub: Hub,
    pub shutdown: CancellationToken,
}

/// `GET /scans/{id}/events`. Authentication (the user extractor) runs before it.
pub async fn scan_events(
    State(state): State<Arc<AppState>>,
    ApiPath(scan_id): ApiPath<ScanId>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let scan_id = scan_id.to_string();
    let readable = state
        .store
        .caller_can_read(&scan_id)
        .await
        .map_err(|error| ApiError::Internal(Box::new(error)))?;
    if !readable {
        return Err(ApiError::NotFound);
    }
    let after = match headers.get("last-event-id") {
        None => 0,
        Some(value) => value
            .to_str()
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .ok_or_else(|| {
                ApiError::invalid_request("Last-Event-ID", "Send the last seq number applied.")
            })?,
    };
    let keep_alive = KeepAlive::new()
        .interval(Duration::from_secs(15))
        .text("keepalive");
    Ok(Sse::new(event_stream(state, scan_id, after)).keep_alive(keep_alive))
}

pub fn event_stream(
    state: Arc<AppState>,
    scan_id: String,
    mut last: u64,
) -> impl Stream<Item = Result<Event, Infallible>> {
    stream! {
        // 1. Subscribe before reading the store.
        let mut live = state.hub.subscribe(&scan_id);
        loop {
            // 2. Read the scan's state *before* its events. The writer commits an
            // event and the state it leads to together, so if the scan was at rest
            // here, the replay below holds every event up to that point.
            let at_rest = match state.store.at_rest().await {
                Ok(at_rest) => at_rest,
                Err(error) => {
                    tracing::warn!(error = %Chain(&error), scan_id, "state read failed; ending the stream");
                    return;
                }
            };
            // 3. Replay everything after `last`.
            match state.store.events_after(last).await {
                Ok(events) => {
                    for event in events {
                        last = event.seq;
                        yield Ok(event.to_sse());
                    }
                }
                Err(error) => {
                    tracing::warn!(error = %Chain(&error), scan_id, "replay failed; ending the stream");
                    return;
                }
            }
            // A scan at rest replays and closes.
            if at_rest {
                return;
            }
            // 4. Live events until rest, lag, a gap, a closed channel or shutdown.
            loop {
                let received = tokio::select! {
                    () = state.shutdown.cancelled() => return,
                    received = live.recv() => received,
                };
                match received {
                    // The replay already sent the resting event: nothing else will end
                    // the stream, so end it here.
                    Ok(message) if message.event.seq <= last && message.rests => return,
                    Ok(message) if message.event.seq <= last => {}
                    Ok(message) if message.event.seq > last + 1 => break,
                    Ok(message) => {
                        last = message.event.seq;
                        yield Ok(message.event.to_sse());
                        if message.rests {
                            return;
                        }
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        tracing::debug!(skipped, scan_id, "subscriber lagged; replaying");
                        break;
                    }
                    Err(RecvError::Closed) => {
                        // Not a resting point. The hub keeps the new sender, so this
                        // receiver stays open; the replay fills any gap.
                        live = state.hub.subscribe(&scan_id);
                        break;
                    }
                }
            }
        }
    }
}

// copy of: rust-axum-api/error-responses.md ApiPath
/// `axum::extract::Path` with contracts 2 rejections.
#[derive(axum::extract::FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(ApiError))]
pub struct ApiPath<T>(pub T);

impl From<axum::extract::rejection::PathRejection> for ApiError {
    fn from(rejection: axum::extract::rejection::PathRejection) -> Self {
        use axum::extract::path::ErrorKind;
        use axum::extract::rejection::PathRejection;
        match rejection {
            PathRejection::FailedToDeserializePathParams(e) => match e.into_kind() {
                ErrorKind::ParseErrorAtKey {
                    key, expected_type, ..
                } => Self::invalid_request(key, format!("expected {expected_type}")),
                ErrorKind::DeserializeError { key, message, .. } => {
                    Self::invalid_request(key, message)
                }
                ErrorKind::InvalidUtf8InPathParam { key } => {
                    Self::invalid_request(key, "invalid UTF-8")
                }
                // The route and the type disagree: a bug, not the client's input.
                kind @ (ErrorKind::WrongNumberOfParameters { .. }
                | ErrorKind::UnsupportedType { .. }) => Self::Internal(kind.to_string().into()),
                other => Self::invalid_request("path", other.to_string()),
            },
            other => Self::Internal(other.body_text().into()),
        }
    }
}
// end: ApiPath

// copy of: rust-axum-api/error-responses.md ErrorCode
/// contracts 2 `ErrorCode`. In the domain crate; `ToSchema` adds it to the generated spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    Unauthenticated,
    Forbidden,
    InvalidSignature,
    NotFound,
    InvalidState,
    RequestInProgress,
    LeaseLost,
    UploadTooLarge,
    UnsupportedMediaType,
    ImageQuality,
    MissingRequiredShot,
    TooManyImages,
    IdempotencyConflict,
    RateLimited,
    BudgetExceeded,
    Internal,
    ProcessingUnavailable,
    UpstreamUnavailable,
}
// end: ErrorCode

// copy of: rust-axum-api/error-responses.md ApiError
/// contracts 1, roles: each role has every right of the one before.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Viewer,
    Reviewer,
    Editor,
    Owner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityProblem {
    TooBlurry,
    TooDark,
    TooBright,
}

impl QualityProblem {
    /// contracts 2 `QualityHint`: one hint per problem.
    pub const fn hint(self) -> &'static str {
        match self {
            Self::TooBlurry => "tap_to_focus",
            Self::TooDark => "more_light",
            Self::TooBright => "avoid_glare",
        }
    }

    const fn message(self) -> &'static str {
        match self {
            Self::TooBlurry => {
                "This photo is too blurry. Hold the phone still and tap the leaf to focus."
            }
            Self::TooDark => "This photo is too dark. Move into better light.",
            Self::TooBright => {
                "This photo is too bright. Shade the plant or step out of direct sun."
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StateHint {
    NewScan,
    NeighbourRemoved,
    PlantRemoved,
}

/// contracts 2 `invalid_state` details; every key optional.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct InvalidStateDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_plan_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_map_version: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<StateHint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Budget {
    ChatTokens,
    Scans,
}

/// contracts 2: the four upload formats.
pub const ACCEPTED_MEDIA_TYPES: [&str; 4] = ["image/jpeg", "image/png", "image/webp", "image/heic"];
/// contracts 2: `max_bytes` of `upload_too_large`, an image over 12 MB (read as MiB).
pub const IMAGE_MAX_BYTES: usize = 12 * 1024 * 1024;
/// `retry_after_s` on a `503` when the cause gives no better number. The value is this
/// skill's choice; contracts 2 only says the field is sent with `503`.
pub const UNAVAILABLE_RETRY_AFTER_S: u64 = 5;

/// One variant per contracts 2 code. `Display` is the `message`: a sentence safe to show.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{problem}")]
    InvalidRequest { field: String, problem: String },
    #[error("Please sign in again.")]
    Unauthenticated,
    #[error("Ask an owner to change your role.")]
    Forbidden { required_role: Role },
    #[error("This link has expired.")]
    InvalidSignature,
    #[error("We couldn't find that.")]
    NotFound,
    #[error("This can't be done right now.")]
    InvalidState(InvalidStateDetails),
    #[error("This request is still being processed.")]
    RequestInProgress { retry_after_s: u64 },
    #[error("This job's lease is no longer valid.")]
    LeaseLost { job_id: String },
    #[error("This photo is too large to upload.")]
    UploadTooLarge { max_bytes: usize },
    #[error("This file type isn't supported. Use JPEG, PNG, WebP or HEIC.")]
    UnsupportedMediaType,
    #[error("{}", .problem.message())]
    ImageQuality {
        problem: QualityProblem,
        blur_score: f64,
        exposure: f64,
    },
    #[error("A photo of the whole plant is needed.")]
    MissingRequiredShot { missing: Vec<String> },
    #[error("There are already enough photos of this kind.")]
    TooManyImages { kind: String, max: u32 },
    #[error("This request doesn't match the earlier one with the same key.")]
    IdempotencyConflict,
    #[error("Too many requests. Try again in {retry_after_s} seconds.")]
    RateLimited { retry_after_s: u64 },
    #[error("You've reached today's limit.")]
    BudgetExceeded {
        budget: Budget,
        resets_at: Timestamp,
    },
    #[error("Something went wrong on our side.")]
    Internal(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("We couldn't process this photo right now. It wasn't saved.")]
    ProcessingUnavailable {
        retry_after_s: u64,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("A service we depend on is unavailable. Try again shortly.")]
    UpstreamUnavailable {
        upstream: &'static str,
        retry_after_s: u64,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl ApiError {
    pub fn invalid_request(field: impl Into<String>, problem: impl Into<String>) -> Self {
        Self::InvalidRequest {
            field: field.into(),
            problem: problem.into(),
        }
    }

    pub fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::InvalidRequest { .. } => StatusCode::BAD_REQUEST,
            Self::Unauthenticated => StatusCode::UNAUTHORIZED,
            Self::Forbidden { .. } | Self::InvalidSignature => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::InvalidState(_) | Self::RequestInProgress { .. } | Self::LeaseLost { .. } => {
                StatusCode::CONFLICT
            }
            Self::UploadTooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            Self::UnsupportedMediaType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::ImageQuality { .. }
            | Self::MissingRequiredShot { .. }
            | Self::TooManyImages { .. }
            | Self::IdempotencyConflict => StatusCode::UNPROCESSABLE_ENTITY,
            Self::RateLimited { .. } | Self::BudgetExceeded { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ProcessingUnavailable { .. } | Self::UpstreamUnavailable { .. } => {
                StatusCode::SERVICE_UNAVAILABLE
            }
        }
    }

    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::InvalidRequest { .. } => ErrorCode::InvalidRequest,
            Self::Unauthenticated => ErrorCode::Unauthenticated,
            Self::Forbidden { .. } => ErrorCode::Forbidden,
            Self::InvalidSignature => ErrorCode::InvalidSignature,
            Self::NotFound => ErrorCode::NotFound,
            Self::InvalidState(_) => ErrorCode::InvalidState,
            Self::RequestInProgress { .. } => ErrorCode::RequestInProgress,
            Self::LeaseLost { .. } => ErrorCode::LeaseLost,
            Self::UploadTooLarge { .. } => ErrorCode::UploadTooLarge,
            Self::UnsupportedMediaType => ErrorCode::UnsupportedMediaType,
            Self::ImageQuality { .. } => ErrorCode::ImageQuality,
            Self::MissingRequiredShot { .. } => ErrorCode::MissingRequiredShot,
            Self::TooManyImages { .. } => ErrorCode::TooManyImages,
            Self::IdempotencyConflict => ErrorCode::IdempotencyConflict,
            Self::RateLimited { .. } => ErrorCode::RateLimited,
            Self::BudgetExceeded { .. } => ErrorCode::BudgetExceeded,
            Self::Internal(_) => ErrorCode::Internal,
            Self::ProcessingUnavailable { .. } => ErrorCode::ProcessingUnavailable,
            Self::UpstreamUnavailable { .. } => ErrorCode::UpstreamUnavailable,
        }
    }

    /// contracts 2: `true` for the codes it marks retryable, and for `image_quality`,
    /// whose example sends `true` (a retake can pass).
    pub const fn retryable(&self) -> bool {
        matches!(
            self,
            Self::RequestInProgress { .. }
                | Self::ImageQuality { .. }
                | Self::RateLimited { .. }
                | Self::ProcessingUnavailable { .. }
                | Self::UpstreamUnavailable { .. }
        )
    }

    /// contracts 2: sent with `request_in_progress`, `rate_limited` and both `503` codes.
    pub const fn retry_after_s(&self) -> Option<u64> {
        match self {
            Self::RequestInProgress { retry_after_s }
            | Self::RateLimited { retry_after_s }
            | Self::ProcessingUnavailable { retry_after_s, .. }
            | Self::UpstreamUnavailable { retry_after_s, .. } => Some(*retry_after_s),
            _ => None,
        }
    }

    pub fn details(&self) -> Option<serde_json::Value> {
        use serde_json::json;
        let details = match self {
            Self::InvalidRequest { field, problem } => {
                json!({ "field": field, "problem": problem })
            }
            Self::Forbidden { required_role } => json!({ "required_role": required_role }),
            Self::InvalidState(details) => json!(details),
            Self::LeaseLost { job_id } => json!({ "job_id": job_id }),
            Self::UploadTooLarge { max_bytes } => json!({ "max_bytes": max_bytes }),
            Self::UnsupportedMediaType => json!({ "accepted": ACCEPTED_MEDIA_TYPES }),
            Self::ImageQuality {
                problem,
                blur_score,
                exposure,
            } => json!({
                "problem": problem, "blur_score": blur_score,
                "exposure": exposure, "hint": problem.hint(),
            }),
            Self::MissingRequiredShot { missing } => json!({ "missing": missing }),
            Self::TooManyImages { kind, max } => json!({ "kind": kind, "max": max }),
            Self::BudgetExceeded { budget, resets_at } => {
                json!({ "budget": budget, "resets_at": resets_at })
            }
            Self::UpstreamUnavailable { upstream, .. } => json!({ "upstream": upstream }),
            Self::Unauthenticated
            | Self::InvalidSignature
            | Self::NotFound
            | Self::RequestInProgress { .. }
            | Self::IdempotencyConflict
            | Self::RateLimited { .. }
            | Self::Internal(_)
            | Self::ProcessingUnavailable { .. } => return None,
        };
        Some(details)
    }

    /// The source chain of a `5xx`, for the log line; never part of the body.
    fn cause(&self) -> Option<String> {
        match self {
            Self::Internal(source)
            | Self::ProcessingUnavailable { source, .. }
            | Self::UpstreamUnavailable { source, .. } => Some(Chain(&**source).to_string()),
            _ => None,
        }
    }

    pub fn parts(&self) -> ErrorParts {
        ErrorParts {
            code: self.code(),
            message: self.to_string(),
            retryable: self.retryable(),
            retry_after_s: self.retry_after_s(),
            details: self.details(),
            cause: self.cause(),
        }
    }
}

/// Everything of the body except `request_id`, left on the response for `render_errors`.
#[derive(Debug, Clone)]
pub struct ErrorParts {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    pub retry_after_s: Option<u64>,
    pub details: Option<serde_json::Value>,
    /// The source chain, logged by `render_errors`; set for `5xx` only.
    pub cause: Option<String>,
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let mut response = axum::response::IntoResponse::into_response(self.status());
        response.extensions_mut().insert(self.parts());
        response
    }
}
// end: ApiError

// copy of: rust-contract-types/primitives.md Timestamp
/// RFC 3339 in UTC, whole seconds, `Z` suffix: `"2026-10-18T14:03:22Z"` (contracts 1).
/// Keeps full precision in memory; only the wire form is truncated. Whole seconds are
/// this skill's choice, not the contracts' (contracts 12, question 12).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, utoipa::ToSchema, schemars::JsonSchema,
)]
#[schema(value_type = String, format = DateTime, example = "2026-10-18T14:03:22Z")]
#[schemars(transparent)]
pub struct Timestamp(chrono::DateTime<chrono::Utc>);

impl Timestamp {
    pub fn now() -> Self {
        Self(chrono::Utc::now())
    }

    /// For the store, which binds `chrono::DateTime<Utc>` (database 2).
    pub const fn into_inner(self) -> chrono::DateTime<chrono::Utc> {
        self.0
    }
}

impl From<chrono::DateTime<chrono::Utc>> for Timestamp {
    fn from(t: chrono::DateTime<chrono::Utc>) -> Self {
        Self(t)
    }
}

impl serde::Serialize for Timestamp {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let text = self.0.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        serializer.serialize_str(&text)
    }
}

impl<'de> serde::Deserialize<'de> for Timestamp {
    /// Accepts what chrono's RFC 3339 parser accepts and converts it to UTC.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <chrono::DateTime<chrono::Utc> as serde::Deserialize>::deserialize(deserializer).map(Self)
    }
}
// end: Timestamp

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
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        response::{IntoResponse, Response},
        routing::get,
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    const SCAN: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5F";

    fn event(seq: u64) -> ScanEvent {
        ScanEvent {
            seq,
            kind: "stage_started",
            data: format!(r#"{{"v":1,"seq":{seq}}}"#),
        }
    }

    /// The serialised writer, reduced: store the event and the scan's state together,
    /// then broadcast.
    fn publish(state: &AppState, seq: u64, rests: bool) {
        state.store.events.lock().unwrap().push(event(seq));
        if rests {
            state.store.at_rest.store(true, Ordering::SeqCst);
        }
        let live = Arc::new(Live {
            event: event(seq),
            rests,
        });
        let _ = state.hub.sender("s1", Hub::CAPACITY).send(live);
    }

    fn state() -> Arc<AppState> {
        Arc::new(AppState {
            store: Store::default(),
            hub: Hub::default(),
            shutdown: CancellationToken::new(),
        })
    }

    async fn body_text(response: Response) -> String {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    fn ids(text: &str) -> Vec<&str> {
        text.lines()
            .filter_map(|line| line.strip_prefix("id: "))
            .collect()
    }

    /// Collects the whole body; fails if the stream doesn't end within 2 s.
    async fn ids_until_closed(state: &Arc<AppState>, after: u64) -> Vec<String> {
        let body = Sse::new(event_stream(state.clone(), "s1".to_owned(), after))
            .into_response()
            .into_body();
        let collected = tokio::time::timeout(Duration::from_secs(2), body.collect())
            .await
            .expect("the stream must close");
        let bytes = collected.unwrap().to_bytes();
        ids(std::str::from_utf8(&bytes).unwrap())
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    async fn call(state: &Arc<AppState>, uri: &str, last_event_id: Option<&str>) -> Response {
        let app = Router::new()
            .route("/scans/{id}/events", get(scan_events))
            .with_state(state.clone());
        let mut request = Request::get(uri);
        if let Some(value) = last_event_id {
            request = request.header("last-event-id", value);
        }
        app.oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    fn error_parts(response: &Response) -> &ErrorParts {
        response.extensions().get::<ErrorParts>().unwrap()
    }

    fn field(response: &Response) -> &serde_json::Value {
        &error_parts(response).details.as_ref().unwrap()["field"]
    }

    #[tokio::test]
    async fn errors_before_the_stream_are_ordinary_error_responses() {
        let state = state();
        state.store.readable.lock().unwrap().push(SCAN.to_owned());
        let uri = format!("/scans/{SCAN}/events");

        let response = call(&state, "/scans/not-a-ulid/events", None).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(field(&response), "id");

        let other = "/scans/01JAX3K9T6R2B8V0QZ4M7D1E5G/events";
        let response = call(&state, other, None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = call(&state, &uri, Some("seven")).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(field(&response), "Last-Event-ID");

        let response = call(&state, &uri, Some("3")).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "text/event-stream");

        // A store error is `500 internal`, not a `404` or an empty stream.
        state.store.down.store(true, Ordering::SeqCst);
        let response = call(&state, &uri, None).await;
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error_parts(&response).code, ErrorCode::Internal);
    }

    #[tokio::test]
    async fn event_wire_format_matches_contracts_4() {
        let finding = ScanEvent {
            seq: 17,
            kind: "finding",
            data: r#"{"v":1,"seq":17,"type":"finding"}"#.to_owned(),
        };
        let sse = Sse::new(futures::stream::iter([Ok::<_, Infallible>(
            finding.to_sse(),
        )]));
        let response = sse.into_response();
        assert_eq!(response.headers()["content-type"], "text/event-stream");
        assert_eq!(response.headers()["cache-control"], "no-cache");
        assert_eq!(
            body_text(response).await,
            "id: 17\nevent: finding\ndata: {\"v\":1,\"seq\":17,\"type\":\"finding\"}\n\n"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn keepalive_is_a_keepalive_comment_after_15_idle_seconds() {
        let keep_alive = KeepAlive::new().text("keepalive");
        let sse = Sse::new(futures::stream::pending::<Result<Event, Infallible>>())
            .keep_alive(keep_alive);
        let mut body = sse.into_response().into_body();
        let start = tokio::time::Instant::now();
        let frame = body.frame().await.unwrap().unwrap();
        assert_eq!(frame.into_data().unwrap(), ": keepalive\n\n");
        assert_eq!(start.elapsed(), Duration::from_secs(15));
    }

    #[tokio::test]
    async fn resume_replays_then_forwards_live_without_duplicates() {
        let state = state();
        for seq in 1..=3 {
            publish(&state, seq, false);
        }
        // The app sent Last-Event-ID: 1.
        let sse = Sse::new(event_stream(state.clone(), "s1".to_owned(), 1));
        let mut body = sse.into_response().into_body();
        for expected in ["id: 2\n", "id: 3\n"] {
            let frame = body.frame().await.unwrap().unwrap().into_data().unwrap();
            assert!(frame.starts_with(expected.as_bytes()));
        }
        // Live: a repeat of 3, then 4, then 5, which brings the scan to rest.
        let repeat = Arc::new(Live {
            event: event(3),
            rests: false,
        });
        let _ = state.hub.sender("s1", Hub::CAPACITY).send(repeat);
        publish(&state, 4, false);
        publish(&state, 5, true);
        // `collect` returns only because the stream ends after 5.
        let rest = body.collect().await.unwrap().to_bytes();
        assert_eq!(ids(std::str::from_utf8(&rest).unwrap()), ["4", "5"]);
    }

    #[tokio::test]
    async fn a_resting_event_committed_during_the_replay_closes_the_stream() {
        let state = state();
        publish(&state, 1, false);
        // The writer commits 2, the resting event, after the stream read the state
        // (not at rest) and before it reads the events, so the replay sends 2.
        let writer = state.clone();
        *state.store.racing_write.lock().unwrap() = Some(Box::new(move || {
            publish(&writer, 2, true);
        }));
        assert_eq!(ids_until_closed(&state, 0).await, ["1", "2"]);
    }

    #[tokio::test]
    async fn lagged_subscriber_replays_from_the_store() {
        let state = state();
        // A tiny channel so the subscriber lags.
        let _ = state.hub.sender("s1", 2);
        publish(&state, 1, false);
        let sse = Sse::new(event_stream(state.clone(), "s1".to_owned(), 0));
        let mut body = sse.into_response().into_body();
        let first = body.frame().await.unwrap().unwrap().into_data().unwrap();
        assert!(first.starts_with(b"id: 1\n"));
        // Ten events while the client isn't reading: the receiver lags.
        for seq in 2..=11 {
            publish(&state, seq, seq == 11);
        }
        let rest = body.collect().await.unwrap().to_bytes();
        assert_eq!(
            ids(std::str::from_utf8(&rest).unwrap()),
            ["2", "3", "4", "5", "6", "7", "8", "9", "10", "11"]
        );
    }

    #[tokio::test]
    async fn a_closed_channel_resubscribes_instead_of_ending() {
        let state = state();
        publish(&state, 1, false);
        let sse = Sse::new(event_stream(state.clone(), "s1".to_owned(), 0));
        let mut body = sse.into_response().into_body();
        let first = body.frame().await.unwrap().unwrap().into_data().unwrap();
        assert!(first.starts_with(b"id: 1\n"));
        // The hub drops the scan's only sender: the receiver gets `Closed`.
        state.hub.channels.lock().unwrap().clear();
        publish(&state, 2, false);
        publish(&state, 3, true);
        let rest = body.collect().await.unwrap().to_bytes();
        assert_eq!(ids(std::str::from_utf8(&rest).unwrap()), ["2", "3"]);
    }

    #[tokio::test]
    async fn scan_at_rest_replays_and_closes() {
        let state = state();
        publish(&state, 1, true);
        assert_eq!(ids_until_closed(&state, 0).await, ["1"]);
    }

    #[tokio::test]
    async fn a_store_error_ends_the_stream_instead_of_waiting() {
        let state = state();
        publish(&state, 1, false);
        state.store.down.store(true, Ordering::SeqCst);
        assert_eq!(ids_until_closed(&state, 0).await, Vec::<String>::new());
    }

    #[tokio::test]
    async fn shutdown_ends_the_stream() {
        let state = state();
        let sse = Sse::new(event_stream(state.clone(), "s1".to_owned(), 0));
        let body = sse.into_response().into_body();
        state.shutdown.cancel();
        let collected = tokio::time::timeout(Duration::from_secs(1), body.collect()).await;
        assert!(
            collected.is_ok(),
            "the stream must end when the token is cancelled"
        );
    }
}
```

What to check in a review of this endpoint (each point is sourced in section 1):

- The scan ID is `ApiPath<ScanId>`; membership, `Last-Event-ID` and store errors are answered before the stream starts, as ordinary `ErrorBody` responses.
- Subscribe happens before the first store read, and the replay is driven by `last`, not by the `Last-Event-ID` header after the first pass.
- Every `yield` updates `last` first; the live branch drops `seq <= last`, except that a dropped event marked as resting ends the stream.
- `Lagged`, a `seq` gap and `Closed` all lead back to the replay, never to `return`.
- The `select!` includes the shutdown token.
- A store error ends the stream with a `warn`; no `unwrap_or(false)` and no `?` inside `stream!` that would turn it into an `Err` item.

## 4. Chat streams

`POST /conversations/{id}/messages` also answers with SSE, but it is **not resumable**: no `id:` lines, no replay, an `error` event ends a failed turn ([contracts 8, sending a message](../../../docs/architecture/contracts.md#sending-a-message)). The same `Sse` and `KeepAlive` apply; leave out `.id(...)`. When the client disconnects, hyper drops the response body, which drops the stream; the turn's code must notice (the stream's drop, or a `CancellationToken` cancelled from a drop guard) to stop the LLM call and store the message with `interrupted: true` ([contracts 8](../../../docs/architecture/contracts.md#sending-a-message)). That task structure is in [rust-async](../rust-async/SKILL.md#4-select-and-cancellation).
