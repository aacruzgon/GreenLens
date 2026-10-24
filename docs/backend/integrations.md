# PDM Backend: `integrations` crate

> **Status:** Draft v0.8.2, 2026-10-24. v0.8.2: for a route with the exchange before the message (`classify_with_context`), Jev's `state` is `{"earlier": {"gardener", "assistant"}, "message"}` and each question starts with `CONTEXT_PREFIX`, which says to judge `message`; ran on 2026-10-24 at 729 input tokens for a follow-up (section 13). v0.8.1: Jev's client speaks TypeSafe's real API (backend roadmap S5): one `POST /v1/systemone` on a pinned model (`JEV_MODEL`, default `jev-1.14.0`) with the text as `state` and a `noul` question per route label, one `noul` for the memory gate (`remember`, with `skip` its complement) and one `choice` for the chunk topic; the request, response, refusals and timeout are ✓ against the live API and wiremock, and the memory labels are now `domain::ports::MEMORY_LABELS` (`remember`, `skip`), the ones `agent`'s gate reads, instead of the stub's own (section 13; open question 8 settled). v0.8: live site data (backend roadmap S2): the fixtures were recorded again on 2026-10-20 and Nominatim's and Zippopotam's answers added; where SoilGrids has no data at the point, `slc::SoilLandscapes` answers from Soil Landscapes of Canada v3.2, read offline from a 0.9 MB file, before any nearby probe (architecture open decision 10, proposed); a GPS garden whose reverse lookup fails is labelled by its time zone ("Toronto time zone") instead of failing with `503`, the label falls back to the county, and Nominatim's `{"error": …}` answer is no place (section 19, items 2 and 4). v0.7.5: Slack's re-read starts at `newest` when that is older than its 7 days, so a sync after a week or more without one skips no message, and the gap doesn't count toward a thread's 30 quiet days (found by EON T-E4.5's Slack half). v0.7.4: Jev's route task sends `FARM_ROUTE_LABELS`, the route labels with `farm_records`, for a garden with farm records (gap G10); the label lists come from `domain::ports`, and Jev keeps only the scores of the labels it sent. v0.7.3: `google_drive::DriveSource` implements the `FarmSource` port (E4.3): the start token taken before a recursive first listing, then `changes.list` read to its last page with only the files under the folder kept, through a cursor that holds the folder tree and the files held, so a folder moved out takes its files and one moved in is listed; 25 files a page; `FolderListing::tree` records each folder's parent, `DriveConfig::era` points the client at a simulator, and `era_sim::fidelity` reads a simulator's fidelity. After its audit: `changes_page` reads one page of `changes.list` and the sync's backlog goes a page at a time; a change with no file id (a shared drive's own) is skipped; a refused change token or an unreadable cursor resyncs with a listing that reports every file in `FarmPage::listed`; `files.get` (`DriveClient::file`) checks the chosen folder first; past 500 files or folders is `too_large`; a file whose read keeps failing is tried once a run, then once a day after 5; a binary `text/plain` file is kept by name. v0.7.2: `tests/era_hosted.rs` runs the two clients against the team's hosted Era company in the nightly `era-hosted` workflow, five metered calls a run, and the simulator command leaves out the strict and hosted tests (section 18). v0.7.1: a Slack thread's cursor keeps `reply_count`, so a deleted reply reads the thread again, and the end of a pass checks older threads for new replies (`conversations.replies`, `limit=1`, round-robin) up to the sync's `probe_limit`, two per connector run; a failed check other than a refused token keeps the page, a deleted parent is forgotten and its record removed, an answer for another message is skipped, and threads past 180 days are forgotten before any check; transport errors drop the request URL. v0.7: `slack::SlackSource` implements the `FarmSource` port for the sync (E4.2): one history page a call, the replies of changed threads, a name per user and team for a day, and its own cursor; `FarmClientError::into_source_error` maps a client failure to the port's error (section 15). v0.6: adds `farm`, `slack` and `google_drive`, the standalone clients for the farm connectors, and `era_sim`, the test support for Era's simulators (section 15, which moves the sections after it down one); the Era tests run in one serial nextest group (section 18). v0.5: adds `agent_memory`, the `BeliefStore` over `SurrealDB` Agent Memory's REST API (section 14; the sections after it moved down one), 13 tests; 77 in all. v0.4: checked against Anthropic's documentation and the first live calls: the sanitiser turns `oneOf` into `enum` or `anyOf` (refused live), turns a nullable enum into one `enum` holding `null` so it isn't a union-typed parameter, and drops `format`s the API doesn't list; token counts that are `null` read as 0 and a missing `usage` defaults; `StopReason::ModelContextWindowExceeded`; 64 tests. v0.3.3: the Clerk tests generate their RSA key pair at run time, so no key file is committed, and the Svix tests use a made-up secret. v0.3.2: section 4 says the worker builds its client with a 600 s read timeout, not the 60 s default, because its VLM and hosted LLM calls aren't streamed, and their 240 s and 300 s deadlines end a slow call; the skills table became a plain list of the extra dependencies. v0.3.1: a JWKS outage is `upstream: "auth"`, as `api` answers it. v0.3: the summary adds `past_7_days.longest_humid_run_days` (the longest run of humid days among the 7 before today, from the hourly history), and the failure tables follow the contracts: a weather failure fails a scan's run with `upstream_unavailable`, `user.created` changes nothing, and `SoilGrids` is retried 15 minutes after an outage and 7 days after no data. v0.2: `frost_risk` and `hours_rh_over_90` come from the shared `domain::rules::weather::WeatherThresholds` (`OpenMeteoConfig::thresholds`): frost is now a 2 m minimum ≤ 0.5 °C, the value `api`'s frost trigger used. v0.1: every outbound HTTP adapter (the Anthropic Messages client, the OpenAI-compatible VLM client, Open-Meteo, SoilGrids, the geocoders, Clerk, Svix, Expo Push and Jev), compiled on Rust 1.99.1 and tested against live fixtures and mock servers.

Companion to the [backend architecture](README.md) and the [`domain` crate](domain.md), whose ports this crate implements. Every listing below is copied from the compiled source of `crates/integrations`. `cargo check`, `cargo clippy -p integrations --all-targets` and `cargo nextest run -p integrations` all pass: 197 tests, against `wiremock` servers and the live responses recorded on 2026-10-15 and again on 2026-10-20.

## Contents

1. [Purpose and boundaries](#1-purpose-and-boundaries)
2. [Dependencies](#2-dependencies)
3. [Module layout](#3-module-layout)
4. [Shared HTTP: client, retries, pacing, SSE](#4-shared-http-client-retries-pacing-sse)
5. [Anthropic Messages API](#5-anthropic-messages-api)
6. [OpenAI-compatible VLM servers](#6-openai-compatible-vlm-servers)
7. [Open-Meteo weather](#7-open-meteo-weather)
8. [SoilGrids](#8-soilgrids)
9. [Geocoding](#9-geocoding)
10. [Clerk](#10-clerk)
11. [Svix and the auth webhook](#11-svix-and-the-auth-webhook)
12. [Expo Push](#12-expo-push)
13. [Jev](#13-jev)
14. [Agent Memory](#14-agent-memory)
15. [Farm sources: Slack and Google Drive](#15-farm-sources-slack-and-google-drive)
16. [Concurrency and lifetimes](#16-concurrency-and-lifetimes)
17. [Errors](#17-errors)
18. [Testing](#18-testing)
19. [Open questions](#19-open-questions)
20. [Sources](#20-sources)

---

## 1. Purpose and boundaries

**An addition to architecture 3.** [Architecture 3](../architecture/README.md#rust-crates) first listed seven library crates. This design adds `integrations`, one crate for every outbound HTTP call, so the worker's fallback adapter and `agent` share one LLM client and `api` wires every provider from one place. It depends only on `domain` ([crate graph](README.md#5-crate-graph-and-dependency-rules)).

**What it owns.**

| Upstream | Module | Implements | Used by |
|---|---|---|---|
| Anthropic Messages API | `anthropic` | a typed client (`agent` builds `ChatModel` on it) | `agent`, the `fallback` worker |
| vLLM 0.31 and llama.cpp | `openai_compat` | a typed client (the worker builds `VisionModel` on it) | the `vlm` workers |
| Open-Meteo forecast | `open_meteo` | `WeatherProvider` | `api` (scans, `GET /gardens/{id}/weather`, the scheduler) |
| ISRIC SoilGrids | `soilgrids` | `SoilProvider` | `api` (garden setup) |
| Soil Landscapes of Canada (an embedded file, no HTTP) | `slc` | inside `SoilGrids`, where it has no data | `api` (garden setup) |
| Open-Meteo geocoding, Nominatim, Zippopotam, `tzf-rs` | `geocode` | `Geocoder` | `api` (`POST /gardens`, `PATCH /gardens/{id}`) |
| Clerk JWKS and Backend API | `clerk` | `AuthVerifier`, `AuthAdmin` | `api` (every request, `DELETE /me`) |
| Svix | `svix` | `WebhookVerifier` | `api` (`POST /webhooks/auth`) |
| Expo Push | `expo_push` | `Notifier` | `api` |
| Jev | `jev` | `TextClassifier` | `api`, when configured |
| `SurrealDB` Agent Memory | `agent_memory` | `BeliefStore` | `api` (memory mirror, beliefs endpoint, `search_memory`), when configured |
| Slack Web API | `slack` | a typed client, and `SlackSource`, the `FarmSource` port on it | `api`, the `connectors` task (E4) |
| Google Drive API and OAuth | `google_drive` | a typed client, and `DriveSource`, the `FarmSource` port on it | `api`, the `connectors` task (E4) |

**What it never does.** It reads no database and no object store, and it holds no business rule. A failed fetch is reported, and the caller decides what to do: skip the gardens in a weather cell, set `soil_profile_status`, answer `503 upstream_unavailable`. It never reads the system clock for domain time: adapters that stamp `fetched_at` take an `Arc<dyn Clock>`, and token checks take `now` as an argument. No `reqwest` type appears in a public signature. Errors leave as the port's enum ([domain 9](domain.md#9-boundary-traits)), or as the client's own enum for the two LLM clients. The library error is kept as the `source`.

**Dependencies beyond the common pins.** `reqwest` adds the `stream` feature (`bytes_stream()` for SSE) and `query` (`RequestBuilder::query` needs it in 0.13 ✓) to `json`. The webhook's HMAC and constant-time comparison use `aws-lc-rs` (`hmac`, `constant_time`), already in the tree. Tests run real HTTP through the real `reqwest` client against `wiremock` 0.6.5 (dev only), headers, query strings and SSE bodies included. The project's Rust skills follow this crate, the crate list included.

---

## 2. Dependencies

```toml
[package]
name = "integrations"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
domain.workspace = true

async-trait.workspace = true
aws-lc-rs.workspace = true
bytes.workspace = true
chrono.workspace = true
data-encoding.workspace = true
futures.workspace = true
jsonwebtoken.workspace = true
moka.workspace = true
rand.workspace = true
reqwest.workspace = true
secrecy.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tokio.workspace = true
tracing.workspace = true
tzf-rs.workspace = true
ulid = { workspace = true, optional = true }

[features]
# Test support for Era's simulators (`era_sim`). Never enabled in a release build.
era-sim = ["dep:ulid"]

[dev-dependencies]
domain = { workspace = true, features = ["test-util", "mock"] }
integrations = { path = ".", features = ["era-sim"] }
svix.workspace = true
tokio = { workspace = true, features = ["test-util"] }
ulid.workspace = true
wiremock.workspace = true
```

The root `Cargo.toml` pins these versions (all checked on crates.io on 2026-10-15):

| Crate | Version, features | Why |
|---|---|---|
| `domain` | path | The ports, their error enums and the types they carry |
| `reqwest` | 0.13.5, default features + `json`, `stream`, `query` | The HTTP client. Its default TLS in 0.13 is rustls with `aws-lc-rs` and the platform verifier ✓ |
| `secrecy` | 0.10.3 | `SecretString` for every key: its `Debug` prints a placeholder, so a logged config never shows a key |
| `rand` | 0.10.3 | Jitter in retry back-off (`rand::random_range`) ✓ |
| `data-encoding` | 2.11.1 | Base64 for images and Svix, through `encode_display`, which writes straight into the serializer ✓ |
| `aws-lc-rs` | 1.18.2 | HMAC-SHA256 and constant-time comparison for Svix; already in the tree through rustls and `jsonwebtoken` |
| `jsonwebtoken` | 11.1.0, `aws_lc_rs`, `use_pem` (no defaults) | RS256 verification of Clerk session tokens. Version 11 needs exactly one crypto backend; with none it panics at the first `decode` (research note) |
| `moka` | 0.12.16, `future` | The JWKS key cache and the geocoder's 30-day cache |
| `tzf-rs` | 2.1.3, `bundled` only | Offline coordinates → IANA zone. The default features pull `clap` for its CLI, so they are off |
| `async-trait`, `futures`, `bytes`, `serde`, `serde_json`, `thiserror`, `tokio`, `tracing`, `chrono` | workspace | As in every crate |
| `svix` (dev) | 2.8.0 | The test oracle for the manual verifier. As a normal dependency it would add about 19 crates, and with default features off it doesn't compile (research note) |
| `wiremock` (dev) | 0.6.5 | Mock HTTP servers |

---

## 3. Module layout

```
crates/integrations/
├─ Cargo.toml
├─ data/
│  └─ slc_v3r2.bin             Soil Landscapes of Canada v3.2, built by tools/slc/build_slc.py
├─ fixtures/                   live responses, recorded again on 2026-10-20
│  ├─ openmeteo_forecast_multi.json   Toronto and Ottawa, past_days=7, forecast_days=16, 23:05 UTC
│  ├─ geocoding_toronto.json          Open-Meteo's geocoder, "Toronto", count=3
│  ├─ nominatim_search_m5v3l9.json, nominatim_reverse_{toronto,niagara_county,ocean}.json
│  ├─ zippopotam_m5v.json
│  ├─ soilgrids_{iowa,guelph,toronto}.json   six depths: one with data, two all-null urban pixels
│  ├─ soilgrids_toronto_5km_north.json       the first point near central Toronto with data
│  └─ soilgrids_probe_toronto_{2km,5km}_north.json   one-value probes: null, then data
└─ src/
   ├─ lib.rs
   ├─ http.rs                  the shared client, RetryPolicy, retry(), Pacer, TransportError
   ├─ sse.rs                   SseParser and the byte-stream → event-stream adapter
   ├─ anthropic.rs             re-exports; module docs
   ├─ anthropic/
   │  ├─ request.rs            MessagesRequest<'a> and its borrowed blocks; Base64<'a>
   │  ├─ response.rs           Message, ResponseBlock, StopReason, Usage (owned)
   │  ├─ stream.rs             StreamEvent, MessageStream ('static), MessageAccumulator, Output
   │  ├─ schema.rs             sanitize_schema: strips or rewrites what structured output rejects
   │  ├─ error.rs              AnthropicError, retry rules, → ModelError
   │  ├─ client.rs             AnthropicConfig, ModelIds, AnthropicClient
   │  └─ tests.rs              wiremock tests
   ├─ openai_compat.rs         ChatCompletionRequest<'a>, DataUrl<'a>, OpenAiCompatClient
   ├─ open_meteo.rs            OpenMeteo: WeatherProvider; fetch_cells (the batch rule)
   ├─ open_meteo/
   │  ├─ wire.rs               Forecast, Hourly, Daily, ForecastResponse
   │  ├─ summary.rs            cell_weather(), moisture_0_9cm()
   │  └─ tests.rs
   ├─ soilgrids.rs             SoilGrids: SoilProvider; to_depths, nearby_points
   ├─ slc.rs                   SoilLandscapes: the offline Soil Landscapes of Canada lookup
   ├─ geocode.rs               CompositeGeocoder: Geocoder; TimeZoneFinder, PostalCode, round_for
   ├─ geocode/tests.rs
   ├─ clerk.rs                 ClerkVerifier: AuthVerifier; ClerkAdmin: AuthAdmin
   ├─ svix.rs                  SvixVerifier: WebhookVerifier; map_clerk_event
   ├─ agent_memory.rs          AgentMemory: BeliefStore (REST: scopes, facts, entities, query, forget)
   ├─ farm.rs                  FarmClientError, FarmErrorCode: what both farm clients share
   ├─ slack.rs                 SlackClient, SlackConfig, permalink
   ├─ slack/
   │  ├─ source.rs             SlackSource: the FarmSource port for the sync
   │  ├─ wire.rs               Slack's response shapes (private)
   │  └─ tests.rs              wiremock tests
   ├─ google_drive.rs          DriveClient, DriveConfig, ExportFormat; the byte and count caps
   ├─ google_drive/
   │  ├─ source.rs             DriveSource: the FarmSource port for the sync (tests in source/tests.rs)
   │  ├─ wire.rs               Drive's and Google's response shapes
   │  ├─ pkce.rs               Pkce: verifier and S256 challenge
   │  └─ tests.rs              wiremock tests
   ├─ era_sim.rs               feature `era-sim`, test support only: Tenant, Faults, fixture()
   ├─ expo_push.rs             ExpoPush: Notifier (push, check_receipts)
   └─ jev.rs                   JevClassifier: TextClassifier (TypeSafe System One)
```

`tests/` holds the Era tests (`era_fixture.rs`, `era_slack.rs`, `era_drive.rs`, `era_strict.rs`, and `era_hosted.rs` for the hosted company; section 18). `era_sim` is compiled only with the `era-sim` feature, which the crate enables for its own tests, so no release build contains it.

---

## 4. Shared HTTP: client, retries, pacing, SSE

**One client.** `reqwest::Client` is an `Arc` inside, so `api` builds one at start and clones it into every adapter, and the worker does the same. The client has a 5 s connect timeout and, by default, a 60 s *read* timeout. The read timeout is the longest gap between two reads, restarts on every chunk, and also covers the wait for the response headers. The worker builds its client with a 600 s read timeout instead, because its VLM and hosted LLM calls aren't streamed: the server sends nothing until the answer is complete ([worker 1](worker.md#1-purpose-and-boundaries)). The client has no total timeout, because an LLM stream can last minutes. Each non-streaming call sets its own deadline with `RequestBuilder::timeout`; in the worker that is 240 s for a VLM call (`OpenAiCompatConfig.timeout`) and 300 s for a hosted LLM call (`AnthropicConfig.request_timeout`), both under the 600 s read timeout, so the deadline is what ends a slow call.

```rust
/// The `User-Agent` of every request. Nominatim requires an identifying one; the others log it.
pub const USER_AGENT: &str = concat!(
    "PDM/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/aacruzgon/GreenLens)"
);

/// Settings of the shared client.
#[derive(Debug, Clone)]
pub struct HttpConfig {
    /// TCP and TLS connect deadline.
    pub connect_timeout: Duration,
    /// The longest gap between two reads of one response. Anthropic sends `ping` events while
    /// the model thinks, so 60 s is safe for streams and longer than `SoilGrids`' slowest query.
    pub read_timeout: Duration,
    /// How long an idle pooled connection is kept.
    pub pool_idle_timeout: Duration,
    /// The `User-Agent` header.
    pub user_agent: String,
}

/// Builds the shared client. Call once at start and clone it into every adapter.
pub fn client(config: &HttpConfig) -> Result<reqwest::Client, HttpBuildError> {
    reqwest::Client::builder()
        .connect_timeout(config.connect_timeout)
        .read_timeout(config.read_timeout)
        .pool_idle_timeout(config.pool_idle_timeout)
        .user_agent(config.user_agent.clone())
        .build()
        .map_err(|e| HttpBuildError(Box::new(e)))
}
```

**Retries.** `retry` runs one call until it succeeds, fails for good, or runs out of retries. Each attempt returns an `Attempt`, so the caller decides per status what is retryable, and the helper decides only how long to wait. The wait is "full jitter": a uniform random duration up to `min(cap, base · 2^n)`. A server's `Retry-After` sets the minimum wait. A `Retry-After` longer than `max_retry_after` gives up instead.

```rust
/// How often and how long to retry one call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Retries after the first attempt; 0 disables retrying.
    pub max_retries: u32,
    /// The first back-off ceiling; it doubles on every retry.
    pub base: Duration,
    /// The largest back-off ceiling.
    pub cap: Duration,
    /// A `Retry-After` longer than this gives up instead of waiting.
    pub max_retry_after: Duration,
}
```

```rust
impl RetryPolicy {
    /// No retries: the caller owns the deadline (the worker, which retries a whole job).
    pub const NONE: Self = Self {
        max_retries: 0,
        base: Duration::ZERO,
        cap: Duration::ZERO,
        max_retry_after: Duration::ZERO,
    };

    /// Two retries, 0.5 s then 1 s ceilings, at most 8 s: what Anthropic's SDKs do.
    pub const STANDARD: Self = Self {
        max_retries: 2,
        base: Duration::from_millis(500),
        cap: Duration::from_secs(8),
        max_retry_after: Duration::from_secs(60),
    };

    /// The wait before retry number `retry` (0-based): "full jitter", a uniform random
    /// duration in `[0, min(cap, base · 2^retry)]`, never shorter than the server's
    /// `Retry-After`. `None` when the server asks for longer than `max_retry_after`.
    pub fn delay(&self, retry: u32, retry_after: Option<Duration>) -> Option<Duration> {
        if let Some(after) = retry_after
            && after > self.max_retry_after
        {
            return None;
        }
        let ceiling = self
            .base
            .saturating_mul(2u32.saturating_pow(retry))
            .min(self.cap);
        let ceiling_ms = u64::try_from(ceiling.as_millis()).unwrap_or(u64::MAX);
        let jitter = Duration::from_millis(if ceiling_ms == 0 {
            0
        } else {
            rand::random_range(0..=ceiling_ms)
        });
        Some(retry_after.map_or(jitter, |after| after.max(jitter)))
    }
}
```

```rust
/// What one attempt of a retried call produced.
#[derive(Debug)]
pub enum Attempt<T, E> {
    /// Success.
    Done(T),
    /// A failure worth another attempt, with the server's `Retry-After` if it sent one.
    Retry {
        /// The error, returned if no retry is left.
        error: E,
        /// The server's requested wait.
        retry_after: Option<Duration>,
    },
    /// A failure that another attempt would repeat.
    Fail(E),
}

/// Runs `attempt` until it is done, fails for good, or the policy runs out.
///
/// Generic rather than boxed: `F` is the closure that makes one attempt (called with the
/// 0-based attempt number), `Fut` the future it returns, `T` and `E` the call's success and
/// error types. Each caller gets a monomorphised copy, so nothing is allocated per attempt and
/// the futures need not be `'static`: `Fut` may borrow the caller's request body. Waits use
/// `tokio::time::sleep`, which tests pause and advance.
pub async fn retry<T, E, F, Fut>(policy: RetryPolicy, mut attempt: F) -> Result<T, E>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Attempt<T, E>>,
{
    let mut n = 0;
    loop {
        match attempt(n).await {
            Attempt::Done(value) => return Ok(value),
            Attempt::Fail(error) => return Err(error),
            Attempt::Retry { error, retry_after } => {
                if n >= policy.max_retries {
                    return Err(error);
                }
                let Some(wait) = policy.delay(n, retry_after) else {
                    return Err(error);
                };
                tracing::debug!(attempt = n + 1, wait_ms = wait.as_millis(), "retrying");
                tokio::time::sleep(wait).await;
                n += 1;
            }
        }
    }
}
```

`retry` is generic rather than boxed. `F` is the closure that makes one attempt, `Fut` is the future it returns, and `T` and `E` are the call's types. Each caller gets its own monomorphised copy, so nothing is allocated per attempt. `Fut` is not required to be `'static`, so an attempt can borrow the caller's request body and client ([Book 10.1, generics](https://doc.rust-lang.org/book/ch10-01-syntax.html)). Waits use `tokio::time::sleep`, so tests pause time and advance it.

**Pacing.** Nominatim allows 1 request a second and SoilGrids 5 a minute. A `Pacer` spaces calls to one upstream. Callers queue on a tokio mutex, which is fair and serves them first come, first served, so concurrent garden creations line up instead of bursting.

```rust
/// Spaces calls to one upstream: each [`Pacer::wait`] returns no sooner than `interval` after
/// the previous one returned. Callers queue on a tokio mutex (fair, first come first served),
/// so concurrent fetches line up instead of bursting. Used for Nominatim's 1 request a second
/// and `SoilGrids`' 5 calls a minute. Time is tokio's, so tests pause it.
#[derive(Debug)]
pub struct Pacer {
    interval: Duration,
    last: tokio::sync::Mutex<Option<tokio::time::Instant>>,
}
```

```rust
/// `Retry-After` in delta-seconds. The HTTP-date form is treated as absent: none of our
/// upstreams sends it, and reading it would need the wall clock outside the `Clock` port.
pub fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

/// Whether a status is worth retrying for an idempotent call: 408, 429 and every 5xx.
pub fn retryable_status(status: StatusCode) -> bool {
    status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
}

/// A transport failure, without exposing `reqwest::Error` in any public signature.
#[derive(Debug, thiserror::Error)]
#[error("{}", if *.timeout { "request timed out" } else { "request failed" })]
pub struct TransportError {
    /// The deadline or read gap ran out.
    pub timeout: bool,
    /// Whether no connection was made (DNS, refused, TLS).
    pub connect: bool,
    /// The client error.
    #[source]
    pub source: BoxError,
}
```

`TransportError` is how a failed connection crosses a public boundary without the `reqwest::Error` type: two flags and a boxed source.

**Server-sent events.** Both LLM upstreams stream SSE. The parser buffers bytes until a whole line arrives, so a chunk may end anywhere, even inside a UTF-8 sequence. It accepts CRLF and LF line ends and skips `:` comments. A test feeds a stream one byte at a time.

```rust
/// One dispatched event.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SseEvent {
    /// The `event:` field; `"message"` when absent.
    pub event: String,
    /// The `data:` lines joined with `\n`.
    pub data: String,
}

/// An incremental parser for the subset of the WHATWG rules both upstreams use: `event:`,
/// `data:` (several lines joined with `\n`), `:` comments, and a blank line to dispatch. CRLF and
/// LF both end a line; a chunk may end anywhere, even inside a UTF-8 sequence, because bytes
/// are buffered until a whole line is in.
#[derive(Debug, Default)]
pub struct SseParser {
    buf: Vec<u8>,
    current: SseEvent,
    has_data: bool,
}

/// Turns a byte stream into an event stream. Generic over the chunk stream `S` and its error
/// `E` so tests feed it hand-made chunks; the output owns everything it needs, so it is
/// `'static` whenever `S` is (an upstream `reqwest::Response::bytes_stream()` is).
pub fn events<S, E>(bytes: S) -> impl Stream<Item = Result<SseEvent, E>> + Send
where
    S: Stream<Item = Result<Bytes, E>> + Send + Unpin,
    E: Send,
{
    stream::unfold(
        (
            bytes,
            SseParser::default(),
            std::collections::VecDeque::new(),
        ),
        |(mut bytes, mut parser, mut pending)| async move {
            loop {
                if let Some(event) = pending.pop_front() {
                    return Some((Ok(event), (bytes, parser, pending)));
                }
                match bytes.next().await {
                    Some(Ok(chunk)) => pending.extend(parser.push(&chunk)),
                    Some(Err(e)) => return Some((Err(e), (bytes, parser, pending))),
                    None => return None,
                }
            }
        },
    )
}
```

`events` is generic over the chunk stream `S` and its error `E`, so tests feed it hand-made chunks. `stream::unfold` moves the byte stream and the parser into its state, so the output borrows nothing: it is `'static` whenever `S` is, and `Response::bytes_stream()` is. Under edition 2024 the `impl Stream` return type would capture any lifetime in scope ([Edition Guide, RPIT capture](https://doc.rust-lang.org/edition-guide/rust-2024/rpit-lifetime-capture.html)), but the signature has no lifetime to capture, so no `use<..>` bound is needed.

---

## 5. Anthropic Messages API

**Endpoint.** `POST {base_url}/v1/messages`, with headers `x-api-key`, `anthropic-version: 2023-06-01`, `content-type: application/json`, and `anthropic-beta` only when configured. Anthropic publishes no Rust SDK, so the client is raw HTTP over serde types (research notes, Anthropic section; *unverified* live, because no key was available on 2026-10-15). The types serialise to the documented JSON, and the tests check that.

**Who calls it.** `agent` implements `ChatModel` on `stream` for chat turns and on `create` for the plan writer, memory extraction and its LLM classifier. The fallback worker's vision adapter calls `create` with image blocks and `output_config.format`. The worker uses the raw client rather than `ChatModel` because it must see `stop_reason` (`refusal`, `max_tokens`).

### Borrowed requests, owned responses

`MessagesRequest<'a>` borrows everything a call needs. **`'a` is the caller's data:** the system prompt, the conversation, the tool schemas and the image bytes. The client serialises the request once into an owned `Bytes` body before its first `.await`, so it never holds the request across the network. Text fields are `Cow<'a, str>` and schemas are `Cow<'a, Value>`. A caller with an owned value moves it in as `Cow::Owned`, for example a sanitised schema or a formatted prompt. A caller with a borrowed one pays nothing ([std `Cow`](https://doc.rust-lang.org/std/borrow/enum.Cow.html)). In `agent`, `ChatModel::stream<'a>(&'a self, request: &'a ChatRequest)` builds a `MessagesRequest<'a>` that borrows the domain request's strings and image `Bytes`.

```rust
/// One `POST /v1/messages` body, without `stream` (the client sets it).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MessagesRequest<'a> {
    /// A model id from configuration, such as `claude-opus-5-5`.
    pub model: Cow<'a, str>,
    /// Output budget, thinking included.
    pub max_tokens: u32,
    /// The system prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<System<'a>>,
    /// The conversation, oldest first, alternating user and assistant.
    pub messages: Vec<InputMessage<'a>>,
    /// Tools the model may call.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Tool<'a>>,
    /// Forced choices (`any`, `tool`) are a 400 on Opus 5.5 and Sonnet 5.5; send `auto` or
    /// nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice<'a>>,
    /// Thinking. Opus 5.5 always thinks adaptively, so `None` (the default) is right for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<Thinking>,
    /// Effort and structured output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig<'a>>,
    /// Top-level automatic prompt caching (the last cacheable block).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    /// An opaque user id (a hash, never an email), for abuse detection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata<'a>>,
}

/// `system`: a string, or text blocks when a block needs `cache_control`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum System<'a> {
    /// Plain text.
    Text(Cow<'a, str>),
    /// Blocks.
    Blocks(Vec<TextBlock<'a>>),
}

/// A `{"type":"text"}` block of `system`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename = "text")]
pub struct TextBlock<'a> {
    /// The text.
    pub text: Cow<'a, str>,
    /// A cache breakpoint after this block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// `cache_control`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename = "ephemeral")]
pub struct CacheControl {
    /// 5 minutes (default) or 1 hour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<CacheTtl>,
}

/// One message.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InputMessage<'a> {
    /// Its author.
    pub role: Role,
    /// Its blocks. Results of parallel tool calls go in one user message.
    pub content: Vec<ContentBlock<'a>>,
}

/// One content block of a request.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock<'a> {
    /// Text.
    Text {
        /// The text.
        text: Cow<'a, str>,
        /// A cache breakpoint.
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// An image; put images before the text that refers to them.
    Image {
        /// The bytes or a URL.
        source: ImageSource<'a>,
        /// A cache breakpoint.
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// A tool call the model made, replayed.
    ToolUse {
        /// `toolu_…`.
        id: Cow<'a, str>,
        /// The tool.
        name: Cow<'a, str>,
        /// Its arguments.
        input: Cow<'a, Value>,
    },
    /// A tool's result.
    ToolResult {
        /// The call it answers.
        tool_use_id: Cow<'a, str>,
        /// The result.
        content: ToolResultContent<'a>,
        /// The tool failed.
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        is_error: bool,
        /// A cache breakpoint.
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// A thinking block, replayed byte for byte (preserved thinking).
    Thinking {
        /// The (possibly empty) summary.
        thinking: Cow<'a, str>,
        /// The signature binding it to this conversation.
        signature: Cow<'a, str>,
    },
    /// A redacted thinking block, replayed byte for byte.
    RedactedThinking {
        /// The encrypted data.
        data: Cow<'a, str>,
    },
}

/// `tool_result.content`: a string or blocks (text and images).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ToolResultContent<'a> {
    /// Text, usually JSON.
    Text(Cow<'a, str>),
    /// Blocks.
    Blocks(Vec<ContentBlock<'a>>),
}

/// `image.source`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource<'a> {
    /// Inline bytes: JPEG, PNG, GIF or WebP, at most 10 MB encoded.
    Base64 {
        /// `image/webp`.
        media_type: Cow<'a, str>,
        /// The raw bytes, encoded on the way out.
        data: Base64<'a>,
    },
    /// A URL the API fetches; it must be reachable from the internet.
    Url {
        /// The URL.
        url: Cow<'a, str>,
    },
}

/// Raw bytes that serialise as standard base64 without an intermediate `String`:
/// `collect_str` hands `data_encoding`'s `Display` adapter to the serializer, which writes it
/// into the body as it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Base64<'a>(pub &'a [u8]);
```

`Base64<'a>` holds raw image bytes and serialises them as base64 through `Serializer::collect_str`, which takes `data_encoding`'s `Display` adapter. `serde_json` writes the text straight into the body, so a 1 MB image becomes about 1.3 MB of body once, and is never also built as a `String`.

```rust
/// A tool definition.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tool<'a> {
    /// The name.
    pub name: Cow<'a, str>,
    /// What it does.
    pub description: Cow<'a, str>,
    /// JSON Schema of the input; sanitise it first when `strict` is set
    /// ([`super::sanitize_schema`]).
    pub input_schema: Cow<'a, Value>,
    /// Guarantees the input validates; needs `additionalProperties: false`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub strict: bool,
    /// A cache breakpoint after the tool list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// `tool_choice`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolChoice<'a> {
    /// The model decides.
    Auto {
        /// One tool call per turn at most.
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        disable_parallel_tool_use: bool,
    },
    /// Some tool must be called (400 on Opus 5.5 and Sonnet 5.5).
    Any,
    /// This tool must be called (400 on Opus 5.5 and Sonnet 5.5).
    Tool {
        /// The tool.
        name: Cow<'a, str>,
    },
    /// No tool may be called.
    None,
}

/// `thinking`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Thinking {
    /// The model decides how much to think.
    Adaptive {
        /// What comes back in thinking blocks.
        #[serde(skip_serializing_if = "Option::is_none")]
        display: Option<ThinkingDisplay>,
    },
    /// No thinking: Haiku 5.5 at effort `high` or lower; a 400 on Opus 5.5.
    Disabled,
}

/// `output_config`.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct OutputConfig<'a> {
    /// How hard the model works.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    /// Structured output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<OutputFormat<'a>>,
}

/// `output_config.effort`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    /// `low`.
    Low,
    /// `medium`, Opus 5.5's default.
    Medium,
    /// `high`.
    High,
    /// `xhigh`.
    Xhigh,
    /// `max`.
    Max,
}

/// `output_config.format`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputFormat<'a> {
    /// The reply is one text block holding JSON that matches the schema. The schema must be
    /// sanitised first ([`super::sanitize_schema`]): unsupported keywords are a 400.
    JsonSchema {
        /// The schema.
        schema: Cow<'a, Value>,
    },
}

/// The serialised body: the caller's request plus `stream`. Two lifetimes: `'r` is the borrow
/// of the request for the moment of serialising, `'a` the data the request itself borrows.
/// They differ because the client only holds the request for one call while its data lives as
/// long as the caller's context.
#[derive(Serialize)]
pub(crate) struct Body<'r, 'a> {
    #[serde(flatten)]
    pub request: &'r MessagesRequest<'a>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub stream: bool,
}
```

**`Body<'r, 'a>` has two lifetimes** because two different borrows meet. `'r` borrows the request for the moment of serialising. `'a` is what the request borrows, and it lives as long as the caller's context. One lifetime would force the request's borrow to last as long as its data, which an `&'r MessagesRequest<'a>` created inside `create` cannot do. `#[serde(flatten)]` adds `stream` beside the request's fields.

**Model rules that shape the request** (research notes; *unverified* live). Opus 5.5 always thinks adaptively: send no `thinking` (or `adaptive`), never `disabled` or `budget_tokens`. Forced `tool_choice` (`any`, `tool`) is a 400 on Opus 5.5 and Sonnet 5.5: send `auto` or nothing. Assistant prefill is a 400. Non-default `temperature` and `top_p` are rejected, so the request type has neither. Thinking blocks are bound to the conversation that produced them ("preserved thinking"), so a tool loop replays them unchanged with `Message::replay`. `agent` carries them as `ContentPart::Reasoning` (a domain addition by the agent author).

### Responses

```rust
/// A whole message: the non-streaming response, or what [`super::MessageAccumulator`] builds
/// from a stream.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Message {
    /// `msg_…`.
    pub id: String,
    /// The model that answered.
    pub model: String,
    /// Always `assistant`.
    pub role: Role,
    /// The blocks, in order.
    pub content: Vec<ResponseBlock>,
    /// Why it stopped; `None` only while streaming.
    pub stop_reason: Option<StopReason>,
    /// The refusal category and similar details.
    #[serde(default)]
    pub stop_details: Option<Value>,
    /// Token counts. Defaulted: a documented stream example sends `message_start` without it.
    #[serde(default)]
    pub usage: Usage,
}

/// A borrowed view of one tool call in a [`Message`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolUse<'m> {
    /// `toolu_…`.
    pub id: &'m str,
    /// The tool.
    pub name: &'m str,
    /// Its arguments.
    pub input: &'m Value,
}

/// One block of a response.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseBlock {
    /// Text.
    Text {
        /// The text.
        #[serde(default)]
        text: String,
    },
    /// A tool call. Streamed, `input` starts as `{}` and is filled at `content_block_stop`.
    ToolUse {
        /// `toolu_…`.
        id: String,
        /// The tool.
        name: String,
        /// Its arguments.
        #[serde(default)]
        input: Value,
    },
    /// Thinking: replay it unchanged on the next turn of a tool loop.
    Thinking {
        /// The summary, empty with `display: omitted`.
        #[serde(default)]
        thinking: String,
        /// The signature.
        #[serde(default)]
        signature: String,
    },
    /// Redacted thinking: replay it unchanged.
    RedactedThinking {
        /// The encrypted data.
        data: String,
    },
    /// A block type this client does not model (server tools, new types). Ignored.
    #[serde(other)]
    Unknown,
}

/// `stop_reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Complete.
    EndTurn,
    /// `max_tokens` reached: structured output may be cut off.
    MaxTokens,
    /// A stop sequence matched.
    StopSequence,
    /// It wants tool results.
    ToolUse,
    /// A long server-tool turn paused; send it back to continue.
    PauseTurn,
    /// A safety classifier declined, with HTTP 200; `stop_details.category` says why. The
    /// output may not match the schema.
    Refusal,
    /// The conversation filled the model's context window.
    ModelContextWindowExceeded,
    /// A reason added after this client was written.
    #[serde(other)]
    Other,
}

/// Token counts. The API reference types the cache counts as `number or null`; a null reads as
/// 0 in every field. In a stream, `message_start` carries the input counts and a placeholder
/// `output_tokens`, and each `message_delta` carries **cumulative** counts: the accumulator
/// replaces, never adds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub struct Usage {
    /// Uncached input tokens.
    #[serde(default, deserialize_with = "null_as_zero")]
    pub input_tokens: u32,
    /// Output tokens, thinking included.
    #[serde(default, deserialize_with = "null_as_zero")]
    pub output_tokens: u32,
    /// Input tokens written to the cache.
    #[serde(default, deserialize_with = "null_as_zero")]
    pub cache_creation_input_tokens: u32,
    /// Input tokens read from the cache.
    #[serde(default, deserialize_with = "null_as_zero")]
    pub cache_read_input_tokens: u32,
}
```

`Message::replay(&self) -> Vec<ContentBlock<'_>>` turns an answer into the assistant turn of the next request, borrowing from the message. The elided lifetime ties the blocks to `self`, so the borrow checker requires the message to outlive the next request, and no text or signature is copied. `ToolUse<'m>` is a borrowed view of one tool call, where `'m` is the message it points into.

Unknown block types (`server_tool_use`, new ones) decode as `ResponseBlock::Unknown`. Unknown stop reasons decode as `StopReason::Other`, so a new API value never fails a call; `model_context_window_exceeded` has its own variant and callers treat it like `max_tokens`. The API reference types every cache count, and `input_tokens` in `message_delta`, as `number or null`, so a `null` reads as 0; `usage` itself defaults, because a documented stream example sends `message_start` without it. `StopReason::Refusal` arrives with HTTP 200: the fallback adapter treats it as invalid output, and `agent` ends the turn with its refusal message.

### Streaming

The `event:` name equals the data's `type`. The order is `message_start`, then for each block `content_block_start`, `content_block_delta`… and `content_block_stop`, then one or more `message_delta`, then `message_stop`. `ping` can come at any point. An `error` event can arrive **after HTTP 200** (`overloaded_error`). Usage in `message_delta` is **cumulative**, so it replaces and never adds.

```rust
/// One SSE `data:` payload; the SSE `event:` name equals its `type`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// The message with empty content, the input usage and a placeholder output count.
    MessageStart {
        /// The message so far.
        message: Message,
    },
    /// A block starts. A `tool_use` block's `input` is `{}` until its JSON deltas end.
    ContentBlockStart {
        /// The block's index in `content`.
        index: usize,
        /// The block, empty.
        content_block: ResponseBlock,
    },
    /// More of a block.
    ContentBlockDelta {
        /// The block.
        index: usize,
        /// The piece.
        delta: Delta,
    },
    /// A block is complete.
    ContentBlockStop {
        /// The block.
        index: usize,
    },
    /// The stop reason and cumulative usage; may come more than once.
    MessageDelta {
        /// The stop reason.
        delta: MessageDeltaBody,
        /// Cumulative counts.
        #[serde(default)]
        usage: Option<UsageUpdate>,
    },
    /// The end.
    MessageStop,
    /// Keep-alive.
    Ping,
    /// An error after HTTP 200 (`overloaded_error`). The client turns it into
    /// [`AnthropicError::Stream`] and ends the stream; callers never see this variant.
    Error {
        /// The error.
        error: ApiErrorDetail,
    },
    /// An event type added after this client was written. Ignored.
    #[serde(other)]
    Unknown,
}

/// `content_block_delta.delta`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Delta {
    /// Text.
    TextDelta {
        /// The text.
        text: String,
    },
    /// Part of a tool call's JSON; concatenate per index, parse at the block's stop.
    InputJsonDelta {
        /// The fragment.
        partial_json: String,
    },
    /// Thinking summary text.
    ThinkingDelta {
        /// The text.
        thinking: String,
    },
    /// A thinking block's signature, just before its stop.
    SignatureDelta {
        /// The signature.
        signature: String,
    },
    /// A delta type added later. Ignored.
    #[serde(other)]
    Unknown,
}

/// `message_delta.delta`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MessageDeltaBody {
    /// Why it stopped.
    pub stop_reason: Option<StopReason>,
    /// The refusal category.
    #[serde(default)]
    pub stop_details: Option<Value>,
}
```

```rust
/// The owned event stream of one call. `'static` and `Send`: it owns the HTTP response, so the
/// SSE handler can move it into a spawned task or a response body. `Ping` and `Unknown` events
/// are already filtered out; an `error` event arrives as `Err(AnthropicError::Stream)` and is
/// the last item.
pub struct MessageStream {
    inner: BoxStream<'static, Result<StreamEvent, AnthropicError>>,
    request_id: Option<String>,
}
```

`MessageStream` is owned and `'static`: it holds the HTTP response and a boxed stream over it, and borrows neither the client nor the request. Pings and unknown events are filtered out. An `error` event becomes `Err(AnthropicError::Stream)`, and it is always the last item. `BoxStream<'static, _>` is `Pin<Box<dyn Stream + Send + 'static>>`, so the stream can be moved into a spawned task or an SSE body. A `'static` stream also satisfies `ChatModel::stream`'s `BoxStream<'a, _>` for any `'a` ([Reference, subtyping and variance](https://doc.rust-lang.org/reference/subtyping.html)).

```rust
/// What a stream event completed, for a caller that streams to the user: text as it comes,
/// whole tool calls and thinking blocks, usage, and the stop.
#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    /// More answer text.
    Text(String),
    /// A complete tool call (its JSON parsed).
    ToolUse {
        /// `toolu_…`.
        id: String,
        /// The tool.
        name: String,
        /// Its arguments.
        input: Value,
    },
    /// A complete thinking or redacted-thinking block, to replay in the tool loop.
    Reasoning(ResponseBlock),
    /// Cumulative usage, after `message_start` and each `message_delta`.
    Usage(Usage),
    /// The stop reason.
    Stop(StopReason),
}

/// Folds stream events into whole blocks and, at the end, a [`Message`]. Tool-call JSON is
/// concatenated per block index and parsed at `content_block_stop` (an empty accumulation is
/// `{}`); usage updates replace rather than add.
#[derive(Debug, Default)]
pub struct MessageAccumulator {
    message: Option<Message>,
    blocks: BTreeMap<usize, ResponseBlock>,
    json: BTreeMap<usize, String>,
}
```

```rust
impl MessageAccumulator {
    /// Applies one event; returns what it completed, if anything.
    pub fn apply(&mut self, event: StreamEvent) -> Result<Option<Output>, AnthropicError> {
        Ok(match event {
            StreamEvent::MessageStart { message } => {
                let usage = message.usage;
                self.message = Some(message);
                Some(Output::Usage(usage))
            }
            StreamEvent::ContentBlockStart {
                index,
                content_block,
            } => {
                self.blocks.insert(index, content_block);
                None
            }
            StreamEvent::ContentBlockDelta { index, delta } => {
                let block = self.blocks.get_mut(&index);
                match (delta, block) {
                    (Delta::TextDelta { text }, Some(ResponseBlock::Text { text: t })) => {
                        t.push_str(&text);
                        Some(Output::Text(text))
                    }
                    (Delta::InputJsonDelta { partial_json }, _) => {
                        self.json.entry(index).or_default().push_str(&partial_json);
                        None
                    }
                    (
                        Delta::ThinkingDelta { thinking },
                        Some(ResponseBlock::Thinking { thinking: t, .. }),
                    ) => {
                        t.push_str(&thinking);
                        None
                    }
                    (
                        Delta::SignatureDelta { signature },
                        Some(ResponseBlock::Thinking { signature: s, .. }),
                    ) => {
                        s.push_str(&signature);
                        None
                    }
                    _ => None,
                }
            }
            StreamEvent::ContentBlockStop { index } => {
                let json = self.json.remove(&index);
                match self.blocks.get_mut(&index) {
                    Some(ResponseBlock::ToolUse { id, name, input }) => {
                        let parsed = match json.as_deref() {
                            None | Some("") => Value::Object(serde_json::Map::new()),
                            Some(s) => {
                                serde_json::from_str(s).map_err(|e| AnthropicError::ToolInput {
                                    index,
                                    source: Box::new(e),
                                })?
                            }
                        };
                        input.clone_from(&parsed);
                        Some(Output::ToolUse {
                            id: id.clone(),
                            name: name.clone(),
                            input: parsed,
                        })
                    }
                    Some(
                        b @ (ResponseBlock::Thinking { .. }
                        | ResponseBlock::RedactedThinking { .. }),
                    ) => Some(Output::Reasoning(b.clone())),
                    _ => None,
                }
            }
            StreamEvent::MessageDelta { delta, usage } => {
                let message = self.message.get_or_insert_with(empty_message);
                if let Some(update) = usage {
                    message.usage.merge(&update);
                }
                if delta.stop_details.is_some() {
                    message.stop_details = delta.stop_details;
                }
                match delta.stop_reason {
                    Some(reason) => {
                        message.stop_reason = Some(reason);
                        Some(Output::Stop(reason))
                    }
                    None => Some(Output::Usage(message.usage)),
                }
            }
            StreamEvent::Error { error } => {
                return Err(AnthropicError::Stream {
                    kind: error.kind,
                    message: error.message,
                });
            }
            StreamEvent::MessageStop | StreamEvent::Ping | StreamEvent::Unknown => None,
        })
    }

    /// The usage so far.
    pub fn usage(&self) -> Usage {
        self.message.as_ref().map(|m| m.usage).unwrap_or_default()
    }

    /// The whole message. Fails if the stream ended before `message_start`.
    pub fn finish(self) -> Result<Message, AnthropicError> {
        let mut message = self.message.ok_or(AnthropicError::Truncated)?;
        message.content = self.blocks.into_values().collect();
        Ok(message)
    }
}
```

The accumulator is shared by streaming callers and `MessageStream::collect_message`. Tool-call JSON is joined per block index and parsed at `content_block_stop`. An empty accumulation means a tool with no arguments and parses as `{}`. `Output` maps one to one onto `ChatChunk`: `Text` to `TextDelta`, `ToolUse` to `ToolCall`, `Reasoning` to `Reasoning`, `Usage` to `Usage` (`Usage::to_domain` counts cached input as input), and `Stop` to `Done`.

### The client

```rust
/// Model ids per caller. Ids are complete as written (no date suffix) and come from
/// configuration, so a model change is a deploy, not a code change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelIds {
    /// Chat turns: the strongest model.
    pub chat: String,
    /// The plan writer.
    pub plan_writer: String,
    /// Memory extraction: a cheap model.
    pub memory_extraction: String,
    /// Off-topic redirects: the cheapest model.
    pub off_topic: String,
    /// The fallback worker's vision calls.
    pub vision_fallback: String,
    /// The router and safety-gate classifier when Jev is not configured.
    pub classifier: String,
}

/// Client settings.
#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    /// `x-api-key`. Never logged: `SecretString`'s `Debug` prints a placeholder.
    pub api_key: SecretString,
    /// `https://api.anthropic.com`; a test server in tests.
    pub base_url: String,
    /// Model ids.
    pub models: ModelIds,
    /// Retries before the first response byte. The worker sets [`RetryPolicy::NONE`].
    pub retry: RetryPolicy,
    /// Total deadline of a non-streaming call. Streams have only the shared client's read gap.
    pub request_timeout: Duration,
    /// `anthropic-beta` values, empty for none.
    pub betas: Vec<String>,
}

/// The Messages API client. Cheap to clone (an `Arc` and the shared `reqwest::Client`), and
/// `Send + Sync + 'static`, so `agent` and the worker hold it by value.
#[derive(Debug, Clone)]
pub struct AnthropicClient {
    http: reqwest::Client,
    config: Arc<AnthropicConfig>,
}
```

```rust
impl AnthropicClient {
    /// A client over the shared HTTP client.
    pub fn new(http: reqwest::Client, config: AnthropicConfig) -> Self {
        Self {
            http,
            config: Arc::new(config),
        }
    }

    /// The configured model ids.
    pub fn models(&self) -> &ModelIds {
        &self.config.models
    }

    /// One non-streaming call. Retries per the configured policy.
    ///
    /// The request is serialised before the first `.await` into an owned body, so the borrow
    /// of `request` (and of everything it borrows) ends there in practice; the future still
    /// names both lifetimes because it holds the reference until it is first polled.
    pub async fn create(&self, request: &MessagesRequest<'_>) -> Result<Message, AnthropicError> {
        let body = encode(request, false)?;
        let response = self.send(body, false).await?;
        let bytes = response
            .bytes()
            .await
            .map_err(TransportError::from_reqwest)?;
        serde_json::from_slice(&bytes).map_err(|e| AnthropicError::Decode(Box::new(e)))
    }

    /// One streaming call. Retries per the policy until the API answers 200; from then on
    /// errors arrive in the stream and are not retried (the caller may restart the call if it
    /// has sent nothing to the user yet).
    ///
    /// The returned [`MessageStream`] is `'static`: it owns the response and borrows neither
    /// `self` nor `request`. That is the point of serialising first: the borrowed request
    /// lives only as long as this `async fn`'s first poll, while the stream can be moved into a
    /// spawned task or an SSE body. `ChatModel::stream<'a>` in `agent` can still return a
    /// `BoxStream<'a, _>`, since a `'static` stream outlives any `'a`.
    pub async fn stream(
        &self,
        request: &MessagesRequest<'_>,
    ) -> Result<MessageStream, AnthropicError> {
        let body = encode(request, true)?;
        let response = self.send(body, true).await?;
        let request_id = header(response.headers(), "request-id");
        let events = sse::events(
            response
                .bytes_stream()
                .map_err(TransportError::from_reqwest),
        )
        .map_err(AnthropicError::from)
        .and_then(|ev| async move { StreamEvent::from_sse(&ev) })
        .try_filter(|ev| {
            std::future::ready(!matches!(ev, StreamEvent::Ping | StreamEvent::Unknown))
        })
        .map(|item| match item {
            Ok(StreamEvent::Error { error }) => Err(AnthropicError::Stream {
                kind: error.kind,
                message: error.message,
            }),
            other => other,
        })
        .scan(false, |failed, item| {
            // An error is the last item: stop after yielding it.
            if *failed {
                return std::future::ready(None);
            }
            *failed = item.is_err();
            std::future::ready(Some(item))
        });
        Ok(MessageStream::new(events.boxed(), request_id))
    }

    /// POSTs the body, retrying statuses and transport errors the policy allows. `Bytes`
    /// clones are reference counts, so each attempt re-sends the same buffer.
    async fn send(
        &self,
        body: Bytes,
        streaming: bool,
    ) -> Result<reqwest::Response, AnthropicError> {
        let url = format!("{}/v1/messages", self.config.base_url.trim_end_matches('/'));
        http::retry(self.config.retry, |_| {
            let mut builder = self
                .http
                .post(&url)
                .header("x-api-key", self.config.api_key.expose_secret())
                .header("anthropic-version", API_VERSION)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.clone());
            if !self.config.betas.is_empty() {
                builder = builder.header("anthropic-beta", self.config.betas.join(","));
            }
            if streaming {
                builder = builder.header(reqwest::header::ACCEPT, "text/event-stream");
            } else {
                builder = builder.timeout(self.config.request_timeout);
            }
            async move {
                let response = match builder.send().await {
                    Ok(r) => r,
                    Err(e) => {
                        let error = AnthropicError::from(TransportError::from_reqwest(e));
                        return Attempt::Retry {
                            error,
                            retry_after: None,
                        };
                    }
                };
                if response.status().is_success() {
                    return Attempt::Done(response);
                }
                let error = api_error(response).await;
                if error.retryable() {
                    let retry_after = error.retry_after();
                    Attempt::Retry { error, retry_after }
                } else {
                    Attempt::Fail(error)
                }
            }
        })
        .await
    }
}
```

**Retries** happen only before the first byte of the response. `create` retries per the policy, and so does `stream` until the API answers 200. After that, errors arrive in the stream and are not retried: `agent` restarts the call only if it has sent nothing to the user. The worker passes `RetryPolicy::NONE`, because it owns the job deadline and its one validation retry ([contracts 6, after decoding](../architecture/contracts.md#after-decoding)). Model ids come from configuration, complete as written (`claude-opus-5-5`, `claude-haiku-5-5`, with no date suffix).

| Response | `AnthropicError` | Retried | `ModelError` |
|---|---|---|---|
| Connect failure, timeout, dropped body | `Transport` | yes | `Unavailable` |
| 408, 409, 429, 500–599, 529 | `Api { status, kind, retry_after, .. }` | yes, honouring `retry-after` | `Unavailable` (`503 upstream_unavailable` for chat) |
| 429 with `error.details.error_code: "enforced_spend_limit_reached"` | `SpendCap` | **never** | `Unavailable` |
| 400, 401, 403, 404, 413 | `Api` | no | `Failed` (our bug: logged with the message) |
| `error` event after 200 | `Stream { kind, .. }` | no (`retryable()` is true for `overloaded_error`, `api_error` and `rate_limit_error`, so the caller may restart) | `Unavailable` |
| Body or event that doesn't parse; tool JSON that doesn't parse | `Decode`, `ToolInput` | no | `InvalidOutput` |

```rust
/// Why a Messages call failed. No `reqwest` type appears in it.
#[derive(Debug, thiserror::Error)]
pub enum AnthropicError {
    /// No HTTP response: connect failure, timeout, a dropped stream.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// The API answered with an error status.
    #[error("Anthropic API {status} {kind}: {message}")]
    Api {
        /// The HTTP status.
        status: u16,
        /// `error.type`, such as `rate_limit_error`.
        kind: String,
        /// `error.message`.
        message: String,
        /// `request-id`.
        request_id: Option<String>,
        /// `retry-after`, seconds.
        retry_after: Option<Duration>,
    },
    /// A 429 with `error.details.error_code == "enforced_spend_limit_reached"`: the
    /// organisation's monthly spend cap. Never retried.
    #[error("Anthropic spend cap reached")]
    SpendCap {
        /// `request-id`.
        request_id: Option<String>,
    },
    /// An `error` event after HTTP 200 (`overloaded_error`, `api_error`).
    #[error("Anthropic stream error {kind}: {message}")]
    Stream {
        /// `error.type`.
        kind: String,
        /// `error.message`.
        message: String,
    },
    /// A body or event that didn't parse.
    #[error("Anthropic response did not parse")]
    Decode(#[source] BoxError),
    /// A tool call's streamed JSON didn't parse at its block's stop.
    #[error("tool input of block {index} did not parse")]
    ToolInput {
        /// The block.
        index: usize,
        /// The parse error.
        #[source]
        source: BoxError,
    },
    /// The stream ended before `message_start`, or before a stop reason.
    #[error("Anthropic stream ended early")]
    Truncated,
    /// The request couldn't be serialised (a non-string map key in a schema).
    #[error("request did not serialise")]
    Encode(#[source] BoxError),
}
```

### Structured output and the schema sanitiser

`output_config.format = {type: "json_schema", schema}` makes the reply one text block holding JSON. The API compiles the schema to a grammar and **rejects with a 400** the keywords it can't enforce. [Contracts 6](../architecture/contracts.md#after-decoding) names `maxLength`, `maxItems`, `minimum` and `maximum`. Anthropic's documentation (research notes) also excludes `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf` and `minItems` above 1, and every object needs `additionalProperties: false`. The contracts' VLM schemas use these keywords because xgrammar enforces them on vLLM, so the fallback worker and the plan writer sanitise first. Afterwards they enforce the stripped limits themselves (truncate strings and arrays, clamp numbers and boxes) from the returned list.

The first live calls (2026-10-16) added two rules. `oneOf` is refused (`Schema type 'oneOf' is not supported`), and `schemars` writes it for every enum with documented variants and every tagged union, so the sanitiser rewrites it: a `oneOf` of constants becomes an `enum`, with each variant's description appended to the parent's as a `- "value": …` line, and any other `oneOf` becomes an `anyOf`. The official Python SDK's `transform_schema` maps every `oneOf` to `anyOf`; the `enum` rewrite is this crate's own and keeps those parameters out of the union limit below. A `oneOf` beside an existing `enum` keeps the `enum` and becomes an `anyOf`, so both still apply, and only scalar constants become an `enum`, because the API refuses objects and arrays in one. A `format` survives only on a string and only if the API lists it (`SUPPORTED_FORMATS`), so `schemars`'s `uint32` and `double` are dropped and returned in `stripped`. Strict tools also share one grammar per request with its own size limit: the docs give 20 strict tools, 24 optional parameters and 16 union-typed parameters, and nineteen strict tools were refused as too large, so `agent` marks only its write tools strict ([agent 9](agent.md#9-model-adapters)). A nullable enum, `anyOf: [{enum: […]}, {type: "null"}]`, becomes one `enum` with `null` among its values: the same answers without a union, so the fallback's per-question `image` no longer counts toward the 16 (refused live at 20). The grammar's overall size has a limit of its own, which the worker meets by splitting a large indicators call ([worker 8](worker.md#8-the-vlm-adapters)).

```rust
/// Keywords removed wherever they appear in a schema position.
pub const UNSUPPORTED_KEYWORDS: &[&str] = &[
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
    "minLength",
    "maxLength",
    "maxItems",
    "minProperties",
    "maxProperties",
];

/// The string formats the API accepts (Anthropic's structured outputs docs, JSON Schema limitations).
pub const SUPPORTED_FORMATS: &[&str] = &[
    "date-time",
    "time",
    "date",
    "duration",
    "email",
    "hostname",
    "uri",
    "ipv4",
    "ipv6",
    "uuid",
];

/// One keyword the sanitiser removed, so the caller can enforce it after decoding.
#[derive(Debug, Clone, PartialEq)]
pub struct Stripped {
    /// JSON Pointer to the schema object that held it, such as `/properties/summary`.
    pub at: String,
    /// The keyword.
    pub keyword: &'static str,
    /// Its value.
    pub value: Value,
}

/// A sanitised schema. `'a` is the input schema: when nothing had to change, `schema` borrows
/// it (`Cow::Borrowed`) and nothing is copied; otherwise it owns a cleaned copy. Either way it
/// drops into [`super::OutputFormat::JsonSchema`] or [`super::Tool::input_schema`], which take a
/// `Cow<'a, Value>`.
#[derive(Debug, Clone, PartialEq)]
pub struct Sanitized<'a> {
    /// The schema to send.
    pub schema: Cow<'a, Value>,
    /// What was removed, in document order.
    pub stripped: Vec<Stripped>,
    /// Objects that had no `additionalProperties` and got `false`.
    pub closed: Vec<String>,
}

/// Sanitises a schema, copying it only if something changes.
pub fn sanitize_schema(schema: &Value) -> Sanitized<'_> {
    let mut copy = schema.clone();
    let (stripped, closed) = sanitize_in_place(&mut copy);
    Sanitized {
        // Rewrites (`oneOf`) change the schema without stripping anything.
        schema: if copy == *schema {
            Cow::Borrowed(schema)
        } else {
            Cow::Owned(copy)
        },
        stripped,
        closed,
    }
}

/// Sanitises a schema in place; returns what was stripped and which objects were closed.
pub fn sanitize_in_place(schema: &mut Value) -> (Vec<Stripped>, Vec<String>) {
    let mut stripped = Vec::new();
    let mut closed = Vec::new();
    walk(schema, &mut String::new(), &mut stripped, &mut closed);
    (stripped, closed)
}
```

`Sanitized<'a>` borrows the input schema when nothing had to change (`Cow::Borrowed`) and owns a cleaned copy otherwise; a rewrite alone (`oneOf`) counts as a change. It plugs straight into `OutputFormat::JsonSchema` and `Tool::input_schema`, which take a `Cow<'a, Value>`. The walk visits only schema positions: the keys of `properties` are names, so a property named `maximum` is kept. The test checks exactly that.

---

## 6. OpenAI-compatible VLM servers

**Endpoint.** `POST {base_url}/v1/chat/completions` on vLLM 0.31 or llama.cpp's `llama-server` ([contracts 6, per-job schema](../architecture/contracts.md#per-job-schema)). Also `GET /v1/models`, for the worker's start-up check of the served name, and `GET /health`. Calls are non-streaming: a findings call is at most 1,500 tokens, and the worker validates the whole answer.

**Request.** Images are `data:` URLs, written while serialising (`DataUrl<'a>`, the same technique as `Base64<'a>`). `response_format` is `json_schema` with `strict: true`. The output limit is `max_completion_tokens` on vLLM, where `max_tokens` is deprecated in 0.31, and `max_tokens` on llama.cpp. Repetition penalty is `repetition_penalty` on vLLM and `repeat_penalty` on llama.cpp. `ServerFlavour` chooses the names, so the worker writes one request. Sampling follows the vision decisions: temperature 0, `top_p` 1, presence penalty 0, repetition penalty 1 (`Sampling::DETERMINISTIC`). vLLM refuses structured output combined with tools, so the request type has no tools. The `guided_json` field was removed in vLLM 0.12 and is not used.

```rust
/// Which server answers. They accept the same body except for two fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerFlavour {
    /// vLLM: `max_completion_tokens` (`max_tokens` is deprecated in 0.31) and
    /// `repetition_penalty`.
    Vllm,
    /// llama.cpp: `max_tokens` and `repeat_penalty`.
    LlamaCpp,
}

/// Client settings.
#[derive(Debug, Clone)]
pub struct OpenAiCompatConfig {
    /// `http://vllm-nv:8000`, without `/v1`.
    pub base_url: String,
    /// `Authorization: Bearer`, when the server was started with `--api-key`.
    pub api_key: Option<SecretString>,
    /// Total deadline of one call.
    pub timeout: Duration,
    /// The server.
    pub flavour: ServerFlavour,
    /// Retries of transport errors and 5xx. The worker sets [`RetryPolicy::NONE`]: it owns the
    /// job deadline and its one validation retry.
    pub retry: RetryPolicy,
}

/// Sampling parameters ([contracts 6]; `docs/vision` model decisions).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
pub struct Sampling {
    /// 0 for the VLM.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    /// 1; sent as `repetition_penalty` (vLLM) or `repeat_penalty` (llama.cpp).
    #[serde(skip)]
    pub repetition_penalty: Option<f32>,
    /// A fixed seed, for reproducible evaluation runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}

/// One chat-completions request.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatCompletionRequest<'a> {
    /// The served model name (`--served-model-name`).
    pub model: Cow<'a, str>,
    /// System, then user.
    pub messages: Vec<ChatMessage<'a>>,
    /// Output budget: 1,500 for findings and indicators, 100 for guided checks.
    pub max_output_tokens: u32,
    /// Sampling.
    pub sampling: Sampling,
    /// Structured output.
    pub response_format: Option<ResponseFormat<'a>>,
}

/// One message.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChatMessage<'a> {
    /// Its author.
    pub role: ChatRole,
    /// Its content.
    pub content: ChatContent<'a>,
}

/// `content`: a string or parts.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ChatContent<'a> {
    /// Text.
    Text(Cow<'a, str>),
    /// Text and images.
    Parts(Vec<ContentPart<'a>>),
}

/// One part of a user message.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart<'a> {
    /// Text.
    Text {
        /// The text.
        text: Cow<'a, str>,
    },
    /// An image as a `data:` URL.
    ImageUrl {
        /// The URL object.
        image_url: ImageUrl<'a>,
    },
}

/// Borrowed image bytes that serialise as a `data:` URL without building the string first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataUrl<'a> {
    /// `image/webp`.
    pub media_type: &'a str,
    /// The encoded image.
    pub bytes: &'a [u8],
}

/// `response_format`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat<'a> {
    /// Constrained to a schema (xgrammar on vLLM, a GBNF grammar on llama.cpp).
    JsonSchema {
        /// The schema.
        json_schema: JsonSchemaSpec<'a>,
    },
}

/// `response_format.json_schema`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JsonSchemaSpec<'a> {
    /// `findings`, `indicators`, `guided`.
    pub name: Cow<'a, str>,
    /// The schema, with its length and integer bounds: xgrammar enforces them.
    pub schema: Cow<'a, Value>,
    /// Always true for the VLM.
    pub strict: bool,
}
```

`Body<'r, 'a>` is the flavour-specific wire form. Its two lifetimes have the same meaning as in section 5: `'r` is the borrow for serialising, `'a` the worker's data.

**Response.** `finish_reason` is an enum. `Length` means the output was cut, and the worker counts it as a validation failure: one retry, then `model_error` with `retryable: true` ([contracts 6](../architecture/contracts.md#after-decoding)). `reasoning_content` carries a thinking model's reasoning when the server separates it. Qwen3.5's thinking is switched off, so it should stay empty.

```rust
/// The response.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ChatCompletion {
    /// `chatcmpl-…`.
    pub id: String,
    /// The served model.
    pub model: String,
    /// One choice (we never ask for `n` > 1).
    pub choices: Vec<Choice>,
    /// Token counts.
    #[serde(default)]
    pub usage: Option<CompletionUsage>,
}

/// One choice.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Choice {
    /// 0.
    pub index: u32,
    /// The answer.
    pub message: AssistantMessage,
    /// Why it stopped. `Length` means the output was cut: the worker treats it as a validation
    /// failure ([contracts 6]).
    pub finish_reason: Option<FinishReason>,
}

/// `choices[].message`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AssistantMessage {
    /// The text: the JSON, with structured output.
    #[serde(default)]
    pub content: Option<String>,
    /// A reasoning model's thinking (`reasoning_content`), when the server separates it.
    #[serde(default)]
    pub reasoning_content: Option<String>,
}

/// `finish_reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// Natural end or a stop string.
    Stop,
    /// `max_completion_tokens` reached.
    Length,
    /// A tool call (unused).
    ToolCalls,
    /// A content filter (unused by our servers).
    ContentFilter,
    /// Anything else, including a missing reason.
    #[serde(other)]
    Other,
}

/// Why a call failed.
#[derive(Debug, thiserror::Error)]
pub enum OpenAiCompatError {
    /// No response.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// An error status, with the start of the body.
    #[error("VLM server answered {status}: {body}")]
    Status {
        /// The HTTP status.
        status: u16,
        /// At most 512 bytes of the body.
        body: String,
        /// `Retry-After`.
        retry_after: Option<Duration>,
    },
    /// The body didn't parse.
    #[error("VLM server response did not parse")]
    Decode(#[source] BoxError),
    /// The request didn't serialise.
    #[error("request did not serialise")]
    Encode(#[source] BoxError),
}
```

```rust
impl OpenAiCompatClient {
    /// A client over the shared HTTP client.
    pub fn new(http: reqwest::Client, config: OpenAiCompatConfig) -> Self {
        Self {
            http,
            config: Arc::new(config),
        }
    }

    /// The server flavour.
    pub fn flavour(&self) -> ServerFlavour {
        self.config.flavour
    }

    /// One chat completion. The request is serialised before the first `.await`; the response
    /// is owned.
    pub async fn chat(
        &self,
        request: &ChatCompletionRequest<'_>,
    ) -> Result<ChatCompletion, OpenAiCompatError> {
        let body = serde_json::to_vec(&Body::new(request, self.config.flavour))
            .map(Bytes::from)
            .map_err(|e| OpenAiCompatError::Encode(Box::new(e)))?;
        let url = self.url("/v1/chat/completions");
        let bytes = http::retry(self.config.retry, |_| {
            let builder = self
                .authorised(self.http.post(&url))
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.clone());
            async move { attempt(builder).await }
        })
        .await?;
        serde_json::from_slice(&bytes).map_err(|e| OpenAiCompatError::Decode(Box::new(e)))
    }

    /// `GET /v1/models`: the served names, for the start-up check against configuration.
    pub async fn models(&self) -> Result<Vec<String>, OpenAiCompatError> {
        #[derive(Deserialize)]
        struct List {
            data: Vec<Entry>,
        }
        #[derive(Deserialize)]
        struct Entry {
            id: String,
        }
        let url = self.url("/v1/models");
        let builder = self.authorised(self.http.get(&url));
        let bytes = match attempt(builder).await {
            Attempt::Done(b) => b,
            Attempt::Retry { error, .. } | Attempt::Fail(error) => return Err(error),
        };
        let list: List =
            serde_json::from_slice(&bytes).map_err(|e| OpenAiCompatError::Decode(Box::new(e)))?;
        Ok(list.data.into_iter().map(|e| e.id).collect())
    }

    /// `GET /health`: 200 once the model is loaded (both servers).
    pub async fn health(&self) -> Result<(), OpenAiCompatError> {
        let builder = self.authorised(self.http.get(self.url("/health")));
        match attempt(builder).await {
            Attempt::Done(_) => Ok(()),
            Attempt::Retry { error, .. } | Attempt::Fail(error) => Err(error),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.config.base_url.trim_end_matches('/'))
    }

    fn authorised(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let builder = builder.timeout(self.config.timeout);
        match &self.config.api_key {
            Some(key) => builder.bearer_auth(key.expose_secret()),
            None => builder,
        }
    }
}
```

| Failure | `OpenAiCompatError` | `retryable()` | `ModelError` |
|---|---|---|---|
| Connect failure, timeout | `Transport` | yes | `Failed` (the job is retried, `retryable: true`) |
| 503 while the model loads, other 5xx, 408, 429 | `Status` | yes | `Failed` |
| 400 (a schema the grammar compiler rejects, too many images) | `Status` | no | `Failed` |
| Body that doesn't parse | `Decode` | no | `InvalidOutput` |

The VLM is not a hosted API: a 5xx means a crashed or restarting sidecar, which contracts 5 treats as worth another attempt. That is why the mapping uses `Failed` rather than `Unavailable`.

---

## 7. Open-Meteo weather

**Endpoint.** `GET https://api.open-meteo.com/v1/forecast` (no key; the commercial host takes `apikey`). One request fills a whole `CellWeather`: 14 hourly variables, 3 daily ones, `past_days=7` and `forecast_days=14` ([contracts 7, WeatherSummary](../architecture/contracts.md#weathersummary); [architecture 6.4](../architecture/README.md#64-soil) and [8.3](../architecture/README.md#83-forecast-alerts)). `timezone` is the cell's IANA zone, so `time` strings and daily dates are local.

**Several cells.** `latitude`, `longitude` and `timezone` are comma-separated lists of up to 50 cells. Two or more locations come back as a JSON **array** in request order, and one location as a bare **object** (both observed live). `ForecastResponse` is an untagged enum of the two.

```rust
/// The `upstream` of `503 upstream_unavailable` and every log line.
pub const PROVIDER: &str = "open_meteo";

/// Most cells in one request ([contracts 7]). 101 worked live; the URL length is the real
/// ceiling (1,001 cells returned 414).
pub const MAX_CELLS_PER_REQUEST: usize = 50;

/// Settings.
#[derive(Debug, Clone)]
pub struct OpenMeteoConfig {
    /// `https://api.open-meteo.com/v1/forecast`, or the commercial host.
    pub forecast_url: String,
    /// `apikey` for the commercial host; `None` on the free API (non-commercial use only).
    pub api_key: Option<secrecy::SecretString>,
    /// Total deadline of one request.
    pub timeout: Duration,
    /// Hourly history fetched: 7, for `past_7_days`.
    pub past_days: u8,
    /// Forecast days, today included: 7–16. 14 by default.
    pub forecast_days: u8,
    /// The weather thresholds: a forecast day is `frost_risk` when its 2 m minimum is frost
    /// under them. `api` passes the same instance its triggers and the pipeline read.
    pub thresholds: WeatherThresholds,
}

/// The Open-Meteo adapter. `clock` stamps `fetched_at` and picks the current hour, so tests
/// fix time.
#[derive(Clone)]
pub struct OpenMeteo {
    http: reqwest::Client,
    config: Arc<OpenMeteoConfig>,
    clock: Arc<dyn Clock>,
}
```

```rust
/// One location.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Forecast {
    /// Snapped to the model grid (43.65 → 43.646603).
    pub latitude: f64,
    /// Snapped to the model grid.
    pub longitude: f64,
    /// The offset at request time, DST-aware: −14,400 for Toronto in October.
    pub utc_offset_seconds: i32,
    /// The IANA zone the times are in.
    pub timezone: String,
    /// Units per hourly variable.
    #[serde(default)]
    pub hourly_units: BTreeMap<String, String>,
    /// Hourly series; `time` is local and naive (`2026-10-15T19:00`).
    #[serde(default)]
    pub hourly: Hourly,
    /// Daily series; `time` is a local date.
    #[serde(default)]
    pub daily: Daily,
}

/// An object for one location, an array for several.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum ForecastResponse {
    /// Two or more locations, in request order.
    Many(Vec<Forecast>),
    /// One location.
    One(Box<Forecast>),
}
```

**Building the summary.** `cell_weather` is a pure function over a borrowed `&Forecast`, tested against the live fixture. The current hour is `now` shifted by the response's `utc_offset_seconds` and floored to the hour. `current` is that hour's row. The 24-hour sums and the 7-day figures (168 rows) end with it. `forecast` holds the daily rows from today, at most `forecast_days`, each with its hours' mean humidity and `hours_rh_over_90`. `past_7_days.longest_humid_run_days` is the longest run of consecutive humid days among the 7 whole local days before today, a humid day having at least `humid_day_hours` (10) hours over 90% (`WeatherThresholds::longest_humid_run`), so the pipeline can tell a humid stretch in the past week ([contracts 9](../architecture/contracts.md#garden-map), crowding), which the 168-hour total can't show. Open-Meteo has no 0–9 cm layer, so `moisture_0_9cm_m3m3` is the depth-weighted mean of the 0–1, 1–3 and 3–9 cm layers, `(1·m₀₋₁ + 2·m₁₋₃ + 6·m₃₋₉) / 9`, as contracts 7 now specifies. The units are checked against the expected ones (°C, %, km/h, mm, m³/m³), and a response in other units is a bad response.

```rust
/// The 0–9 cm soil moisture contracts 7 asks for, which Open-Meteo doesn't have: the
/// depth-weighted mean of its 0–1, 1–3 and 3–9 cm layers, `(1·m₀₋₁ + 2·m₁₋₃ + 6·m₃₋₉) / 9`.
pub fn moisture_0_9cm(m0_1: f64, m1_3: f64, m3_9: f64) -> f64 {
    m0_1.mul_add(1.0, m1_3.mul_add(2.0, m3_9 * 6.0)) / 9.0
}

/// Builds a cell's weather at `now`.
///
/// The current hour is `now` shifted by the response's `utc_offset_seconds` and floored to the
/// hour, matched against the local `time` strings. `current` is that hour's row; the 24-hour
/// sums and the 7-day figures end with it (168 rows). `forecast` holds the daily rows from the
/// local date of `now`, at most `forecast_days`, each with its hours' mean humidity and hours
/// above 90 % ([`WeatherThresholds::humid_hours`]). `past_7_days.longest_humid_run_days` is
/// the longest run of humid days among the 7 whole local days before today
/// ([`WeatherThresholds::longest_humid_run`]). A day is `frost_risk` when its 2 m minimum
/// is frost under `thresholds` ([`WeatherThresholds::is_frost`]: at or below 0.5 °C by
/// default; a design choice, since Open-Meteo has no frost field).
pub fn cell_weather(
    forecast: &Forecast,
    now: Timestamp,
    forecast_days: usize,
    thresholds: &WeatherThresholds,
) -> Result<CellWeather, SummaryError> {
    for &(variable, expected) in EXPECTED_UNITS {
        if let Some(got) = forecast.hourly_units.get(variable)
            && got != expected
        {
            return Err(SummaryError::Units {
                variable,
                got: got.clone(),
                expected,
            });
        }
    }
    let h = &forecast.hourly;
    let local = now.into_inner().naive_utc()
        + ChronoDuration::seconds(i64::from(forecast.utc_offset_seconds));
    let local_hour = local
        .with_minute(0)
        .and_then(|t| t.with_second(0))
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(local);
    let key = local_hour.format(HOUR_FORMAT).to_string();
    let i = h
        .time
        .iter()
        .position(|t| *t == key)
        .ok_or(SummaryError::HourMissing(key))?;
    let last_24 = i.saturating_sub(23)..i + 1;
    let last_168 = i.saturating_sub(167)..i + 1;
    let need =
        |series: &[Option<f64>], name: &'static str| at(series, i).ok_or(SummaryError::Null(name));

    let today = local_hour.date();
    let mut days = Vec::new();
    for (d, date) in forecast.daily.time.iter().enumerate() {
        let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|_| SummaryError::Time(date.clone()))?;
        if date < today {
            continue;
        }
        if days.len() == forecast_days {
            break;
        }
        let (Some(t_min_c), Some(t_max_c)) = (
            at(&forecast.daily.temperature_2m_min, d),
            at(&forecast.daily.temperature_2m_max, d),
        ) else {
            break; // the model's range ended
        };
        let rh: Vec<f64> = h
            .time
            .iter()
            .enumerate()
            .filter(|(_, t)| hour_date(t) == Some(date))
            .filter_map(|(j, _)| at(&h.relative_humidity_2m, j))
            .collect();
        days.push(ForecastDay {
            date: LocalDate::new(date),
            t_min_c,
            t_max_c,
            precip_mm: at(&forecast.daily.precipitation_sum, d).unwrap_or(0.0),
            mean_rh_pct: mean(&rh),
            hours_rh_over_90: WeatherThresholds::humid_hours(rh.iter().copied()),
            frost_risk: thresholds.is_frost(t_min_c),
        });
    }

    let past_rh: Vec<f64> = last_168
        .clone()
        .filter_map(|j| at(&h.relative_humidity_2m, j))
        .collect();
    let daily_humid = past_daily_humid_hours(h, today);
    Ok(CellWeather {
        provider: WeatherProviderName::OpenMeteo,
        fetched_at: now,
        current: CurrentWeather {
            temp_c: need(&h.temperature_2m, "temperature_2m")?,
            rh_pct: need(&h.relative_humidity_2m, "relative_humidity_2m")?,
            wind_kmh: need(&h.wind_speed_10m, "wind_speed_10m")?,
            precip_mm_last_24h: sum(&h.precipitation, last_24.clone()),
        },
        past_7_days: PastWeather {
            precip_mm: sum(&h.precipitation, last_168),
            mean_rh_pct: mean(&past_rh),
            hours_rh_over_90: WeatherThresholds::humid_hours(past_rh.iter().copied()),
            longest_humid_run_days: thresholds.longest_humid_run(daily_humid),
        },
        forecast: days,
        soil: SoilNow {
            moisture_0_9cm_m3m3: moisture_0_9cm(
                need(&h.soil_moisture_0_to_1cm, "soil_moisture_0_to_1cm")?,
                need(&h.soil_moisture_1_to_3cm, "soil_moisture_1_to_3cm")?,
                need(&h.soil_moisture_3_to_9cm, "soil_moisture_3_to_9cm")?,
            ),
            moisture_9_27cm_m3m3: need(&h.soil_moisture_9_to_27cm, "soil_moisture_9_to_27cm")?,
            temp_6cm_c: need(&h.soil_temperature_6cm, "soil_temperature_6cm")?,
            et0_mm_last_24h: sum(&h.et0_fao_evapotranspiration, last_24),
        },
    })
}

/// Why a response could not be summarised.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SummaryError {
    /// The current local hour isn't in the hourly series (wrong `past_days`, a stale clock).
    #[error("hour {0} not in the hourly series")]
    HourMissing(String),
    /// A value needed for `current` or `soil` is `null`.
    #[error("{0} is null at the current hour")]
    Null(&'static str),
    /// A variable came back in other units.
    #[error("{variable} in {got}, expected {expected}")]
    Units {
        /// The variable.
        variable: &'static str,
        /// What came back.
        got: String,
        /// What we need.
        expected: &'static str,
    },
    /// A date or hour that didn't parse.
    #[error("unparsable time {0}")]
    Time(String),
}
```

The fixture, recorded at 23:05 UTC on 2026-10-20, gives Toronto at 19:00: 11.3 °C, RH 69 %, wind 5.6 km/h, 1.6 mm and 5 hours above 90 % over 7 days, 0.406 m³/m³ at 0–9 cm, 15.6 °C at 6 cm, ET₀ 2.38 mm over 24 hours. Toronto's 14 days have no frost (lowest minimum 1.0 °C); Ottawa's forecast shows frost on 2026-10-28 (−1.5 °C), 30 and 31. Hours over 90 % on 13–19 October were 6, 0, 0, 5, 0, 0, 0 in Toronto (no humid day) and 22, 11, 0, 10, 0, 0, 0 in Ottawa (2 days in a row: a humid stretch in the past week). The response's shape matches the 2026-10-15 recording key for key and unit for unit: nothing changed but the values.

**`frost_risk`** is not an Open-Meteo field. A day is at risk when its 2 m minimum is frost under `OpenMeteoConfig::thresholds` (`WeatherThresholds::is_frost`: at or below 0.5 °C by default), because air at 2 m runs warmer than the ground and ground frost forms with the air at 1–2 °C. `api` passes the same `WeatherThresholds` to its triggers and the pipeline ([domain 7, weather thresholds](domain.md#weather-thresholds)), so the cached `frost_risk` agrees with both. `hours_rh_over_90` counts hours strictly over 90% (`WeatherThresholds::humid_hours`).

**The scheduler's rule.** [Contracts 7](../architecture/contracts.md#weathersummary): a failed multi-location request is retried once, then its cells are fetched one by one, and a cell that still fails skips its gardens until the next run. `fetch_cells` implements that over any `&dyn WeatherProvider`, so it is tested with `domain`'s `MockWeatherProvider`, and `api`'s scheduler calls it with the wired adapter.

```rust
/// The scheduler's rule ([contracts 7], fetching and caching): each batch of at most 50 cells
/// is one request; a failed batch is retried once, then its cells are fetched one by one, and a
/// cell that still fails is reported as failed (its gardens are skipped until the next run).
///
/// Takes `&dyn WeatherProvider`, so the rule is tested with `domain`'s mock and the scheduler
/// in `api` uses it with whatever adapter is wired.
pub async fn fetch_cells(provider: &dyn WeatherProvider, cells: &[CellRequest]) -> Vec<CellResult> {
    let mut out = Vec::with_capacity(cells.len());
    for chunk in cells.chunks(MAX_CELLS_PER_REQUEST) {
        let batch = match provider.fetch_batch(chunk).await {
            Ok(v) => Ok(v),
            Err(first) => {
                tracing::warn!(provider = PROVIDER, cells = chunk.len(), error = %first, "batch failed; retrying once");
                provider.fetch_batch(chunk).await
            }
        };
        match batch {
            Ok(weathers) if weathers.len() == chunk.len() => {
                out.extend(chunk.iter().zip(weathers).map(|(c, w)| CellResult {
                    cell: c.cell,
                    weather: Ok(w),
                }));
            }
            _ => {
                for c in chunk {
                    out.push(CellResult {
                        cell: c.cell,
                        weather: provider.fetch_cell(c).await,
                    });
                }
            }
        }
    }
    out
}
```

| Failure | `ProviderError` | Contract behaviour |
|---|---|---|
| Timeout, connect, 5xx | `Unavailable { provider: "open_meteo" }` | A scan's run fails with `upstream_unavailable` (retryable, so the user can resubmit), as contracts 3 says for "weather down" ([pipeline 6](pipeline.md#6-retrieve)); `GET /gardens/{id}/weather` answers `503 upstream_unavailable` with `upstream: "open_meteo"`; the scheduler skips the cell's gardens until its next run |
| 429 | `RateLimited { retry_after_s }` | The same, and the scheduler waits for its next run |
| 400 (`{"error":true,"reason":…}`), wrong count of locations, nulls at the current hour, wrong units | `BadResponse` | The same; logged as a bug |

**Limits.** The free tier allows 600 calls a minute, 5,000 an hour and 10,000 a day, and is **non-commercial only**. A request with more than 10 variables or more than 2 weeks weighs more than one call. Ours weighs about 2.8 per location by the documented examples (*unverified* formula). Each location in a multi-location request is assumed to count. The cache (one fetch per cell and hour, in `api`) is what keeps the quota.

---

## 8. SoilGrids

**Endpoint.** `GET https://rest.isric.org/soilgrids/v2.0/properties/query`, with `lon`, `lat`, repeated `property=` (8), repeated `depth=` (6) and `value=mean` ([architecture 6.4](../architecture/README.md#64-soil); [contracts 10, other entity shapes](../architecture/contracts.md#other-entity-shapes)). Values are integers in "mapped units", and value ÷ `d_factor` is the contract's unit, because every contract unit equals SoilGrids' `target_units`. For example pH ×10 ÷ 10, clay g/kg ÷ 10 = %, nitrogen cg/kg ÷ 100 = g/kg, bulk density cg/cm³ ÷ 100 = kg/dm³. The depth labels are the contract's `depth_cm` values plus "cm".

```rust
/// Settings.
#[derive(Debug, Clone)]
pub struct SoilGridsConfig {
    /// `https://rest.isric.org/soilgrids/v2.0/properties/query`.
    pub query_url: String,
    /// Total deadline: a full query took 9–15 s live, so 60 s.
    pub timeout: Duration,
    /// The least time between two calls: 15 s keeps under the documented 5 calls a minute.
    pub min_interval: Duration,
    /// Radii of the rings probed around a point with no data, km.
    pub rings_km: Vec<f64>,
    /// Points per ring (north, east, south, west for 4).
    pub points_per_ring: u32,
}

/// The adapter. One per process: the limiter is shared by every garden's fetch.
pub struct SoilGrids {
    http: reqwest::Client,
    config: SoilGridsConfig,
    clock: Arc<dyn Clock>,
    /// Spaces calls by `min_interval`.
    pacer: Pacer,
    /// Soil Landscapes of Canada, asked when the point itself has no data.
    local: Option<Arc<SoilLandscapes>>,
}

/// Converts a response; `None` when every value is null (no data at this pixel).
pub fn to_depths(response: &QueryResponse) -> Option<Vec<SoilDepth>> {
    let mut depths: Vec<SoilDepth> = DEPTHS
        .iter()
        .filter_map(|label| band(label))
        .map(|depth_cm| SoilDepth {
            depth_cm,
            ph_h2o: None,
            clay_pct: None,
            sand_pct: None,
            silt_pct: None,
            organic_carbon_g_kg: None,
            nitrogen_g_kg: None,
            cec_cmol_kg: None,
            bulk_density_kg_dm3: None,
        })
        .collect();
    let mut any = false;
    for layer in &response.properties.layers {
        if layer.unit_measure.d_factor == 0.0 {
            continue;
        }
        for d in &layer.depths {
            let Some(target) =
                band(&d.label).and_then(|b| depths.iter_mut().find(|s| s.depth_cm == b))
            else {
                continue;
            };
            let value = d.values.mean.map(|m| m / layer.unit_measure.d_factor);
            any |= value.is_some();
            let slot = match layer.name.as_str() {
                "phh2o" => &mut target.ph_h2o,
                "clay" => &mut target.clay_pct,
                "sand" => &mut target.sand_pct,
                "silt" => &mut target.silt_pct,
                "soc" => &mut target.organic_carbon_g_kg,
                "nitrogen" => &mut target.nitrogen_g_kg,
                "cec" => &mut target.cec_cmol_kg,
                "bdod" => &mut target.bulk_density_kg_dm3,
                _ => continue,
            };
            *slot = value;
        }
    }
    any.then_some(depths)
}

/// The points probed around `at`, nearest ring first: `points_per_ring` points on each circle,
/// starting north, converted with 111.32 km per degree of latitude.
pub fn nearby_points(at: Coordinates, rings_km: &[f64], points_per_ring: u32) -> Vec<Coordinates> {
    let mut out = Vec::new();
    for &km in rings_km {
        for k in 0..points_per_ring {
            let bearing = std::f64::consts::TAU * f64::from(k) / f64::from(points_per_ring);
            let dlat = km * bearing.cos() / 111.32;
            let dlon = km * bearing.sin() / (111.32 * at.lat.to_radians().cos().max(0.01));
            if let Some(p) = Coordinates::new(at.lat + dlat, at.lon + dlon) {
                out.push(p);
            }
        }
    }
    out
}
```

**Null pixels.** Central Toronto, Scarborough, (43.75, −79.55) and Guelph all answered `"mean": null` for every property and depth on 2026-10-15, and again on 2026-10-20, with Ottawa (45.42, −75.70) too; High Park (43.646, −79.463), rural Wellington County (43.60, −80.40) and Iowa had data. SoilGrids masks built-up land, so its null pixels are the cities. Around central Toronto the four probes at 2 km were null, and the 5 km ring had data only to the north (43.695, −79.38). The response shapes matched the 2026-10-15 recordings, and the Iowa values were the same. The target users are Toronto gardeners, so "no data here" is a distinct outcome, not a success:

1. Fetch the full profile at the garden's point. If any value is present, done.
2. Otherwise, with `with_local_fallback`, ask Soil Landscapes of Canada for the point (below). If it has a soil there, done: no further call.
3. Otherwise probe the nearby points, nearest ring first (4 points at 2 km, then 4 at 5 km). Each probe is a one-value query (pH at 0–5 cm), which answered in 0.6–10 s live against 13–19 s for a full query.
4. At the first point with data, fetch the full profile.
5. Otherwise answer `ProviderError::NotFound { provider: "soilgrids" }`.

SLC comes before the probes because it describes the garden's own landscape polygon and costs no call, where the probes cost up to 8 paced calls (2 minutes) and describe a place 2–5 km away. An outage (`Unavailable`, `RateLimited`, `BadResponse`) never falls back: the point may have SoilGrids data, which is finer (250 m), so the retry after 15 minutes asks SoilGrids again.

**Soil Landscapes of Canada** ([architecture 6.4](../architecture/README.md#64-soil), open decision 10, *proposed*). Agriculture and Agri-Food Canada's SLC v3.2 maps the soil landscapes of Canada's agricultural regions at 1:1,000,000 under the Open Government Licence – Canada 2.0, cities included. Every null point above lies in a polygon with a described soil: central Toronto, Scarborough and High Park in polygon 563001 (100 % Chinguacousy clay loam: 0–27 cm pH 7.4, 29 % clay, 1.9 % organic carbon), North York in 562001 (60 % Peel), Guelph in 560004 (37 % Burford), Ottawa in 545001 (41 % Ste. Rosalie clay). Iowa, Lake Ontario and central Montréal have none. `backend/tools/slc/build_slc.py` (standard library only) builds `data/slc_v3r2.bin` from the published GeoPackage and the component and soil-layer tables:

- each polygon's **dominant component** (largest share), whose soil's layers are averaged onto the six depth bands by overlap; a band less than half covered stays empty, so most profiles end at 100 cm;
- pH from `PH2` ("pH as per project report", read as pH in water: 0.3–0.5 above `PHCA`, the calcium chloride pH; the method is *unverified*), `TSAND`, `TSILT`, `TCLAY` in %, `ORGCARB` % × 10 = g/kg, `CEC` meq/100 g = cmol(c)/kg, `BD` g/cm³ = kg/dm³; no nitrogen, which SLC doesn't map; `-9` is empty;
- the 3,923 polygons with a soil, rasterised on a **0.01° grid** (about 1.1 km by 0.8 km in southern Ontario, within the map's accuracy) as 126,760 runs over 1,265 distinct soils: 0.9 MB, 0.46 MB gzipped. A lookup scans the runs of one grid row.

The file is embedded with `include_bytes!` and parsed once at start (`SoilLandscapes::embedded`); `api` passes it to `SoilGrids::with_local_fallback`. The profile goes into the same `SoilProfile`, so the wire doesn't change. Attribution: "Contains information licensed under the Open Government Licence – Canada", with AAFC named as the source: in the module's docs here, and due on an app screen with Nominatim's ODbL notice, which the app doesn't show yet either (a frontend item).

```rust
/// The parsed lookup. Built once at start and shared, like the time-zone finder.
pub struct SoilLandscapes {
    lat0_e4: i64,
    lon0_e4: i64,
    step_e4: i64,
    cols: usize,
    /// Per soil, the six bands' values × 100, `EMPTY` where none.
    soils: Vec<[[u16; VALUES]; 6]>,
    /// Index of each row's first run; one more entry than rows.
    row_starts: Vec<u32>,
    runs: Vec<Run>,
}

/// Why the embedded file didn't load: only a bad build of `data/slc_v3r2.bin` causes it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SlcError {
    /// Not an `SLC1` file of version 1.
    #[error("not a version {VERSION} SLC lookup")]
    Header,
    /// The file is shorter or longer than its header says.
    #[error("the SLC lookup is {got} bytes, its header says {expected}")]
    Length {
        /// Bytes read.
        got: usize,
        /// Bytes the header implies.
        expected: usize,
    },
    /// A run points past the grid or the soils.
    #[error("run {0} is out of range")]
    Run(usize),
}
```

**Retries** ([contracts 9, the soil profile](../architecture/contracts.md#gardens)). Every failure sets `soil_profile_status: unavailable`; `api` stores the next attempt in `garden.soil_retry_at` by `domain::rules::limits::soil_retry_after`: 7 days after `NotFound`, because a null pixel doesn't change on a quick retry, and 15 minutes after anything else. The adapter stops at the first failing call, so an outage costs one call per garden and attempt; only a null pixel costs the 9 paced calls, now once a week ([api 16](api.md#16-the-scheduler)).

**Limits.** SoilGrids is in beta and allows 5 calls a minute (from a third-party plugin page; ISRIC's own pages don't say, *unverified*). The `Pacer` spaces calls 15 s apart for the whole process. The timeout is 60 s. The worst case, a null pixel outside SLC with every probe null, makes 9 calls, about 2 minutes; a null Canadian point inside SLC makes 1. Garden setup runs in the background, so the user doesn't wait.

| Outcome | Result | `soil_profile_status` ([contracts 9, gardens](../architecture/contracts.md#gardens)) |
|---|---|---|
| Data at the point, in SLC, or nearby | `Ok(SoilProfile)` | `ready` |
| No data at the point, in SLC or anywhere probed | `NotFound` | `unavailable`, retried after 7 days |
| Timeout, connect, 5xx | `Unavailable` | `unavailable`, retried after 15 minutes ([architecture 8.5](../architecture/README.md#85-garden-setup)) |
| 429 | `RateLimited` | `unavailable`, retried after 15 minutes |
| 4xx, unparsable body | `BadResponse` | `unavailable`, retried after 15 minutes; logged as a bug |

---

## 9. Geocoding

[Architecture 8.5](../architecture/README.md#85-garden-setup) and [contracts 9, gardens](../architecture/contracts.md#gardens): a garden's location comes from GPS with consent, or from a typed postal code or city, geocoded once. It yields coordinates, a label ("Toronto, ON") and the IANA zone that dates every `LocalDate`.

| Input | Coordinates | Label | Zone |
|---|---|---|---|
| GPS (`exact`, `about_1_km`) | the fix, rounded by `round_for` | Nominatim `reverse` at 0.01°; the time zone's city when it fails | `tzf-rs`, offline |
| Canadian postal code `A1A 1A1` | Nominatim `search?postalcode=…&country=CA`, then Zippopotam by its first 3 characters | Nominatim (`ISO3166-2-lvl4` → "ON"), Zippopotam (`state abbreviation`) | `tzf-rs` |
| Forward sortation area `A1A` | Zippopotam `/ca/{fsa}` | Zippopotam | `tzf-rs` |
| US ZIP | Open-Meteo geocoding, `countryCode=US` | Open-Meteo | Open-Meteo |
| City ("Guelph, ON") | Open-Meteo geocoding, `countryCode=CA` | `name, ` + the province code from `admin1` | Open-Meteo, else `tzf-rs` |

**Why three services.** Open-Meteo's geocoder does not resolve Canadian postal codes: "M5V 3L9", "M5V", "K1A 0B1" and "N1G" all returned nothing live. Nominatim resolves a full code but not a bare forward sortation area. Zippopotam resolves the area only, and its terms are unchecked (*unverified*), so `zippopotam_url: None` turns it off.

**Nominatim's policy:** at most 1 request a second, an identifying user agent (`USER_AGENT` names the project and its repository), results cached, ODbL attribution, and no autocomplete. Geocoding once per garden, with a 30-day cache keyed by the normalised query, fits the policy.

**When the reverse lookup fails** (an outage, a `429`, a body that doesn't parse, or no place at the point), a GPS garden is still created: its label is the time zone's city, `zone_label` ("America/Toronto" → "Toronto time zone"). It names the zone, not the town, because an Ottawa garden shares Toronto's zone, so the label never claims a place it can't vouch for. That answer isn't cached, so the next lookup at the point asks Nominatim again, and a location change (`PATCH`) relabels the garden. The zone is found first, offline, so a point with no zone (open ocean) is still `NotFound` and costs no call. The failure is logged once at `warn` with the provider and the error's kind, never the point. A place whose only name is its county gets the county ("Niagara County, NY", which Nominatim gave for a point in Lake Ontario), and Nominatim's `200 {"error": "Unable to geocode"}` (open water, 2026-10-20) is no place.

**Live answers on 2026-10-20.** Nominatim: `M5V3L9` → Toronto, ON; `N1G2W1` → Guelph, ON (`state_district` "Southwestern Ontario" is new to the address, and unused); a bare `M5V` → `[]`; the reverse lookups of Toronto, Guelph and Scarborough → their city (Scarborough is a `suburb` of Toronto), rural (43.60, −80.40) → Centre Wellington. Zippopotam: `M5V` → "Downtown Toronto (CN Tower / King and Spadina / Railway Lands / …)", `N1G` → "Guelph Central". Open-Meteo's geocoder: "Toronto" → the same three places as on 2026-10-15, "M5V 3L9" → nothing, "90210" in the US → Beverly Hills. Each answered in 0.2–0.8 s.

**Privacy.** A reverse lookup sends only the point rounded to 0.01°, whatever the precision. The label is city-level anyway, so an exact fix never leaves the server ([architecture 11](../architecture/README.md#11-security-privacy-and-safety)). The caller's own point is returned and stored at the precision the user chose.

```rust
/// Settings.
#[derive(Debug, Clone)]
pub struct GeocoderConfig {
    /// `https://geocoding-api.open-meteo.com/v1/search`.
    pub open_meteo_url: String,
    /// `https://nominatim.openstreetmap.org`.
    pub nominatim_url: String,
    /// `https://api.zippopotam.us`; `None` disables the FSA fallback.
    pub zippopotam_url: Option<String>,
    /// ISO country for city searches: `CA`.
    pub country_code: String,
    /// Total deadline per request.
    pub timeout: Duration,
    /// Nominatim spacing: 1 s.
    pub nominatim_interval: Duration,
    /// How long an answer is cached.
    pub cache_ttl: Duration,
    /// Most cached answers.
    pub cache_capacity: u64,
}

/// Offline coordinates → IANA zone, from `tzf-rs`'s bundled boundaries. Building it loads the
/// data (a few megabytes), so it is built once at start and shared.
pub struct TimeZoneFinder(tzf_rs::DefaultFinder);

/// The coordinates stored for a precision ([contracts 9]): `exact` as given, `about_1_km`
/// rounded to 0.01°, `about_10_km` to 0.1°. Geocoded postal codes and cities are stored as
/// geocoded.
pub fn round_for(at: Coordinates, precision: LocationPrecision) -> Coordinates {
    match precision {
        LocationPrecision::About1Km => Coordinates {
            lat: (at.lat * 100.0).round() / 100.0,
            lon: (at.lon * 100.0).round() / 100.0,
        },
        LocationPrecision::About10Km => at.round_to_10_km(),
        LocationPrecision::Exact | LocationPrecision::PostalCode | LocationPrecision::City => at,
    }
}

/// A postal code, normalised and classified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostalCode {
    /// `M5V3L9`: a full Canadian code, upper case, no space.
    Canadian(String),
    /// `M5V`: a forward sortation area only.
    CanadianFsa(String),
    /// `90210`.
    UsZip(String),
}

/// "Ontario" → "ON", for labels built from Open-Meteo's full `admin1` names.
pub fn province_code(admin1: &str) -> Option<&'static str> {
    Some(match admin1 {
        "Ontario" => "ON",
        "Quebec" | "Québec" => "QC",
        "British Columbia" => "BC",
        "Alberta" => "AB",
        "Manitoba" => "MB",
        "Saskatchewan" => "SK",
        "Nova Scotia" => "NS",
        "New Brunswick" => "NB",
        "Newfoundland and Labrador" => "NL",
        "Prince Edward Island" => "PE",
        "Northwest Territories" => "NT",
        "Yukon" => "YT",
        "Nunavut" => "NU",
        _ => return None,
    })
}

/// Nominatim `reverse`: a place, or `{"error": "Unable to geocode"}` with `200` where none
/// is near (open water, observed live 2026-10-20).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum NominatimReverse {
    /// The place at the point.
    Place(NominatimPlace),
    /// No place.
    Error {
        /// "Unable to geocode".
        error: String,
    },
}

/// The label of a garden whose reverse lookup failed: the city that names its time zone,
/// "America/Toronto" → "Toronto time zone". It names the zone, not the town: an Ottawa garden
/// gets "Toronto time zone" too, so it never claims a place it can't vouch for.
pub fn zone_label(zone: &TimeZone) -> String {
    let city = zone
        .0
        .rsplit('/')
        .next()
        .unwrap_or(&zone.0)
        .replace('_', " ");
    format!("{city} time zone")
}

/// The composite geocoder.
#[derive(Clone)]
pub struct CompositeGeocoder {
    http: reqwest::Client,
    config: Arc<GeocoderConfig>,
    zones: Arc<TimeZoneFinder>,
    nominatim: Arc<Pacer>,
    cache: Cache<String, Geocoded>,
}
```

`GeocodeQuery<'a>` (in `domain`) borrows the request's strings: `'a` is the handler's request body, which lives for the whole call. The `#[async_trait]` method boxes a future that holds the query, which `async_trait` allows because the boxed future's lifetime is tied to every borrowed argument.

`TimeZoneFinder` wraps `tzf_rs::DefaultFinder`, which takes `(lng, lat)` in that order. It answers `""` where no zone covers the point (open ocean), and that becomes `None`. Building it loads a few megabytes of boundaries, so `api` builds one at start and shares it through an `Arc`.

| Failure | `ProviderError` | Contract behaviour |
|---|---|---|
| Unknown postal code or city; unparsable postal code; no zone at the point | `NotFound { provider: "geocoder" }` | `400 invalid_request` with `details.field` = `location.postal_code`, `location.city` or `location.lat` (decided in `api`; contracts 2 has no dedicated code, section 17) |
| GPS: any failure of the reverse lookup | none: `Ok`, labelled by `zone_label` | `201`; the label is "<zone city> time zone" |
| Timeout, connect, 5xx (postal code, city) | `Unavailable` | `503 upstream_unavailable` with `upstream: "geocoder"` |
| 429 | `RateLimited` | the same |
| Unparsable body | `BadResponse` | the same; logged |

---

## 10. Clerk

### Session tokens (`AuthVerifier`)

[Contracts 1, authentication headers](../architecture/contracts.md#authentication-headers), [architecture 4](../architecture/README.md#4-users-and-authorization) and [frontend 8](../frontend/README.md#8-authentication-clerk): the app sends `Authorization: Bearer <Clerk session token>`, and the API verifies it against the instance's JWKS.

- **Algorithm:** RS256 only. A token signed with HS256 is rejected (`Validation::new(Algorithm::RS256)`) ✓.
- **Keys:** `{issuer}/.well-known/jwks.json` on the Frontend API. It needs no secret and isn't rate limited. The URL comes from configuration, never from the token's `iss`, which rules out SSRF. Keys are cached by `kid` for an hour. An unknown `kid` refetches the whole set, at most once every 30 s, and the refetch is single-flight (the mutex), so random `kid`s can't hammer Clerk.
- **Claims:** `iss` must equal the configured issuer. `exp`, `nbf`, `iss` and `sub` are required. `exp` and `nbf` are checked against the caller's `now` with a 5 s leeway. `jsonwebtoken`'s own time checks read the system clock, so they are off: its default `leeway` is 60 s, and `validate_nbf` is false by default. `aud` is not checked, because session tokens have none. `azp` is checked only when present, against the web origins: native apps send no `Origin`, so their tokens carry no `azp`.

```rust
/// Verifier settings.
#[derive(Debug, Clone)]
pub struct ClerkVerifierConfig {
    /// The instance's Frontend API URL, which is the `iss` claim:
    /// `https://clerk.example.com`. Development and production differ.
    pub issuer: String,
    /// Web origins allowed in `azp`. Native apps send no `Origin`, so their tokens carry no
    /// `azp` and skip the check.
    pub allowed_azp: Vec<String>,
    /// Clock skew allowed on `exp` and `nbf`.
    pub leeway: Duration,
    /// How long a key stays cached.
    pub key_ttl: Duration,
    /// The least time between two JWKS fetches.
    pub min_refetch: Duration,
    /// JWKS fetch deadline.
    pub timeout: Duration,
}

/// The claims PDM reads from a Clerk session token (v2). Unknown claims are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ClerkClaims {
    /// The user id: `user_…`.
    pub sub: String,
    /// The session id.
    #[serde(default)]
    pub sid: Option<String>,
    /// The Frontend API URL.
    pub iss: String,
    /// Expiry, Unix seconds.
    pub exp: i64,
    /// Not before, Unix seconds.
    pub nbf: i64,
    /// The requesting origin; absent for native apps.
    #[serde(default)]
    pub azp: Option<String>,
    /// Session status (`pending` with organisations, unused by PDM).
    #[serde(default)]
    pub sts: Option<String>,
    /// A display name, when a custom session-token claim adds it.
    #[serde(default)]
    pub name: Option<String>,
}

