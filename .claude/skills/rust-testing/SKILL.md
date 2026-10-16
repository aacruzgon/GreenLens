---
name: rust-testing
description: "Tests for the PDM backend: #[tokio::test] and paused time, axum handler tests with oneshot, insta snapshots, proptest, mockall at the domain ports, wiremock for HTTP clients, cargo-nextest, SurrealDB tests on the embedded kv-mem engine. Use when writing, reviewing or running any backend test."
---

# Testing the backend

How to test the `backend/` workspace: async tests and the paused clock, handler tests without a network, snapshots of contract shapes, property tests for the rules in the contracts, mocks at trait boundaries, the test runner and database tests. The tests prove the shapes and behaviours in the project documents, and **the project documents win**: an expected value in a test comes from [contracts §1](../../../docs/architecture/contracts.md#1-conventions) (wire format, confidence bands, regions, idempotency), [§2](../../../docs/architecture/contracts.md#2-errors) (`ErrorBody` and the codes), [§4](../../../docs/architecture/contracts.md#4-scan-events-sse) (SSE) and [§5](../../../docs/architecture/contracts.md#5-worker-api-and-jobs) (leases), never from what the code happens to return.

Versions: tokio 1.53.3 with `test-util` in dev-dependencies, tower 0.5.3, http-body-util 0.1.5, insta 1.49.0 (`json`, `redactions`), proptest 1.12.0, mockall 0.15.0, async-trait 0.1.92, `surrealdb` 3.3.4 with `kv-mem` in dev-dependencies, cargo-nextest 0.9.148, and wiremock 0.6.5, which the design pins but the example checker doesn't build ([versions.md](../rust/versions.md); [backend README 4](../../../docs/backend/README.md#4-stack-and-version-pins)). The backend's test plan is [backend README 11](../../../docs/backend/README.md#11-testing-and-tooling); each crate document ends with its own tests. Claims marked ✓ were run on 2026-10-15 and 2026-10-16 (cargo-insta 1.49.0; cargo-nextest 0.9.148, the pin); the database tests ran on the embedded 3.3.4 engine and against `surrealdb/surrealdb:v3.3.4@sha256:649a60a3…`.

| File | Covers |
|---|---|
| This file | Where tests go, async tests and paused time, snapshots, mocks and fakes, the runner and CI, checklist |
| [handlers.md](handlers.md) | axum handler tests: `oneshot`, several calls with `ready`/`call`, the `ErrorBody` snapshot through the real `render_errors`, reading an SSE stream |
| [mocks.md](mocks.md) | What `domain`'s `mock` and `test-util` features give, mockall at an `#[async_trait]` port behind `Arc<dyn Trait>`, an in-memory `BlobStore` fake with the storage rules |
| [properties.md](properties.md) | proptest for the confidence bands, region boxes, SSE `seq` dedupe and the multipart idempotency fingerprint |
| [database.md](database.md) | SurrealDB tests: the `TestDb` harness, by default one embedded `kv-mem` datastore per test with the real schema; a namespace per test, signed in as `api`, for the few ignored tests against the pinned image |

## 1. Where tests go

- **Unit tests** sit in the module they test, in `#[cfg(test)] mod tests`. **Integration tests** go in `tests/*.rs` of the crate and see only its public API ([Cargo book, tests](https://doc.rust-lang.org/cargo/reference/cargo-targets.html#tests)). Handler tests are a `#[cfg(test)]` module of the `api` binary itself, `bins/api/src/tests.rs`, so they call the real `crate::router::app(state)` and see the crate's private items; there is no `src/lib.rs` ([backend api 3](../../../docs/backend/api.md#3-module-layout), [21](../../../docs/backend/api.md#21-testing); [handlers.md](handlers.md#the-app-under-test)). An integration test in `bins/api/tests/` would see only a library's public API, which a binary-only package doesn't have ([Cargo book, integration tests](https://doc.rust-lang.org/cargo/reference/cargo-targets.html#integration-tests)).
- **Helpers use `expect`.** `allow-unwrap-in-tests` ([clippy.toml](../rust-cargo-lints/SKILL.md#1-layout-and-the-root-manifest)) covers `#[test]` functions and `#[cfg(test)]` modules only ([`unwrap_used`](https://clippy.rust-lang.org/rust-1.99.0/index.html#unwrap_used)). ✓ A helper `fn` in `tests/it.rs` that calls `.unwrap()` is flagged; the same call inside the `#[test]` isn't. Write `expect("why")` in helpers.
- **No `println!` in tests.** `print_stdout` is on and `allow-print-in-tests` is not set ([`print_stdout`](https://clippy.rust-lang.org/rust-1.99.0/index.html#print_stdout)); assert instead.
- A test that needs a service outside the process (the pinned SurrealDB image, RustFS) is `#[ignore = "needs SurrealDB"]` (or the service's name), so `cargo test` passes on a laptop without Docker ([Reference, `ignore`](https://doc.rust-lang.org/reference/attributes/testing.html#the-ignore-attribute); [database.md](database.md#2-the-pinned-image); [backend blob 12](../../../docs/backend/blob.md#12-testing)). Store tests on the embedded engine are not ignored: they need nothing outside the process.

## 2. Async tests and the paused clock

- **`#[tokio::test]` builds a current-thread runtime, one per test** ([`tokio::test`](https://docs.rs/tokio/1.53.3/tokio/attr.test.html#current-thread-runtime)). That is the default for every test here.
- **`flavor = "multi_thread"`** only when the code under test needs worker threads: `block_in_place` panics on a current-thread runtime ([`block_in_place`](https://docs.rs/tokio/1.53.3/tokio/task/fn.block_in_place.html)), and a test of real parallelism needs more than one thread. It needs tokio's `rt-multi-thread` feature (in `full`).
- **`start_paused = true`** starts the runtime with the clock frozen. It needs the `test-util` feature ([`tokio::test`](https://docs.rs/tokio/1.53.3/tokio/attr.test.html#configure-the-runtime-to-start-with-time-paused)) and the current-thread flavour: the macro rejects `multi_thread` with "The `start_paused` option requires the `current_thread` runtime flavor" (✓ `tokio-macros-2.7.2/src/entry.rs`, and the block below).
- **Auto-advance.** While time is paused and the runtime has nothing else to do, the clock jumps to the next timer, so a `sleep`, `interval` or `timeout` of minutes finishes at once and `Instant::elapsed()` reads the exact virtual time ([`pause`, auto-advance](https://docs.rs/tokio/1.53.3/tokio/time/fn.pause.html#auto-advance)). A `spawn_blocking` task holds auto-advance off while it runs.
- **`time::advance(d)`** jumps the clock by `d` without running the expired timers in order; to fire timers in order, `sleep` instead ([`advance`](https://docs.rs/tokio/1.53.3/tokio/time/fn.advance.html)). Use `advance` to stop just before a deadline and check nothing fired yet.
- Paused time only affects `tokio::time`: "the `Instant` in std continues to work as normal" ([`pause`](https://docs.rs/tokio/1.53.3/tokio/time/fn.pause.html)). While a test waits on real I/O (a socket to SurrealDB) the runtime has nothing else to do, so auto-advance jumps the clock to the next timer and a `timeout` around the query fires at once. Don't pause a test that talks to the database, the embedded engine included ([database.md](database.md#1-the-embedded-engine)), or to a mock HTTP server: ✓ in `integrations`' tests auto-advance jumped to the request's timeout while `wiremock` answered ([backend integrations 16](../../../docs/backend/integrations.md#16-testing)).

The heartbeat cadence and lease length come from [contracts §5](../../../docs/architecture/contracts.md#leases-runs-and-attempts) (heartbeat every 10 s, lease 2 minutes). These tests run in milliseconds:

```rust
use std::time::Duration;
use tokio::time::{self, Instant};

/// Sends a heartbeat every 10 s until `stop` resolves; returns how many it sent.
async fn heartbeats(stop: impl Future<Output = ()>) -> u32 {
    let mut ticks = time::interval(Duration::from_secs(10));
    let mut sent = 0;
    tokio::pin!(stop);
    loop {
        tokio::select! {
            () = &mut stop => return sent,
            _ = ticks.tick() => sent += 1,
        }
    }
}

#[tokio::test(start_paused = true)]
async fn auto_advance_runs_35_seconds_at_once() {
    let start = Instant::now();
    let sent = heartbeats(time::sleep(Duration::from_secs(35))).await;
    assert_eq!(sent, 4); // at 0, 10, 20 and 30 s
    assert_eq!(start.elapsed(), Duration::from_secs(35));
}

#[tokio::test(start_paused = true)]
async fn timeout_fires_at_the_deadline() {
    let start = Instant::now();
    let result = time::timeout(Duration::from_secs(120), std::future::pending::<()>()).await;
    assert!(result.is_err());
    assert_eq!(start.elapsed(), Duration::from_secs(120));
}

#[tokio::test(start_paused = true)]
async fn advance_stops_just_before_the_lease_ends() {
    let lease = time::sleep(Duration::from_secs(120));
    tokio::pin!(lease);
    time::advance(Duration::from_secs(119)).await;
    assert!(futures::poll!(&mut lease).is_pending());
    time::advance(Duration::from_secs(1)).await;
    assert!(futures::poll!(&mut lease).is_ready());
}
```

```rust,compile_fail
#[tokio::test(flavor = "multi_thread", start_paused = true)]
async fn paused_clock_needs_current_thread() {}
```

## 3. Handler tests

Build the `Router` with test state and call it as a tower `Service`; no port, no HTTP client ([axum testing example](https://github.com/tokio-rs/axum/blob/axum-v0.8.9/examples/testing/src/main.rs)). [handlers.md](handlers.md) has the compiled tests. The rules:

- `app(state).oneshot(request)` for one request ([`ServiceExt::oneshot`](https://docs.rs/tower/0.5.3/tower/trait.ServiceExt.html#method.oneshot)); `into_service()` then `ready().await?.call(request)` for several requests against the same state ([`ServiceExt::ready`](https://docs.rs/tower/0.5.3/tower/trait.ServiceExt.html#method.ready)).
- Read the body with `BodyExt::collect().await?.to_bytes()` ([`collect`](https://docs.rs/http-body-util/0.1.5/http_body_util/trait.BodyExt.html#method.collect)); read a stream frame by frame with `BodyExt::frame()`.
- For every status an endpoint can return, assert the status, the headers that matter (`content-type`, `x-request-id`, `retry-after`) and the whole [contracts §2](../../../docs/architecture/contracts.md#2-errors) body, with `request_id` redacted and checked against the `x-request-id` header.
- Test the real middleware stack: the request-ID layers ([`rust-tracing`](../rust-tracing/SKILL.md)) and the error mapping ([`rust-axum-api`](../rust-axum-api/SKILL.md)) are part of what produces the body.

## 4. Snapshots (insta)

- **`assert_json_snapshot!(value, { selector => redaction }, @"…")`** serialises the value with serde and compares it with the inline snapshot after `@` ([insta, inline snapshots](https://insta.rs/docs/quickstart/#inline-snapshots); [redactions](https://insta.rs/docs/redactions/#static-redactions)). Selectors: `.error.request_id`, `.items[].id` (every element), `.**.ts` (any depth) ([selectors](https://insta.rs/docs/redactions/#selectors)). ✓ `.**.ts` redacted `ts` at the top level, in a nested object and inside an array element.
- **Redact what changes per run**: ULIDs, timestamps, `request_id`, signed URLs. Use `insta::dynamic_redaction` to check the format before replacing it, so a redaction never hides a wrong value ([dynamic redactions](https://insta.rs/docs/redactions/#dynamic-redactions)).
- **Keys keep their insertion order.** The workspace enables serde_json's `preserve_order` (the worker's VLM schemas need it, and features unify), so a `serde_json::Map` is an `IndexMap` and a `json!` value or a struct serialises its keys in the order they were written, not sorted ([serde_json features](https://docs.rs/crate/serde_json/1.0.154/features#preserve_order); [versions.md](../rust/versions.md#dependencies)). ✓ The snapshot below lists `id`, `name`, `created_at`, the struct's field order. Field order isn't part of the contract, but a snapshot records it, so reordering a struct's fields changes its snapshots.
- **`INSTA_UPDATE`**: `auto` (default) is `no` when `CI` is set and `new` otherwise; `new` writes pending files; `always` writes `.snap` files directly (file snapshots only; inline snapshots always go through a pending file); `unseen` is `always` for new snapshots and `new` for changed ones; `no` writes nothing; `force` rewrites even passing snapshots ([insta, controlling updates](https://insta.rs/docs/advanced/#controlling-snapshot-updating); ✓ `insta-1.49.0/src/env.rs`, `snapshot_update_behavior`).
- **CI.** insta treats `CI` as set unless it is unset, empty, `false` or `0` (✓ `insta-1.49.0/src/utils.rs`, `is_ci`); `TF_BUILD` counts too ([insta, CI](https://insta.rs/docs/quickstart/#continuous-integration)). A mismatch or a missing snapshot always fails the assertion unless the update mode writes the snapshot in place. ✓ With `CI=true` a failing inline snapshot panicked and wrote nothing; without `CI` it panicked and wrote `tests/.handler.rs.pending-snap`.
- **Reviewing.** `cargo insta review` (interactive) or `cargo insta accept` writes pending results into the `.snap` files or the inline `@"…"` strings; `cargo insta test --review` runs and reviews in one go ([cargo-insta](https://insta.rs/docs/cli/#review)). ✓ `cargo insta accept` rewrote the inline snapshot in place. Commit `.snap` files (in `snapshots/` next to the test) with the code; never commit `.snap.new` or `.pending-snap` ([snapshot files](https://insta.rs/docs/snapshot-files/#snapshot-files)).

A pure serde snapshot, with the ULID and timestamp checked before they are redacted:

```rust
use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Serialize)]
struct PlantSummary {
    id: String,
    name: String,
    created_at: DateTime<Utc>,
}

#[test]
fn plant_list_redacts_ids_and_times() {
    let page = serde_json::json!({
        "items": [PlantSummary {
            id: ulid::Ulid::generate().to_string(),
            name: "Tomato 1".to_owned(),
            created_at: Utc::now(),
        }],
    });
    insta::assert_json_snapshot!(page, {
        ".items[].id" => insta::dynamic_redaction(|value, _| {
            let id = value.as_str().expect("id is a string");
            assert!(id.parse::<ulid::Ulid>().is_ok(), "not a ULID: {id}");
            "[ulid]"
        }),
        ".items[].created_at" => insta::dynamic_redaction(|value, _| {
            let ts = value.as_str().expect("timestamp is a string");
            assert!(ts.ends_with('Z'), "not UTC with Z: {ts}");
            "[timestamp]"
        }),
    }, @r#"
    {
      "items": [
        {
          "id": "[ulid]",
          "name": "Tomato 1",
          "created_at": "[timestamp]"
        }
      ]
    }
    "#);
}
```

✓ `chrono` writes `DateTime<Utc>` with a `Z`, and fractional seconds only when the value has them: `Utc::now()` gave `…:05.651263Z`, a whole-second value `2026-10-18T14:03:22Z`. Whether the wire keeps fractions is decided in [`rust-contract-types`](../rust-contract-types/SKILL.md).

## 5. Property tests (proptest)

Use proptest for pure logic with a rule in the contracts, where examples would miss the edges. The backend plans it for the dose check, scoring, the scan state machine and RLE round-trips ([backend README 11](../../../docs/backend/README.md#11-testing-and-tooling)); [properties.md](properties.md) has four compiled and run: the confidence bands (≥ 0.8 high, ≥ 0.5 medium), region boxes in 0..1, SSE `seq` dedupe on resume, the multipart fingerprint ignoring the boundary. The expected value comes from the contract, not from the code: a property that restates the implementation's conditions passes whatever they are, and one that only checks "these two inputs agree" passes for a constant function, so pair it with one that different inputs differ.

- `proptest! { #[test] fn name(x in strategy) { … } }`; assert with `prop_assert!`/`prop_assert_eq!` so failures shrink ([proptest! macro](https://proptest-rs.github.io/proptest/proptest/tutorial/macro-proptest.html)).
- **Cases.** 256 by default; `#![proptest_config(ProptestConfig::with_cases(n))]` inside the macro, or `PROPTEST_CASES` in the environment ([config](https://proptest-rs.github.io/proptest/proptest/tutorial/config.html)).
- **Shrinking.** A failure is reduced to a minimal input before it is reported ([shrinking](https://proptest-rs.github.io/proptest/proptest/tutorial/shrinking-basics.html)). ✓ `x < 500` over `0..1000` reported `minimal failing input: x = 500`.
- **Regressions file.** The failing seed is saved and replayed first on later runs; commit the file ([failure persistence](https://proptest-rs.github.io/proptest/proptest/failure-persistence.html)). ✓ For an integration test `tests/props.rs` it is `tests/props.proptest-regressions`; for unit tests under `src/` it is `proptest-regressions/<path>.txt` beside `src/` (`FileFailurePersistence::SourceParallel`, [docs](https://docs.rs/proptest/1.12.0/proptest/test_runner/enum.FileFailurePersistence.html)). A seed found in CI is printed (`cc …`) but lost with the runner, so copy it into the file.
- Keep exact boundary values (0.8, 0.5) in a plain `#[test]` table too: a float strategy over a range almost never produces one exact value, so a property alone would miss an off-by-one at the boundary ([properties.md](properties.md#confidence-bands)).

## 6. Mocks and fakes

[mocks.md](mocks.md) has a mock and a fake compiled and run.

- **Mock only a port.** The boundaries are the ports in `domain::ports` ([backend domain 9](../../../docs/backend/domain.md#9-boundary-traits); [architecture 2, design principles](../../../docs/architecture/README.md#design-principles)). Don't add a trait just to mock a function.
- **The mocks come from `domain`.** Its `mock` feature generates `Mock<Trait>` for every port except `ChatModel`, and `test-util` gives `FixedClock`, `SequentialIds`, `FakeSigner`, `FixedPhotos` and `ScriptedChat` (for `ChatModel`); a crate's tests enable both in `[dev-dependencies]` ([backend domain 11](../../../docs/backend/domain.md#11-testing)). `api`'s handler tests build their `Store` from one mock per repository trait (`bins/api/src/tests/fake_store.rs`, generated from `domain::ports::store` by `backend/tools/apigen/gen_fake_store.py`, regenerated when a store trait changes; [backend api 3](../../../docs/backend/api.md#3-module-layout), [21](../../../docs/backend/api.md#21-testing)).
- **The shape.** Ports are `#[async_trait]` traits, and the mock attribute goes above it: `#[mockall::automock]` then `#[async_trait::async_trait]`. mockall then sees each `async fn`, `.returning(…)` gives the output value itself, and the mock goes straight into `Arc<dyn Trait>` ([rust-async, traits 3](../rust-async/traits.md#3-what-the-project-does-with-its-boundary-traits); [mockall, async traits](https://docs.rs/mockall/0.15.0/mockall/index.html#async-traits)). `.withf(…)` matches arguments, `.times(n)` counts calls (checked when the mock drops) ([mockall](https://docs.rs/mockall/0.15.0/mockall/index.html#getting-started)).
- **Fakes for state.** When a test needs state across calls, a small in-memory fake with the real rules beats a mock scripted call by call: `blob`'s `MemoryBlobStore` keeps `RustFS`'s (a put never overwrites and is `AlreadyExists`, a missing key is `NotFound`, deleting one counts as deleted; [backend blob 8](../../../docs/backend/blob.md#8-the-in-memory-store); [storage 4](../../../docs/storage/README.md#4-writes-and-deletes)) ([mocks.md](mocks.md#a-fake-for-state)). Database behaviour is never faked: store code runs on the embedded engine ([database.md](database.md)).
- **Outbound HTTP clients** (`integrations`: the LLM API, Open-Meteo, SoilGrids, the geocoders, Clerk, Expo) are tested against `wiremock` servers through the real `reqwest` client, so headers, query strings, retries and SSE bodies are what is under test, plus responses recorded from the live services as fixtures ([backend integrations 1](../../../docs/backend/integrations.md#1-purpose-and-boundaries), [16](../../../docs/backend/integrations.md#16-testing); [wiremock 0.6.5](https://docs.rs/wiremock/0.6.5/wiremock/)). Code above the client mocks the port instead (`fetch_cells` with `MockWeatherProvider`). No example here uses wiremock: it is pinned in [versions.md](../rust/versions.md#pinned-by-the-design-not-built-by-the-check) but not built by the checker.

## 7. Running tests locally and in CI

Use cargo-nextest as the runner. It runs **each test in its own process** ([nextest, why process-per-test](https://nexte.st/docs/design/why-process-per-test/)) (✓ two tests recorded different process IDs; under `cargo test` they shared one), so global state, environment variables and a panicking test can't leak into another test. The other side of that: a `static` set up "once per test binary" runs once per test, all at the same moment, which is why each database test makes its own datastore, or on the server its own namespace ([database.md](database.md#2-the-pinned-image)). Install it with `cargo install cargo-nextest --locked` ([installation from source](https://nexte.st/docs/installation/from-source/)) or a pre-built binary.

```toml
# backend/.config/nextest.toml
[profile.default]
slow-timeout = { period = "30s", terminate-after = 4 }   # SLOW at 30 s, killed at 120 s

[profile.ci]
fail-fast = false        # report every failure, not the first
retries = 0              # a flaky test is a bug to fix, not to retry

[profile.ci.junit]
path = "junit.xml"       # written to target/nextest/ci/junit.xml
```

- The file lives at `.config/nextest.toml` in the workspace root; select a profile with `--profile ci`; every profile inherits from `default` ([configuration](https://nexte.st/docs/configuration/); [reference](https://nexte.st/docs/configuration/reference/)). ✓ Loaded by nextest 0.9.148, and the JUnit file appeared under the workspace's `target/nextest/ci/`.
- **Slow tests.** A test is marked SLOW after `period` (60 s by default) and killed after `terminate-after` periods ([slow tests](https://nexte.st/docs/features/slow-tests/)).
- **Retries.** `--retries n` or `retries` in a profile re-runs a failed test up to n times and marks a later pass FLAKY, which still exits 0 by default ([retries](https://nexte.st/docs/features/retries/)). `flaky-result = "fail"` in the profile (or `--flaky-result fail`) makes a test that only passed on a retry fail the run ([failing flaky tests](https://nexte.st/docs/features/retries/#failing-flaky-tests)). ✓ A test that failed once and then passed exited 0 with `--retries 1` and 100 with `flaky-result = "fail"`. This skill's choice: keep `retries = 0` in CI so flakiness shows as a failure; if CI ever retries, set `flaky-result = "fail"` with it. Use `--retries 2` by hand when investigating.
- **Ignored tests.** `--run-ignored only` runs just the `#[ignore]`d ones, `all` runs both ([running](https://nexte.st/docs/running/#other-runner-options)); with `cargo test` it is `-- --ignored` or `-- --include-ignored`.
- **Doctests.** nextest doesn't run them; run `cargo test --doc` separately ([running](https://nexte.st/docs/running/)). The backend has few, but the step keeps a doc example from rotting.

CI, from `backend/` (lints and formatting are in [`rust-cargo-lints`](../rust-cargo-lints/SKILL.md#4-ci-commands)):

```bash
export CI=true                    # insta: never write snapshots in CI
cargo nextest run --workspace --locked --profile ci   # store tests run on the embedded engine here
cargo test --workspace --locked --doc
# the few server tests: start the pinned image first (database.md, section 2)
PDM_TEST_SURREALDB_URL=ws://127.0.0.1:18000 \
  cargo nextest run --workspace --locked --profile ci --run-ignored only
```

## Review checklist

- [ ] Each endpoint has a handler test for every status it can return, asserting the status, the headers and the full contracts §2 body with `request_id` checked and redacted.
- [ ] Expected values come from contracts.md (codes, field names, thresholds, intervals), not from running the code.
- [ ] Time-based behaviour (heartbeats, leases, keepalives, timeouts, retries) is tested with `start_paused = true`, never with real sleeps.
- [ ] Snapshots are inline or committed `.snap` files; every ULID, timestamp, `request_id` and signed URL is redacted with a format check.
- [ ] Contract rules over a range of inputs have a property test plus exact boundary tests; regressions files are committed.
- [ ] Mocks only at the `domain` ports, from its `mock` feature; stateful boundaries use fakes with the real rules; outbound HTTP clients run against `wiremock`; store queries run on the embedded engine, one datastore per test, multi-threaded where they race.
- [ ] Tests that need a service are `#[ignore = "needs …"]` and read their endpoint from the environment.
- [ ] No `unwrap()` in test helpers outside `#[test]`/`#[cfg(test)]`, no `println!`, no real sleeps.

## Related skills

[`rust`](../rust/SKILL.md) (entry, versions, checker) · [`rust-cargo-lints`](../rust-cargo-lints/SKILL.md) (lints in tests, CI) · [`rust-surrealdb`](../rust-surrealdb/SKILL.md) (store patterns and their live tests) · [`rust-axum-api`](../rust-axum-api/SKILL.md) (`ApiError`, the router, SSE) · [`rust-tracing`](../rust-tracing/SKILL.md) (request IDs) · [`rust-async`](../rust-async/SKILL.md) (trait shapes, cancellation) · [`rust-contract-types`](../rust-contract-types/SKILL.md) (serde output) · [`rust-errors`](../rust-errors/SKILL.md) · [`rust-workers`](../rust-workers/SKILL.md) (lease and heartbeat logic)
