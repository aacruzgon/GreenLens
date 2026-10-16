---
name: rust-errors
description: "Error design for the PDM backend: thiserror enums per crate, ApiError mapped to every contracts 2 code, anyhow only in main and config, Display vs source and logging the chain, panics vs errors, expect, worker job errors. Use when writing or reviewing any error type or Result."
---

# Errors in the PDM backend

This skill is the error design: which type each layer returns, how lower errors become the `ApiError` that maps to [contracts 2](../../../docs/architecture/contracts.md#2-errors), how errors print and log, and how worker jobs report failure. Versions: thiserror 2.0.21, anyhow 1.0.104 ([versions](../rust/versions.md)). **The project documents win**: the codes, statuses, `retryable`, `retry_after_s` and `details` are fixed by contracts 2, worker failures by [contracts 5](../../../docs/architecture/contracts.md#post-internaljobsidcomplete), and `unsupported_version` by [contracts 1, versioning](../../../docs/architecture/contracts.md#versioning). How an `ApiError` becomes the HTTP body (with its `request_id`) is in [rust-axum-api](../rust-axum-api/error-responses.md).

Every example here, in [mapping.md](mapping.md) and in [jobs.md](jobs.md) is compiled, linted and its tests run by `python3 .claude/skills/rust/scripts/check_examples.py --skill rust-errors`.

## Which error type, where

| Layer | Type | Rules |
|---|---|---|
| `domain` | thiserror enums per concern (`TransitionError` for the scan state machine, …) | No I/O errors; values the caller can act on |
| `store` | `StoreError` (thiserror), canonical in [mapping.md](mapping.md) | `NotFound`; `Conflict { what }`, a lost compare-and-set (`THROW 'stale_plan'`, `THROW 'claim_lost'`, a lease check); `TxConflict`, SurrealDB's retryable transaction conflict, which `retry_conflicts` re-runs; `AlreadyExists`; `Thrown(code)`, any other `THROW`; `Unavailable`, the connection; `Query`, anything else. Which SDK error is which is [rust-surrealdb](../rust-surrealdb/SKILL.md#how-failures-arrive)'s |
| `blob` | `BlobError` (thiserror), canonical in [mapping.md](mapping.md) | `Unavailable`, or `Rejected { operation, source }` |
| `agent`, `vision`, `knowledge`, `pipeline` | their own thiserror enums | Same rules as the store |
| `api` handlers | `ApiError` (thiserror + `IntoResponse`) | One variant per contracts 2 code; [mapping.md](mapping.md) |
| `worker` jobs | `JobError` (thiserror) | Becomes `CompleteJob.error`, never an HTTP status ([worker job errors](#worker-job-errors)) |
| `main`, config loading, start-up | `anyhow::Result` | Nowhere else ([anyhow](#anyhow-only-at-the-edges)) |

Library crates of the workspace return concrete enums so callers can match; the binaries' start-up code may use `anyhow` *Judgement*, [Effective Rust, item 4](https://effective-rust.com/errors.html#libraries-versus-applications). `thiserror` derives `Display`, `Error` and `From` and adds nothing to the public API beyond what a hand-written impl would ([thiserror docs](https://docs.rs/thiserror/2.0.21/thiserror/index.html)).

## thiserror

- `#[error("…")]` is the `Display`. Fields are interpolated as `{field}` or `{0}`; extra arguments may use `.field` (`#[error("{}", .problem.message())]`) ([thiserror, details](https://docs.rs/thiserror/2.0.21/thiserror/index.html#details)).
- `#[source]`, or a field named `source`, makes `Error::source()` return it. `#[from]` adds a `From` impl **and** implies `#[source]`; use it only when the conversion needs no context. `#[error(transparent)]` forwards both `Display` and `source()` to the inner error, for a wrapper that adds nothing ([thiserror, details](https://docs.rs/thiserror/2.0.21/thiserror/index.html#details)).
- A variant built at a call that knows what it was doing carries that as **context fields** (`operation`, `scan_id`), so prefer a struct variant with `#[source] source` over `#[from]` there (this skill's choice: the log line then says what failed). Where one `From` classifies every error of a dependency, as `StoreError`'s does, it can't know the call, so the variant has no context field and the function's `#[instrument]` span names it ([mapping.md](mapping.md), design points).
- A source may be `Box<dyn Error + Send + Sync>` when the type isn't fixed (a panic payload, a dependency's opaque error); keep `Send + Sync` so the error can cross tasks: [`tokio::spawn`](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn.html) needs a `Send` output, and `ApiError`'s sources are `Send + Sync` ([rust-axum-api](../rust-axum-api/error-responses.md#2-the-compiled-example)).

## The mapping table

Every contracts 2 code, the `ApiError` variant that produces it, and its fixed properties. `retry_after_s` "with" means the field is always sent, as a whole number of seconds (`u64`): contracts 2 says it comes "with 409 request_in_progress, 429 rate_limited and 503". A `503` with no better number sends `UNAVAILABLE_RETRY_AFTER_S`, 5 s, this skill's choice. The `ApiError` enum is canonical in [rust-axum-api](../rust-axum-api/error-responses.md#2-the-compiled-example), whose test checks every row.

| HTTP | `code` | Variant | `retryable` | `retry_after_s` | `details` |
|---|---|---|---|---|---|
| 400 | `invalid_request` | `InvalidRequest { field, problem }` | false | | `{ field, problem }` |
| 401 | `unauthenticated` | `Unauthenticated` | false | | |
| 403 | `forbidden` | `Forbidden { required_role }` | false | | `{ required_role }` |
| 403 | `invalid_signature` | `InvalidSignature` | false | | |
| 404 | `not_found` | `NotFound` | false | | |
| 409 | `invalid_state` | `InvalidState(InvalidStateDetails)` | false | | `{ status?, allowed?, current_plan_id?, current_map_version?, hint? }` |
| 409 | `request_in_progress` | `RequestInProgress { retry_after_s }` | true | with | |
| 409 | `lease_lost` | `LeaseLost { job_id }` | false | | `{ job_id }` |
| 413 | `upload_too_large` | `UploadTooLarge { max_bytes }` | false | | `{ max_bytes }` |
| 415 | `unsupported_media_type` | `UnsupportedMediaType` | false | | `{ accepted }` |
| 422 | `image_quality` | `ImageQuality { problem, blur_score, exposure }` | true (as in the contracts 2 example) | | `{ problem, blur_score, exposure, hint }` |
| 422 | `missing_required_shot` | `MissingRequiredShot { missing }` | false | | `{ missing }` |
| 422 | `too_many_images` | `TooManyImages { kind, max }` | false | | `{ kind, max }` |
| 422 | `idempotency_conflict` | `IdempotencyConflict` | false | | |
| 429 | `rate_limited` | `RateLimited { retry_after_s }` | true | with | |
| 429 | `budget_exceeded` | `BudgetExceeded { budget, resets_at }` | false | | `{ budget, resets_at }` |
| 500 | `internal` | `Internal(source)` | false | | |
| 503 | `processing_unavailable` | `ProcessingUnavailable { retry_after_s, source }` | true | with | |
| 503 | `upstream_unavailable` | `UpstreamUnavailable { upstream, retry_after_s, source }` | true | with | `{ upstream }` |

Conversions from the lower layers ([mapping.md](mapping.md)):

| From | To |
|---|---|
| `StoreError::NotFound` | `NotFound` (also what a non-member gets, [contracts 1, roles](../../../docs/architecture/contracts.md#roles-and-access)) |
| `StoreError::Unavailable` | `UpstreamUnavailable { upstream: "database", retry_after_s: 5 }` |
| `BlobError::Unavailable` | `UpstreamUnavailable { upstream: "storage", retry_after_s: 5 }` |
| `StoreError::Conflict` | **Mapped by the handler** before `?`: `lease_lost`, `invalid_state` with `current_plan_id` or `current_map_version`, or `request_in_progress`, depending on the call. Reaching the blanket `From` means the handler forgot, so it is `internal` |
| `StoreError::TxConflict` | Retried by `retry_conflicts` ([rust-surrealdb, retrying conflicts](../rust-surrealdb/SKILL.md#retrying-conflicts)); one that survives every attempt is `Internal` |
| `StoreError::AlreadyExists`, `StoreError::Thrown` | Mapped at the call site where the call gives them a meaning (an edge that already exists, a `THROW` code of its own); otherwise `Internal` |
| `StoreError::Query`, `BlobError::Rejected`, anything unexpected | `Internal`, logged with its chain |
| A dependency that is down or timed out (LLM API, weather) | `UpstreamUnavailable { upstream: "llm" \| "weather" }` ([contracts 2](../../../docs/architecture/contracts.md#2-errors): "LLM API, weather or another dependency down") |
| An upload processing step that failed (face blurring) | `ProcessingUnavailable`: the image is not stored (fails closed, [contracts 2](../../../docs/architecture/contracts.md#2-errors)) |

`From` impls hold only the context-free rows; a mapping that depends on the call is a `map_err` at the call site ([mapping.md](mapping.md), `complete`).

## Display and source

- **An error's `Display` shows only its own message, never its source's.** std: "the underlying error should be either returned by the outer error's `Error::source()`, or rendered by the outer error's `Display` implementation, but not both" ([`std::error::Error`](https://doc.rust-lang.org/std/error/trait.Error.html)). With thiserror: don't write `{0}` or `{source}` in the `#[error]` of a variant whose field is the source.
- **Messages of the lower layers** are "concise lowercase sentences without trailing punctuation" ([`std::error::Error`](https://doc.rust-lang.org/std/error/trait.Error.html)): `query failed: load scan`. **`ApiError` is the exception**: its `Display` is the contracts 2 `message`, "human-readable, safe to show", so it is a sentence for a screen, and `Internal`'s never mentions the cause.
- **Logging walks the chain** with [`source()`](https://doc.rust-lang.org/std/error/trait.Error.html#method.source), joining the messages with `": "`: `Chain` below, the one walker. Log the chain once, where the error is handled (the `ApiError` response, the job's `complete`), not at every `?`: this skill's rule, so each failure is one log line. The field layout of the log line is [rust-tracing](../rust-tracing/SKILL.md#6-errors)'s.
- **`anyhow::Error`** prints only the outermost message with `{}`, the whole chain joined by `": "` with `{:#}`, and a "Caused by:" list with `{:?}` ([anyhow, display representations](https://docs.rs/anyhow/1.0.104/anyhow/struct.Error.html#display-representations)).

```rust
// canonical: rust-errors/SKILL.md Chain
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

/// A lower layer's error: its own message, the source kept for `source()`.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("query failed: {what}")]
    Query {
        what: &'static str,
        #[source]
        source: std::io::Error,
    },
}

/// Wrong: the source is both rendered and returned, so a chain prints it twice.
#[derive(Debug, thiserror::Error)]
pub enum Doubled {
    #[error("query failed: {0}")]
    Query(#[source] std::io::Error),
}

#[test]
fn display_is_own_message_and_chain_walks_the_sources() {
    let reset = || std::io::Error::other("connection reset");
    let error = LoadError::Query {
        what: "load scan",
        source: reset(),
    };
    assert_eq!(error.to_string(), "query failed: load scan");
    assert_eq!(
        Chain(&error).to_string(),
        "query failed: load scan: connection reset"
    );

    let doubled = Doubled::Query(reset());
    assert_eq!(
        Chain(&doubled).to_string(),
        "query failed: connection reset: connection reset"
    );
}

#[test]
fn anyhow_formats_choose_how_much_of_the_chain_to_print() {
    use anyhow::Context;
    let result: anyhow::Result<()> =
        Err(std::io::Error::other("no such file")).context("reading api.toml");
    let error = result.unwrap_err();
    assert_eq!(format!("{error}"), "reading api.toml");
    assert_eq!(format!("{error:#}"), "reading api.toml: no such file");
    assert!(format!("{error:?}").contains("Caused by:"));
}
```

## Adding context

- In the workspace crates, context is **fields**: `Rejected { operation: "put_object", source }`, or a span field where one `From` classifies the errors ([thiserror](#thiserror)). A message names the operation, not the inputs a user sent; IDs are fine, request bodies and tokens are not ([rust-tracing, never in a log](../rust-tracing/SKILL.md#3-never-in-a-log)).
- In start-up code, [`anyhow::Context`](https://docs.rs/anyhow/1.0.104/anyhow/trait.Context.html): `.context("…")` for a fixed string, `.with_context(|| format!(…))` when the message is built, so it is only built on failure.
- Don't wrap an error just to say "failed": each level adds what *it* was doing, or passes the error on unchanged with `?` (this skill's rule: a chain of "failed: failed:" says nothing).

## anyhow only at the edges

- `main`, configuration loading and start-up (connect to SurrealDB, sign in, fetch the JWKS, bind the port) return `anyhow::Result` and add context to every step. Returning `Err` from `main` prints the `{:?}` form, with "Caused by:" ([anyhow, display representations](https://docs.rs/anyhow/1.0.104/anyhow/struct.Error.html#display-representations)).
- **Never in a handler's or a crate's public signature.** A handler returns `Result<_, ApiError>`; an `anyhow::Error` there would have no code, no status, and no way to keep its source out of the user's message (*Judgement*, [Effective Rust, item 4](https://effective-rust.com/errors.html#libraries-versus-applications); the codes are [contracts 2](../../../docs/architecture/contracts.md#2-errors)'s).
- Don't `impl From<anyhow::Error> for ApiError`; it would turn every unexpected error into a silent `internal` at `?` (this skill's rule, which follows from the one above).

```rust
use anyhow::Context;

pub struct Config {
    pub surreal_url: String,
    pub web_origins: Vec<String>,
}

/// Start-up only: read from the environment (`.env` is loaded by the container).
pub fn load_config(get: impl Fn(&str) -> Option<String>) -> anyhow::Result<Config> {
    let surreal_url = get("SURREAL_URL").context("SURREAL_URL is not set")?;
    let origins = get("WEB_ORIGINS").context("WEB_ORIGINS is not set")?;
    let web_origins = origins
        .split(',')
        .map(str::trim)
        .map(str::to_owned)
        .collect();
    Ok(Config {
        surreal_url,
        web_origins,
    })
}

#[test]
fn missing_settings_name_themselves() {
    let error = load_config(|_| None).err().unwrap();
    assert_eq!(error.to_string(), "SURREAL_URL is not set");
    let ok = load_config(|key| match key {
        "SURREAL_URL" => Some("ws://surrealdb:8000".to_owned()),
        "WEB_ORIGINS" => Some("https://app.example.org, http://localhost:8081".to_owned()),
        _ => None,
    })
    .unwrap();
    assert_eq!(ok.web_origins.len(), 2);
}
```

## Panics and `expect`

- **A panic is a bug, not an error path.** In `api`, `CatchPanicLayer` turns a handler panic into `500 internal` with an `ErrorBody` ([rust-axum-api](../rust-axum-api/error-responses.md#2-the-compiled-example)) so one bad request doesn't take the connection down; it doesn't make the panic acceptable. A panic in a spawned task ends that task and surfaces as a `JoinError` ([`JoinError::is_panic`](https://docs.rs/tokio/1.53.3/tokio/task/struct.JoinError.html#method.is_panic); [rust-async](../rust-async/SKILL.md)).
- **Anything the client, the database, a worker or a model can send is an error, never a panic**: bad JSON, an unknown enum value from the VLM, a missing record, a malformed depth map. The Book: when failure is expected, "it's more appropriate to return a `Result` than to make a `panic!` call" ([To `panic!` or not to `panic!`](https://doc.rust-lang.org/book/ch09-03-to-panic-or-not-to-panic.html#guidelines-for-error-handling)).
- **No `unwrap()` outside tests** (`clippy::unwrap_used`, [rust-cargo-lints](../rust-cargo-lints/SKILL.md)). For an invariant the code itself guarantees, `expect` with a message saying **why it can't fail**: std recommends that `expect` messages describe the reason you expect the value to be `Ok`/`Some` ([`Result::expect`, recommended message style](https://doc.rust-lang.org/std/result/enum.Result.html#recommended-message-style)).
- Constants are a good place for `expect`: in a `const` item, `NonZeroU32::new(10).expect("10 is not zero")` is evaluated at compile time, and a panic during const evaluation is a compile error, so a mistake fails the build ([Reference, constant evaluation](https://doc.rust-lang.org/reference/const_eval.html); [`Option::expect`](https://doc.rust-lang.org/std/option/enum.Option.html#method.expect) is a `const fn`). The `compile_fail` block below proves it: a zero fails with `E0080`.

```rust
use std::num::NonZeroU32;

/// Evaluated at compile time: a zero here fails the build, not a request.
pub const INVITE_ATTEMPTS_PER_MINUTE: NonZeroU32 = NonZeroU32::new(10).expect("10 is not zero");

/// A `Value`'s maps have string keys, so this can't fail (`serde_json::to_vec`, errors).
pub fn body_bytes(body: &serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(body).expect("a serde_json::Value always serialises")
}
```

A mistake in such a constant stops the build:

```rust,compile_fail,E0080
use std::num::NonZeroU32;

pub const INVITE_ATTEMPTS_PER_MINUTE: NonZeroU32 = NonZeroU32::new(0).expect("0 is not zero");
```

## Worker job errors

A job that fails is **not** an HTTP error. The worker reports it in `POST /internal/jobs/{id}/complete` with `status: "failed"` and `error: { code, message, retryable }`, and the API answers `204` ([contracts 2](../../../docs/architecture/contracts.md#2-errors), last paragraph; [contracts 5, complete](../../../docs/architecture/contracts.md#post-internaljobsidcomplete)). HTTP errors on the worker API mean something about the *call*: `401` for a bad worker token, `409 lease_lost` for a lease that is gone ([rust-axum-api auth 2](../rust-axum-api/auth.md#2-workers-bearer-token-and-x-lease-token)).

- `retryable: true` counts toward the job's 3 attempts; `false` fails the job for good, which for a run's job sends the scan to `failed` with `processing_error` ([contracts 5, leases](../../../docs/architecture/contracts.md#leases-runs-and-attempts)).
- `message` is for operators (the job table doubles as an audit trail, [architecture 12](../../../docs/architecture/README.md#operations)); the user sees the API's `ScanFailure`, never this text. So it is the full chain, `Chain(&error).to_string()`.
- **`unsupported_version`**: every job carries `"v": 1`. A worker that receives a `v` it doesn't know fails the job with this code instead of guessing ([contracts 1, versioning](../../../docs/architecture/contracts.md#versioning)). Read `v` **before** the payload, because a new version may change the payload's shape: the `JobHead` region of [rust-contract-types](../rust-contract-types/unions.md#3-the-worker-checks-v-first) is the one implementation. Only a decoded `v` other than 1 is `unsupported_version`; a body whose head doesn't decode can't be completed at all (no `job_id` or `lease_token` to send), so it is logged and the lease left to expire.
- **Not in the contracts yet:** `unsupported_version` is sent with `retryable: false`, because a retry on the same worker can't succeed ([contracts 12, question 19](../../../docs/architecture/contracts.md#12-open-questions)).
- **Not in the contracts yet:** codes other than `unsupported_version` ("model errors" in contracts 2) aren't listed; `model_error` is this skill's placeholder, and [rust-workers](../rust-workers/SKILL.md) uses `max_runtime_exceeded` ([contracts 12, question 18](../../../docs/architecture/contracts.md#12-open-questions)). The job envelope types themselves are [rust-contract-types](../rust-contract-types/SKILL.md)'s.

The compiled `JobError`, `JobFailure` and claim triage, with tests, are in [jobs.md](jobs.md).

## Review checklist

- [ ] Each crate returns its own thiserror enum; no `anyhow` outside `main`, config and start-up; no `Box<dyn Error>` in a public signature where an enum would let the caller match.
- [ ] No `#[error]` string renders the variant's own source.
- [ ] Variants that wrap an error carry context fields; `#[from]` only where no context is needed.
- [ ] Every `ApiError` variant matches a row of the mapping table; a new contracts 2 code adds a variant, a row and a test case in the same pull request ([AGENTS.md, sources of truth](../../../AGENTS.md#sources-of-truth)).
- [ ] `StoreError::Conflict` is matched by the handler, not left to `?`; `TxConflict` goes through `retry_conflicts`.
- [ ] `Internal` and `Upstream*` keep the source for the log and never show it to the user.
- [ ] Errors are logged once, with the chain, where handled.
- [ ] No `unwrap()` outside tests; every `expect` says why it can't fail.
- [ ] Worker failures go in `complete`'s `error`; `v` is checked before the payload, with `JobHead`; an unreadable head is never reported as `unsupported_version`.

## Sibling skills

[rust](../rust/SKILL.md) · [rust-axum-api](../rust-axum-api/SKILL.md) (`IntoResponse`, `request_id`, rejections) · [rust-tracing](../rust-tracing/SKILL.md) (logging the chain) · [rust-async](../rust-async/SKILL.md) (`JoinError`, cancellation) · [rust-contract-types](../rust-contract-types/SKILL.md) (`ErrorCode`, `CompleteJob`) · [rust-surrealdb](../rust-surrealdb/SKILL.md) (which SDK error is which) · [rust-workers](../rust-workers/SKILL.md) (attempts, `complete`) · [rust-testing](../rust-testing/SKILL.md) · [rust-cargo-lints](../rust-cargo-lints/SKILL.md).