/// Verifies session tokens. One per process; `Clone` shares the key cache.
#[derive(Clone)]
pub struct ClerkVerifier {
    http: reqwest::Client,
    config: Arc<ClerkVerifierConfig>,
    validation: Arc<Validation>,
    keys: Cache<String, Arc<DecodingKey>>,
    /// When the JWKS was last fetched; the mutex makes concurrent misses single-flight.
    last_fetch: Arc<Mutex<Option<Instant>>>,
}
```

```rust
#[async_trait]
impl AuthVerifier for ClerkVerifier {
    async fn verify(&self, bearer: &str, now: Timestamp) -> Result<VerifiedToken, AuthError> {
        let invalid = |e: jsonwebtoken::errors::Error| AuthError::InvalidToken(Some(Box::new(e)));
        let header = decode_header(bearer).map_err(invalid)?;
        let kid = header
            .kid
            .ok_or_else(|| AuthError::InvalidToken(Some("no kid".into())))?;
        let key = self.key_for(&kid).await?;
        let claims = decode::<ClerkClaims>(bearer, &key, &self.validation)
            .map_err(invalid)?
            .claims;
        self.check_claims(&claims, now)?;
        let auth_id = claims
            .sub
            .parse::<AuthId>()
            .map_err(|e| AuthError::InvalidToken(Some(Box::new(e))))?;
        let expires_at = chrono::DateTime::from_timestamp(claims.exp, 0)
            .map(Timestamp::from)
            .ok_or_else(|| AuthError::InvalidToken(Some("exp out of range".into())))?;
        Ok(VerifiedToken {
            auth_id,
            display_name: claims.name,
            expires_at,
        })
    }
}
```

| Failure | `AuthError` | Contract |
|---|---|---|
| Malformed, wrong algorithm, bad signature, wrong issuer, expired, not yet valid, `azp` not allowed, unknown `kid` (also after a refetch, or while refetching is rate limited) | `InvalidToken(Some(source))` | `401 unauthenticated`; the app refreshes the token once, then signs out |
| JWKS unreachable or 5xx | `Unavailable` | `503 upstream_unavailable`, `upstream: "auth"` (the role, as `database` and `storage` are; `PROVIDER = "clerk"` names the vendor in logs only) |

### Account deletion (`AuthAdmin`)

`DELETE https://api.clerk.com/v1/users/{user_id}` with the instance secret key, the last step of `DELETE /me`'s cascade ([contracts 9, account](../architecture/contracts.md#account)). It is retried until it succeeds, so a 404 (already gone) is success and the call is idempotent. One call is one attempt, and the cascade schedules the retries.

