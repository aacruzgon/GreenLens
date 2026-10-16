# JSON logs: setup, line shape, errors, spawned tasks, secrets

Part of [rust-tracing](SKILL.md). Each section is a compiled block; the tests capture the subscriber's JSON output in memory and assert on it. Versions: tracing 0.1.44, tracing-subscriber 0.3.23 (`env-filter`, `json`), anyhow 1.0.104.

## Contents

1. [Setup in `main`](#1-setup-in-main)
2. [What a line looks like](#2-what-a-line-looks-like)
3. [Logging errors with their chain](#3-logging-errors-with-their-chain)
4. [Spans across `tokio::spawn` and `spawn_blocking`](#4-spans-across-tokiospawn-and-spawn_blocking)
5. [Secrets can't be logged by accident](#5-secrets-cant-be-logged-by-accident)

## 1. Setup in `main`

One function builds the subscriber, so `main` and the tests use the same settings. `main` calls `init_tracing` once, first, before anything logs.

- `EnvFilter::builder().with_default_directive(INFO).from_env()` reads `RUST_LOG`. If it is unset or empty, the default directive applies: `from_env` reads an unset variable as `""` and `parse("")` adds the default (`filter/env/builder.rs`; the doc comment's "if the environment variable is unset, no directive is added" doesn't match the code). An invalid `RUST_LOG` is an error, so a typo fails start-up instead of silently logging at the wrong level ([`Builder::from_env`](https://docs.rs/tracing-subscriber/0.3.23/tracing_subscriber/filter/struct.Builder.html#method.from_env)).
- `.json()` writes newline-delimited JSON to **stdout**, the fmt subscriber's default writer, which Docker collects ([`fmt` module](https://docs.rs/tracing-subscriber/0.3.23/tracing_subscriber/fmt/index.html)).
- `flatten_event(true)` puts the message and the event's fields at the top level instead of under `"fields"`. `with_current_span(false)` drops the `"span"` object, which repeats the last entry of `"spans"`. `with_span_list(true)` keeps `"spans"`, root first, so the request span's `request_id` is on every line, however deeply nested. The defaults are `false`, `true`, `true` ([`Json`](https://docs.rs/tracing-subscriber/0.3.23/tracing_subscriber/fmt/format/struct.Json.html)).
- `try_init` (from `SubscriberInitExt`) sets the global default and, with the default `tracing-log` feature, installs `LogTracer`, so crates that log through `log` (rustls) appear too ([`try_init`](https://docs.rs/tracing-subscriber/0.3.23/tracing_subscriber/util/trait.SubscriberInitExt.html#method.try_init)).

```rust
use anyhow::Context as _;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::util::SubscriberInitExt as _;

/// The project's log format: one JSON object per line.
pub fn subscriber<W>(
    writer: W,
    filter: EnvFilter,
) -> impl tracing::Subscriber + Send + Sync + 'static
where
    W: for<'w> MakeWriter<'w> + Send + Sync + 'static,
{
    tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(true)
        .with_env_filter(filter)
        .with_writer(writer)
        .finish()
}

/// Called once, first thing in `main`, in both binaries.
pub fn init_tracing() -> anyhow::Result<()> {
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env()
        .context("RUST_LOG holds an invalid directive")?;
    subscriber(std::io::stdout, filter)
        .try_init()
        .context("a global tracing subscriber was already set")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing()?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "api starting");
    Ok(())
}
```

`RUST_LOG=info,api=debug,tower_http=debug` raises the project's own crate and tower-http to `debug`. Two traps, both proved by the tests below:

- **A directive list replaces the default.** The builder adds the default directive only when `RUST_LOG` yields no directives at all (`filter/env/builder.rs`, `from_directives`), and a target that matches no directive is off. So `RUST_LOG=api=debug` alone logs `api` at `debug` and **nothing else**, not even a SurrealDB or tower-http warning. Always start the list with a level: `RUST_LOG=info,api=debug` ([`EnvFilter` directives](https://docs.rs/tracing-subscriber/0.3.23/tracing_subscriber/filter/struct.EnvFilter.html#directives)).
- **A disabled span records no fields.** A span below the filter's level is disabled ([`Span::is_disabled`](https://docs.rs/tracing/0.1.44/tracing/struct.Span.html#method.is_disabled)), so under `RUST_LOG=warn` the `INFO` request span is gone, and an `ERROR` line inside it has no `spans` and no `request_id`. The filter must keep `info` for the request span, and an error event that must be found by request records `request_id` as its own field, as `render_errors` does ([rust-axum-api](../rust-axum-api/error-responses.md#1-the-design-one-renderer)).

```rust
use std::io;
use std::sync::{Arc, Mutex};

use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::MakeWriter;

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

impl Captured {
    pub fn lines(&self) -> Vec<serde_json::Value> {
        let bytes = self.0.lock().map(|b| b.clone()).unwrap_or_default();
        String::from_utf8_lossy(&bytes)
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }
}

/// `init_tracing`'s filter, from a given `RUST_LOG` value.
pub fn filter(rust_log: &str) -> Result<EnvFilter, tracing_subscriber::filter::ParseError> {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .parse(rust_log)
}

pub fn subscriber(out: Captured, filter: EnvFilter) -> impl tracing::Subscriber + Send + Sync {
    tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(true)
        .with_env_filter(filter)
        .with_writer(out)
        .finish()
}

fn emit() {
    let span = tracing::info_span!("request", request_id = "req_01JAX4B2Q8");
    let _entered = span.enter();
    tracing::info!(target: "surrealdb", "connected");
    tracing::debug!(target: "api", "loading scan");
    tracing::error!(target: "api", "internal error");
    tracing::error!(target: "api", request_id = "req_01JAX4B2Q8", "internal error, with its id");
}

fn run(rust_log: &str) -> Vec<serde_json::Value> {
    let out = Captured::default();
    let filter = filter(rust_log).expect("the tests' directives are valid");
    tracing::subscriber::with_default(subscriber(out.clone(), filter), emit);
    out.lines()
}

#[test]
fn a_directive_list_replaces_the_default() {
    let targets = |lines: &[serde_json::Value]| -> Vec<String> {
        lines.iter().map(|l| l["target"].to_string()).collect()
    };
    // Only `api`: the SurrealDB line is gone.
    let only_api = run("api=debug");
    assert_eq!(only_api.len(), 3);
    assert!(!targets(&only_api).contains(&r#""surrealdb""#.to_owned()));
    // With a level first, everything at info and `api` at debug.
    assert_eq!(run("info,api=debug").len(), 4);
    // Empty: the default directive, `info`.
    assert_eq!(run("").len(), 3);
}

#[test]
fn warn_drops_the_request_span_and_its_id() {
    let at_info = run("info");
    assert_eq!(at_info[1]["spans"][0]["request_id"], "req_01JAX4B2Q8");
    let at_warn = run("warn");
    assert_eq!(at_warn.len(), 2);
    assert!(
        at_warn[0].get("spans").is_none(),
        "the request span is disabled"
    );
    assert!(at_warn[0].get("request_id").is_none());
    // An event that records the id itself keeps it under any filter.
    assert_eq!(at_warn[1]["request_id"], "req_01JAX4B2Q8");
}
```

## 2. What a line looks like

The test below captures the output of `subscriber` for an event inside two spans and checks the keys. It also shows a trap: a `u128` field, such as `Duration::as_millis()`, is written as a **string** (`"latency_ms":"12"`), so convert to `u64` first.

```rust
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;

/// An in-memory writer for tests.
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

impl Captured {
    pub fn lines(&self) -> Vec<serde_json::Value> {
        let bytes = self.0.lock().map(|b| b.clone()).unwrap_or_default();
        String::from_utf8_lossy(&bytes)
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }
}

pub fn subscriber(writer: Captured) -> impl tracing::Subscriber + Send + Sync + 'static {
    tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(true)
        .with_env_filter(EnvFilter::new("info"))
        .with_writer(writer)
        .finish()
}

#[test]
fn line_shape() {
    let out = Captured::default();
    tracing::subscriber::with_default(subscriber(out.clone()), || {
        let request = tracing::info_span!("request", request_id = "req_01JAX4B2Q8");
        let _r = request.enter();
        let job = tracing::info_span!("store", table = "scan");
        let _j = job.enter();
        let latency = Duration::from_millis(12);
        tracing::info!(
            latency_ms = latency.as_millis(),
            latency_ms_u64 = u64::try_from(latency.as_millis()).unwrap_or(u64::MAX),
            "query done"
        );
        tracing::debug!("filtered out at info");
    });
    let lines = out.lines();
    assert_eq!(lines.len(), 1);
    let line = &lines[0];
    assert_eq!(line["message"], "query done");
    assert_eq!(line["level"], "INFO");
    assert!(
        line.get("fields").is_none(),
        "flatten_event puts fields at the top"
    );
    assert!(line.get("span").is_none(), "with_current_span(false)");
    assert_eq!(line["spans"][0]["request_id"], "req_01JAX4B2Q8");
    assert_eq!(line["spans"][1]["name"], "store");
    assert_eq!(line["latency_ms"], "12", "u128 becomes a string");
    assert_eq!(line["latency_ms_u64"], 12);
    assert!(line["timestamp"].as_str().is_some_and(|t| t.ends_with('Z')));
}
```

The test enters spans with `Span::enter` because it is synchronous code; in async code use `Instrument` ([section 4](#4-spans-across-tokiospawn-and-spawn_blocking)). Fields are `"name"` plus the span's fields; the keys of each object are `timestamp`, `level`, the event's fields and `message`, `target`, `spans` (`fmt/format/json.rs`, `format_event`).

## 3. Logging errors with their chain

[rust-errors](../rust-errors/SKILL.md) owns the error design; std's rule is that a wrapped error is either returned by `source()` or written in the outer `Display`, "but not both" ([`std::error::Error`](https://doc.rust-lang.org/std/error/trait.Error.html)). For logging that means:

- Recording `error = &err as &dyn Error` or `error = %err` writes **only the outer `Display`** in JSON: tracing-serde 0.2.0 has no `record_error`, so tracing-core's default records the `Display` alone, and the sources are lost. (The text formatter appends `error.sources=[…]`; the JSON one doesn't.)
- For an `anyhow::Error`, record `error = format!("{err:#}")`: the alternate `Display` writes the whole chain separated by `: ` ([anyhow display representations](https://docs.rs/anyhow/1.0.104/anyhow/struct.Error.html#display-representations)).
- For a `thiserror` enum, walk `source()` with a small `Display` adapter (below). Because each layer's `Display` leaves its source out, every message appears once.
- Never `?err` (`Debug`) for anyhow: it is the multi-line "Caused by:" report, and with `RUST_BACKTRACE` set, a backtrace.
- Log an error **once**, where it is handled (the `render_errors` middleware for `500`s, the job loop for a failed job), not at every `?`.

```rust
use std::error::Error;
use std::io;
use std::sync::{Arc, Mutex};

use tracing_subscriber::fmt::MakeWriter;

/// A lower layer's error whose `Display` omits its source.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("loading scan")]
    Load(#[source] io::Error),
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

#[test]
fn json_keeps_only_what_you_format() {
    let out = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_writer(out.clone())
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        let err = LoadError::Load(io::Error::other("connection reset"));
        tracing::error!(error = &err as &dyn Error, "as dyn Error");
        tracing::error!(error = %Chain(&err), "with Chain");
        // anyhow only at start-up (rust-errors): a step of `main` that failed.
        let any = anyhow::Error::new(io::Error::other("connection refused"))
            .context("connecting to SurrealDB");
        tracing::error!(error = %any, "anyhow plain");
        tracing::error!(error = format!("{any:#}"), "anyhow alternate");
    });
    let bytes = out.0.lock().unwrap().clone();
    let lines: Vec<serde_json::Value> = String::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines[0]["error"], "loading scan");
    assert_eq!(lines[1]["error"], "loading scan: connection reset");
    assert_eq!(lines[2]["error"], "connecting to SurrealDB");
    assert_eq!(
        lines[3]["error"],
        "connecting to SurrealDB: connection refused"
    );
}
```

## 4. Spans across `tokio::spawn` and `spawn_blocking`

A spawned task starts with no current span. Attach one with `.instrument(span)` or `.in_current_span()` from [`tracing::Instrument`](https://docs.rs/tracing/0.1.44/tracing/trait.Instrument.html#method.in_current_span) ("This can be used to propagate the current span when spawning a new future"). A blocking closure has no future to instrument: capture `Span::current()` and run the closure in [`Span::in_scope`](https://docs.rs/tracing/0.1.44/tracing/struct.Span.html#method.in_scope). With a `TaskTracker` ([rust-async](../rust-async/SKILL.md)) the same applies: `tracker.spawn(fut.in_current_span())`.

Never hold a `Span::enter()` guard across `.await`: "Holding the drop guard returned by `Span::enter` across `.await` points will result in incorrect traces", because another task runs while the span is still entered ([`Span::enter`, in asynchronous code](https://docs.rs/tracing/0.1.44/tracing/struct.Span.html#in-asynchronous-code)). `#[instrument]` on an `async fn` and `Instrument` do it correctly.

```rust
use std::io;
use std::sync::{Arc, Mutex};

use tracing::Instrument as _;
use tracing_subscriber::fmt::MakeWriter;

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

/// A panic in a spawned task comes back as the `JoinError`; the caller handles it.
async fn run_job() -> Result<(), tokio::task::JoinError> {
    tokio::spawn(async { tracing::info!("plain spawn") }).await?;
    tokio::spawn(async { tracing::info!("in_current_span") }.in_current_span()).await?;
    let span = tracing::Span::current();
    tokio::task::spawn_blocking(move || span.in_scope(|| tracing::info!("blocking in_scope"))).await
}

// The default test runtime is current-thread, so the thread-local subscriber sees spawned tasks;
// `spawn_blocking` runs on another thread, so this test sets it for the whole process instead.
#[tokio::test]
async fn spawned_tasks_need_the_span() {
    let out = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_writer(out.clone())
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();
    let span = tracing::info_span!("job", job_id = "01JAX5AAAAAAAAAAAAAAAAAAAA");
    run_job().instrument(span).await.unwrap();
    let text = String::from_utf8(out.0.lock().unwrap().clone()).unwrap();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines[0]["message"], "plain spawn");
    assert!(lines[0].get("spans").is_none(), "the span was lost");
    assert_eq!(lines[1]["spans"][0]["job_id"], "01JAX5AAAAAAAAAAAAAAAAAAAA");
    assert_eq!(lines[2]["spans"][0]["job_id"], "01JAX5AAAAAAAAAAAAAAAAAAAA");
}
```

Clippy can enforce the `.await` rule. [`await_holding_invalid_type`](https://clippy.rust-lang.org/rust-1.99.0/index.html#await_holding_invalid_type) (group `suspicious`, on by default) reads a list of types from `clippy.toml`; with the lines below, holding `span.enter()` across `.await` warns "holding a disallowed type across an await point `tracing::span::Entered`" (checked with clippy 0.1.99). The workspace `clippy.toml` is owned by [rust-cargo-lints](../rust-cargo-lints/SKILL.md):

```toml
await-holding-invalid-types = [
  { path = "tracing::span::Entered", reason = "a span guard across .await gives wrong traces; use tracing::Instrument" },
  { path = "tracing::span::EnteredSpan", reason = "a span guard across .await gives wrong traces; use tracing::Instrument" },
]
```

## 5. Secrets can't be logged by accident

`#[instrument]` without `skip_all` records **every argument with `Debug`** ([`#[instrument]`](https://docs.rs/tracing/0.1.44/tracing/attr.instrument.html)), and a `?value` field does the same. So types that hold a secret (the worker token, a lease token, a signed URL's signature) get a `Debug` that prints nothing of it and no `Display` at all. `%secret` then doesn't compile, and `?secret` prints `LeaseToken(..)`.

```rust
use std::fmt;

use serde::{Deserialize, Serialize};

/// The `X-Lease-Token` value (contracts 5). Sent on the wire, never logged.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LeaseToken(String);

impl LeaseToken {
    /// For the one place that needs the value: building the request header.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for LeaseToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LeaseToken(..)")
    }
}

#[test]
fn debug_hides_the_value() {
    let token: LeaseToken = serde_json::from_str(r#""lt_9f2c41""#).unwrap();
    assert_eq!(format!("{token:?}"), "LeaseToken(..)");
    assert_eq!(token.expose(), "lt_9f2c41");
}
```

The server's own secrets (database passwords, storage keys, the signing secret, API keys, worker tokens) are read at start-up into [`secrecy::SecretString`](https://docs.rs/secrecy/0.10.3/secrecy/type.SecretString.html) (secrecy 0.10.3), the design's type for every configuration secret ([backend README 9](../../../docs/backend/README.md#9-configuration); [backend api 5](../../../docs/backend/api.md#5-configuration)). It works like `LeaseToken` above: `Debug` prints `SecretBox<str>([REDACTED])` (`secrecy-0.10.3/src/lib.rs`), there is no `Display`, and [`ExposeSecret::expose_secret`](https://docs.rs/secrecy/0.10.3/secrecy/trait.ExposeSecret.html#tymethod.expose_secret) is the one way to the value, so a logged config struct shows no secret ([SKILL.md, never in a log](SKILL.md#3-never-in-a-log)). `secrecy` is pinned in [versions.md's third table](../rust/versions.md#pinned-by-the-design-not-built-by-the-check), which the checker doesn't build, so no block here uses it.

A `reqwest::Error` prints its URL, query string included, as ` for url (…)` (`reqwest-0.13.5/src/error.rs`, `Display`). Strip it with [`Error::without_url`](https://docs.rs/reqwest/0.13.5/reqwest/struct.Error.html#method.without_url) before logging, and log the route as its own field:

```rust
#[tokio::test]
async fn reqwest_errors_print_the_url_until_it_is_stripped() {
    // Port 1 refuses the connection at once; the query stands for a signed URL.
    let url = "http://127.0.0.1:1/scans/01JAX/images/01JAY?exp=1760000000&sig=c2VjcmV0";
    let error = reqwest::Client::new().get(url).send().await.unwrap_err();
    assert!(error.to_string().contains("sig=c2VjcmV0"));
    let error = error.without_url();
    assert_eq!(error.to_string(), "error sending request");
    tracing::warn!(error = %error, route = "/scans/{id}/images/{image_id}", "fetch failed");
}
```

```rust,compile_fail,E0277
#[derive(Debug)]
pub struct LeaseToken(String);

pub fn claimed(token: &LeaseToken) {
    // No `Display` for the token, so `%` doesn't compile.
    tracing::info!(lease_token = %token, "claimed");
}
```
