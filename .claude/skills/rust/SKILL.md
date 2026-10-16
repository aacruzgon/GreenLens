---
name: rust
description: "Entry point for the PDM Rust backend (backend/ workspace: api, worker, domain, store, blob…): which rust-* skill covers what, the pinned crate versions, and how to compile-check skill examples. Use before writing or reviewing any Rust, Cargo.toml or backend design."
---

# Rust backend (entry skill)

The backend is a Cargo workspace in `backend/` with the crates `domain`, `store`, `blob`, `integrations`, `agent`, `vision`, `knowledge` and `pipeline` and the binaries `api` and `worker` ([architecture 3](../../../docs/architecture/README.md#rust-crates), [13](../../../docs/architecture/README.md#13-repository-layout); [backend README 5](../../../docs/backend/README.md#5-crate-graph-and-dependency-rules), which adds `integrations` for every outbound HTTP adapter). It is a long-running server in Docker, not a library, so library-only advice (MSRV policy, `#[non_exhaustive]` everywhere, `missing_docs`) doesn't apply.

## The project documents win

Every shape and behaviour the backend produces is fixed by the design documents. When a skill, a crate's docs or habit disagree with them, the documents win, and the disagreement is reported rather than silently resolved.

| Document | Fixes |
|---|---|
| [contracts.md](../../../docs/architecture/contracts.md) | Wire format, IDs, timestamps ([§1](../../../docs/architecture/contracts.md#1-conventions)), the error body and codes ([§2](../../../docs/architecture/contracts.md#2-errors)), SSE events and resume ([§4](../../../docs/architecture/contracts.md#4-scan-events-sse)), the worker API, leases and jobs ([§5](../../../docs/architecture/contracts.md#5-worker-api-and-jobs)) |
| [architecture](../../../docs/architecture/README.md) | Components and crates (§3), flows (§8), API surface (§9), security (§11), deployment (§12) |
| [database](../../../docs/database/README.md) and [schema.surql](../../../backend/migrations/schema.surql) | SurrealDB 3.3.4: every verified write pattern, read and SDK rule |
| [storage](../../../docs/storage/README.md) | RustFS 1.0.1: keys, signed URLs served by the API, writes, deletes, the `aws-sdk-s3` client |
| [frontend](../../../docs/frontend/README.md) | What the app expects from the API: Clerk checks (§8), error handling (§9), streams (§10), CORS (§17) |
| [backend](../../../docs/backend/README.md) | How the Rust code is built: the crate graph and conventions ([README 5, 7](../../../docs/backend/README.md#7-rust-conventions)), every type and port of `domain` ([domain](../../../docs/backend/domain.md)), and one document per crate and binary. The skills follow these documents ([README 7](../../../docs/backend/README.md#7-rust-conventions), "The project's Rust skills") |

Where a skill and [docs/backend](../../../docs/backend/README.md) differ, the skill follows docs/backend and links the section. Once code exists, the Rust types in `domain` become the source of truth for the contracts, and a contract change updates the type, `contracts.md` and, if stored, `schema.surql` in one pull request ([AGENTS.md, sources of truth](../../../AGENTS.md#sources-of-truth)).

## Which skill to use

| Task | Skill |
|---|---|
| Tasks, `spawn` vs `spawn_blocking`, `select!`, cancellation, channels, locks, timeouts, shutdown, async traits | [`/rust-async`](../rust-async/SKILL.md) |
| Routes, extractors (auth and the `Membership<R>` access proofs), `ApiError` → `ErrorBody`, the SSE endpoint, tower-http layers, rate limits, OpenAPI | [`/rust-axum-api`](../rust-axum-api/SKILL.md) |
| `domain` types: serde for the wire format, tagged unions, optional vs nullable, `v: 1`, IDs, timestamps, `ToSchema` and `JsonSchema` | [`/rust-contract-types`](../rust-contract-types/SKILL.md) |
| Error enums, the port errors of `domain::ports`, mapping to contract codes, `anyhow` at the edges, `source()` vs `Display`, panics | [`/rust-errors`](../rust-errors/SKILL.md) |
| Logging setup, spans, `#[instrument]`, request IDs, what never to log | [`/rust-tracing`](../rust-tracing/SKILL.md) |
| The `store` crate: connecting, binds, typed results, transactions, conflict retries, the database doc's write patterns | [`/rust-surrealdb`](../rust-surrealdb/SKILL.md) |
| The `worker` binary: claim, lease, heartbeat, cancel, complete, `ort` sessions, the VLM sidecar client, backpressure | [`/rust-workers`](../rust-workers/SKILL.md) |
| Unit, handler, snapshot, property and database tests; nextest | [`/rust-testing`](../rust-testing/SKILL.md) |
| Workspace layout, `[workspace.dependencies]`, lints, rustfmt, release profile | [`/rust-cargo-lints`](../rust-cargo-lints/SKILL.md) |

Most changes touch several: a new endpoint needs `rust-contract-types` (its types), `rust-axum-api` (the route), `rust-errors` (its failures), `rust-tracing` (its span) and `rust-testing` (its tests).

## Pinned versions

The skills are written against Rust 1.99 (edition 2024) and exact crate versions, listed with what was checked in [versions.md](versions.md), which also holds the backend design's other pins (images, geometry, tokenizers, configuration). The main ones:

| Area | Crates |
|---|---|
| Runtime | `tokio` 1.53.3, `tokio-util` 0.7.20 |
| HTTP server | `axum` 0.8.9, `tower` 0.5.3, `tower-http` 0.7.1, `governor` 0.10.4, `utoipa` 6.0.0, `utoipa-axum` 0.3.0 |
| Types | `serde` 1.0.229, `serde_json` 1.0.154, `schemars` 1.2.3, `chrono` 0.4.45, `ulid` 3.0.0 |
| Errors and logs | `thiserror` 2.0.21, `anyhow` 1.0.104, `tracing` 0.1.44, `tracing-subscriber` 0.3.23 |
| Storage | `surrealdb` 3.3.4 (matches the server; `kv-mem` for tests), `aws-sdk-s3` 1.152.0 |
| Async traits | `async-trait` 0.1.92 |
| Workers | `reqwest` 0.13.5, `ort` 2.0.0-rc.14 (a release candidate; no stable 2.x exists; loaded at run time with `load-dynamic`) |
| Tests | `insta` 1.49.0, `proptest` 1.12.0, `mockall` 0.15.0, `wiremock` 0.6.5, cargo-nextest 0.9.148 |

Several of these are recent majors (`tower-http` 0.7, `utoipa` 6, `reqwest` 0.13, `ulid` 3, `surrealdb` 3, `ort` rc.14). Don't write their APIs from memory: read the source in `~/.cargo/registry/src/*/<crate>-<version>/` or the versioned docs on docs.rs.

## Rules for working with these skills

1. **Docs first.** Read the project section a task touches before the skill. The skills link to it; follow the link.
2. **Every rule has a source.** Each rule in the `rust-*` skills ends with a link to the section it comes from. Allowed sources:
   - the project documents;
   - the official Rust docs: std, the Reference, Cargo, Clippy, rustfmt;
   - a crate's docs on docs.rs at the pinned version, or its repository pinned to a release tag (`blob/utoipa-6.0.0/…`), never `master` or `main`;
   - a vendor's own documentation for its own service: [clerk.com](https://clerk.com/docs) for Clerk, [docs.svix.com](https://docs.svix.com/) for Svix webhooks, and [MDN](https://developer.mozilla.org/en-US/docs/Web/HTTP) for HTTP semantics;
   - books: the [Rust Performance Book](https://nnethercote.github.io/perf-book/), [Effective Rust](https://effective-rust.com/), [Rust Atomics and Locks](https://mara.nl/atomics/). A rule from a book is marked *Judgement*, and only such a rule.

   A claim with none of these behind it is either proved by a test block or flagged as this skill's choice. Nothing comes from third-party skill collections. When adding a rule, open the source first and link it.
3. **Every example compiles.** Run the check below after editing any skill. It must end with `OK: every block passed`.
4. **Versions move together.** To move a crate, change its row in [versions.md](versions.md), run the check, fix the examples it breaks, and re-read the crate's changelog for behaviour the skills describe. The SurrealDB crate stays at the server's version ([database 1](../../../docs/database/README.md#1-version-and-setup)).

## Checking the examples

```bash
python3 .claude/skills/rust/scripts/check_examples.py                     # every rust* skill
python3 .claude/skills/rust/scripts/check_examples.py --skill rust-async --no-test
python3 .claude/skills/rust/scripts/check_examples.py --fix-copies        # refill copy regions
python3 .claude/skills/rust/scripts/check_examples.py --skill rust-surrealdb --live
python3 .claude/skills/rust/scripts/check_examples.py --versions             # every pin builds together
```

Options: `--skill NAME` (repeatable), `--no-test`, `--live`, `--keep` (keep the temporary crate and print its path), `--fix-copies`. `--versions` checks no example: it builds a probe crate with every pin of [versions.md](versions.md), the design's third table included, and runs `cargo generate-lockfile` and `cargo check --all-targets` on it; it needs libheif 1.19 or newer for `libheif-rs` (a minute or so with a warm cache). Run it after changing any version. The script ([scripts/check_examples.py](scripts/check_examples.py), Python 3.11+ standard library) extracts every ```rust block from `.claude/skills/rust*/**/*.md` and writes them into a throwaway crate at `<tmp>/backend/crates/skill-examples`, outside the repository, with the crate versions from [versions.md](versions.md) and the `[workspace.lints]` and `clippy.toml` from [rust-cargo-lints](../rust-cargo-lints/SKILL.md). The real [schema.surql](../../../backend/migrations/schema.surql) is copied to `<tmp>/backend/migrations/schema.surql`, so a test can load it with `include_str!("../../../migrations/schema.surql")`, as a `store` test would. Then it:

1. checks the shared regions (below);
2. runs `rustfmt --check` (edition 2024) on every compiled block, with an empty `rustfmt.toml` so no personal or parent configuration applies;
3. runs `cargo check` and `cargo clippy` on every ```rust block; any warning fails. `RUSTFLAGS`, `CARGO_ENCODED_RUSTFLAGS` and `CARGO_BUILD_RUSTFLAGS` are cleared first, so local flags can't change the result;
4. checks that every ```rust,compile_fail block fails to compile. With a code (```rust,compile_fail,E0277) every error must carry that code and no other;
5. runs every block that contains tests (`#[test]`, `#[tokio::test]`, `#[cfg(test)]`, `proptest!`), with `INSTA_UPDATE=no` and `CI=true`, and prints the tests run, passed and ignored, listing each ignored one;
6. with `--live`, runs the ignored SurrealDB tests too (below).

It fails when the selection holds no ```rust block, or when `--skill` names no `rust*` directory; `--skill rust` alone fails, because this skill has no blocks. There is no baseline of accepted failures. Fence rules:

- The only fences are ```rust, ```rust,ignore, ```rust,compile_fail and ```rust,compile_fail,E0000. Any other info string that starts with `rust` (`rust,no_run`, `rust edition2024`) is an error, and so is a `~~~rust` fence, so no block is skipped silently.
- A block without `fn main` gets `fn main() {}` appended, so write module-level items. Each compiled block starts with `#![allow(dead_code, reason = …)]`, because examples define items they don't call; nothing else is allowed.
- ```rust,ignore is for a deliberately partial fragment. The nearest non-blank line above it must say `Not compiled: <reason>`. Keep these rare.
- The only allowed `#[ignore]` is `#[ignore = "needs SurrealDB"]`, for a test that needs the pinned server image (its users, permissions or WebSocket). Database tests on the embedded `kv-mem` engine are not ignored and run in the plain run. The plain run compiles an ignored test and lists it as ignored; `--live` runs it.
- Blocks may use `ort`, which is built with `load-dynamic` and links everywhere, but no block may build a session: that needs ONNX Runtime at run time ([versions.md, notes](versions.md#notes-and-things-not-confirmed)).

**Shared regions.** A definition that several blocks need (`StoreError`, `ApiError`, `Timestamp`, `Chain` and the others) exists once, as a canonical region, and any other block that needs it carries an exact copy. A region runs from its marker to `// end: <Name>`:

```text
// canonical: rust-errors/mapping.md StoreError     (only in that file)
// copy of: rust-errors/mapping.md StoreError       (in any other block)
...
// end: StoreError
```

| Region | Canonical file | Holds |
|---|---|---|
| `ErrorCode` | [rust-axum-api/error-responses.md](../rust-axum-api/error-responses.md) | The contracts 2 `ErrorCode`, with `ToSchema` for the OpenAPI spec |
| `ApiError` | [rust-axum-api/error-responses.md](../rust-axum-api/error-responses.md) | The `ApiError` enum with every contracts 2 code, its supporting types and methods, `ErrorParts`, `IntoResponse`. A copy also needs `ErrorCode`, `Timestamp` and `Chain` |
| `render_errors` | [rust-axum-api/error-responses.md](../rust-axum-api/error-responses.md) | `ErrorBody`, `render_errors`, `from_status`, `without_allow`, `Panicked`, `panic_response` |
| `ApiJson`, `ApiQuery`, `ApiPath` | [rust-axum-api/error-responses.md](../rust-axum-api/error-responses.md) | The JSON, query and path extractors with `ApiError` rejections |
| `StoreError`, `BlobError`, `BoxError`, `DomainError` | [rust-errors/mapping.md](../rust-errors/mapping.md) | The port errors of `domain::ports` as [backend domain 6](../../../docs/backend/domain.md#port-errors) writes them (`StoreError` with its `Conflict`, `BlobError`), the boxed source they carry, and the `DomainError` variants the examples use. A copy of `StoreError` also needs `BoxError`, `DomainError` and `JobId`/`PlanId` in scope |
| `Chain` | [rust-errors/SKILL.md](../rust-errors/SKILL.md) | The error-chain `Display` adapter (`outer: inner: root`) |
| `retry_conflicts` | [rust-surrealdb/errors-and-retries.md](../rust-surrealdb/errors-and-retries.md) | The store kit: `MAX_ATTEMPTS`, `backoff`, the `thrown` codes, `DbFailure`, `first_failure`, `classify`, `into_store`, `take_failed`, `checked`, `retry_conflicts` and `run_tx`. A copy also needs `StoreError` (with its `BoxError`, `DomainError` and `JobId`/`PlanId`) |
| `Timestamp` | [rust-contract-types/primitives.md](../rust-contract-types/primitives.md) | The whole-second RFC 3339 newtype |
| `MakeReqId` | [rust-tracing/request-id.md](../rust-tracing/request-id.md) | The `req_<ULID>` request-ID maker |
| `V1`, `JobHead` | [rust-contract-types/primitives.md](../rust-contract-types/primitives.md), [unions.md](../rust-contract-types/unions.md) | The `"v": 1` type, and the worker's job-head decoder (`ClaimedJob`, `UnreadableHead`, `decode_claim`) |
| `JobErrorCode` | [rust-contract-types/unions.md](../rust-contract-types/unions.md) | `JobErrorCode` and `JobError`, the `complete` body's failure ([backend domain-types 5](../../../docs/backend/domain-types.md#5-worker-api-and-jobs)) |
| `TestDb` | [rust-testing/database.md](../rust-testing/database.md) | The SurrealDB test harness with the real schema: `TestDb::memory()`, one embedded `kv-mem` datastore per test, and `TestDb::server()`, a namespace per test on the pinned image with the `api` user |
| `Terminate` | [rust-workers/ort.md](../rust-workers/ort.md) | The drop guard and `await_run` that stop an ONNX run when its wait is dropped |

The check fails when a copy differs from its canonical region, when a canonical region names a file other than its own or sits outside a compiled ```rust block, and when a `struct`, `enum`, `fn`, `trait` or `type` with a region's name is defined outside a region of that name (a stand-in with another name, such as `FakeStore`, is fine). To add a copy, write the two empty markers and run `--fix-copies`, which rewrites every copy from its canonical region; never edit inside a copy by hand. Regions use fully qualified paths, so a copy compiles whatever the block imports. Link to the canonical block instead of copying wherever an example doesn't need the definition.

**`--live`** needs docker and cargo-nextest at exactly the version pinned in [versions.md](versions.md#toolchain); another version fails. It starts the `surrealdb/surrealdb` image pinned by digest in [database 1](../../../docs/database/README.md#1-version-and-setup) (in memory, on a random local port, passed to the tests as `PDM_TEST_SURREALDB_URL`), runs `cargo nextest run --run-ignored only` on the blocks with a `needs SurrealDB` test, fails if none ran, and stops the container afterwards. The plain run already runs those blocks' other tests on the embedded engine, which `kv-mem` in the checker's dev-dependencies builds for every block ([rust-testing, database](../rust-testing/database.md#4-running-them)).

The build cache is `~/.cache/pdm-rust-skills-check/target` (or `$PDM_RUST_SKILLS_TARGET_DIR`, which must be outside the repository). The first run downloads and builds every dependency, which takes a few minutes; later runs take under a minute. Never add a `Cargo.toml`, `Cargo.lock` or `target/` under `.claude/`.

## Reviewing backend code

For a change to `backend/`, check in this order and report each finding with the skill rule and its link:

1. **Contract and design.** Do the request, response, event and job shapes match contracts.md byte for byte (field names, optional vs `null`, enum spellings, `v: 1`)? Is every failure one of the codes in contracts §2, with the right status, `retryable` and `details`? Does the code sit in the crate and module docs/backend gives it, with no dependency the crate graph forbids ([backend README 5](../../../docs/backend/README.md#5-crate-graph-and-dependency-rules))?
2. **Authorization.** Does every garden-scoped call take a `Membership<R>` proof from the access extractor, and answer `404` to non-members ([contracts 1, roles](../../../docs/architecture/contracts.md#roles-and-access); [backend domain 8](../../../docs/backend/domain.md#8-authorization-proofs))?
3. **Storage.** Does every write follow its pattern in database §4 and storage §4 (transactions sent as one query, conflict retries, deletion order, never cancelling a RustFS delete)?
4. **Async.** Ports behind `Arc<dyn Port>` with `#[async_trait]`, no blocking work on runtime threads, no lock guard held across `.await`, cancellation-safe `select!` branches, bounded channels and pools, every spawned task tracked for shutdown.
5. **Errors and logs.** No `unwrap()` outside tests, errors logged once with their chain, no secrets, tokens, signed URLs, bodies or locations in logs.
6. **Tests.** Handler tests for each status the endpoint can return, and the lint set passing with `-D warnings`.