```rust
/// Backend API settings.
#[derive(Debug, Clone)]
pub struct ClerkAdminConfig {
    /// `https://api.clerk.com/v1`.
    pub base_url: String,
    /// The instance secret key, `sk_…`. Never logged.
    pub secret_key: SecretString,
    /// Deadline per call.
    pub timeout: Duration,
}

/// Deletes accounts at Clerk, the last step of account deletion. The caller (the deletion
/// cascade) retries until it succeeds, so one call is one attempt.
#[derive(Debug, Clone)]
pub struct ClerkAdmin {
    http: reqwest::Client,
    config: Arc<ClerkAdminConfig>,
}
```

| Status | Result |
|---|---|
| 2xx, 404 | `Ok(())` |
| 429 | `RateLimited { retry_after_s }` (Clerk sends `Retry-After`) |
| 5xx, transport | `Unavailable` |
| 400, 401, 403 | `BadResponse`: a configuration bug (a wrong key). Retrying won't help, so alert |

---

## 11. Svix and the auth webhook

[Contracts 9, auth webhook](../architecture/contracts.md#auth-webhook): `POST /webhooks/auth` from Clerk through Svix. The body is verified before it is parsed, and a timestamp more than 5 minutes off is rejected.

**The scheme** (Svix's manual verification page): the signed content is `{svix-id}.{svix-timestamp}.{raw body}`. The key is the base64 secret after `whsec_`, the MAC is HMAC-SHA256, and the output is base64. `svix-signature` holds space-separated `v1,<base64>` entries, several during a secret rotation, and any match is enough. Here every entry is compared in constant time (`aws_lc_rs::constant_time::verify_slices_are_equal`) and the results are folded rather than short-circuited, so timing doesn't reveal which entry matched. The timestamp tolerance is ±5 minutes in both directions, as the `svix` crate does it.

```rust
/// The Svix verifier with Clerk's payload mapping.
pub struct SvixVerifier {
    key: hmac::Key,
}
```

```rust
impl SvixVerifier {
    /// From the endpoint's signing secret, `whsec_<base64>` (the prefix is optional, as in the
    /// `svix` crate).
    pub fn new(secret: &SecretString) -> Result<Self, BadSecret> {
        let s = secret.expose_secret();
        let b64 = s.strip_prefix("whsec_").unwrap_or(s);
        let raw = data_encoding::BASE64
            .decode(b64.as_bytes())
            .map_err(|_| BadSecret)?;
        if raw.is_empty() {
            return Err(BadSecret);
        }
        Ok(Self {
            key: hmac::Key::new(hmac::HMAC_SHA256, &raw),
        })
    }

