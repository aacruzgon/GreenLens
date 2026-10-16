# Pinned versions for the Rust skills

The `rust*` skills and their examples are written against the versions below. `scripts/check_examples.py` reads the two crate tables in this file and builds every example with exactly these versions (`=x.y.z`). Change a version here, then run the check and fix what breaks.

Checked on 2026-10-15. Versions are the newest stable release on crates.io that day (`https://crates.io/api/v1/crates/<name>`, field `max_stable_version`), except where a note says otherwise.

## Toolchain

| Item | Version | Notes |
|---|---|---|
| Rust | 1.99 | Stable. `rustc 1.99.1 (58cf8f9c3 2026-10-13)` is installed; the check compares major and minor only. [Release notes](https://doc.rust-lang.org/releases.html) |
| Edition | 2024 | [Edition guide](https://doc.rust-lang.org/edition-guide/rust-2024/index.html) |
| Clippy | 0.1.99 | Ships with the toolchain. Lint groups in the skills come from the [1.99.0 lint list](https://clippy.rust-lang.org/rust-1.99.0/index.html) |
| cargo-nextest | 0.9.148 | A tool, not a dependency (`cargo install cargo-nextest --version 0.9.148 --locked`). ✓ Installed on 2026-10-16. `check_examples.py --live` runs the SurrealDB tests with it and refuses any other installed version. [nexte.st](https://nexte.st/) |
| cargo-insta | 1.49.0 | A tool, matches `insta`. [insta.rs](https://insta.rs/docs/cli/) |

## Crates

The check puts the first table under `[dependencies]` and the second under `[dev-dependencies]`. A feature cell lists features in backticks; "(default features off)" sets `default-features = false`.

### Dependencies

| Crate | Version | Features | Used for |
|---|---|---|---|
| `tokio` | 1.53.3 | `full` | Runtime, tasks, sync primitives, time, signals |
| `tokio-util` | 0.7.20 | `rt` | `CancellationToken`, `TaskTracker` (`rt` is needed for `TaskTracker`) |
| `tokio-stream` | 0.1.19 | `sync` | `BroadcastStream` for the SSE hub |
| `futures` | 0.3.34 | | `Stream`, `StreamExt` |
| `async-stream` | 0.3.6 | | `stream!` for SSE streams. Latest release is from 2024-10-01; it is still the current version |
| `axum` | 0.8.9 | `macros`, `multipart` | HTTP server, routing, extractors, SSE |
| `tower` | 0.5.3 | `util`, `timeout`, `limit` | `ServiceBuilder`, `ServiceExt::oneshot` in tests |
| `tower-http` | 0.7.1 | `trace`, `catch-panic`, `cors`, `limit`, `timeout`, `request-id`, `util` | Middleware layers |
| `http-body-util` | 0.1.5 | | `BodyExt::collect` in handler tests |
| `governor` | 0.10.4 | | Rate limits |
| `jsonwebtoken` | 11.1.0 | `aws_lc_rs` | Clerk session-token verification. Version 11 needs exactly one crypto backend feature (`aws_lc_rs` or `rust_crypto`); `aws-lc-rs` is already in the tree through rustls |
| `serde` | 1.0.229 | `derive` | Wire types |
| `serde_json` | 1.0.154 | | JSON |
| `utoipa` | 6.0.0 | `axum_extras`, `chrono` | OpenAPI from the `domain` types. Pulls `utoipa-gen` 6.0.1 |
| `utoipa-axum` | 0.3.0 | | `OpenApiRouter`, `routes!` |
| `schemars` | 1.2.3 | `chrono04` | JSON Schema for the fixed shapes |
| `chrono` | 0.4.45 | `serde` | Timestamps (`DateTime<Utc>`), local dates (`NaiveDate`). The database doc binds `chrono::DateTime<Utc>` |
| `ulid` | 3.0.0 | `serde` | Record keys |
| `uuid` | 1.28.0 | `v4`, `serde` | `Idempotency-Key` values |
| `thiserror` | 2.0.21 | | Error enums |
| `anyhow` | 1.0.104 | | Start-up and glue errors |
| `tracing` | 0.1.44 | | Spans and events |
| `tracing-subscriber` | 0.3.23 | `env-filter`, `json` | JSON logs, `RUST_LOG` filtering |
| `surrealdb` | 3.3.4 | | SurrealDB client. Default features are `protocol-ws`, `rustls` and `parse` |
| `aws-sdk-s3` | 1.152.0 | `default-https-client`, `rt-tokio`, `http-1x` (default features off) | RustFS client |
| `reqwest` | 0.13.5 | `json` | HTTP client to the VLM sidecar, the LLM API and the worker API |
| `ort` | 2.0.0-rc.14 | `cuda` | ONNX Runtime on the perception worker. A release candidate: no stable 2.x exists (see notes) |
| `ndarray` | 0.17.2 | | Tensors for `ort`. Must be the `ndarray` minor that `ort` uses (0.17) |
| `rand` | 0.10.3 | | Jitter for retries |
| `serde_path_to_error` | 0.1.20 | | The JSON path of a body field that failed to deserialize, read from axum's `JsonRejection` for `invalid_request`'s `details.field`. axum 0.8.9 already depends on it (`^0.1.8`), so this adds no crate to the tree |
| `aws-lc-rs` | 1.18.2 | | Constant-time comparison (`constant_time::verify_slices_are_equal`) of worker tokens after hashing them with SHA-256 (`digest`), and HMAC-SHA256 (`hmac`) for the Svix-signed auth webhook, which Svix says to compare in constant time. Already in the tree through `jsonwebtoken`'s `aws_lc_rs` feature (`^1.15.0`) and rustls |
| `base64` | 0.23.1 | | Decoding the Svix webhook's `whsec_` secret and `v1,` signatures ([rust-axum-api auth 3](../rust-axum-api/auth.md#3-the-auth-webhook)). Already in the tree through `reqwest` 0.13.5 and `hyper-util` (`cargo tree -i base64@0.23.1`), beside 0.22.1 from `jsonwebtoken`, `pem` and `surrealdb-iam`; 0.23.1 is the newest stable on crates.io |

### Dev-dependencies

| Crate | Version | Features | Used for |
|---|---|---|---|
| `tokio` | 1.53.3 | `full`, `test-util` | `#[tokio::test(start_paused = true)]`, `time::advance` |
| `insta` | 1.49.0 | `json`, `redactions` | Snapshot tests |
| `proptest` | 1.12.0 | | Property tests |
| `mockall` | 0.15.0 | | Mocks for trait boundaries |

## Compatibility checks

What was checked on 2026-10-15, and how.

| Question | Finding | How |
|---|---|---|
| Do the crates resolve together? | Yes. `cargo generate-lockfile` and `cargo check` succeed with every pin above on Rust 1.99.1 | A probe crate with these exact pins |
| Does `utoipa-axum` match `axum`? | Yes. `utoipa-axum` 0.3.0 depends on `axum = "0.8.4"` (so 0.8.9 is used) and `utoipa = "6.0.0"`. `cargo tree -i axum-core` shows one `axum` | `utoipa-axum-0.3.0/Cargo.toml` in `~/.cargo/registry` |
| What changed in utoipa 6? | OpenAPI 3.2 support, MSRV 1.88, expression `ignore` removed from `ToSchema` and `IntoParams`, YAML moved to `yaml_serde` | [utoipa CHANGELOG at tag `utoipa-6.0.0`](https://github.com/juhaku/utoipa/blob/utoipa-6.0.0/utoipa/CHANGELOG.md) |
| Does the `surrealdb` 3.3.4 crate support server 3.3.4? | Yes. The SDK checks the server's version on connect against `SUPPORTED_VERSIONS = ">=3.0.0-alpha.1, <4.0.0"` and refuses anything outside it. The database doc pins both at 3.3.4 and moves them together ([database 1](../../../docs/database/README.md#1-version-and-setup)) | `surrealdb-3.3.4/src/lib.rs`, constant `SUPPORTED_VERSIONS` and `check_server_version` |
| Does `aws-sdk-s3` need its default features? | No. The default `rustls` feature is the legacy client: it pulls `hyper` 0.14 and `rustls` 0.21 as a second HTTP stack. With default features off and `default-https-client`, `rt-tokio`, `http-1x`, the client in [storage 5](../../../docs/storage/README.md#5-rust-client) compiles and `hyper` 0.14 leaves the tree | `cargo tree -i hyper@0.14.32 -e features` before and after |
| Is `aws-config` needed? | No. Storage 5 builds the client from `aws_sdk_s3::config::Builder` with explicit credentials, so the examples don't use `aws-config`. If it is ever needed, the matching version is 1.12.0 | crates.io |
| Duplicate crates worth knowing about | `reqwest` 0.13.5 uses `tower-http` 0.6.11 internally, beside our 0.7.1; no type crosses between them. `surrealdb-types` 3.3.4 uses `ulid` 1.2.1, beside our 3.0.0; the store converts ULIDs to strings for `RecordId::new`, so no type crosses either | `cargo tree -d` |

## Notes and things not confirmed

- **`ort` is a release candidate.** `ort` has no stable 2.x release; `2.0.0-rc.14` (2026-10-12, MSRV 1.92) is the newest and describes itself as a wrapper for ONNX Runtime 1.31. Pin it exactly: release candidates may break between each other. Its prebuilt CUDA binaries need CUDA 13.2+ and cuDNN 9.23+, and only a `cuda13` build is published for x86_64 Linux ([ort docs at tag `v2.0.0-rc.14`, execution providers, CUDA](https://github.com/pykeio/ort/blob/v2.0.0-rc.14/docs/content/perf/execution-providers.mdx); `ort-sys-2.0.0-rc.14/build/download/`). Architecture 10 asks for "CUDA 12.8+" builds: 13.2 satisfies that, but a rig image on CUDA 12.x will not work with these binaries. Whether they run on the rig's cards is a day-1 check on the rig and is *not confirmed* here ([rust-workers, ort.md](../rust-workers/ort.md#2-cuda-on-the-rig)). The check builds `ort` with the `cuda` feature on macOS, which compiles the CUDA execution-provider bindings but cannot run them.
- **The `cuda` feature off the rig.** With `ort`'s default `download-binaries` feature, the build script looks for a prebuilt ONNX Runtime matching the requested features. pyke publishes no CUDA build for macOS (on `aarch64-apple-darwin` only `coreml` and `coreml, webgpu` exist), so the script defers the failure to the link step: `cargo check` and `cargo clippy` succeed, and linking a binary that uses `ort` fails with "Builds with these feature sets are available…". ✓ Both were run on 2026-10-15 (`ort-sys-2.0.0-rc.14/build/main.rs`, cfg `link_error_bad_dist_features`). So examples that use `ort` are checked but never linked, and test blocks must not use `ort`. In 2.0.0-rc.14, `Session::builder` takes an `&Environment` (from `ort::init().build()`), so code written for other release candidates may not compile.
- The check resolves a fresh lock file each run. Direct dependencies are exact pins; transitive ones may move to newer semver-compatible releases between runs.
