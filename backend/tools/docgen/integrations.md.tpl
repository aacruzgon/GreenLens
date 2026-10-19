# PDM Backend: `integrations` crate

> **Status:** Draft v0.6, 2026-10-19. v0.6: adds `farm`, `slack` and `google_drive`, the standalone clients for the farm connectors, and `era_sim`, the test support for Era's simulators (section 15, which moves the sections after it down one); the Era tests run in one serial nextest group (section 18). v0.5: adds `agent_memory`, the `BeliefStore` over `SurrealDB` Agent Memory's REST API (section 14; the sections after it moved down one), 13 tests; 77 in all. v0.4: checked against Anthropic's documentation and the first live calls: the sanitiser turns `oneOf` into `enum` or `anyOf` (refused live), turns a nullable enum into one `enum` holding `null` so it isn't a union-typed parameter, and drops `format`s the API doesn't list; token counts that are `null` read as 0 and a missing `usage` defaults; `StopReason::ModelContextWindowExceeded`; 64 tests. v0.3.3: the Clerk tests generate their RSA key pair at run time, so no key file is committed, and the Svix tests use a made-up secret. v0.3.2: section 4 says the worker builds its client with a 600 s read timeout, not the 60 s default, because its VLM and hosted LLM calls aren't streamed, and their 240 s and 300 s deadlines end a slow call; the skills table became a plain list of the extra dependencies. v0.3.1: a JWKS outage is `upstream: "auth"`, as `api` answers it. v0.3: the summary adds `past_7_days.longest_humid_run_days` (the longest run of humid days among the 7 before today, from the hourly history), and the failure tables follow the contracts: a weather failure fails a scan's run with `upstream_unavailable`, `user.created` changes nothing, and `SoilGrids` is retried 15 minutes after an outage and 7 days after no data. v0.2: `frost_risk` and `hours_rh_over_90` come from the shared `domain::rules::weather::WeatherThresholds` (`OpenMeteoConfig::thresholds`): frost is now a 2 m minimum ≤ 0.5 °C, the value `api`'s frost trigger used. v0.1: every outbound HTTP adapter (the Anthropic Messages client, the OpenAI-compatible VLM client, Open-Meteo, SoilGrids, the geocoders, Clerk, Svix, Expo Push and Jev), compiled on Rust 1.99.1 and tested against live fixtures and mock servers.