    /// Checks the signature and timestamp only. `'a` is the request: the headers and body are
    /// borrowed for the check and nothing outlives it.
    pub fn verify_signature<'a>(
        &self,
        headers: WebhookHeaders<'a>,
        body: &'a [u8],
        now: Timestamp,
    ) -> Result<(), AuthError> {
        let ts: i64 = headers
            .timestamp
            .trim()
            .parse()
            .map_err(|_| AuthError::InvalidWebhook)?;
        let tolerance = i64::try_from(WEBHOOK_TOLERANCE.as_secs()).unwrap_or(300);
        if (now.unix_seconds() - ts).abs() > tolerance {
            return Err(AuthError::InvalidWebhook);
        }
        let mut ctx = hmac::Context::with_key(&self.key);
        ctx.update(headers.id.as_bytes());
        ctx.update(b".");
        ctx.update(headers.timestamp.as_bytes());
        ctx.update(b".");
        ctx.update(body);
        let expected = ctx.sign();
        let matched = headers
            .signature
            .split(' ')
            .filter_map(|entry| entry.strip_prefix("v1,"))
            .filter_map(|b64| data_encoding::BASE64.decode(b64.as_bytes()).ok())
            // `fold`, not `any`: every entry is compared, so timing doesn't reveal which matched.
            .fold(false, |acc, got| {
                acc | constant_time::verify_slices_are_equal(expected.as_ref(), &got).is_ok()
            });
        if matched {
            Ok(())
        } else {
            Err(AuthError::InvalidWebhook)
        }
    }
}
```

**Clerk's payload.** The envelope is `{object: "event", type, data, timestamp, instance_id}`, and `data` is the User object, so `data.id` is the user id. For `user.deleted`, `data` is a `DeletedObject` whose `id` is optional in Clerk's OpenAPI schema. The mapping is deterministic: a redelivery (same `svix-id`) maps to the same `AuthWebhookEvent`. `api` acts on `user.deleted` only: the account cascade starts with the account tombstone's compare-and-set, so a redelivery, or a `user.deleted` during a `DELETE /me` cascade, starts nothing. `user.created` changes nothing ([api 22](api.md#22-decisions-on-the-contracts-open-questions), question 24; [contracts 9, auth webhook](../architecture/contracts.md#auth-webhook)): the user record is made by the first authenticated request, so a late `user.created` can't resurrect a deleted user, and the tombstone refuses the deleted account's still-valid tokens.

```rust
/// Maps a verified body to the domain event. Deterministic, so a redelivered event (same
/// `svix-id`) maps to the same value and the handler's upsert/cascade keeps it idempotent.
/// `None` for other event types, for a `user.*` event without `data.id`, and for a body that
/// doesn't parse (logged: Svix would only redeliver the same bytes).
///
/// `ClerkEvent<'b>` borrows its `type` from the body (`'b`) instead of allocating it.
pub fn map_clerk_event(body: &[u8]) -> Option<AuthWebhookEvent> {
    let event: ClerkEvent<'_> = match serde_json::from_slice(body) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(error = %e, "signed auth webhook body did not parse; ignored");
            return None;
        }
    };
    let event_type = match event.kind.as_ref() {
        "user.created" => AuthEventType::UserCreated,
        "user.deleted" => AuthEventType::UserDeleted,
        _ => return None,
    };
    let id: AuthId = event.data.id?.parse().ok()?;
    Some(AuthWebhookEvent {
        event_type,
        data: AuthEventData { id },
    })
}
```

`ClerkEvent<'b>` borrows its `type` string from the body (`#[serde(borrow)]` on a `Cow<'b, str>`) instead of allocating it ([serde, borrowing data](https://serde.rs/lifetimes.html)). `'b` is the body, which outlives the mapping.

| Input | Result | Contract |
|---|---|---|
| Bad or missing signature, timestamp off by more than 5 min, unparsable timestamp | `Err(AuthError::InvalidWebhook)` | `401 unauthenticated` (`invalid_signature` is the signed-URL `403`); Svix redelivers |
| Verified `user.created`, `user.deleted` | `Ok(Some(event))` | `user.created`: nothing; `user.deleted`: the account cascade, once; `204` |
| Verified other type, `user.*` without `data.id`, unparsable body | `Ok(None)` | `204`; an unparsable signed body is logged |

---

## 12. Expo Push

[Contracts 9, push notifications](../architecture/contracts.md#push-notifications). `POST https://exp.host/--/api/v2/push/send` takes at most 100 messages. Each message has one token in `to`, so ticket *i* answers message *i*. The payload goes in `data`.

```rust
/// Settings.
#[derive(Debug, Clone)]
pub struct ExpoPushConfig {
    /// `https://exp.host/--/api/v2/push`.
    pub base_url: String,
    /// Needed only when "enhanced push security" is on for the Expo project.
    pub access_token: Option<SecretString>,
    /// Deadline per request.
    pub timeout: Duration,
    /// How old a ticket must be before its receipt is asked for: 15 minutes.
    pub receipt_delay: Duration,
    /// Tickets without a receipt after this are dropped: Expo keeps receipts 24 hours.
    pub receipt_expiry: Duration,
}

/// One message on the wire. Borrows the domain message (`'m`) for the length of the request.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpoMessage<'m> {
    /// One token.
    pub to: &'m str,
    /// Title.
    pub title: &'m str,
    /// Body.
    pub body: &'m str,
    /// `PushPayload`.
    pub data: &'m PushPayload,
    /// `"default"`: the iOS sound.
    pub sound: &'static str,
    /// `"high"` for questions and alerts, so they wake the device.
    pub priority: &'static str,
}

/// A ticket or a receipt.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Ticket {
    /// Accepted; a receipt id for tickets (receipts have none).
    Ok {
        /// The receipt id.
        #[serde(default)]
        id: Option<String>,
    },
    /// Refused.
    Error {
        /// Why.
        #[serde(default)]
        message: String,
        /// `error`: `DeviceNotRegistered`, `MessageTooBig`, `MessageRateExceeded`, …
        #[serde(default)]
        details: Option<ErrorDetails>,
    },
}

/// The Expo adapter. `Clone` shares the pending tickets.
#[derive(Clone)]
pub struct ExpoPush {
    http: reqwest::Client,
    config: Arc<ExpoPushConfig>,
    pending: Arc<std::sync::Mutex<Vec<Pending>>>,
}
```

`ExpoMessage<'m>` borrows a domain `PushMessage` for the length of one request: `'m` is the batch the caller handed over, and nothing is cloned to build the body.

**Unregistered devices.** A ticket error `DeviceNotRegistered` goes back in `PushReport::unregistered` straight away. Ok tickets are kept in memory with their token. `check_receipts` (a `Notifier` method with a default, added by the `api` author) asks `getReceipts` (at most 1,000 ids) about tickets at least 15 minutes old, as Expo advises. It returns the tokens whose receipt says `DeviceNotRegistered`, drops every id it got a receipt for, and drops ids without a receipt after 24 hours, when Expo forgets them. `api`'s scheduler calls it every 15 minutes and forgets the tokens. The adapter never touches the store.

A push is a hint, never the only copy. A batch that fails is logged and dropped, and `push` fails only when every batch failed. Pending tickets are not persisted: a restart loses at most 15 minutes of receipts, and the next push to a dead token reports it again. Keep one `ExpoPush` per process, because clones share the pending list.

| Failure | Result |
|---|---|
| 429 | `RateLimited` (Expo allows 600 notifications a second per project) |
| 5xx, transport | `Unavailable` |
| 4xx, or request-level `errors` (`PUSH_TOO_MANY_NOTIFICATIONS`, `UNAUTHORIZED`) | `BadResponse` |
| Ticket errors other than `DeviceNotRegistered` (`MessageTooBig`, `MessageRateExceeded`) | logged, not retried |

---

## 13. Jev

Jev is TypeSafe's System One model, a zero-shot text classifier ([architecture 10](../architecture/README.md#10-models-and-gpu-placement)). `api` wires `JevClassifier` when `JEV_API_KEY` and `JEV_URL` are set, with `Calibration::jev()` for the router ([backend agent 7](agent.md#7-router-and-safety-gate)). Otherwise it wires `agent::model::LlmClassifier` (the cheapest model with structured output) and `Calibration::llm()`. Nothing on the critical path depends on Jev.

```rust
/// The Jev version the router's calibration was fitted on (2026-10-23).
pub const DEFAULT_MODEL: &str = "jev-1.14.0";

/// The default deadline of one call, connect to last byte: within the router's
/// `classifier_timeout` (3 s), so a slow call fails here and is logged as Jev's.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

/// Settings.
#[derive(Debug, Clone)]
pub struct JevConfig {
    /// `JEV_URL`, such as `https://api.typesafe.ai`.
    pub base_url: String,
    /// `JEV_API_KEY`, sent as a bearer token.
    pub api_key: SecretString,
    /// `JEV_MODEL`: a versioned model id, [`DEFAULT_MODEL`] unless set.
    pub model: String,
    /// Deadline of one call: [`DEFAULT_TIMEOUT`].
    pub timeout: Duration,
}

/// The Jev client.
#[derive(Debug, Clone)]
pub struct JevClassifier {
    http: reqwest::Client,
    config: Arc<JevConfig>,
}
```

**The call** ✓ (TypeSafe's [API reference](https://docs.typesafe.ai/api/system-one.md) and [models page](https://docs.typesafe.ai/models.md), read on 2026-10-23, and run against `https://api.typesafe.ai` the same day). One `POST {JEV_URL}/v1/systemone` per classification, with `Authorization: Bearer <JEV_API_KEY>` and a JSON body:

```json
{
  "model": "jev-1.14.0",
  "state": "Why are my tomato leaves turning yellow?",
  "questions": {
    "diagnosis_question": {"type": "noul", "instructions": "Does this gardener's message ask what is wrong with a plant: …?"},
    "care_question": {"type": "noul", "instructions": "…"},
    "task_change": {"type": "noul", "instructions": "…"},
    "safety_sensitive": {"type": "noul", "instructions": "…"},
    "off_topic": {"type": "noul", "instructions": "…"}
  }
}
```

The answer names the version that answered and the usage; a `noul` is the probability of yes, a `choice` has a probability per option:

```json
{
  "model": "jev-1.14.0",
  "answers": {
    "diagnosis_question": {"type": "noul", "noul": 0.97},
    "care_question": {"type": "noul", "noul": 0.6}
  },
  "usage": {"input_tokens": 436, "output_tokens": 93}
}
```