Companion to the [backend architecture](README.md) and the [`domain` crate](domain.md), whose ports this crate implements. Every listing below is copied from the compiled source of `crates/integrations`. `cargo check`, `cargo clippy -p integrations --all-targets` and `cargo test -p integrations` all pass: 77 tests, against `wiremock` servers and the live responses recorded on 2026-10-15.

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
| Open-Meteo geocoding, Nominatim, Zippopotam, `tzf-rs` | `geocode` | `Geocoder` | `api` (`POST /gardens`, `PATCH /gardens/{id}`) |
| Clerk JWKS and Backend API | `clerk` | `AuthVerifier`, `AuthAdmin` | `api` (every request, `DELETE /me`) |
| Svix | `svix` | `WebhookVerifier` | `api` (`POST /webhooks/auth`) |
| Expo Push | `expo_push` | `Notifier` | `api` |
| Jev | `jev` | `TextClassifier` | `api`, when configured |
| `SurrealDB` Agent Memory | `agent_memory` | `BeliefStore` | `api` (memory mirror, beliefs endpoint, `search_memory`), when configured |
| Slack Web API | `slack` | a typed client (E4's sync builds the `FarmSource` port on it) | `api`, the `connectors` task (E4) |
| Google Drive API and OAuth | `google_drive` | a typed client (E4's sync builds the `FarmSource` port on it) | `api`, the `connectors` task (E4) |

**What it never does.** It reads no database and no object store, and it holds no business rule. A failed fetch is reported, and the caller decides what to do: skip the gardens in a weather cell, set `soil_profile_status`, answer `503 upstream_unavailable`. It never reads the system clock for domain time: adapters that stamp `fetched_at` take an `Arc<dyn Clock>`, and token checks take `now` as an argument. No `reqwest` type appears in a public signature. Errors leave as the port's enum ([domain 9](domain.md#9-boundary-traits)), or as the client's own enum for the two LLM clients. The library error is kept as the `source`.

**Dependencies beyond the common pins.** `reqwest` adds the `stream` feature (`bytes_stream()` for SSE) and `query` (`RequestBuilder::query` needs it in 0.13 ✓) to `json`. The webhook's HMAC and constant-time comparison use `aws-lc-rs` (`hmac`, `constant_time`), already in the tree. Tests run real HTTP through the real `reqwest` client against `wiremock` 0.6.5 (dev only), headers, query strings and SSE bodies included. The project's Rust skills follow this crate, the crate list included.

---

## 2. Dependencies

@@file ../Cargo.toml@@

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
├─ fixtures/                   live responses recorded on 2026-10-15
│  ├─ openmeteo_forecast_multi.json   Toronto and Ottawa, past_days=7, forecast_days=16
│  ├─ geocoding_toronto.json
│  └─ soilgrids_{iowa,guelph,toronto}.json   one with data, two all-null urban pixels
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
   ├─ geocode.rs               CompositeGeocoder: Geocoder; TimeZoneFinder, PostalCode, round_for
   ├─ geocode/tests.rs
   ├─ clerk.rs                 ClerkVerifier: AuthVerifier; ClerkAdmin: AuthAdmin
   ├─ svix.rs                  SvixVerifier: WebhookVerifier; map_clerk_event
   ├─ agent_memory.rs          AgentMemory: BeliefStore (REST: scopes, facts, entities, query, forget)
   ├─ farm.rs                  FarmClientError, FarmErrorCode: what both farm clients share
   ├─ slack.rs                 SlackClient, SlackConfig, permalink
   ├─ slack/
   │  ├─ wire.rs               Slack's response shapes (private)
   │  └─ tests.rs              wiremock tests
   ├─ google_drive.rs          DriveClient, DriveConfig, ExportFormat; the byte and count caps
   ├─ google_drive/
   │  ├─ wire.rs               Drive's and Google's response shapes
   │  ├─ pkce.rs               Pkce: verifier and S256 challenge
   │  └─ tests.rs              wiremock tests
   ├─ era_sim.rs               feature `era-sim`, test support only: Tenant, Faults, fixture()
   ├─ expo_push.rs             ExpoPush: Notifier (push, check_receipts)
   └─ jev.rs                   JevClassifier: TextClassifier (stub)
```

`tests/` holds the Era tests (`era_fixture.rs`, `era_slack.rs`, `era_drive.rs`; section 18). `era_sim` is compiled only with the `era-sim` feature, which the crate enables for its own tests, so no release build contains it.

---

## 4. Shared HTTP: client, retries, pacing, SSE

**One client.** `reqwest::Client` is an `Arc` inside, so `api` builds one at start and clones it into every adapter, and the worker does the same. The client has a 5 s connect timeout and, by default, a 60 s *read* timeout. The read timeout is the longest gap between two reads, restarts on every chunk, and also covers the wait for the response headers. The worker builds its client with a 600 s read timeout instead, because its VLM and hosted LLM calls aren't streamed: the server sends nothing until the answer is complete ([worker 1](worker.md#1-purpose-and-boundaries)). The client has no total timeout, because an LLM stream can last minutes. Each non-streaming call sets its own deadline with `RequestBuilder::timeout`; in the worker that is 240 s for a VLM call (`OpenAiCompatConfig.timeout`) and 300 s for a hosted LLM call (`AnthropicConfig.request_timeout`), both under the 600 s read timeout, so the deadline is what ends a slow call.

@@items http.rs USER_AGENT HttpConfig client@@

**Retries.** `retry` runs one call until it succeeds, fails for good, or runs out of retries. Each attempt returns an `Attempt`, so the caller decides per status what is retryable, and the helper decides only how long to wait. The wait is "full jitter": a uniform random duration up to `min(cap, base · 2^n)`. A server's `Retry-After` sets the minimum wait. A `Retry-After` longer than `max_retry_after` gives up instead.

@@items http.rs RetryPolicy@@

@@impl http.rs impl RetryPolicy@@

@@items http.rs Attempt retry@@

`retry` is generic rather than boxed. `F` is the closure that makes one attempt, `Fut` is the future it returns, and `T` and `E` are the call's types. Each caller gets its own monomorphised copy, so nothing is allocated per attempt. `Fut` is not required to be `'static`, so an attempt can borrow the caller's request body and client ([Book 10.1, generics](https://doc.rust-lang.org/book/ch10-01-syntax.html)). Waits use `tokio::time::sleep`, so tests pause time and advance it.

**Pacing.** Nominatim allows 1 request a second and SoilGrids 5 a minute. A `Pacer` spaces calls to one upstream. Callers queue on a tokio mutex, which is fair and serves them first come, first served, so concurrent garden creations line up instead of bursting.

@@items http.rs Pacer@@

@@items http.rs retry_after retryable_status TransportError@@

`TransportError` is how a failed connection crosses a public boundary without the `reqwest::Error` type: two flags and a boxed source.

**Server-sent events.** Both LLM upstreams stream SSE. The parser buffers bytes until a whole line arrives, so a chunk may end anywhere, even inside a UTF-8 sequence. It accepts CRLF and LF line ends and skips `:` comments. A test feeds a stream one byte at a time.

@@items sse.rs SseEvent SseParser events@@

`events` is generic over the chunk stream `S` and its error `E`, so tests feed it hand-made chunks. `stream::unfold` moves the byte stream and the parser into its state, so the output borrows nothing: it is `'static` whenever `S` is, and `Response::bytes_stream()` is. Under edition 2024 the `impl Stream` return type would capture any lifetime in scope ([Edition Guide, RPIT capture](https://doc.rust-lang.org/edition-guide/rust-2024/rpit-lifetime-capture.html)), but the signature has no lifetime to capture, so no `use<..>` bound is needed.

---

## 5. Anthropic Messages API

**Endpoint.** `POST {base_url}/v1/messages`, with headers `x-api-key`, `anthropic-version: 2023-06-01`, `content-type: application/json`, and `anthropic-beta` only when configured. Anthropic publishes no Rust SDK, so the client is raw HTTP over serde types (research notes, Anthropic section; *unverified* live, because no key was available on 2026-10-15). The types serialise to the documented JSON, and the tests check that.

**Who calls it.** `agent` implements `ChatModel` on `stream` for chat turns and on `create` for the plan writer, memory extraction and its LLM classifier. The fallback worker's vision adapter calls `create` with image blocks and `output_config.format`. The worker uses the raw client rather than `ChatModel` because it must see `stop_reason` (`refusal`, `max_tokens`).

### Borrowed requests, owned responses

`MessagesRequest<'a>` borrows everything a call needs. **`'a` is the caller's data:** the system prompt, the conversation, the tool schemas and the image bytes. The client serialises the request once into an owned `Bytes` body before its first `.await`, so it never holds the request across the network. Text fields are `Cow<'a, str>` and schemas are `Cow<'a, Value>`. A caller with an owned value moves it in as `Cow::Owned`, for example a sanitised schema or a formatted prompt. A caller with a borrowed one pays nothing ([std `Cow`](https://doc.rust-lang.org/std/borrow/enum.Cow.html)). In `agent`, `ChatModel::stream<'a>(&'a self, request: &'a ChatRequest)` builds a `MessagesRequest<'a>` that borrows the domain request's strings and image `Bytes`.

@@items anthropic/request.rs MessagesRequest System TextBlock CacheControl InputMessage ContentBlock ToolResultContent ImageSource Base64@@

`Base64<'a>` holds raw image bytes and serialises them as base64 through `Serializer::collect_str`, which takes `data_encoding`'s `Display` adapter. `serde_json` writes the text straight into the body, so a 1 MB image becomes about 1.3 MB of body once, and is never also built as a `String`.

@@items anthropic/request.rs Tool ToolChoice Thinking OutputConfig Effort OutputFormat Body@@

**`Body<'r, 'a>` has two lifetimes** because two different borrows meet. `'r` borrows the request for the moment of serialising. `'a` is what the request borrows, and it lives as long as the caller's context. One lifetime would force the request's borrow to last as long as its data, which an `&'r MessagesRequest<'a>` created inside `create` cannot do. `#[serde(flatten)]` adds `stream` beside the request's fields.

**Model rules that shape the request** (research notes; *unverified* live). Opus 5.5 always thinks adaptively: send no `thinking` (or `adaptive`), never `disabled` or `budget_tokens`. Forced `tool_choice` (`any`, `tool`) is a 400 on Opus 5.5 and Sonnet 5.5: send `auto` or nothing. Assistant prefill is a 400. Non-default `temperature` and `top_p` are rejected, so the request type has neither. Thinking blocks are bound to the conversation that produced them ("preserved thinking"), so a tool loop replays them unchanged with `Message::replay`. `agent` carries them as `ContentPart::Reasoning` (a domain addition by the agent author).

### Responses

@@items anthropic/response.rs Message ToolUse ResponseBlock StopReason Usage@@

`Message::replay(&self) -> Vec<ContentBlock<'_>>` turns an answer into the assistant turn of the next request, borrowing from the message. The elided lifetime ties the blocks to `self`, so the borrow checker requires the message to outlive the next request, and no text or signature is copied. `ToolUse<'m>` is a borrowed view of one tool call, where `'m` is the message it points into.

Unknown block types (`server_tool_use`, new ones) decode as `ResponseBlock::Unknown`. Unknown stop reasons decode as `StopReason::Other`, so a new API value never fails a call; `model_context_window_exceeded` has its own variant and callers treat it like `max_tokens`. The API reference types every cache count, and `input_tokens` in `message_delta`, as `number or null`, so a `null` reads as 0; `usage` itself defaults, because a documented stream example sends `message_start` without it. `StopReason::Refusal` arrives with HTTP 200: the fallback adapter treats it as invalid output, and `agent` ends the turn with its refusal message.

### Streaming

The `event:` name equals the data's `type`. The order is `message_start`, then for each block `content_block_start`, `content_block_delta`… and `content_block_stop`, then one or more `message_delta`, then `message_stop`. `ping` can come at any point. An `error` event can arrive **after HTTP 200** (`overloaded_error`). Usage in `message_delta` is **cumulative**, so it replaces and never adds.

@@items anthropic/stream.rs StreamEvent Delta MessageDeltaBody@@

@@items anthropic/stream.rs MessageStream@@

`MessageStream` is owned and `'static`: it holds the HTTP response and a boxed stream over it, and borrows neither the client nor the request. Pings and unknown events are filtered out. An `error` event becomes `Err(AnthropicError::Stream)`, and it is always the last item. `BoxStream<'static, _>` is `Pin<Box<dyn Stream + Send + 'static>>`, so the stream can be moved into a spawned task or an SSE body. A `'static` stream also satisfies `ChatModel::stream`'s `BoxStream<'a, _>` for any `'a` ([Reference, subtyping and variance](https://doc.rust-lang.org/reference/subtyping.html)).

@@items anthropic/stream.rs Output MessageAccumulator@@

@@impl anthropic/stream.rs impl MessageAccumulator@@

The accumulator is shared by streaming callers and `MessageStream::collect_message`. Tool-call JSON is joined per block index and parsed at `content_block_stop`. An empty accumulation means a tool with no arguments and parses as `{}`. `Output` maps one to one onto `ChatChunk`: `Text` to `TextDelta`, `ToolUse` to `ToolCall`, `Reasoning` to `Reasoning`, `Usage` to `Usage` (`Usage::to_domain` counts cached input as input), and `Stop` to `Done`.

### The client

@@items anthropic/client.rs ModelIds AnthropicConfig AnthropicClient@@

@@impl anthropic/client.rs impl AnthropicClient@@

**Retries** happen only before the first byte of the response. `create` retries per the policy, and so does `stream` until the API answers 200. After that, errors arrive in the stream and are not retried: `agent` restarts the call only if it has sent nothing to the user. The worker passes `RetryPolicy::NONE`, because it owns the job deadline and its one validation retry ([contracts 6, after decoding](../architecture/contracts.md#after-decoding)). Model ids come from configuration, complete as written (`claude-opus-5-5`, `claude-haiku-5-5`, with no date suffix).

| Response | `AnthropicError` | Retried | `ModelError` |
|---|---|---|---|
| Connect failure, timeout, dropped body | `Transport` | yes | `Unavailable` |
| 408, 409, 429, 500–599, 529 | `Api { status, kind, retry_after, .. }` | yes, honouring `retry-after` | `Unavailable` (`503 upstream_unavailable` for chat) |
| 429 with `error.details.error_code: "enforced_spend_limit_reached"` | `SpendCap` | **never** | `Unavailable` |
| 400, 401, 403, 404, 413 | `Api` | no | `Failed` (our bug: logged with the message) |
| `error` event after 200 | `Stream { kind, .. }` | no (`retryable()` is true for `overloaded_error`, `api_error` and `rate_limit_error`, so the caller may restart) | `Unavailable` |
| Body or event that doesn't parse; tool JSON that doesn't parse | `Decode`, `ToolInput` | no | `InvalidOutput` |

@@items anthropic/error.rs AnthropicError@@

### Structured output and the schema sanitiser

`output_config.format = {type: "json_schema", schema}` makes the reply one text block holding JSON. The API compiles the schema to a grammar and **rejects with a 400** the keywords it can't enforce. [Contracts 6](../architecture/contracts.md#after-decoding) names `maxLength`, `maxItems`, `minimum` and `maximum`. Anthropic's documentation (research notes) also excludes `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf` and `minItems` above 1, and every object needs `additionalProperties: false`. The contracts' VLM schemas use these keywords because xgrammar enforces them on vLLM, so the fallback worker and the plan writer sanitise first. Afterwards they enforce the stripped limits themselves (truncate strings and arrays, clamp numbers and boxes) from the returned list.

The first live calls (2026-10-16) added two rules. `oneOf` is refused (`Schema type 'oneOf' is not supported`), and `schemars` writes it for every enum with documented variants and every tagged union, so the sanitiser rewrites it: a `oneOf` of constants becomes an `enum`, with each variant's description appended to the parent's as a `- "value": …` line, and any other `oneOf` becomes an `anyOf`. The official Python SDK's `transform_schema` maps every `oneOf` to `anyOf`; the `enum` rewrite is this crate's own and keeps those parameters out of the union limit below. A `oneOf` beside an existing `enum` keeps the `enum` and becomes an `anyOf`, so both still apply, and only scalar constants become an `enum`, because the API refuses objects and arrays in one. A `format` survives only on a string and only if the API lists it (`SUPPORTED_FORMATS`), so `schemars`'s `uint32` and `double` are dropped and returned in `stripped`. Strict tools also share one grammar per request with its own size limit: the docs give 20 strict tools, 24 optional parameters and 16 union-typed parameters, and nineteen strict tools were refused as too large, so `agent` marks only its write tools strict ([agent 9](agent.md#9-model-adapters)). A nullable enum, `anyOf: [{enum: […]}, {type: "null"}]`, becomes one `enum` with `null` among its values: the same answers without a union, so the fallback's per-question `image` no longer counts toward the 16 (refused live at 20). The grammar's overall size has a limit of its own, which the worker meets by splitting a large indicators call ([worker 8](worker.md#8-the-vlm-adapters)).

@@items anthropic/schema.rs UNSUPPORTED_KEYWORDS SUPPORTED_FORMATS Stripped Sanitized sanitize_schema sanitize_in_place@@

`Sanitized<'a>` borrows the input schema when nothing had to change (`Cow::Borrowed`) and owns a cleaned copy otherwise; a rewrite alone (`oneOf`) counts as a change. It plugs straight into `OutputFormat::JsonSchema` and `Tool::input_schema`, which take a `Cow<'a, Value>`. The walk visits only schema positions: the keys of `properties` are names, so a property named `maximum` is kept. The test checks exactly that.

---

## 6. OpenAI-compatible VLM servers

**Endpoint.** `POST {base_url}/v1/chat/completions` on vLLM 0.31 or llama.cpp's `llama-server` ([contracts 6, per-job schema](../architecture/contracts.md#per-job-schema)). Also `GET /v1/models`, for the worker's start-up check of the served name, and `GET /health`. Calls are non-streaming: a findings call is at most 1,500 tokens, and the worker validates the whole answer.

**Request.** Images are `data:` URLs, written while serialising (`DataUrl<'a>`, the same technique as `Base64<'a>`). `response_format` is `json_schema` with `strict: true`. The output limit is `max_completion_tokens` on vLLM, where `max_tokens` is deprecated in 0.31, and `max_tokens` on llama.cpp. Repetition penalty is `repetition_penalty` on vLLM and `repeat_penalty` on llama.cpp. `ServerFlavour` chooses the names, so the worker writes one request. Sampling follows the vision decisions: temperature 0, `top_p` 1, presence penalty 0, repetition penalty 1 (`Sampling::DETERMINISTIC`). vLLM refuses structured output combined with tools, so the request type has no tools. The `guided_json` field was removed in vLLM 0.12 and is not used.

@@items openai_compat.rs ServerFlavour OpenAiCompatConfig Sampling ChatCompletionRequest ChatMessage ChatContent ContentPart DataUrl ResponseFormat JsonSchemaSpec@@

`Body<'r, 'a>` is the flavour-specific wire form. Its two lifetimes have the same meaning as in section 5: `'r` is the borrow for serialising, `'a` the worker's data.

**Response.** `finish_reason` is an enum. `Length` means the output was cut, and the worker counts it as a validation failure: one retry, then `model_error` with `retryable: true` ([contracts 6](../architecture/contracts.md#after-decoding)). `reasoning_content` carries a thinking model's reasoning when the server separates it. Qwen3.5's thinking is switched off, so it should stay empty.

@@items openai_compat.rs ChatCompletion Choice AssistantMessage FinishReason OpenAiCompatError@@

@@impl openai_compat.rs impl OpenAiCompatClient@@

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

@@items open_meteo.rs PROVIDER MAX_CELLS_PER_REQUEST OpenMeteoConfig OpenMeteo@@

@@items open_meteo/wire.rs Forecast ForecastResponse@@

**Building the summary.** `cell_weather` is a pure function over a borrowed `&Forecast`, tested against the live fixture. The current hour is `now` shifted by the response's `utc_offset_seconds` and floored to the hour. `current` is that hour's row. The 24-hour sums and the 7-day figures (168 rows) end with it. `forecast` holds the daily rows from today, at most `forecast_days`, each with its hours' mean humidity and `hours_rh_over_90`. `past_7_days.longest_humid_run_days` is the longest run of consecutive humid days among the 7 whole local days before today, a humid day having at least `humid_day_hours` (10) hours over 90% (`WeatherThresholds::longest_humid_run`), so the pipeline can tell a humid stretch in the past week ([contracts 9](../architecture/contracts.md#garden-map), crowding), which the 168-hour total can't show. Open-Meteo has no 0–9 cm layer, so `moisture_0_9cm_m3m3` is the depth-weighted mean of the 0–1, 1–3 and 3–9 cm layers, `(1·m₀₋₁ + 2·m₁₋₃ + 6·m₃₋₉) / 9`, as contracts 7 now specifies. The units are checked against the expected ones (°C, %, km/h, mm, m³/m³), and a response in other units is a bad response.

@@items open_meteo/summary.rs moisture_0_9cm cell_weather SummaryError@@

The fixture gives Toronto at 19:00 on 2026-10-15: 10.3 °C, RH 73 %, wind 12.1 km/h, 12.4 mm and 24 hours above 90 % over 7 days, 0.297 m³/m³ at 0–9 cm, ET₀ 1.14 mm over 24 hours. Ottawa's forecast shows frost on 2026-10-17 (minimum −0.0 °C). Hours over 90 % on 8–14 October were 0, 0, 0, 6, 12, 6, 0 in Toronto (a longest humid run of 1 day) and 4, 1, 3, 2, 6, 22, 11 in Ottawa (2 days: a humid stretch in the past week).

**`frost_risk`** is not an Open-Meteo field. A day is at risk when its 2 m minimum is frost under `OpenMeteoConfig::thresholds` (`WeatherThresholds::is_frost`: at or below 0.5 °C by default), because air at 2 m runs warmer than the ground and ground frost forms with the air at 1–2 °C. `api` passes the same `WeatherThresholds` to its triggers and the pipeline ([domain 7, weather thresholds](domain.md#weather-thresholds)), so the cached `frost_risk` agrees with both. `hours_rh_over_90` counts hours strictly over 90% (`WeatherThresholds::humid_hours`).

**The scheduler's rule.** [Contracts 7](../architecture/contracts.md#weathersummary): a failed multi-location request is retried once, then its cells are fetched one by one, and a cell that still fails skips its gardens until the next run. `fetch_cells` implements that over any `&dyn WeatherProvider`, so it is tested with `domain`'s `MockWeatherProvider`, and `api`'s scheduler calls it with the wired adapter.

@@items open_meteo.rs fetch_cells@@

| Failure | `ProviderError` | Contract behaviour |
|---|---|---|
| Timeout, connect, 5xx | `Unavailable { provider: "open_meteo" }` | A scan's run fails with `upstream_unavailable` (retryable, so the user can resubmit), as contracts 3 says for "weather down" ([pipeline 6](pipeline.md#6-retrieve)); `GET /gardens/{id}/weather` answers `503 upstream_unavailable` with `upstream: "open_meteo"`; the scheduler skips the cell's gardens until its next run |
| 429 | `RateLimited { retry_after_s }` | The same, and the scheduler waits for its next run |
| 400 (`{"error":true,"reason":…}`), wrong count of locations, nulls at the current hour, wrong units | `BadResponse` | The same; logged as a bug |

**Limits.** The free tier allows 600 calls a minute, 5,000 an hour and 10,000 a day, and is **non-commercial only**. A request with more than 10 variables or more than 2 weeks weighs more than one call. Ours weighs about 2.8 per location by the documented examples (*unverified* formula). Each location in a multi-location request is assumed to count. The cache (one fetch per cell and hour, in `api`) is what keeps the quota.

---

## 8. SoilGrids

**Endpoint.** `GET https://rest.isric.org/soilgrids/v2.0/properties/query`, with `lon`, `lat`, repeated `property=` (8), repeated `depth=` (6) and `value=mean` ([architecture 6.4](../architecture/README.md#64-soil); [contracts 10, other entity shapes](../architecture/contracts.md#other-entity-shapes)). Values are integers in "mapped units", and value ÷ `d_factor` is the contract's unit, because every contract unit equals SoilGrids' `target_units`. For example pH ×10 ÷ 10, clay g/kg ÷ 10 = %, nitrogen cg/kg ÷ 100 = g/kg, bulk density cg/cm³ ÷ 100 = kg/dm³. The depth labels are the contract's `depth_cm` values plus "cm".

@@items soilgrids.rs SoilGridsConfig SoilGrids to_depths nearby_points@@

**Null pixels.** Central Toronto, Scarborough, (43.75, −79.55) and Guelph all answered `"mean": null` for every property and depth on 2026-10-15. The target users are Toronto gardeners, so "no data here" is a distinct outcome, not a success:

1. Fetch the full profile at the garden's point. If any value is present, done.
2. Otherwise probe the nearby points, nearest ring first (4 points at 2 km, then 4 at 5 km). Each probe is a one-value query (pH at 0–5 cm), which answered in about 0.3 s live against 9–15 s for a full query.
3. At the first point with data, fetch the full profile.
4. Otherwise answer `ProviderError::NotFound { provider: "soilgrids" }`.

**Retries** ([contracts 9, the soil profile](../architecture/contracts.md#gardens)). Every failure sets `soil_profile_status: unavailable`; `api` stores the next attempt in `garden.soil_retry_at` by `domain::rules::limits::soil_retry_after`: 7 days after `NotFound`, because a null pixel doesn't change on a quick retry, and 15 minutes after anything else. The adapter stops at the first failing call, so an outage costs one call per garden and attempt; only a null pixel costs the 9 paced calls, now once a week ([api 16](api.md#16-the-scheduler)).

**Limits.** SoilGrids is in beta and allows 5 calls a minute (from a third-party plugin page; ISRIC's own pages don't say, *unverified*). The `Pacer` spaces calls 15 s apart for the whole process. The timeout is 60 s. The worst case, a null pixel with every probe null, makes 9 calls, about 2 minutes. Garden setup runs in the background, so the user doesn't wait.

| Outcome | Result | `soil_profile_status` ([contracts 9, gardens](../architecture/contracts.md#gardens)) |
|---|---|---|
| Data at the point or nearby | `Ok(SoilProfile)` | `ready` |
| No data anywhere probed | `NotFound` | `unavailable`, retried after 7 days |
| Timeout, connect, 5xx | `Unavailable` | `unavailable`, retried after 15 minutes ([architecture 8.5](../architecture/README.md#85-garden-setup)) |
| 429 | `RateLimited` | `unavailable`, retried after 15 minutes |
| 4xx, unparsable body | `BadResponse` | `unavailable`, retried after 15 minutes; logged as a bug |

---

## 9. Geocoding

[Architecture 8.5](../architecture/README.md#85-garden-setup) and [contracts 9, gardens](../architecture/contracts.md#gardens): a garden's location comes from GPS with consent, or from a typed postal code or city, geocoded once. It yields coordinates, a label ("Toronto, ON") and the IANA zone that dates every `LocalDate`.

| Input | Coordinates | Label | Zone |
|---|---|---|---|
| GPS (`exact`, `about_1_km`) | the fix, rounded by `round_for` | Nominatim `reverse` at 0.01° | `tzf-rs`, offline |
| Canadian postal code `A1A 1A1` | Nominatim `search?postalcode=…&country=CA`, then Zippopotam by its first 3 characters | Nominatim (`ISO3166-2-lvl4` → "ON"), Zippopotam (`state abbreviation`) | `tzf-rs` |
| Forward sortation area `A1A` | Zippopotam `/ca/{fsa}` | Zippopotam | `tzf-rs` |
| US ZIP | Open-Meteo geocoding, `countryCode=US` | Open-Meteo | Open-Meteo |
| City ("Guelph, ON") | Open-Meteo geocoding, `countryCode=CA` | `name, ` + the province code from `admin1` | Open-Meteo, else `tzf-rs` |

**Why three services.** Open-Meteo's geocoder does not resolve Canadian postal codes: "M5V 3L9", "M5V", "K1A 0B1" and "N1G" all returned nothing live. Nominatim resolves a full code but not a bare forward sortation area. Zippopotam resolves the area only, and its terms are unchecked (*unverified*), so `zippopotam_url: None` turns it off.

**Nominatim's policy:** at most 1 request a second, an identifying user agent (`USER_AGENT` names the project and its repository), results cached, ODbL attribution, and no autocomplete. Geocoding once per garden, with a 30-day cache keyed by the normalised query, fits the policy.

**Privacy.** A reverse lookup sends only the point rounded to 0.01°, whatever the precision. The label is city-level anyway, so an exact fix never leaves the server ([architecture 11](../architecture/README.md#11-security-privacy-and-safety)). The caller's own point is returned and stored at the precision the user chose.

@@items geocode.rs GeocoderConfig TimeZoneFinder round_for PostalCode province_code CompositeGeocoder@@

`GeocodeQuery<'a>` (in `domain`) borrows the request's strings: `'a` is the handler's request body, which lives for the whole call. The `#[async_trait]` method boxes a future that holds the query, which `async_trait` allows because the boxed future's lifetime is tied to every borrowed argument.

`TimeZoneFinder` wraps `tzf_rs::DefaultFinder`, which takes `(lng, lat)` in that order. It answers `""` where no zone covers the point (open ocean), and that becomes `None`. Building it loads a few megabytes of boundaries, so `api` builds one at start and shares it through an `Arc`.

| Failure | `ProviderError` | Contract behaviour |
|---|---|---|
| Unknown postal code or city; unparsable postal code; no zone at the point | `NotFound { provider: "geocoder" }` | `400 invalid_request` with `details.field` = `location.postal_code` or `location.city` (decided in `api`; contracts 2 has no dedicated code, section 17) |
| Timeout, connect, 5xx | `Unavailable` | `503 upstream_unavailable` with `upstream: "geocoder"` |
| 429 | `RateLimited` | the same |
| Unparsable body | `BadResponse` | the same; logged |

---

## 10. Clerk

### Session tokens (`AuthVerifier`)

[Contracts 1, authentication headers](../architecture/contracts.md#authentication-headers), [architecture 4](../architecture/README.md#4-users-and-authorization) and [frontend 8](../frontend/README.md#8-authentication-clerk): the app sends `Authorization: Bearer <Clerk session token>`, and the API verifies it against the instance's JWKS.

- **Algorithm:** RS256 only. A token signed with HS256 is rejected (`Validation::new(Algorithm::RS256)`) ✓.
- **Keys:** `{issuer}/.well-known/jwks.json` on the Frontend API. It needs no secret and isn't rate limited. The URL comes from configuration, never from the token's `iss`, which rules out SSRF. Keys are cached by `kid` for an hour. An unknown `kid` refetches the whole set, at most once every 30 s, and the refetch is single-flight (the mutex), so random `kid`s can't hammer Clerk.
- **Claims:** `iss` must equal the configured issuer. `exp`, `nbf`, `iss` and `sub` are required. `exp` and `nbf` are checked against the caller's `now` with a 5 s leeway. `jsonwebtoken`'s own time checks read the system clock, so they are off: its default `leeway` is 60 s, and `validate_nbf` is false by default. `aud` is not checked, because session tokens have none. `azp` is checked only when present, against the web origins: native apps send no `Origin`, so their tokens carry no `azp`.

@@items clerk.rs ClerkVerifierConfig ClerkClaims ClerkVerifier@@

@@impl clerk.rs impl AuthVerifier for ClerkVerifier@@

| Failure | `AuthError` | Contract |
|---|---|---|
| Malformed, wrong algorithm, bad signature, wrong issuer, expired, not yet valid, `azp` not allowed, unknown `kid` (also after a refetch, or while refetching is rate limited) | `InvalidToken(Some(source))` | `401 unauthenticated`; the app refreshes the token once, then signs out |
| JWKS unreachable or 5xx | `Unavailable` | `503 upstream_unavailable`, `upstream: "auth"` (the role, as `database` and `storage` are; `PROVIDER = "clerk"` names the vendor in logs only) |

### Account deletion (`AuthAdmin`)

`DELETE https://api.clerk.com/v1/users/{user_id}` with the instance secret key, the last step of `DELETE /me`'s cascade ([contracts 9, account](../architecture/contracts.md#account)). It is retried until it succeeds, so a 404 (already gone) is success and the call is idempotent. One call is one attempt, and the cascade schedules the retries.

@@items clerk.rs ClerkAdminConfig ClerkAdmin@@

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

@@items svix.rs SvixVerifier@@

@@impl svix.rs impl SvixVerifier@@

**Clerk's payload.** The envelope is `{object: "event", type, data, timestamp, instance_id}`, and `data` is the User object, so `data.id` is the user id. For `user.deleted`, `data` is a `DeletedObject` whose `id` is optional in Clerk's OpenAPI schema. The mapping is deterministic: a redelivery (same `svix-id`) maps to the same `AuthWebhookEvent`. `api` acts on `user.deleted` only: the account cascade starts with the account tombstone's compare-and-set, so a redelivery, or a `user.deleted` during a `DELETE /me` cascade, starts nothing. `user.created` changes nothing ([api 22](api.md#22-decisions-on-the-contracts-open-questions), question 24; [contracts 9, auth webhook](../architecture/contracts.md#auth-webhook)): the user record is made by the first authenticated request, so a late `user.created` can't resurrect a deleted user, and the tombstone refuses the deleted account's still-valid tokens.

@@items svix.rs map_clerk_event@@

`ClerkEvent<'b>` borrows its `type` string from the body (`#[serde(borrow)]` on a `Cow<'b, str>`) instead of allocating it ([serde, borrowing data](https://serde.rs/lifetimes.html)). `'b` is the body, which outlives the mapping.

| Input | Result | Contract |
|---|---|---|
| Bad or missing signature, timestamp off by more than 5 min, unparsable timestamp | `Err(AuthError::InvalidWebhook)` | `401 unauthenticated` (`invalid_signature` is the signed-URL `403`); Svix redelivers |
| Verified `user.created`, `user.deleted` | `Ok(Some(event))` | `user.created`: nothing; `user.deleted`: the account cascade, once; `204` |
| Verified other type, `user.*` without `data.id`, unparsable body | `Ok(None)` | `204`; an unparsable signed body is logged |

---

## 12. Expo Push

[Contracts 9, push notifications](../architecture/contracts.md#push-notifications). `POST https://exp.host/--/api/v2/push/send` takes at most 100 messages. Each message has one token in `to`, so ticket *i* answers message *i*. The payload goes in `data`.

@@items expo_push.rs ExpoPushConfig ExpoMessage Ticket ExpoPush@@

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

Jev is a zero-shot text classifier in early access ([architecture 10](../architecture/README.md#10-models-and-gpu-placement)). Its API isn't public, so the request and response here are placeholders (*unverified*). `api` wires `JevClassifier` only when `JEV_*` is configured. Otherwise it wires `agent::model::LlmClassifier` (the cheapest model with structured output), which is the real fallback. Nothing on the critical path depends on Jev.

@@items jev.rs JevConfig JevClassifier@@

Any transport failure or error status is `ModelError::Unavailable { model: "jev" }`. The router then falls back as `agent` decides.

---

## 14. Agent Memory

**What it is.** `SurrealDB` Agent Memory is a hosted memory service on `SurrealDB` Cloud. The `memory` table stays the system of record; each **shared** memory is mirrored into the service as one triple, and the service supersedes the older value of the same key and keeps the history ([domain, the belief store](domain.md)). A private chat memory never leaves the database. `AgentMemory` implements `BeliefStore` over the REST API. Every call is `{base_url}/api/v1/{context_id}/…` with `Authorization: Bearer <key>`, JSON in and out.

@@items agent_memory.rs AgentMemoryConfig AgentMemory@@

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

**What they are.** Two standalone clients for a garden's farm connectors ([connectors 5 and 6](../connectors/README.md#5-connecting-and-tokens)). They implement no port yet: E4's sync builds the `FarmSource` port on them ([EON E4](../EON/README.md)). Both take the token as an argument of each call, so one client serves every workspace and account, and both reuse the shared `retry`, `RetryPolicy` and `Pacer` of section 4. Tokens are `SecretString` and never appear in an error or a log line.

### The shared error

One error type for both, with the codes a connector's `last_error` stores ([connectors 6](../connectors/README.md#6-sync)).

@@items farm.rs FarmErrorCode FarmClientError@@

`FarmClientError::code()` maps a failure to the stored string:

| Failure | `last_error` |
|---|---|
| `AuthRefused` | `auth_refused`: the owner must reconnect |
| `RateLimited` | `rate_limited`: the sync resumes from its cursor on a later run |
| `NotFound` | `source_missing`: a chosen channel or folder is gone or not visible |
| `Unavailable`, `Refused`, `Malformed` | `vendor_unavailable` |
| (set by the sync, never by a client) | `too_large` |

`Refused` is `vendor_unavailable` because nothing the owner does to the token fixes it, and the sync skips a refused single file instead of stopping.

### Slack

Read methods only, plus the OAuth exchange and the revoke: `auth.test`, `conversations.list`, `conversations.history`, `conversations.replies`, `users.info`, `auth.revoke` and `oauth.v2.access`.

@@items slack.rs VENDOR SLACK_BASE_URL TIER_1_INTERVAL TIER_1_PAGE_LIMIT SlackConfig AuthTest SlackChannel SlackMessage HistoryPage SlackUser OAuthAccess SlackClient@@

@@impl slack.rs impl SlackClient@@

**`ok: false` at HTTP 200.** Slack refuses with status 200 and `{"ok": false, "error": "<code>"}`, so the client reads `ok` before anything else. `not_authed`, `invalid_auth`, `token_revoked`, `token_expired` and `account_inactive` are `AuthRefused`, and so are `missing_scope` and `no_permission`: the owner must reconnect to grant the scope. `channel_not_found`, `not_in_channel`, `thread_not_found` and `user_not_found` are `NotFound`. `ratelimited` and HTTP 429 are retried after the `Retry-After` the server sent, then `RateLimited`. A 5xx, `internal_error` or a timeout is retried, then `Unavailable`. Any other code is `Refused`, never retried. A code that isn't a short `snake_case` word is replaced by `unrecognised`, so a body can't leak through the code.

**The pacer.** `conversations.history` and `conversations.replies` are Tier 1 for a new install of an app outside the Slack Marketplace: about one call a minute per method and workspace ([connectors 3](../connectors/README.md#3-decisions)). `TIER_1_INTERVAL` is 1 minute and `TIER_1_PAGE_LIMIT` 15 messages a page. Each (team, method) pair has its own `Pacer`, so every connector holding one team's token shares it. The pacers live in a `moka` cache bounded by `SlackConfig::max_pacers` (10,000), dropped after idling for twice the interval and at least 10 minutes, so the map can't grow without bound and a pacer is never dropped while its last call is inside its interval. A call waits for its turn. The sync bounds a run and continues from its cursor on the next one.

**`SlackConfig::era`.** Points the client at an Era simulator: no pacing and no back-off, but a `Retry-After` is still honoured (up to 60 s). Pages stay at 15, so paging is exercised.

**`users.info` without the email.** The method returns the user's names only (`SlackUser::label` picks the display name, then the real name). The email is never read, even when Slack sends one ([connectors 8](../connectors/README.md#8-privacy-and-security)).

**Permalinks.** `permalink(workspace_url, channel, ts, thread_ts)` builds a message's link from `auth.test`'s workspace URL: `<url>/archives/<channel>/p<ts without the dot>`, with `?thread_ts=<parent>&cid=<channel>` for a reply. A parent (its `thread_ts` equal to its `ts`) gets the plain link. It makes no call.

@@items slack.rs permalink@@

### Google Drive

@@items google_drive.rs VENDOR SCOPE MAX_FILES MAX_FOLDERS MAX_TEXT_BYTES DriveConfig ExportFormat DriveClient@@

@@items google_drive/pkce.rs Pkce InvalidVerifier@@

@@impl google_drive.rs impl DriveClient@@

**OAuth with PKCE.** `Pkce::generate` makes a verifier and its S256 challenge; the app sends the challenge and `exchange_code` sends the verifier. `refresh` trades a refresh token for an access token, and `revoke` ends the grant on disconnect. The only scope is `drive.readonly`, so the token response has no `id_token`. `about_email` calls `about.get` for the account's email. `invalid_grant` on a refresh is `AuthRefused`.

**Reading.** `list_folder` walks a folder recursively across `nextPageToken`, and stops at 500 files or 500 folders (`MAX_FILES`, `MAX_FOLDERS`); `FolderListing::truncated` says so. Trashed files are left out, and a shared drive's folder works. `export` returns a Doc as text or a Sheet as CSV, and `download_text` returns a plain-text file. Both cut the text at `MAX_TEXT_BYTES` (1 MB), so one large file can't fill memory. `start_page_token` gives the cursor a first sync saves, and `changes` reads `changes.list` page by page up to the last page, the one carrying `newStartPageToken`. Drive doesn't order the pages by time, so the caller gets all of them or an error.

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
| Open-Meteo | the **live fixture** of 2026-10-15 (Toronto values above, Ottawa frost); query lists via `wiremock`; object versus array; 429 and 400 mapping; `fetch_cells` with `MockWeatherProvider` |
| SoilGrids | `d_factor` conversion on the live Iowa fixture; the live Toronto and Guelph null fixtures; the nearby-point fallback and `NotFound` with `wiremock` |
| Geocoder | the live Open-Meteo fixture; Nominatim and Zippopotam shapes; the 0.01° reverse lookup; the cache (one request for two queries); `tzf-rs` on Toronto and Vancouver |
| Clerk | tokens signed with a throwaway RSA key, generated once per test run with `aws-lc-rs` so no private key is committed, and served as a JWKS by `wiremock`: issuer, expiry, `nbf`, `azp`, leeway, missing `nbf`, refetch limit; `DELETE` 200, 404 and 429 |
| Svix | the `svix` crate's `Webhook::sign` as the oracle; rotation (several entries), tampering, ±301 s |
| Agent Memory | `wiremock`, exact request bodies (the service ignores unknown fields silently): `record` and the scope cache (one registration for two writes, none after the first), the `400 unknown scope node` re-register and single retry, a 5xx write retried twice, `current` (plant and garden merged, superseded dropped, label and both timestamp forms, 404 as empty), `history` (cursor followed through a short page, newest first), `search` (exact lens, hit filter, prefix stripped), `forget` (trailing slash, cache dropped), the 500, 429, 401, 403 and 422 mapping, `Debug` without the key |
| Slack, Google Drive | `wiremock`, exact requests: `ok:false` at 200 for each code group, `Retry-After`, 5xx retries, the pacer per (team, method) and its bounded cache, paging by cursor, `users.info` without the email, `permalink`; Drive's OAuth with PKCE, `about.get`, the recursive listing and its cap of 500, the 1 MB cut, `changes` to the last page, the `403` reasons, a refused scope as `auth_refused`; no body in any error or log |
| Era | see below |
| Expo | ticket order, `DeviceNotRegistered` in tickets and receipts, batches of 100, a failed whole push |

**The Era tests** (`tests/era_fixture.rs`, `era_slack.rs`, `era_drive.rs`) run the two clients against Era's Slack and Google Drive simulators on the Maple Row Farm fixture. They are `#[ignore]`d, so `cargo nextest run` skips them. Start the simulators and run them with:

```sh
docker compose -f infra/dev.compose.yml --profile era up -d --wait era-slack era-drive
ERA_SLACK_BASE=http://127.0.0.1:8091 ERA_GOOGLE_BASE=http://127.0.0.1:8092 \
  cargo nextest run -p integrations --run-ignored only -E 'test(/era/)'
```

- **They fail, they don't skip.** `era_sim::base_url` panics when `ERA_SLACK_BASE` or `ERA_GOOGLE_BASE` is unset, so a run without the simulators is red.
- **Minted tenant credentials.** Each test creates a `Tenant`, an isolated copy of the company, and passes the credential the simulator mints for it as the client's bearer token. The credential selects the tenant: no `X-Sim-Tenant` header and no test-only constructor exist.
- **One at a time.** Faults are global to a simulator, so a fault set by one test reaches every other test. [`backend/.config/nextest.toml`](../../backend/.config/nextest.toml) puts every test named `era_*` in the nextest group `era` (`max-threads = 1`). The fault tests also hold a file lock and clear the faults when they end, also when they fail. The `era` CI job comes with [EON E6](../EON/README.md#e6-ci).

Paused time (`start_paused`) is used only where no real I/O runs: the retry and pacer tests. With a mock server in the loop, tokio's auto-advance would jump to the request's timeout while the server answers, which the Clerk refetch test showed. Not run: live calls to Anthropic, Clerk, Expo and Nominatim (no keys or accounts on 2026-10-15), and the VLM servers (no GPU here).

---

## 19. Open questions

1. **Frost threshold.** Settled in [domain 7](domain.md#weather-thresholds): one `WeatherThresholds` for `frost_risk`, the frost alert rule of [architecture 8.3](../architecture/README.md#83-forecast-alerts) and fusion, with frost at a 2 m minimum ≤ 0.5 °C. The number itself is still a proposal ([domain 12](domain.md#12-open-questions), item 17).
2. **SoilGrids `NotFound` versus `Unavailable`.** Settled in contracts v0.10 ([contracts 9, the soil profile](../architecture/contracts.md#gardens)): one `unavailable` value on the wire, retried after 7 days for no data and after 15 minutes for an outage (section 8; `api`'s `soil` task). Still open: architecture 6.4's Soil Landscapes of Canada fallback for null pixels.
3. **Postal-code geocoding** relies on Nominatim (policy-bound, 1 request a second) and Zippopotam (terms unchecked). [Contracts 9](../architecture/contracts.md#gardens) says "geocoded once" and names no service, and [contracts 2](../architecture/contracts.md#2-errors) has no code for a location that can't be geocoded (`400 invalid_request` with the field is used). Confirm Zippopotam's terms, or drop it and refuse a bare forward sortation area.
4. **Reverse-geocoding failure for GPS.** A garden created from GPS needs a label. Today a Nominatim failure fails the geocode (`503`). A fallback label (the zone's city) would let creation proceed.
5. **Open-Meteo licence.** The free API is non-commercial. Settle it before any public launch ([architecture 15](../architecture/README.md#15-open-decisions)).
6. **The hosted LLM's box format and the stripped keywords** are from Anthropic's documentation, not a live call ([contracts 12](../architecture/contracts.md#12-open-questions), item 26). One live call on day 2 confirms both.
7. **Late `user.created` after `user.deleted`.** Settled in `api`: `user.created` changes nothing, and the account tombstone refuses a deleted account's tokens (section 11).
8. **Jev's API** is a placeholder until early access arrives.

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
- Nominatim [usage policy](https://operations.osmfoundation.org/policies/nominatim/) · Zippopotam [API](https://api.zippopotam.us)
- Clerk: [session tokens](https://clerk.com/docs/guides/sessions/session-tokens) · [manual JWT verification](https://clerk.com/docs/guides/sessions/manual-jwt-verification) · [webhooks](https://clerk.com/docs/guides/development/webhooks/overview) · [Backend API](https://clerk.com/docs/reference/backend-api) · [system limits](https://clerk.com/docs/guides/how-clerk-works/system-limits)
- Svix: [manual verification](https://docs.svix.com/receiving/verifying-payloads/how-manual)
- Expo: [sending notifications](https://docs.expo.dev/push-notifications/sending-notifications/)

**Project documents and skills**
- [Data contracts](../architecture/contracts.md) · [architecture](../architecture/README.md) · [frontend](../frontend/README.md) · [backend architecture](README.md) · [domain](domain.md) · [domain-types](domain-types.md)
- Skills in the main checkout's `.claude/skills/`: `rust` (with `versions.md`), `rust-async`, `rust-errors`, `rust-tracing`, `rust-testing`

The upstream links were read on 2026-10-15 by the research notes this crate is built from. The crate APIs were checked against their source at the pinned versions in `~/.cargo/registry` and by compiling; the links above were not reopened while writing this document.