**A follow-up** (`classify_with_context`, a route with the exchange before it, [agent 7](agent.md#7-router-and-safety-gate)) ✓ (run on 2026-10-24). The `state` names the message to judge and the exchange, and each route question starts with `CONTEXT_PREFIX` ("The state holds the gardener's newest `message` and, under `earlier`, the exchange just before it. Judge `message`, reading `earlier` only to understand what it refers to."):

```json
{
  "model": "jev-1.14.0",
  "state": {
    "earlier": {
      "gardener": "Will the frost in the forecast hurt my tomatoes this week?",
      "assistant": "The forecast has a low of -1 °C on Thursday night, so frost is likely. …"
    },
    "message": "Thanks. In one or two sentences, what did you just tell me about the forecast?"
  },
  "questions": {"off_topic": {"type": "noul", "instructions": "The state holds … Is this message about something other than plants, gardening or the garden?"}, "…": {}}
}
```

That call took 729 input tokens. A missing half (an unanswered message has no `assistant`) leaves its key out; an empty exchange, and the memory and topic tasks, send the text alone as before.

| Task | Questions | Scores |
|---|---|---|
| `Route { farm_records: false }` | one `noul` per label of `ROUTE_LABELS` | each label's `noul` |
| `Route { farm_records: true }` | the same with `farm_records`, and `safety_sensitive` and `off_topic` worded for the farm's records (gap G10) | each label's `noul` |
| `MemoryWorthy` | one `noul`, `remember` | `remember` = the `noul`, `skip` = 1 − it (`MEMORY_LABELS`) |
| `ChunkTopic` | one `choice`, `topic`, over `TOPIC_LABELS` with a description each | each topic's probability |

The route asks a yes/no question per label rather than one `choice`, so the labels are judged independently, as the LLM classifier scores them: "How much Kocide went into the backpack per litre for the bed 3 spray on July 10?" scored `safety_sensitive` 0.98 and `farm_records` 0.65, where a `choice` split 0.53 and 0.47 between them. `agent` compares `safety_sensitive` with `FARM_SAFETY_FLOOR` (0.5) after the calibration offset, which is +0.1 with Jev. The instructions follow the LLM prompt's label descriptions. Only the labels asked are scored, and a probability is clamped to 0–1. Every label asked must be answered with the type asked (a topic `choice` with every topic): a missing or mistyped answer is `InvalidOutput`, which routes the turn as a care question. Scoring a missing label 0 would let a lone `off_topic` answer win and skip the safety route. All four label lists are `domain::ports`' own, shared with `agent`.

**Model.** `JEV_MODEL` (default `DEFAULT_MODEL`, `jev-1.14.0`) is a versioned id; `api` refuses anything but `jev-<major>.<minor>.<patch>`, so never an alias such as `jev-latest`, which moves with each release, because the router's calibration was fitted on one version. Each answer's version and input tokens are logged at debug level (`jev answered`), and an answer from another version than `JEV_MODEL` is logged as a warning.

**Measured** on 2026-10-23 with `jev-1.14.0` (`tests/live_jev.rs` in `agent`, 82 route calls over two runs and 16 memory-gate calls): median 94–104 ms a call, the slowest 394 ms (the first, on a new connection); 436 input tokens for a route call without farm records, 508 with them, 346 for the memory gate, so about US$0.00002 a route call at US$0.042 per million input tokens (output is free).

**Failures.** A transport failure, the deadline (`DEFAULT_TIMEOUT`, 2 s from connect to the last byte, within the router's 3 s `classifier_timeout`) or a refusal status is `ModelError::Unavailable { model: "jev" }`; the router then routes as a care question and the memory gate skips the turn, as `agent` decides. The error says the status and, on `429` and `529`, the `retry-after` seconds, never the body (a `422` echoes the request, with the user's text). There is no retry: a chat turn can't wait out a rate limit. A body that isn't a `SystemOneResponse` is `InvalidOutput`, but one the deadline cuts off while it is read is `Unavailable`, like any timeout. All of these are covered by tests against a mock server (wiremock, and a raw socket for the stalled body) ✓; only the successful call ran against the live API.

---

## 14. Agent Memory

**What it is.** `SurrealDB` Agent Memory is a hosted memory service on `SurrealDB` Cloud. The `memory` table stays the system of record; each **shared** memory is mirrored into the service as one triple, and the service supersedes the older value of the same key and keeps the history ([domain, the belief store](domain.md)). A private chat memory never leaves the database. `AgentMemory` implements `BeliefStore` over the REST API. Every call is `{base_url}/api/v1/{context_id}/…` with `Authorization: Bearer <key>`, JSON in and out.

```rust
/// Settings.
#[derive(Debug, Clone)]
pub struct AgentMemoryConfig {
    /// The service's origin, such as `https://<instance>.agent-memory.surreal.cloud`.
    pub base_url: String,
    /// The context every call belongs to.
    pub context_id: String,
    /// The API key; never printed.
    pub api_key: SecretString,
    /// Deadline of an entity read (`current`, `history`): 3 s.
    pub read_timeout: Duration,
    /// Deadline of `/query`: 8 s, as it measured 2 to 10 s.
    pub search_timeout: Duration,
    /// Deadline of a write, a scope registration or a forget: 15 s.
    pub write_timeout: Duration,
}

/// The adapter. One per process: the cache of registered scope paths is shared by every call.
pub struct AgentMemory {
    http: reqwest::Client,
    config: AgentMemoryConfig,
    /// Scope paths known to be registered.
    scopes: Cache<String, ()>,
}
```

**Endpoints** (all measured 2026-10-17 against the live service). The `/facts` write body is `snake_case`, the `/query` body `camelCase`, responses `camelCase`, and **unknown fields are ignored silently**, so the tests assert exact request bodies.

| Port method | Call | Deadline | Notes |
|---|---|---|---|
| `record` | `POST /scopes {"path"}`, then `POST /facts` | `write_timeout` (15 s) | Registers the scope path first (`201`, idempotent, parents created). `/facts` body: `infer: "triples"`, `scopes: [[path]]`, `observed_at`, `labels: ["memory=<id>", "source=<pipeline\|user>"]`, `triples: [{entity: {type: "other", name}, key, value}]`. Labels sit at the top level of the body (measured to stick there). |
| `current` | `GET /entities/other/{name}` for the plant and for the garden, at once | `read_timeout` (3 s) | `404` is an empty list. Only attributes with `supersededBy == null`. No retries. About 0.4 s. |
| `history` | `GET /entities/other/{name}/history?limit=500` | `read_timeout` per page | Follows `page.nextCursor` with `&cursor=` until it is absent and **never stops on a short page**. `404` is empty. Sorted newest `learned_at` first. |
| `search` | `POST /query {query, k, lens, include: ["facts"]}` | `search_timeout` (8 s) | `k` is capped at 50. `lens` is `[[plant path], [garden path]]`, or `[[garden path]]`. Keeps hits with `source == "attribute"` whose `resource.name` is one of the two entities; strips the leading `<name>'s ` from the text. Measured at 2 to 10 s, so callers add a shorter timeout of their own. |
| `forget` | `POST /scopes/forget {"path": "<scope path>/"}` | `write_timeout` | Trailing slash. Hard-deletes every fact and entity in the subtree and answers `200 {"forgotten": n}`; `0` when there is nothing, so it is idempotent. |

**Scope registration.** A write to an unregistered path answers `400 {"message": "unknown scope node — register it first"}`. The adapter keeps the registered paths in a `moka` cache (one hour), registers before the first write to a path, and on that `400` drops the entry, registers and retries the write once. A second `400` is a `BadResponse`. After a `forget` the whole cache is dropped, because a garden's forget takes its plants' paths with it and registering again is cheap.

**No idempotency key.** The service replays a stored response for 24 hours for the same `Idempotency-Key`, which would swallow a rebuild that re-sends a memory after a forget, so none is sent. Triples reconcile by entity and key, so a re-send leaves one current value.

**Mapping a read.** `createdAt` comes with or without nanoseconds (`2026-10-17T15:57:02.010341933+00:00`) and is parsed as RFC 3339. `Belief.memory_id` comes from the `memory=<ulid>` label (`None` when it is missing or doesn't parse); `superseded_at` is `validUntil` when `supersededBy` is set; `about_plant` is true for the plant's own entity.

**Retries.** `record` retries twice (0.2 s then 0.4 s ceilings) after a connect error or a 5xx; a timeout is not retried. Reads are not retried: the caller falls back to the database.

| Outcome | Result |
|---|---|
| `2xx` | `Ok` |
| `404` on an entity read or history | an empty list |
| Connect error, timeout, 5xx | `Unavailable` |
| `429` | `RateLimited`, with `Retry-After` seconds |
| `401`, `403` | `Unavailable`, a misconfigured key; the status is in the source error, the key never is |
| Other `4xx`, an unparsable body, an unknown scope after registering | `BadResponse` |

---

## 15. Farm sources: Slack and Google Drive

**What they are.** Two standalone clients for a garden's farm connectors ([connectors 5 and 6](../connectors/README.md#5-connecting-and-tokens)). Slack's implements the `FarmSource` port through `SlackSource`, and Drive's through `DriveSource` (below), which the `connectors` task in `api` calls ([EON E4](../EON/README.md)). Both take the token as an argument of each call, so one client serves every workspace and account, and both reuse the shared `retry`, `RetryPolicy` and `Pacer` of section 4. Tokens are `SecretString` and never appear in an error or a log line.

### The shared error

One error type for both, with the codes a connector's `last_error` stores ([connectors 6](../connectors/README.md#6-sync)).

```rust
/// The code stored in a connector's `last_error` ([connectors 6]).
///
/// [connectors 6]: ../../../docs/connectors/README.md#6-sync
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FarmErrorCode {
    /// The vendor throttled us; the sync resumes from its cursor on a later run.
    RateLimited,
    /// Down, timed out, a 5xx or an answer that didn't parse.
    VendorUnavailable,
    /// The token was refused or revoked: the owner must reconnect.
    AuthRefused,
    /// Over the garden's record limit. Set by the sync, never by a client.
    TooLarge,
    /// A chosen channel or folder is gone or no longer visible to the token: nothing more runs
    /// until the owner changes the sources.
    SourceMissing,
}

/// A failed call to Slack or Google Drive.
#[derive(Debug, thiserror::Error)]
pub enum FarmClientError {
    /// The token is missing, wrong, expired, revoked or lacks a scope (Slack `not_authed`,
    /// `invalid_auth`, `token_revoked`, `account_inactive`, `missing_scope`; Google `401`, a
    /// `403` for an insufficient scope, or `invalid_grant` on a refresh). It is also a bad code
    /// the user brought to an OAuth exchange (Slack `invalid_code`, `code_already_used`,
    /// `code_expired`; Google `invalid_grant`). The rule: a bad user code or token is
    /// `AuthRefused`; a fault in our own configuration is [`Self::Refused`].
    #[error("{vendor}: token refused ({code})")]
    AuthRefused {
        /// `"slack"` or `"google_drive"`.
        vendor: &'static str,
        /// The vendor's short error code.
        code: String,
    },
    /// `429`, or Drive's `403` with reason `userRateLimitExceeded` or `rateLimitExceeded`,
    /// after the client's retries ran out.
    #[error("{vendor}: rate limited")]
    RateLimited {
        /// The vendor.
        vendor: &'static str,
        /// The vendor's `Retry-After`, if it sent one.
        retry_after: Option<Duration>,
    },
    /// Down, timed out or answering 5xx, after the client's retries ran out.
    #[error("{vendor}: unavailable")]
    Unavailable {
        /// The vendor.
        vendor: &'static str,
        /// The transport failure, when there was one.
        #[source]
        source: Option<TransportError>,
    },
    /// The channel, file or folder doesn't exist or isn't visible to the token
    /// (Slack `channel_not_found`, `not_in_channel`; Drive `404`).
    #[error("{vendor}: not found ({code})")]
    NotFound {
        /// The vendor.
        vendor: &'static str,
        /// The vendor's short error code.
        code: String,
    },
    /// Any other refusal: Slack's `ok: false` with another code, a Drive `403` that isn't a
    /// rate limit, a `400`. Never retried. Includes our own misconfiguration of an OAuth exchange
    /// (Slack `bad_redirect_uri`, `invalid_client_id`, `bad_client_secret`, `invalid_client`;
    /// Google `401 invalid_client`), which is not the user's to fix.
    #[error("{vendor}: refused ({code})")]
    Refused {
        /// The vendor.
        vendor: &'static str,
        /// The HTTP status.
        status: u16,
        /// The vendor's short error code.
        code: String,
    },
    /// The answer didn't parse into the expected shape.
    #[error("{vendor}: bad response")]
    Malformed {
        /// The vendor.
        vendor: &'static str,
    },
}
```

`FarmClientError::code()` maps a failure to the stored string:

| Failure | `last_error` |
|---|---|
| `AuthRefused` | `auth_refused`: the owner must reconnect |
| `RateLimited` | `rate_limited`: the sync resumes from its cursor on a later run |
| `NotFound` | `source_missing`: a chosen channel or folder is gone or not visible |
| `Unavailable`, `Refused`, `Malformed` | `vendor_unavailable` |
| (set by the sync, never by a client) | `too_large` |

`Refused` is `vendor_unavailable` because nothing the owner does to the token fixes it, and the sync skips a refused single file instead of stopping.

`FarmClientError::into_source_error(vendor, source_id)` turns a failure into the port's `FarmSourceError`, keeping the same `last_error` code: a `NotFound` while reading a chosen source is `SourceMissing` with that source's id; one on a call about the token or a user says nothing about the owner's choice and is `BadResponse`. Each adapter maps its client's errors through it.

### Slack

Read methods only, plus the OAuth exchange and the revoke: `auth.test`, `conversations.list`, `conversations.history`, `conversations.replies`, `users.info`, `auth.revoke` and `oauth.v2.access`.

```rust
/// The `vendor` of every [`FarmClientError`] this client returns.
pub const VENDOR: &str = "slack";

/// Slack's Web API host. A constant in `api` too: no route can point a connector elsewhere.
pub const SLACK_BASE_URL: &str = "https://slack.com";

/// Tier 1: one `conversations.history` or `conversations.replies` call a minute per workspace
/// for a new install of an app not in the Slack Marketplace ([connectors 3]).
///
/// [connectors 3]: ../../../docs/connectors/README.md#3-decisions
pub const TIER_1_INTERVAL: Duration = Duration::from_mins(1);

/// Tier 1's largest page of `conversations.history` and `conversations.replies`: 15 messages.
pub const TIER_1_PAGE_LIMIT: u32 = 15;

/// Settings.
#[derive(Debug, Clone)]
pub struct SlackConfig {
    /// `https://slack.com`, or an Era simulator for an `era` connector and the tests. The
    /// methods are under `<base_url>/api/`.
    pub base_url: String,
    /// Total deadline of one request.
    pub timeout: Duration,
    /// Retries of `429`, 5xx, `internal_error` and transport failures.
    pub retry: RetryPolicy,
    /// The least time between two `conversations.history` (or two `conversations.replies`)
    /// calls for one team. Zero turns pacing off.
    pub history_interval: Duration,
    /// `limit` of a `conversations.history` or `conversations.replies` page.
    pub history_limit: u32,
    /// `limit` of a `conversations.list` page (a Tier 2 method, not paced).
    pub list_limit: u32,
    /// Most (team, method) pacers kept at once.
    pub max_pacers: u64,
}

/// `auth.test`: who the token is, and the workspace URL permalinks are built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthTest {
    /// The workspace's name.
    pub team: String,
    /// The workspace's id, which keys the pacers.
    pub team_id: String,
    /// The workspace URL, with a trailing slash: `https://maplerow.slack.com/`.
    pub url: String,
    /// The bot's user id.
    pub user_id: String,
    /// The bot's id.
    pub bot_id: Option<String>,
}

/// A channel the bot is a member of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackChannel {
    /// `C…`.
    pub id: String,
    /// The name without `#`.
    pub name: String,
    /// A private channel (`groups:read`).
    pub is_private: bool,
}

/// One message of a channel or a thread. Its `Debug` leaves the text out.
#[derive(Clone, PartialEq, Eq)]
pub struct SlackMessage {
    /// The message's id within its channel: `1791792000.000100`.
    pub ts: String,
    /// The author's user id; absent for some bot and system messages.
    pub user: Option<String>,
    /// The text, in Slack's markup.
    pub text: String,
    /// The thread's parent `ts`: set on a parent that has replies and on each reply.
    pub thread_ts: Option<String>,
    /// Replies in the thread, on a parent.
    pub reply_count: u32,
    /// The newest reply's `ts`, on a parent; the sync reads the replies when it changes.
    pub latest_reply: Option<String>,
    /// `channel_join`, `bot_message` and the like; `None` for a plain message.
    pub subtype: Option<String>,
}

/// One page of `conversations.history`, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HistoryPage {
    /// The messages.
    pub messages: Vec<SlackMessage>,
    /// The cursor of the next (older) page; `None` on the last.
    pub next_cursor: Option<String>,
}

/// A Slack user, as the sync keeps one: the id and the names, never an email. Its `Debug`
/// shows the id only, since names are personal data.
#[derive(Clone, PartialEq, Eq)]
pub struct SlackUser {
    /// `U…`.
    pub id: String,
    /// The handle (`maria`).
    pub name: Option<String>,
    /// The full name (`Maria Lopez`).
    pub real_name: Option<String>,
    /// The display name the user chose (`Maria`); often empty in Slack, then `None`.
    pub display_name: Option<String>,
}

/// `oauth.v2.access`: the bot token and the workspace it belongs to. The token's `Debug` is
/// redacted.
#[derive(Debug)]
pub struct OAuthAccess {
    /// `xoxb-…`, to be sealed at once.
    pub access_token: SecretString,
    /// `bot`.
    pub token_type: Option<String>,
    /// The granted scopes, comma-separated.
    pub scope: String,
    /// The bot's user id.
    pub bot_user_id: Option<String>,
    /// The Slack app's id.
    pub app_id: Option<String>,
    /// The workspace's id.
    pub team_id: String,
    /// The workspace's name.
    pub team_name: String,
}

/// The Slack Web API client. One per process; `Clone` shares the pacers.
#[derive(Clone)]
pub struct SlackClient {
    http: reqwest::Client,
    config: Arc<SlackConfig>,
    pacers: Cache<(String, &'static str), Arc<Pacer>>,
}
```

```rust
impl SlackClient {
    /// A client over the shared `reqwest::Client`.
    pub fn new(http: reqwest::Client, config: SlackConfig) -> Self {
        let idle = config
            .history_interval
            .saturating_mul(2)
            .max(MIN_PACER_IDLE);
        let pacers = Cache::builder()
            .max_capacity(config.max_pacers)
            .time_to_idle(idle)
            .build();
        Self {
            http,
            config: Arc::new(config),
            pacers,
        }
    }

    /// The settings.
    pub fn config(&self) -> &SlackConfig {
        &self.config
    }

    /// `auth.test`: the workspace and the bot behind a token.
    pub async fn auth_test(&self, token: &SecretString) -> Result<AuthTest, FarmClientError> {
        let w: wire::AuthTest = self.call(&Call::read("auth.test", token, &[])).await?;
        Ok(AuthTest {
            team: w.team,
            team_id: w.team_id,
            url: w.url,
            user_id: w.user_id,
            bot_id: w.bot_id,
        })
    }

    /// `conversations.list`, every page: the public and private channels the bot is a member
    /// of, archived ones left out.
    pub async fn conversations_list(
        &self,
        token: &SecretString,
    ) -> Result<Vec<SlackChannel>, FarmClientError> {
        let limit = self.config.list_limit.to_string();
        let mut channels = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut query = vec![
                ("types", "public_channel,private_channel"),
                ("exclude_archived", "true"),
                ("limit", limit.as_str()),
            ];
            if let Some(c) = cursor.as_deref() {
                query.push(("cursor", c));
            }
            let page: wire::ChannelList = self
                .call(&Call::read("conversations.list", token, &query))
                .await?;
            channels.extend(page.channels.into_iter().filter(|c| c.is_member).map(|c| {
                SlackChannel {
                    id: c.id,
                    name: c.name,
                    is_private: c.is_private,
                }
            }));
            cursor = page
                .response_metadata
                .and_then(wire::ResponseMetadata::next);
            if cursor.is_none() {
                return Ok(channels);
            }
        }
        Err(too_many_pages("conversations.list"))
    }

    /// One page of `conversations.history`, newest first: messages after `oldest` (exclusive)
    /// when given, from `cursor` when given. Paced per `team_id`.
    pub async fn conversations_history(
        &self,
        token: &SecretString,
        team_id: &str,
        channel: &str,
        oldest: Option<&str>,
        cursor: Option<&str>,
    ) -> Result<HistoryPage, FarmClientError> {
        let limit = self.config.history_limit.to_string();
        let mut query = vec![("channel", channel), ("limit", limit.as_str())];
        if let Some(oldest) = oldest {
            query.push(("oldest", oldest));
        }
        if let Some(cursor) = cursor {
            query.push(("cursor", cursor));
        }
        let call = Call {
            paced_team: Some(team_id),
            ..Call::read("conversations.history", token, &query)
        };
        let page: wire::Messages = self.call(&call).await?;
        Ok(HistoryPage {
            messages: page.messages.into_iter().map(message).collect(),
            next_cursor: page
                .response_metadata
                .and_then(wire::ResponseMetadata::next),
        })
    }

    /// `conversations.replies`, every page: the parent, then its replies. Each page is paced
    /// per `team_id`.
    pub async fn conversations_replies(
        &self,
        token: &SecretString,
        team_id: &str,
        channel: &str,
        thread_ts: &str,
    ) -> Result<Vec<SlackMessage>, FarmClientError> {
        let limit = self.config.history_limit.to_string();
        let mut messages = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut query = vec![
                ("channel", channel),
                ("ts", thread_ts),
                ("limit", limit.as_str()),
            ];
            if let Some(c) = cursor.as_deref() {
                query.push(("cursor", c));
            }
            let call = Call {
                paced_team: Some(team_id),
                ..Call::read("conversations.replies", token, &query)
            };
            let page: wire::Messages = self.call(&call).await?;
            messages.extend(page.messages.into_iter().map(message));
            cursor = page
                .response_metadata
                .and_then(wire::ResponseMetadata::next);
            if cursor.is_none() {
                return Ok(messages);
            }
        }
        Err(too_many_pages("conversations.replies"))
    }

    /// The parent of a thread and nothing else (`conversations.replies` with `limit=1`): one
    /// call that shows its current `reply_count` and `latest_reply`. `None` when Slack returns
    /// no message. Paced per `team_id`.
    pub async fn conversations_thread_parent(
        &self,
        token: &SecretString,
        team_id: &str,
        channel: &str,
        thread_ts: &str,
    ) -> Result<Option<SlackMessage>, FarmClientError> {
        let query = [("channel", channel), ("ts", thread_ts), ("limit", "1")];
        let call = Call {
            paced_team: Some(team_id),
            ..Call::read("conversations.replies", token, &query)
        };
        let page: wire::Messages = self.call(&call).await?;
        Ok(page.messages.into_iter().next().map(message))
    }

    /// `users.info`: the user's names. Never the email, even when Slack sends one.
    pub async fn users_info(
        &self,
        token: &SecretString,
        user: &str,
    ) -> Result<SlackUser, FarmClientError> {
        let query = [("user", user)];
        let w: wire::UserInfo = self.call(&Call::read("users.info", token, &query)).await?;
        let profile = w.user.profile.unwrap_or_default();
        Ok(SlackUser {
            id: w.user.id,
            name: non_empty(w.user.name),
            real_name: non_empty(profile.real_name).or_else(|| non_empty(w.user.real_name)),
            display_name: non_empty(profile.display_name),
        })
    }

    /// `auth.revoke`: the token stops working. A token that is already revoked answers
    /// `token_revoked` or `invalid_auth`, and that counts as revoked (`Ok`), as Google's
    /// `invalid_token` does: the token is gone either way. Any other refusal is an error.
    pub async fn auth_revoke(&self, token: &SecretString) -> Result<(), FarmClientError> {
        let call = Call {
            verb: Method::POST,
            ..Call::read("auth.revoke", token, &[])
        };
        match self.call::<wire::Empty>(&call).await {
            Ok(_) => Ok(()),
            Err(FarmClientError::AuthRefused { code, .. })
                if code == "token_revoked" || code == "invalid_auth" =>
            {
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// `oauth.v2.access`: exchanges an authorization code for the bot token. The Slack app is a
    /// confidential client, so the client secret goes with the code and there is no PKCE
    /// ([connectors 5]). Not retried after a timeout or a 5xx, since a code works once.
    ///
    /// [connectors 5]: ../../../docs/connectors/README.md#5-connecting-and-tokens
    pub async fn oauth_v2_access(
        &self,
        code: &str,
        redirect_uri: &str,
        client_id: &str,
        client_secret: &SecretString,
    ) -> Result<OAuthAccess, FarmClientError> {
        let body = form(&[
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", client_id),
            ("client_secret", client_secret.expose_secret()),
        ]);
        let call = Call {
            method: "oauth.v2.access",
            verb: Method::POST,
            token: None,
            query: &[],
            form: Some(&body),
            paced_team: None,
            idempotent: false,
        };
        // A bad code is the user's (`AuthRefused`); a bad redirect URI, client id or secret is our
        // own configuration and stays `Refused`.
        let w: wire::OAuthAccess = self.call(&call).await.map_err(|error| match error {
            FarmClientError::Refused { vendor, code, .. } if BAD_CODES.contains(&code.as_str()) => {
                FarmClientError::AuthRefused { vendor, code }
            }
            other => other,
        })?;
        Ok(OAuthAccess {
            access_token: SecretString::from(w.access_token),
            token_type: w.token_type,
            scope: w.scope,
            bot_user_id: w.bot_user_id,
            app_id: w.app_id,
            team_id: w.team.id,
            team_name: w.team.name,
        })
    }

    /// The pacer of one team's method; `None` when pacing is off.
    async fn pacer(&self, team: &str, method: &'static str) -> Option<Arc<Pacer>> {
        let interval = self.config.history_interval;
        if interval.is_zero() {
            return None;
        }
        Some(
            self.pacers
                .get_with((team.to_owned(), method), async move {
                    Arc::new(Pacer::new(interval))
                })
                .await,
        )
    }

    /// One call with its retries; each attempt waits for the pacer first.
    async fn call<T: DeserializeOwned>(&self, call: &Call<'_>) -> Result<T, FarmClientError> {
        let pacer = match call.paced_team {
            Some(team) => self.pacer(team, call.method).await,
            None => None,
        };
        let pacer = pacer.as_deref();
        http::retry(self.config.retry, |_| async move {
            if let Some(pacer) = pacer {
                pacer.wait().await;
            }
            self.attempt(call).await
        })
        .await
    }

    async fn attempt<T: DeserializeOwned>(&self, call: &Call<'_>) -> Attempt<T, FarmClientError> {
        let url = format!("{}/api/{}", self.config.base_url, call.method);
        let mut request = self
            .http
            .request(call.verb.clone(), url)
            .timeout(self.config.timeout);
        if !call.query.is_empty() {
            request = request.query(call.query);
        }
        // An empty token sends no `Authorization` at all, which Slack answers `not_authed`.
        if let Some(token) = call.token.map(ExposeSecret::expose_secret)
            && !token.is_empty()
        {
            request = request.bearer_auth(token);
        }
        if let Some(form) = call.form {
            request = request
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(form.to_owned());
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(e) => return transport(call, e),
        };
        let status = response.status();
        let retry_after = http::retry_after(response.headers());
        match response.bytes().await {
            Ok(body) => judge(call.method, call.idempotent, status, retry_after, &body),
            Err(e) => transport(call, e),
        }
    }
}
```

**`ok: false` at HTTP 200.** Slack refuses with status 200 and `{"ok": false, "error": "<code>"}`, so the client reads `ok` before anything else. `not_authed`, `invalid_auth`, `token_revoked`, `token_expired` and `account_inactive` are `AuthRefused`, and so are `missing_scope` and `no_permission`: the owner must reconnect to grant the scope. `channel_not_found`, `not_in_channel`, `thread_not_found` and `user_not_found` are `NotFound`. `ratelimited` and HTTP 429 are retried after the `Retry-After` the server sent, then `RateLimited`. A 5xx, `internal_error` or a timeout is retried, then `Unavailable`. Any other code is `Refused`, never retried. A code that isn't a short `snake_case` word is replaced by `unrecognised`, so a body can't leak through the code.

**The pacer.** `conversations.history` and `conversations.replies` are Tier 1 for a new install of an app outside the Slack Marketplace: about one call a minute per method and workspace ([connectors 3](../connectors/README.md#3-decisions)). `TIER_1_INTERVAL` is 1 minute and `TIER_1_PAGE_LIMIT` 15 messages a page. Each (team, method) pair has its own `Pacer`, so every connector holding one team's token shares it. The pacers live in a `moka` cache bounded by `SlackConfig::max_pacers` (10,000), dropped after idling for twice the interval and at least 10 minutes, so the map can't grow without bound and a pacer is never dropped while its last call is inside its interval. A call waits for its turn. The sync bounds a run and continues from its cursor on the next one.

**`SlackConfig::era`.** Points the client at an Era simulator: no pacing and no back-off, but a `Retry-After` is still honoured (up to 60 s). Pages stay at 15, so paging is exercised.

**`users.info` without the email.** The method returns the user's names only (`SlackUser::label` picks the display name, then the real name). The email is never read, even when Slack sends one ([connectors 8](../connectors/README.md#8-privacy-and-security)).

**Permalinks.** `permalink(workspace_url, channel, ts, thread_ts)` builds a message's link from `auth.test`'s workspace URL: `<url>/archives/<channel>/p<ts without the dot>`, with `?thread_ts=<parent>&cid=<channel>` for a reply. A parent (its `thread_ts` equal to its `ts`) gets the plain link. It makes no call.

```rust
/// A message's permalink, built from `auth.test`'s workspace URL ([connectors 6]):
/// `<url>archives/<channel>/p<ts without the dot>`, with `?thread_ts=<parent>&cid=<channel>`
/// for a reply. A parent (its `thread_ts` equal to its `ts`) gets the plain link.
///
/// [connectors 6]: ../../../docs/connectors/README.md#6-sync
pub fn permalink(workspace_url: &str, channel: &str, ts: &str, thread_ts: Option<&str>) -> String {
    let base = workspace_url.trim_end_matches('/');
    let id: String = ts.chars().filter(|c| *c != '.').collect();
    match thread_ts.filter(|parent| *parent != ts) {
        Some(parent) => format!("{base}/archives/{channel}/p{id}?thread_ts={parent}&cid={channel}"),
        None => format!("{base}/archives/{channel}/p{id}"),
    }
}
```

### `SlackSource`: the port for the sync

`SlackSource` wraps a `SlackClient` and implements `FarmSource` (E4.2, [connectors 6](../connectors/README.md#6-sync)). `api` builds one per endpoint: the real Slack with Tier 1 pacing, and an Era simulator with `SlackConfig::era`.

```rust
/// The trailing window read again once a day ([connectors 6]).
///
/// [connectors 6]: ../../../docs/connectors/README.md#6-sync
pub const REREAD_WINDOW: Duration = Duration::from_hours(7 * 24);

/// How long an author's name is kept per team ([connectors 6]).
///
/// [connectors 6]: ../../../docs/connectors/README.md#6-sync
pub const USER_NAME_TTL: Duration = Duration::from_hours(24);

/// The farm-connector adapter for Slack: one per process and per endpoint (the real Slack, or
/// an Era simulator), sharing the client's pacers and its own name cache.
#[derive(Clone)]
pub struct SlackSource {
    client: SlackClient,
    /// `(team, user)` → the user's name, or `None` for a user Slack doesn't know.
    names: Cache<(String, String), Option<Arc<str>>>,
}
```

- **A page** is one `conversations.history` page of one channel (newest first), with the replies of the threads on it whose parent changed, and each author's name. A message's record joins its replies, each on its own paragraph after its author's name; joins, topic changes and pins are left out, and a reply sent to the channel too arrives with its thread.
- **The cursor** is the adapter's own JSON in `SyncCursor`: `newest`, the newest `ts` of the finished passes; `pass`, a pass with pages left (its `oldest`, Slack's page cursor and the newest `ts` it has seen); `threads`, each thread parent's `latest_reply`, `reply_count` and a short hash of its text when its replies were read; and `probed`, the round-robin position of the older-thread checks. A new pass reads only what is after `newest` (`oldest` is exclusive), so a quiet channel costs one call. A pass interrupted by a `429` resumes from its page cursor on the next run, reading no page twice. A parent seen again with an unchanged `latest_reply`, `reply_count` and text costs no `conversations.replies` call and isn't sent again: its id comes back in `FarmPage::kept`.
- **Older threads.** At the end of a pass, up to `FarmPageRequest::probe_limit` threads whose parent is older than 7 days and whose last reply is within 30 days are checked, round-robin from `probed`, with `conversations.replies` and `limit=1` (`SlackClient::conversations_thread_parent`). The 30 days count only time the channel was read: each page sets `watched`, and a page read over `AWAY_AFTER` (7 days) after the last one, a pass starting or one with pages left, records the gap in `gaps` as `[start, seconds]` (those that started in the last 180 days are kept); a thread's quiet time leaves out the gaps after its last reply, so a thread active when the sync stopped is still checked when it resumes, and one whose last reply came after a gap is aged without it. `FarmPage::probes` counts them. A thread whose `latest_reply` or `reply_count` moved is read in full and sent. Slack no longer having it forgets it; a deleted parent (a `tombstone`) forgets it and reports its id in `FarmPage::removed`; an answer for another message is skipped. Each check gathers what it found before touching the cursor: a refused token fails the page, and any other failure ends the checks with `probed` moved past the thread that failed, so a thread whose check keeps failing doesn't hold up the others, and returns the page. Threads whose parent left the 180 days are forgotten before the checks.
- **The daily re-read.** `reread_window()` is 7 days. When the sync sets `reread_from`, a new pass starts there instead of at `newest`, so an edited message is sent again, or at `newest` when that is older (a sync after a week or more without one), so nothing between them is skipped; Slack reports no deletion, so the sync deletes what the pass didn't see. A new reply to a thread shows in its parent's `latest_reply`, which a pass reads when the parent is in its window: the next pass when the parent is new, the daily re-read for the last 7 days.
- **Authors.** `users.info` once per (team, user) a day in a `moka` cache, keeping `SlackUser::label()` only, never the email; a user Slack doesn't know has no name.
- **Links** come from the workspace URL in the connector's account (`auth.test`'s `url` at connect), through `permalink`.
- **Duplicates.** Era's simulator lists a parent injected a second time (to add a reply) twice in one page; the copy with the newest `latest_reply` wins.

### Google Drive

```rust
/// The `vendor` of every error and log line.
pub const VENDOR: &str = "google_drive";

/// The one scope PDM asks for ([connectors 5]).
///
/// [connectors 5]: ../../../docs/connectors/README.md#5-connecting-and-tokens
pub const SCOPE: &str = "https://www.googleapis.com/auth/drive.readonly";

/// Most files a folder listing returns ([connectors 6]).
///
/// [connectors 6]: ../../../docs/connectors/README.md#6-sync
pub const MAX_FILES: usize = 500;

/// Most folders a listing walks into, the chosen one included, so a tree of empty folders
/// can't run without end.
pub const MAX_FOLDERS: usize = 500;

/// A file's text is cut at 1 MB ([connectors 6]).
///
/// [connectors 6]: ../../../docs/connectors/README.md#6-sync
pub const MAX_TEXT_BYTES: usize = 1_000_000;

/// Settings.
#[derive(Debug, Clone)]
pub struct DriveConfig {
    /// `https://www.googleapis.com`, or a simulator's base URL. No trailing slash.
    pub api_base: String,
    /// `https://oauth2.googleapis.com/token`.
    pub token_url: String,
    /// `https://oauth2.googleapis.com/revoke`.
    pub revoke_url: String,
    /// Total deadline of one request, a download's body included.
    pub timeout: Duration,
    /// Retries of `429`, rate-limit `403`s, `408`, 5xx and transport failures. The code
    /// exchange is never retried: a code works once.
    pub retry: RetryPolicy,
    /// `pageSize` of `files.list` and `changes.list`, 1 to 1000.
    pub page_size: u32,
}

/// What `files.export` converts a Google file to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// `text/plain`, for a Google Doc.
    PlainText,
    /// `text/csv`, for a Google Sheet: its first tab only.
    Csv,
}

/// The Drive client. Cheap to clone; every call takes the connector's access token.
#[derive(Clone)]
pub struct DriveClient {
    http: reqwest::Client,
    config: Arc<DriveConfig>,
    /// `Some(config.timeout)`. Only the unit tests clear it (paused time: tokio would otherwise
    /// auto-advance to the deadline while the mock server answers); production never does.
    request_timeout: Option<Duration>,
}
```

```rust
/// A PKCE verifier and its `S256` challenge.
#[derive(Debug)]
pub struct Pkce {
    verifier: SecretString,
    challenge: String,
}

/// A verifier that RFC 7636 4.1 doesn't allow: 43 to 128 characters of
/// `[A-Z] / [a-z] / [0-9] / "-" / "." / "_" / "~"`.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("PKCE verifier must be 43 to 128 unreserved characters")]
pub struct InvalidVerifier;
```

```rust
impl DriveClient {
    /// A client over the shared `reqwest::Client` ([`crate::http::client`]).
    pub fn new(http: reqwest::Client, config: DriveConfig) -> Self {
        let request_timeout = Some(config.timeout);
        Self {
            http,
            config: Arc::new(config),
            request_timeout,
        }
    }

    // -----------------------------------------------------------------------------------------
    // OAuth
    // -----------------------------------------------------------------------------------------

    /// Exchanges an authorization code with its PKCE verifier (`grant_type=authorization_code`).
    /// Not retried: a code works once, and a reused one is `invalid_grant`.
    pub async fn exchange_code(
        &self,
        code: &str,
        verifier: &SecretString,
        redirect_uri: &str,
        client_id: &str,
        client_secret: &SecretString,
    ) -> Result<TokenGrant, FarmClientError> {
        let body = form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("code_verifier", verifier.expose_secret()),
            ("redirect_uri", redirect_uri),
            ("client_id", client_id),
            ("client_secret", client_secret.expose_secret()),
        ]);
        let response: wire::TokenResponse = self
            .json("oauth.token", RetryPolicy::NONE, || {
                self.form_post(&self.config.token_url, body.clone())
            })
            .await?;
        Ok(response.into())
    }

    /// A new access token from the refresh token (`grant_type=refresh_token`). A refused
    /// refresh token (`invalid_grant`: revoked, expired, or a Testing-mode token over 7 days
    /// old) is [`FarmClientError::AuthRefused`].
    pub async fn refresh(
        &self,
        refresh_token: &SecretString,
        client_id: &str,
        client_secret: &SecretString,
    ) -> Result<TokenGrant, FarmClientError> {
        let body = form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.expose_secret()),
            ("client_id", client_id),
            ("client_secret", client_secret.expose_secret()),
        ]);
        let response: wire::TokenResponse = self
            .json("oauth.refresh", self.config.retry, || {
                self.form_post(&self.config.token_url, body.clone())
            })
            .await?;
        Ok(response.into())
    }

    /// Revokes a token (the refresh token revokes the whole grant). A token Google no longer
    /// knows (`400 invalid_token`) counts as revoked.
    pub async fn revoke(&self, token: &SecretString) -> Result<(), FarmClientError> {
        let body = form(&[("token", token.expose_secret())]);
        match self
            .send("oauth.revoke", self.config.retry, || {
                self.form_post(&self.config.revoke_url, body.clone())
            })
            .await
        {
            Ok(_) => Ok(()),
            Err(FarmClientError::Refused {
                status: 400, code, ..
            }) if code == "invalid_token" => Ok(()),
            Err(e) => Err(e),
        }
    }

    // -----------------------------------------------------------------------------------------
    // Drive
    // -----------------------------------------------------------------------------------------

    /// The account's email, from `about.get`.
    pub async fn about_email(&self, token: &SecretString) -> Result<String, FarmClientError> {
        let about: wire::About = self
            .json("about.get", self.config.retry, || {
                self.get(token, "/drive/v3/about")
                    .query(&[("fields", "user(emailAddress)")])
            })
            .await?;
        Ok(about.user.email_address)
    }

    /// Every file under `folder_id`, at any depth, across `nextPageToken`, stopping at
    /// [`MAX_FILES`] files or [`MAX_FOLDERS`] folders (`truncated` says so). Trashed files are
    /// left out; a shared drive's folder works.
    pub async fn list_folder(
        &self,
        token: &SecretString,
        folder_id: &str,
    ) -> Result<FolderListing, FarmClientError> {
        let mut listing = FolderListing {
            files: Vec::new(),
            folders: vec![folder_id.to_owned()],
            tree: Vec::new(),
            truncated: false,
        };
        let mut seen: HashSet<String> = HashSet::from([folder_id.to_owned()]);
        let mut next = 0;
        let mut pages = 0_u32;
        let fields = format!("nextPageToken,files({FILE_FIELDS})");
        let page_size = self.page_size();
        'folders: while let Some(folder) = listing.folders.get(next).cloned() {
            next += 1;
            let q = format!("'{}' in parents and trashed=false", quote(&folder));
            let mut page_token: Option<String> = None;
            let mut tokens: HashSet<String> = HashSet::new();
            loop {
                pages += 1;
                if pages > MAX_PAGES {
                    return Err(malformed("files.list"));
                }
                let page: wire::FileList = self
                    .json("files.list", self.config.retry, || {
                        let mut query = vec![
                            ("q", q.as_str()),
                            ("fields", fields.as_str()),
                            ("pageSize", page_size.as_str()),
                            ("supportsAllDrives", "true"),
                            ("includeItemsFromAllDrives", "true"),
                        ];
                        if let Some(t) = &page_token {
                            query.push(("pageToken", t));
                        }
                        self.get(token, "/drive/v3/files").query(&query)
                    })
                    .await?;
                for file in page.files {
                    if file.kind() == FileKind::Folder {
                        if !seen.insert(file.id.clone()) {
                            continue;
                        }
                        // The chosen folder is in `folders` too; `tree` holds only those under it.
                        if listing.tree.len() == MAX_FOLDERS {
                            listing.truncated = true;
                            break 'folders;
                        }
                        listing.tree.push((file.id.clone(), folder.clone()));
                        listing.folders.push(file.id);
                    } else {
                        if listing.files.len() == MAX_FILES {
                            listing.truncated = true;
                            break 'folders;
                        }
                        listing.files.push(file);
                    }
                }
                match page.next_page_token {
                    // A token seen before in this folder is a cycle.
                    Some(t) if !t.is_empty() => {
                        if !tokens.insert(t.clone()) {
                            return Err(malformed("files.list"));
                        }
                        page_token = Some(t);
                    }
                    _ => break,
                }
            }
        }
        tracing::debug!(
            vendor = VENDOR,
            files = listing.files.len(),
            folders = listing.folders.len(),
            truncated = listing.truncated,
            "folder listed"
        );
        Ok(listing)
    }

    /// The folders the account can open, for the owner's source picker
    /// (`GET /connectors/{id}/available-sources`): every folder not in the bin, the account's
    /// own and those shared with it, shared drives' included, across `nextPageToken`, sorted by
    /// name, at most [`MAX_FOLDERS`].
    pub async fn folders(&self, token: &SecretString) -> Result<Vec<DriveFile>, FarmClientError> {
        let q = format!("mimeType='{FOLDER_MIME}' and trashed=false");
        let fields = format!("nextPageToken,files({FILE_FIELDS})");
        let page_size = self.page_size();
        let mut folders: Vec<DriveFile> = Vec::new();
        let mut page_token: Option<String> = None;
        let mut tokens: HashSet<String> = HashSet::new();
        for _ in 0..MAX_PAGES {
            let page: wire::FileList = self
                .json("files.list", self.config.retry, || {
                    let mut query = vec![
                        ("q", q.as_str()),
                        ("fields", fields.as_str()),
                        ("pageSize", page_size.as_str()),
                        ("supportsAllDrives", "true"),
                        ("includeItemsFromAllDrives", "true"),
                    ];
                    if let Some(t) = &page_token {
                        query.push(("pageToken", t));
                    }
                    self.get(token, "/drive/v3/files").query(&query)
                })
                .await?;
            folders.extend(
                page.files
                    .into_iter()
                    .filter(|f| f.kind() == FileKind::Folder && !f.trashed),
            );
            match page.next_page_token {
                Some(t) if !t.is_empty() && folders.len() < MAX_FOLDERS => {
                    if !tokens.insert(t.clone()) {
                        return Err(malformed("files.list"));
                    }
                    page_token = Some(t);
                }
                _ => {
                    folders.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
                    folders.truncate(MAX_FOLDERS);
                    return Ok(folders);
                }
            }
        }
        Err(malformed("files.list"))
    }

    /// A Google Doc or Sheet converted by `files.export`, cut at [`MAX_TEXT_BYTES`].
    pub async fn export(
        &self,
        token: &SecretString,
        file_id: &str,
        format: ExportFormat,
    ) -> Result<FileText, FarmClientError> {
        let path = format!("/drive/v3/files/{}/export", segment(file_id));
        let response = self
            .send("files.export", self.config.retry, || {
                self.get(token, &path).query(&[("mimeType", format.mime())])
            })
            .await?;
        read_text(response).await
    }

    /// A stored file's content (`alt=media`), for `text/plain` files, cut at
    /// [`MAX_TEXT_BYTES`]. A Google file or a file without content is `Refused` with
    /// `fileNotDownloadable`.
    pub async fn download_text(
        &self,
        token: &SecretString,
        file_id: &str,
    ) -> Result<FileText, FarmClientError> {
        let path = format!("/drive/v3/files/{}", segment(file_id));
        let response = self
            .send("files.get", self.config.retry, || {
                self.get(token, &path)
                    .query(&[("alt", "media"), ("supportsAllDrives", "true")])
            })
            .await?;
        read_text(response).await
    }

    /// One file's or folder's metadata (`files.get` with the listing's fields). A file the token
    /// can't see is `NotFound`.
    pub async fn file(
        &self,
        token: &SecretString,
        file_id: &str,
    ) -> Result<DriveFile, FarmClientError> {
        let path = format!("/drive/v3/files/{}", segment(file_id));
        self.json("files.get", self.config.retry, || {
            self.get(token, &path)
                .query(&[("fields", FILE_FIELDS), ("supportsAllDrives", "true")])
        })
        .await
    }

    /// `changes.getStartPageToken`: the token a first sync saves.
    pub async fn start_page_token(&self, token: &SecretString) -> Result<String, FarmClientError> {
        let start: wire::StartPageToken = self
            .json("changes.getStartPageToken", self.config.retry, || {
                self.get(token, "/drive/v3/changes/startPageToken")
                    .query(&[("supportsAllDrives", "true")])
            })
            .await?;
        Ok(start.start_page_token)
    }

    /// One page of `changes.list` from `page_token`: its changes, and either the next page's
    /// token or, on the last page, `newStartPageToken`. A page with neither is `Malformed`. An
    /// unknown or expired token is `Refused` (`400`, reason `invalid`) or `NotFound`.
    pub async fn changes_page(
        &self,
        token: &SecretString,
        page_token: &str,
    ) -> Result<ChangePage, FarmClientError> {
        let fields = format!(
            "nextPageToken,newStartPageToken,changes(fileId,removed,time,file({FILE_FIELDS}))"
        );
        let page_size = self.page_size();
        let page: wire::ChangeList = self
            .json("changes.list", self.config.retry, || {
                self.get(token, "/drive/v3/changes").query(&[
                    ("pageToken", page_token),
                    ("fields", fields.as_str()),
                    ("pageSize", page_size.as_str()),
                    ("supportsAllDrives", "true"),
                    ("includeItemsFromAllDrives", "true"),
                    ("includeRemoved", "true"),
                ])
            })
            .await?;
        let page = ChangePage {
            changes: page.changes,
            next_page_token: page.next_page_token.filter(|t| !t.is_empty()),
            new_start_page_token: page.new_start_page_token.filter(|t| !t.is_empty()),
        };
        if page.next_page_token.is_none() && page.new_start_page_token.is_none() {
            return Err(malformed("changes.list"));
        }
        Ok(page)
    }

    /// Every change since `page_token`, reading `changes.list` page by page until the page
    /// that carries `newStartPageToken`. Drive doesn't order the pages by time, so the caller
    /// gets them all or an error. The sync reads one page at a time instead
    /// ([`Self::changes_page`]), so a long backlog never has to fit in one call.
    pub async fn changes(
        &self,
        token: &SecretString,
        page_token: &str,
    ) -> Result<ChangeSet, FarmClientError> {
        let mut current = page_token.to_owned();
        let mut changes = Vec::new();
        let mut pages = 0_u32;
        let mut tokens: HashSet<String> = HashSet::from([page_token.to_owned()]);
        loop {
            if pages == MAX_PAGES {
                return Err(malformed("changes.list"));
            }
            let page = self.changes_page(token, &current).await?;
            pages += 1;
            changes.extend(page.changes);
            if let Some(new_start) = page.new_start_page_token {
                tracing::debug!(
                    vendor = VENDOR,
                    changes = changes.len(),
                    pages,
                    "changes read"
                );
                return Ok(ChangeSet {
                    changes,
                    new_start_page_token: new_start,
                    pages,
                });
            }
            match page.next_page_token {
                // A token seen before (a page pointing to itself, or a cycle) would loop for ever.
                Some(next) if !next.is_empty() && tokens.insert(next.clone()) => current = next,
                _ => return Err(malformed("changes.list")),
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Requests
    // -----------------------------------------------------------------------------------------

    fn page_size(&self) -> String {
        self.config.page_size.clamp(1, MAX_PAGE_SIZE).to_string()
    }

    fn get(&self, token: &SecretString, path: &str) -> RequestBuilder {
        self.http
            .get(format!("{}{path}", self.config.api_base))
            .bearer_auth(token.expose_secret())
    }

    fn form_post(&self, url: &str, body: String) -> RequestBuilder {
        self.http
            .post(url)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body)
    }

    /// A call whose success is JSON. A body that doesn't parse is `Malformed`, not retried.
    async fn json<T: DeserializeOwned>(
        &self,
        call: &'static str,
        policy: RetryPolicy,
        build: impl Fn() -> RequestBuilder,
    ) -> Result<T, FarmClientError> {
        let response = self.send(call, policy, build).await?;
        let bytes = response
            .bytes()
            .await
            .map_err(|e| unavailable(Some(TransportError::from_reqwest(e.without_url()))))?;
        serde_json::from_slice(&bytes).map_err(|_| malformed(call))
    }

    /// Sends the request `build` makes until it succeeds, fails for good, or `policy` runs out.
    async fn send(
        &self,
        call: &'static str,
        policy: RetryPolicy,
        build: impl Fn() -> RequestBuilder,
    ) -> Result<Response, FarmClientError> {
        http::retry(policy, |_| {
            let mut request = build();
            if let Some(timeout) = self.request_timeout {
                request = request.timeout(timeout);
            }
            async move {
                let response = match request.send().await {
                    Ok(response) => response,
                    Err(e) => {
                        let error = TransportError::from_reqwest(e.without_url());
                        tracing::debug!(
                            vendor = VENDOR,
                            call,
                            timeout = error.timeout,
                            "google drive call failed"
                        );
                        return Attempt::Retry {
                            error: unavailable(Some(error)),
                            retry_after: None,
                        };
                    }
                };
                let status = response.status();
                if status.is_success() {
                    return Attempt::Done(response);
                }
                let retry_after = http::retry_after(response.headers());
                let body = response.bytes().await.unwrap_or_default();
                classify(call, status, retry_after, &wire::error_codes(&body))
            }
        })
        .await
    }
}
```

**OAuth with PKCE.** `Pkce::generate` makes a verifier and its S256 challenge; the app sends the challenge and `exchange_code` sends the verifier. `refresh` trades a refresh token for an access token, and `revoke` ends the grant on disconnect. The only scope is `drive.readonly`, so the token response has no `id_token`. `about_email` calls `about.get` for the account's email. `invalid_grant` on a refresh is `AuthRefused`.

**Reading.** `list_folder` walks a folder recursively across `nextPageToken`, and stops at 500 files or 500 folders (`MAX_FILES`, `MAX_FOLDERS`); `FolderListing::truncated` says so. Trashed files are left out, and a shared drive's folder works. `export` returns a Doc as text or a Sheet as CSV, and `download_text` returns a plain-text file. Both cut the text at `MAX_TEXT_BYTES` (1 MB), so one large file can't fill memory. `file` reads one file's or folder's metadata (`files.get`). `start_page_token` gives the cursor a first sync saves. `changes_page` reads one page of `changes.list`: its changes and either `nextPageToken` or, on the last page, `newStartPageToken` (a page with neither is `Malformed`; an unknown token is `400` reason `invalid` on the simulator, ran 2026-10-20). `changes` reads every page up to the last; the sync uses `changes_page`. A change with no `fileId` (a shared drive's own, `changeType: "drive"`) parses with an empty `file_id`. `FolderListing::tree` gives each folder found under the chosen one with the folder it was listed in. `DriveConfig::era` points the client at an Era simulator (its `/token` and `/o/oauth2/revoke`), with no back-off between retries; a `Retry-After` up to 60 s is still honoured.

### `DriveSource`: the port for the sync

`DriveSource` wraps a `DriveClient` and implements `FarmSource` (E4.3, [connectors 6](../connectors/README.md#6-sync)). `api` builds one per endpoint: Google with `DriveConfig::default()`, and an Era simulator with `DriveConfig::era`. It never renews a token: the sync does, in `api`.

```rust
/// Files read on one page. A page also stops once it holds [`TEXT_PER_PAGE`] bytes of text.
pub const FILES_PER_PAGE: usize = 25;

/// Failed file reads after which a page stops: each can take the client's retries and
/// timeouts.
pub const FAILURES_PER_PAGE: usize = 2;

/// Runs a file's read may fail in before it is tried only once a day, its record left as it
/// was: about an hour of an outage at one run every 15 minutes.
pub const MAX_TRIES: u8 = 5;

/// The farm-connector adapter for Google Drive: one per process and per endpoint (Google, or
/// an Era simulator).
#[derive(Debug, Clone)]
pub struct DriveSource {
    client: DriveClient,
}
```

- **A listing** (the first sync, a resync): `files.get` on the chosen folder (gone, trashed or not a folder is `SourceMissing`), then `changes.getStartPageToken` before the folder is listed recursively, so a change made during the listing is read by the next sync. Every file found is queued, and the page reports every file id listed in `FarmPage::listed`, so the sync deletes the source's records of files no longer there, after storing the page.
- **A page** reads the next files of the queue: at most `FILES_PER_PAGE` (25), and it stops early once it holds 8 MB of text. A Doc is exported as text, a Sheet as CSV (its first tab), a `text/plain` file downloaded, anything else kept by name with no text. A file Drive won't hand over (`Refused`, such as `fileNotDownloadable`), and a `text/plain` file whose characters are more than a tenth U+FFFD or NUL (a binary file), are kept by name; one gone since it was queued (`404`) is reported in `FarmPage::removed`. A refused token or a `429` fails the page, so the sync keeps its cursor. `more` is set while the queue has files or a backlog of changes goes on.
- **A file whose read fails** another way (Drive down for it, a timeout, an answer that doesn't parse) goes to the back of the queue with its tries counted and the run's start (`FarmPageRequest::oldest` plus 180 days) noted, and the page goes on; it stops after `FAILURES_PER_PAGE` (2) such failures. A file is tried once a run; after `MAX_TRIES` (5) runs it is tried once a day (`RETRY_AFTER_TRIES`), never dropped, and its record stays as it was. Once only files waiting for a later run are left, the page reads `changes.list`, so they hold up nothing. A change token is refused only on `400` with reason `invalid`, or `404`.
- **Later syncs**, when the queue is empty, read one page of `changes.list` from the saved token (`changes_page`) and save its `nextPageToken`, or `newStartPageToken` on the last page, so a long backlog spreads over pages and runs and a `429` loses at most one page. Each page's changes are applied to the cursor (folders first, then files) and the files to read again are queued. A token Drive refuses (`400`) or doesn't know (`404`) starts a resync: the listing above.
- **The cursor** is the adapter's JSON in `SyncCursor`: `token`, where `changes.list` continues; `paging`, a backlog in progress; `folders`, every folder under the chosen one with the folder it is in; `files`, every file held with its folder; `queue`, the files still to read with their name, type, `modifiedTime`, `webViewLink` and failed tries. A cursor it can't read starts a listing.
- **Under the folder.** Drive gives a file's `parents`, not its path, and moving a folder changes only that folder. A changed folder is placed under its new parent, or dropped when trashed or deleted, and the tree keeps the folders whose parents lead to the chosen folder. A folder that left takes its subfolders and their files with it; a folder that joined is listed recursively, since its files bring no change of their own. A file change is kept when one of its parents is the chosen folder or a folder of the tree; trashed, deleted or moved elsewhere, the file is dropped. Only files held before are reported in `removed`, so the thousands of changes elsewhere in an account cost only the reading. The latest change of a file wins; a change with no file id, or with no file that isn't a removal, is skipped. A listed file whose parent isn't a folder of the tree is left out.
- **Limits.** Past 500 files or 500 folders under the chosen folder (`MAX_FILES`, `MAX_FOLDERS`; a truncated listing, a change or a folder moved in), the page fails with `FarmSourceError::TooLarge`, stored as `too_large` like the garden's 10,000 records: nothing more runs until the owner chooses a smaller folder.
- **The chosen folder** trashed or deleted is `SourceMissing`; any other `404` (a file, a page token) is a bad answer, `vendor_unavailable`.
- **No re-read.** `reread_window()` is `None`: `changes.list` reports what is gone, so the sync never deletes a Drive record by absence.

**Errors** come in Google's shape, `{"error":{"code","status","errors":[{"reason"}]}}`:

| Answer | Result |
|---|---|
| `401` | `AuthRefused` |
| `429`; `403` with reason `userRateLimitExceeded` or `rateLimitExceeded` | retried, honouring `Retry-After`, then `RateLimited` |
| `403` for a missing scope (`ACCESS_TOKEN_SCOPE_INSUFFICIENT`, or `insufficientPermissions` on a call that reads with the token's scope) | `AuthRefused`, never retried |
| any other `403` (such as `fileNotDownloadable`) | `Refused`, never retried |
| `404` | `NotFound` |
| `408`, 5xx, transport failure | retried, then `Unavailable` |

**No vendor body is kept.** Neither client keeps a response body in an error, a log line or a stored value, because a body may echo the farm's own records ([connectors 8](../connectors/README.md#8-privacy-and-security)). A log line has the method, the status and the short code (Slack's `error`, Google's `reason`). The text that `export` and `download_text` return goes to the caller and nowhere else.

---

## 16. Concurrency and lifetimes

**What is shared.** Every adapter is `Send + Sync + 'static` and is held by `api` as `Arc<dyn Port>`. The clients are cheap to clone: an `Arc` around their configuration plus the shared `reqwest::Client`. State that must be one per process is behind an `Arc` inside:

| State | Where | Why one |
|---|---|---|
| JWKS keys, last fetch | `ClerkVerifier` (moka cache, tokio mutex) | The refetch limit is per process |
| SoilGrids pacer | `SoilGrids` | 5 calls a minute is per client IP |
| Nominatim pacer, geocode cache | `CompositeGeocoder` | 1 request a second; cached answers |
| Pending push tickets | `ExpoPush` (std mutex, never held across `.await`) | Receipts are checked by one scheduler |

The std mutex in `ExpoPush` is locked only for pushes onto and filters of a `Vec`, never across an `.await`, as the tokio docs advise for short critical sections ([tokio `Mutex`](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Mutex.html#which-kind-of-mutex-should-you-use)). The pacers and the JWKS refetch use tokio mutexes, because they are held across a sleep or a request.

**Lifetimes, in one place.**

| Name | Where | What it is |
|---|---|---|
| `'a` | `MessagesRequest<'a>`, `ContentBlock<'a>`, `Tool<'a>`, `Base64<'a>`, `ChatCompletionRequest<'a>`, `DataUrl<'a>` | The caller's prompt, conversation, schemas and image bytes, borrowed until the body is serialised |
| `'r` | `Body<'r, 'a>` (both LLM clients) | The borrow of the request while serialising, shorter than `'a` |
| `'_` (elided) | `Message::replay(&self) -> Vec<ContentBlock<'_>>`, `ResponseBlock::replay` | Blocks that borrow the response they came from |
| `'m` | `ToolUse<'m>`, `ExpoMessage<'m>` | A view into one message: the response's tool call, or the domain push message |
| `'b` | `ClerkEvent<'b>` | The webhook body the event's `type` borrows from |
| `'a` | `GeocodeQuery<'a>`, `WebhookHeaders<'a>` (in `domain`) | The handler's request: strings borrowed for one call |
| `'static` | `MessageStream`, every response type | Owned: crosses tasks, channels and SSE bodies |

The pattern is the brief's convention 4: borrowed views for the synchronous work of building a body, owned data for anything that outlives the call. The one async boundary where both meet is `AnthropicClient::stream`. Its future borrows the request until it is first polled, which is when serialising happens, and the stream it returns borrows nothing.

**Generics, in one place.** `retry<T, E, F, Fut>` (section 4): static dispatch, so no allocation per attempt and no `'static` bound on the attempt's future. `sse::events<S, E>`: any chunk stream, so tests feed it by hand. `ExpoPush::post<B, R>` and `CompositeGeocoder::get_json<T>`: serialise any body and decode any response type, with `R: DeserializeOwned` because the bytes are a local buffer that the result can't borrow from ([serde, `DeserializeOwned`](https://serde.rs/lifetimes.html#trait-bounds)). Ports are trait objects; no adapter is generic over another.

**Cancellation.** Every adapter tolerates being dropped at any `.await`. A dropped `create` or `stream` future closes its connection. A dropped `MessageStream` closes the response. When the app disconnects, `api` drops the turn's stream, and the upstream call stops (research note, `http-s3` probe).

---

## 17. Errors

Each module maps its failures to the port's error enum ([domain 9](domain.md#9-boundary-traits)). The library error is kept as the `source`, so `api`'s log prints the whole chain. Only `api` turns these into HTTP ([contracts 2](../architecture/contracts.md#2-errors)):

| Port error | HTTP in `api` |
|---|---|
| `ProviderError::NotFound` | depends on the caller: `400 invalid_request` on the location field, `soil_profile_status: unavailable` for soil |
| `ProviderError::{Unavailable, RateLimited, BadResponse}` | `503 upstream_unavailable` with `upstream = provider()` where the request needs the provider; otherwise logged |
| `AuthError::InvalidToken` | `401 unauthenticated` |
| `AuthError::InvalidWebhook` | `401 unauthenticated` |
| `AuthError::Unavailable` | `503 upstream_unavailable` |
| `ModelError::Unavailable` (Anthropic) | `503 upstream_unavailable` for chat; the fallback job fails with `retryable: true` |
| `ModelError::{Failed, InvalidOutput}` | the job fails with `model_error`, `retryable: true` ([contracts 5](../architecture/contracts.md#5-worker-api-and-jobs)) |

The two LLM clients keep their own enums (`AnthropicError`, `OpenAiCompatError`), because their callers need `retryable()`, `retry_after()` and the status before they decide. Each enum has `into_model_error(model: &'static str)`, and the caller passes the model name it knows. Keys never appear in an error or a log line: `SecretString` is exposed only at the header it fills.

---

## 18. Testing

| What | How |
|---|---|
| Wire shapes of every request | serialised and compared with the documented JSON (`serde_json::json!`) |
| SSE parsing | byte-at-a-time chunks, CRLF, comments, multi-line `data:` |
| Anthropic streaming | a documented stream with thinking, text, a split tool call, a ping, an unknown event and cumulative usage, folded into a `Message`; an `error` event after 200 through the client |
| Anthropic retries | `wiremock`: a 529 then a 200 (retried), a spend-cap 429 (exactly one request), a 429 with `retry-after` |
| Schema sanitiser | bounds stripped at every depth, a property named `maximum` kept, objects closed, `Cow::Borrowed` when clean |
| OpenAI-compatible | body per flavour; `finish_reason: "length"` surfaced; a 503 classified retryable |
| Open-Meteo | the **live fixture** of 2026-10-20 (Toronto values above, Ottawa frost on 28 October); query lists via `wiremock`; object versus array; 429 and 400 mapping; `fetch_cells` with `MockWeatherProvider` |
| SoilGrids | `d_factor` conversion on the live Iowa fixture (six depths); the live Toronto and Guelph null fixtures and the null probe; central Toronto as answered live, without SLC (the point, four null probes at 2 km, the probe and profile 5 km north: 7 calls); with SLC, a null Toronto point answered by Chinguacousy after one call; outside SLC (Iowa) the probes still run to `NotFound`; an outage doesn't fall back; the nearby-point fallback and `NotFound` with `wiremock` |
| SLC | the embedded file loads (1,265 soils, 126,760 runs); central Toronto and Scarborough are Chinguacousy, with a band straddling a layer boundary averaged by overlap (15–30 cm pH 7.44) and 100–200 cm empty; North York, Guelph and Ottawa have a soil; Iowa, Lake Ontario and points off the grid don't; a short or foreign file is refused. In `api`: `fetch_soil_profile` with the wired adapter stores a Toronto garden as `ready` |
| Geocoder | the live Open-Meteo, Nominatim and Zippopotam fixtures of 2026-10-20; the 0.01° reverse lookup; the reverse shapes (a city, a county only, `{"error"}`); the cache (one request for two queries, also for GPS); a failed reverse lookup (503, 429, `{"error"}`, an unparsable body) labelled "Toronto time zone" for an Ottawa point and not cached; `zone_label`; `tzf-rs` on Toronto and Vancouver |
| Clerk | tokens signed with a throwaway RSA key, generated once per test run with `aws-lc-rs` so no private key is committed, and served as a JWKS by `wiremock`: issuer, expiry, `nbf`, `azp`, leeway, missing `nbf`, refetch limit; `DELETE` 200, 404 and 429 |
| Svix | the `svix` crate's `Webhook::sign` as the oracle; rotation (several entries), tampering, ±301 s |
| Agent Memory | `wiremock`, exact request bodies (the service ignores unknown fields silently): `record` and the scope cache (one registration for two writes, none after the first), the `400 unknown scope node` re-register and single retry, a 5xx write retried twice, `current` (plant and garden merged, superseded dropped, label and both timestamp forms, 404 as empty), `history` (cursor followed through a short page, newest first), `search` (exact lens, hit filter, prefix stripped), `forget` (trailing slash, cache dropped), the 500, 429, 401, 403 and 422 mapping, `Debug` without the key |
| Slack, Google Drive | `wiremock`, exact requests: `ok:false` at 200 for each code group, `Retry-After`, 5xx retries, the pacer per (team, method) and its bounded cache, paging by cursor, `users.info` without the email, `permalink`; Drive's OAuth with PKCE, `about.get`, the recursive listing and its cap of 500, the 1 MB cut, `changes` to the last page, the `403` reasons, a refused scope as `auth_refused`; no body in any error or log. `DriveSource`: the folder checked, then the start token before the listing, nested folders and every id listed; a chosen folder gone, trashed or not a folder as `source_missing`; 25 files a page from the cursor; 501 files as `too_large`, listed or by a change; changes kept under the folder (edited, moved out, trashed, deleted, created in a subfolder, elsewhere, a shared drive's change without a file id); a backlog read one page at a time; a refused or unknown change token, and an unreadable cursor, listing again; a folder moved out taking its file and one moved in listed with its subfolder; the chosen folder trashed as `source_missing`; a change with no file that isn't a removal ignored; a listed file outside the tree left out; the 1 MB cut, a three-byte character included; a locked file and a binary `text/plain` kept by name and a gone one removed; a file whose read keeps failing tried once a run and skipped after 5; a `429` with its `Retry-After` |
| Era | see below |
| Expo | ticket order, `DeviceNotRegistered` in tickets and receipts, batches of 100, a failed whole push |

**The Era tests** (`tests/era_fixture.rs`, `era_slack.rs`, `era_drive.rs`) run the two clients against Era's Slack and Google Drive simulators on the Maple Row Farm fixture. They are `#[ignore]`d, so `cargo nextest run` skips them. Start the simulators and run them with:

```sh
docker compose -f infra/dev.compose.yml --profile era up -d --wait era-slack era-drive
ERA_SLACK_BASE=http://127.0.0.1:8091 ERA_GOOGLE_BASE=http://127.0.0.1:8092 \
  cargo nextest run -p integrations --run-ignored only \
  -E 'test(/(^|::)era_/) and not test(/(^|::)era_strict_/) and not test(/(^|::)era_hosted_/)'
```

- **They fail, they don't skip.** `era_sim::base_url` panics when `ERA_SLACK_BASE` or `ERA_GOOGLE_BASE` is unset, so a run without the simulators is red.
- **High fidelity.** Start each simulator with `-e SIMCORE_FIDELITY=high` as its own `-e`; a missing or joined setting gives `legacy`, a far smaller company. `era_drive_reads_the_farm_folder_and_its_changes` checks `era_sim::fidelity` first: at legacy fidelity a quarter of changes from `day1` to `day2` is 52 changes, one page, where high fidelity gives 2,041 in three pages of 1,000 (ran 2026-10-20). That was why the test failed locally with "1 pages" while passing in CI.
- **Minted tenant credentials.** Each test creates a `Tenant`, an isolated copy of the company, and passes the credential the simulator mints for it as the client's bearer token. The credential selects the tenant: no `X-Sim-Tenant` header and no test-only constructor exist.
- **One at a time.** Faults are global to a simulator, so a fault set by one test reaches every other test. [`backend/.config/nextest.toml`](../../backend/.config/nextest.toml) puts every test named `era_*` in the nextest group `era` (`max-threads = 1`). The fault tests also hold a file lock and clear the faults when they end, also when they fail. The `era` CI job comes with [EON E6](../EON/README.md#e6-ci).
- **The hosted company** (`tests/era_hosted.rs`, EON T-E7.1). Two `era_hosted_` tests run the clients against Holling Holdings, the team's hosted Era company, with `SLACK_BASE_URL`, `GOOGLE_DRIVE_BASE_URL` and `ERA_TENANT_TOKEN`: Slack lists the channels and reads one history page, Drive lists the folders, one folder and exports a Doc. Every request is metered, so they make five in all and print the count, read from an HTTP client that keeps no idle connection and counts DNS lookups. Only the nightly `era-hosted` workflow runs them ([infra 2](../infra/README.md#the-era-hosted-job)).

Paused time (`start_paused`) is used only where no real I/O runs: the retry and pacer tests. With a mock server in the loop, tokio's auto-advance would jump to the request's timeout while the server answers, which the Clerk refetch test showed. Not run: live calls to Anthropic, Clerk and Expo (no keys or accounts on 2026-10-15), and the VLM servers (no GPU here). Nominatim was called live on 2026-10-20, 9 requests 2 s apart with the project's user agent, to record its fixtures; SoilGrids 17 times, 15 s apart.

---

## 19. Open questions

1. **Frost threshold.** Settled in [domain 7](domain.md#weather-thresholds): one `WeatherThresholds` for `frost_risk`, the frost alert rule of [architecture 8.3](../architecture/README.md#83-forecast-alerts) and fusion, with frost at a 2 m minimum ≤ 0.5 °C. The number itself is still a proposal ([domain 12](domain.md#12-open-questions), item 17).
2. **SoilGrids `NotFound` versus `Unavailable`.** Settled in contracts v0.10 ([contracts 9, the soil profile](../architecture/contracts.md#gardens)): one `unavailable` value on the wire, retried after 7 days for no data and after 15 minutes for an outage (section 8; `api`'s `soil` task). The Soil Landscapes of Canada fallback for null pixels is built (section 8) and *proposed* in architecture open decision 10, for the team to confirm. Still open: whether a profile should say which source it came from (a `source` field would be a contract change; the plan writer names its sources, architecture 6.4).
3. **Postal-code geocoding** relies on Nominatim (policy-bound, 1 request a second) and Zippopotam (terms unchecked). [Contracts 9](../architecture/contracts.md#gardens) says "geocoded once" and names no service, and [contracts 2](../architecture/contracts.md#2-errors) has no code for a location that can't be geocoded (`400 invalid_request` with the field is used). Confirm Zippopotam's terms, or drop it and refuse a bare forward sortation area.
4. **Reverse-geocoding failure for GPS.** Settled: a failed reverse lookup labels the garden "<zone city> time zone" and creation goes ahead (section 9; contracts 9, open question 37). Still open: relabelling such a garden when Nominatim answers again, which needs a stored flag and a scheduler task; until then a location change relabels it.
5. **Open-Meteo licence.** The free API is non-commercial. Settle it before any public launch ([architecture 15](../architecture/README.md#15-open-decisions)).
6. **The hosted LLM's box format and the stripped keywords.** Settled by a live run on 2026-10-23 (task P9, [report](../../ml/training/eval/reports/p9-fallback-2026-10-23.md); [contracts 12](../architecture/contracts.md#12-open-questions), item 26): the model answers in pixels of the image as sent (`abs_px_prepared`), and every call with the stripped schemas and images was accepted.
7. **Late `user.created` after `user.deleted`.** Settled in `api`: `user.created` changes nothing, and the account tombstone refuses a deleted account's tokens (section 11).
8. **Jev's API.** Settled on 2026-10-23: the client follows TypeSafe's published System One API and ran against it (section 13).

---

## 20. Sources

**Rust language**
- The Book: [10.1 generics](https://doc.rust-lang.org/book/ch10-01-syntax.html) · [10.3 lifetimes](https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html) · [18.2 trait objects](https://doc.rust-lang.org/book/ch18-02-trait-objects.html) · [17 async and await](https://doc.rust-lang.org/book/ch17-00-async-await.html)
- The Reference: [subtyping and variance](https://doc.rust-lang.org/reference/subtyping.html) · [dyn compatibility](https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility)
- [Edition Guide, RPIT lifetime capture](https://doc.rust-lang.org/edition-guide/rust-2024/rpit-lifetime-capture.html) · [std `Cow`](https://doc.rust-lang.org/std/borrow/enum.Cow.html) · [serde, lifetimes and `DeserializeOwned`](https://serde.rs/lifetimes.html)
- [tokio `Mutex`: which kind to use](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Mutex.html#which-kind-of-mutex-should-you-use) · [tokio `time::pause`](https://docs.rs/tokio/1.53.3/tokio/time/fn.pause.html)

**Crates at the pinned versions** (APIs checked against their source in `~/.cargo/registry`)
- [reqwest 0.13.5](https://docs.rs/reqwest/0.13.5/reqwest/) and its [features](https://docs.rs/crate/reqwest/0.13.5/features) · [jsonwebtoken 11.1.0](https://docs.rs/jsonwebtoken/11.1.0/jsonwebtoken/) · [moka 0.12.16](https://docs.rs/moka/0.12.16/moka/) · [secrecy 0.10.3](https://docs.rs/secrecy/0.10.3/secrecy/)
- [aws-lc-rs 1.18.2 `hmac`](https://docs.rs/aws-lc-rs/1.18.2/aws_lc_rs/hmac/index.html) and [`constant_time`](https://docs.rs/aws-lc-rs/1.18.2/aws_lc_rs/constant_time/index.html) · [data-encoding 2.11.1](https://docs.rs/data-encoding/2.11.1/data_encoding/) · [tzf-rs 2.1.3](https://docs.rs/tzf-rs/2.1.3/tzf_rs/) · [rand 0.10.3](https://docs.rs/rand/0.10.3/rand/)
- [svix 2.8.0 `Webhook`](https://docs.rs/svix/2.8.0/svix/webhooks/struct.Webhook.html) · [wiremock 0.6.5](https://docs.rs/wiremock/0.6.5/wiremock/)

**Upstreams** (the facts come from the research notes, which read these pages and probed the live APIs on 2026-10-15)
- Anthropic: [streaming](https://platform.claude.com/docs/en/build-with-claude/streaming) · [structured outputs](https://platform.claude.com/docs/en/build-with-claude/structured-outputs) · [vision](https://platform.claude.com/docs/en/build-with-claude/vision) · [errors](https://platform.claude.com/docs/en/api/errors) · [rate limits](https://platform.claude.com/docs/en/api/rate-limits) · [models](https://platform.claude.com/docs/en/models/overview)
- vLLM: [structured outputs](https://docs.vllm.ai/en/stable/features/structured_outputs.html) · [docs](https://docs.vllm.ai/en/stable/) · llama.cpp [server README](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md)
- Open-Meteo: [forecast](https://open-meteo.com/en/docs) · [geocoding](https://open-meteo.com/en/docs/geocoding-api) · [terms](https://open-meteo.com/en/terms) · [pricing](https://open-meteo.com/en/pricing)
- ISRIC SoilGrids: [REST docs](https://rest.isric.org/soilgrids/v2.0/docs) · [data policy](https://isric.org/about/data-policy)
- Soil Landscapes of Canada v3.2: [dataset and downloads](https://open.canada.ca/data/en/dataset/5ad5e20c-f2bb-497d-a2a2-440eec6e10cd) · [AAFC's page](https://sis.agr.gc.ca/cansis/nsdb/slc/v3.2/index.html) · [soil layer table](https://sis.agr.gc.ca/cansis/nsdb/soil/v2/slt/index.html) ([`ORGCARB`](https://sis.agr.gc.ca/cansis/nsdb/soil/v2/slt/orgcarb.html), [`CEC`](https://sis.agr.gc.ca/cansis/nsdb/soil/v2/slt/cec.html), [`BD`](https://sis.agr.gc.ca/cansis/nsdb/soil/v2/slt/bd.html), [`PH2`](https://sis.agr.gc.ca/cansis/nsdb/soil/v2/slt/ph2.html)) · [Open Government Licence – Canada 2.0](https://open.canada.ca/en/open-government-licence-canada), all read 2026-10-20
- Nominatim [usage policy](https://operations.osmfoundation.org/policies/nominatim/) · Zippopotam [API](https://api.zippopotam.us)
- Clerk: [session tokens](https://clerk.com/docs/guides/sessions/session-tokens) · [manual JWT verification](https://clerk.com/docs/guides/sessions/manual-jwt-verification) · [webhooks](https://clerk.com/docs/guides/development/webhooks/overview) · [Backend API](https://clerk.com/docs/reference/backend-api) · [system limits](https://clerk.com/docs/guides/how-clerk-works/system-limits)
- Svix: [manual verification](https://docs.svix.com/receiving/verifying-payloads/how-manual)
- Expo: [sending notifications](https://docs.expo.dev/push-notifications/sending-notifications/)
- TypeSafe (Jev), read on 2026-10-23: [System One API](https://docs.typesafe.ai/api/system-one.md) · [models](https://docs.typesafe.ai/models.md) · [noul](https://docs.typesafe.ai/primitives/noul.md) · [errors and rate limits](https://docs.typesafe.ai/api/errors.md)

**Project documents and skills**
- [Data contracts](../architecture/contracts.md) · [architecture](../architecture/README.md) · [frontend](../frontend/README.md) · [backend architecture](README.md) · [domain](domain.md) · [domain-types](domain-types.md)
- Skills in the main checkout's `.claude/skills/`: `rust` (with `versions.md`), `rust-async`, `rust-errors`, `rust-tracing`, `rust-testing`

The upstream links were read on 2026-10-15 by the research notes this crate is built from. The crate APIs were checked against their source at the pinned versions in `~/.cargo/registry` and by compiling; the links above were not reopened while writing this document.
