# Pinned versions for the Rust skills

The `rust*` skills and their examples are written against the versions below. `scripts/check_examples.py` reads the two crate tables in this file and builds every example with exactly these versions (`=x.y.z`). Change a version here, then run the check and fix what breaks.

Checked on 2026-10-15, and again on 2026-10-16 when the crates of the backend design were added. Versions are the newest stable release on crates.io that day (`https://crates.io/api/v1/crates/<name>`, field `max_stable_version`), except where a note says otherwise. Every pin agrees with the backend design's ([backend README 4](../../../docs/backend/README.md#4-stack-and-version-pins)), which takes its versions and features from this file; where the design set a feature, the row says so.

## Toolchain

| Item | Version | Notes |
|---|---|---|
| Rust | 1.99 | Stable. `rustc 1.99.1 (58cf8f9c3 2026-10-13)` is installed; the check compares major and minor only. [Release notes](https://doc.rust-lang.org/releases.html) |
| Edition | 2024 | [Edition guide](https://doc.rust-lang.org/edition-guide/rust-2024/index.html) |
| Clippy | 0.1.99 | Ships with the toolchain. Lint groups in the skills come from the [1.99.0 lint list](https://clippy.rust-lang.org/rust-1.99.0/index.html) |
| cargo-nextest | 0.9.148 | A tool, not a dependency (`cargo install cargo-nextest --version 0.9.148 --locked`). ✓ Installed on 2026-10-16. `check_examples.py --live` runs the SurrealDB tests with it and refuses any other installed version. [nexte.st](https://nexte.st/) |
| cargo-insta | 1.49.0 | A tool, matches `insta`. [insta.rs](https://insta.rs/docs/cli/) |

## Crates

The check puts the first table under `[dependencies]` and the second under `[dev-dependencies]`. A feature cell lists features in backticks; "(default features off)" sets `default-features = false`. Only crates that an example uses are in these two tables; the rest of the design's pins are in [the third table](#pinned-by-the-design-not-built-by-the-check), which the check doesn't read.

### Dependencies

| Crate | Version | Features | Used for |
|---|---|---|---|
| `tokio` | 1.53.3 | `full` | Runtime, tasks, sync primitives, time, signals |
| `tokio-util` | 0.7.20 | `rt` | `CancellationToken`, `TaskTracker` (`rt` is needed for `TaskTracker`) |
| `tokio-stream` | 0.1.19 | `sync` | `BroadcastStream` for the SSE hub |
| `futures` | 0.3.34 | | `Stream`, `StreamExt`, `BoxStream`, `BoxFuture` |
| `async-trait` | 0.1.92 | | `#[async_trait]` on the `domain` ports, which keeps them dyn compatible and mockable ([rust-async 10](../rust-async/SKILL.md#10-async-traits); [backend domain 10](../../../docs/backend/domain.md#dyn-dispatch-and-async-methods)) |
| `async-stream` | 0.3.6 | | `stream!` for SSE streams. Latest release is from 2024-10-01; it is still the current version |
| `axum` | 0.8.9 | `macros`, `multipart` | HTTP server, routing, extractors, SSE |
| `tower` | 0.5.3 | `util`, `timeout`, `limit` | `ServiceBuilder`, `ServiceExt::oneshot` in tests |
| `tower-http` | 0.7.1 | `trace`, `catch-panic`, `cors`, `limit`, `timeout`, `request-id`, `util`, `compression-gzip`, `compression-br` | Middleware layers; the two compression features for `CompressionLayer` ([backend api 7](../../../docs/backend/api.md#7-the-router-and-the-layer-stack)) |
| `http-body-util` | 0.1.5 | | `BodyExt::collect` in handler tests |
| `governor` | 0.10.4 | | Rate limits |
| `jsonwebtoken` | 11.1.0 | `aws_lc_rs`, `use_pem` (default features off) | Clerk session-token verification. Version 11 needs exactly one crypto backend feature (`aws_lc_rs` or `rust_crypto`); `aws-lc-rs` is already in the tree through rustls. `use_pem` reads a PEM key. Its only default feature is `use_pem` (`jsonwebtoken-11.1.0/Cargo.toml`, `[features]`), so turning the defaults off and naming `use_pem` changes nothing today; it keeps the feature set explicit, as the design writes it ([backend integrations 2](../../../docs/backend/integrations.md#2-dependencies)) |
| `serde` | 1.0.229 | `derive` | Wire types |
| `serde_json` | 1.0.154 | `preserve_order` | JSON. `preserve_order` keeps an object's keys in insertion order (`Map` is an `IndexMap`, not a `BTreeMap`): the worker's VLM schemas need property order to be generation order, and the feature unifies across the workspace, so every crate sees it ([backend worker 2](../../../docs/backend/worker.md#2-dependencies); [serde_json features](https://docs.rs/crate/serde_json/1.0.154/features#preserve_order)) |
| `utoipa` | 6.0.0 | `axum_extras`, `chrono` | OpenAPI from the `domain` types. Pulls `utoipa-gen` 6.0.1 |
| `utoipa-axum` | 0.3.0 | | `OpenApiRouter`, `routes!` |
| `schemars` | 1.2.3 | `chrono04` | JSON Schema for the fixed shapes |
| `chrono` | 0.4.45 | `std`, `clock`, `serde` (default features off) | Timestamps (`DateTime<Utc>`), local dates (`NaiveDate`). The database doc binds `chrono::DateTime<Utc>`. The design's feature set ([backend domain 2](../../../docs/backend/domain.md#2-dependencies)) |
| `ulid` | 3.0.0 | `serde` | Record keys |
| `uuid` | 1.28.0 | `v4`, `serde` | `Idempotency-Key` values |
| `thiserror` | 2.0.21 | | Error enums |
| `anyhow` | 1.0.104 | | Start-up and glue errors |
| `tracing` | 0.1.44 | | Spans and events |
| `tracing-subscriber` | 0.3.23 | `env-filter`, `json` | JSON logs, `RUST_LOG` filtering |
| `surrealdb` | 3.3.4 | `protocol-ws`, `rustls` (default features off) | SurrealDB client. The defaults are `protocol-ws`, `rustls` and `parse`; the design drops `parse` (SurrealQL parsing on the client), which nothing needs ([backend store 2](../../../docs/backend/store.md#2-dependencies)) |
| `aws-sdk-s3` | 1.152.0 | `default-https-client`, `rt-tokio`, `http-1x` (default features off) | RustFS client |
| `reqwest` | 0.13.5 | `json`, `stream`, `query` | HTTP client to the VLM sidecar, the LLM API and the worker API. `stream` for `bytes_stream()` (SSE from the LLM API), `query` for `RequestBuilder::query`, which 0.13 puts behind a feature ([backend integrations 2](../../../docs/backend/integrations.md#2-dependencies)) |
| `ort` | 2.0.0-rc.14 | `std`, `ndarray`, `tracing`, `load-dynamic`, `api-29`, `cuda`, `migraphx`, `webgpu` (default features off) | ONNX Runtime on the perception worker and `api`'s CPU models. A release candidate: no stable 2.x exists. `load-dynamic` opens `libonnxruntime` at run time (`ORT_DYLIB_PATH`) instead of linking it, and `api-29` asks for ONNX Runtime API 29 at most, so AMD's 1.29 MIGraphX build loads as well as Microsoft's 1.31. The execution providers are cargo features, chosen at run time by `ORT_EP` (see notes; [backend vision 2](../../../docs/backend/vision.md#2-dependencies)) |
| `ndarray` | 0.17.2 | | Tensors for `ort`. Must be the `ndarray` minor that `ort` uses (0.17) |
| `rand` | 0.10.3 | | Jitter for retries |
| `serde_path_to_error` | 0.1.20 | | The JSON path of a body field that failed to deserialize, read from axum's `JsonRejection` for `invalid_request`'s `details.field`. axum 0.8.9 already depends on it (`^0.1.8`), so this adds no crate to the tree |
| `aws-lc-rs` | 1.18.2 | | Constant-time comparison (`constant_time::verify_slices_are_equal`) of worker tokens after hashing them with SHA-256 (`digest`), and HMAC-SHA256 (`hmac`) for the Svix-signed auth webhook, which Svix says to compare in constant time. Already in the tree through `jsonwebtoken`'s `aws_lc_rs` feature (`^1.15.0`) and rustls |
| `data-encoding` | 2.11.1 | | Base64 for the Svix webhook's `whsec_` secret and `v1,` signatures ([rust-axum-api auth 3](../rust-axum-api/auth.md#3-the-auth-webhook)), as the design's verifier does ([backend integrations 2](../../../docs/backend/integrations.md#2-dependencies)), and for images and the model manifest. Already in the normal tree through `tungstenite`, which `surrealdb`'s `protocol-ws` uses (`cargo tree -e normal -i data-encoding`) |

### Dev-dependencies

| Crate | Version | Features | Used for |
|---|---|---|---|
| `tokio` | 1.53.3 | `full`, `test-util` | `#[tokio::test(start_paused = true)]`, `time::advance` |
| `insta` | 1.49.0 | `json`, `redactions` | Snapshot tests |
| `proptest` | 1.12.0 | | Property tests |
| `mockall` | 0.15.0 | | Mocks for trait boundaries |
| `surrealdb` | 3.3.4 | `protocol-ws`, `rustls`, `kv-mem` (default features off) | The embedded in-memory engine for store tests, one datastore per test ([rust-testing, database](../rust-testing/database.md); [backend store 2](../../../docs/backend/store.md#2-dependencies)). Dev-dependency features apply to every test and example target, so the check builds the engine for all of them (see the compatibility checks) |

### Pinned by the design, not built by the check

The backend design pins these too ([backend README 4](../../../docs/backend/README.md#4-stack-and-version-pins); each crate document's dependency table). No skill example uses them, so the example check doesn't build them; `check_examples.py --versions` does, together with the two tables above. Move a row into a table above when an example needs the crate.

| Crate | Version | Features | Used for (design section) |
|---|---|---|---|
| `bytes` | 1.12.1 | | Image and object bodies ([backend domain 2](../../../docs/backend/domain.md#2-dependencies)) |
| `chrono-tz` | 0.10.4 | | A garden's local date for task due dates ([backend pipeline 2](../../../docs/backend/pipeline.md#2-dependencies)) |
| `secrecy` | 0.10.3 | | `SecretString` for every key and token in the configuration; its `Debug` prints a placeholder ([backend integrations 2](../../../docs/backend/integrations.md#2-dependencies)) |
| `moka` | 0.12.16 | `future` | The JWKS key cache and the geocoder's 30-day cache ([backend integrations 2](../../../docs/backend/integrations.md#2-dependencies)) |
| `tzf-rs` | 2.1.3 | `bundled` (default features off) | Offline coordinates to an IANA zone. The defaults pull `clap` for its command-line tool ([backend integrations 2](../../../docs/backend/integrations.md#2-dependencies)) |
| `regex` | 1.13.1 | | The dose check's patterns and `RegexSet` ([backend agent 2](../../../docs/backend/agent.md#2-dependencies)) |
| `hmac` | 0.13.0 | | Signed image URLs and idempotency fingerprints, with `sha2` ([backend api 2](../../../docs/backend/api.md#2-dependencies)). The Svix webhook uses `aws-lc-rs` instead ([backend integrations 11](../../../docs/backend/integrations.md#11-svix-and-the-auth-webhook)) |
| `aes-gcm` | 0.11.1 | `zeroize` | AES-256-GCM sealing of the farm connectors' vendor tokens under `CONNECTOR_KEY`, the connector id as associated data ([backend api 5](../../../docs/backend/api.md#5-configuration); [connectors 4](../../../docs/connectors/README.md#4-data)). Newest stable on 2026-10-19 (`max_stable_version`, released 2026-08-21); `cargo deny check advisories` passed. `zeroize` wipes the expanded key on drop; the default `getrandom` feature draws each 96-bit nonce from the OS |
| `sha2` | 0.11.1 | | SHA-256 for the HMACs above and the model manifest's checks ([backend vision 2](../../../docs/backend/vision.md#2-dependencies)) |
| `image` | 0.25.10 | `jpeg`, `png`, `webp` (default features off) | Decoding with EXIF orientation, JPEG for the VLM, crops ([backend vision 2](../../../docs/backend/vision.md#2-dependencies)) |
| `imageproc` | 0.27.0 | (default features off) | Laplacian and histogram for the quality gate |
| `fast_image_resize` | 6.1.0 | `image` | SIMD Lanczos3 resizing |
| `png` | 0.18.1 | | 16-bit and 8-bit grayscale PNGs for depth and confidence |
| `webp` | 0.3.1 | (default features off) | Lossy WebP through libwebp, built from source by `libwebp-sys` with the C compiler |
| `libheif-rs` | 3.0.0 | `v1_19` (default features off) | HEIC uploads, behind the `vision` crate's `heic` feature. Needs a system libheif 1.19 or newer, found by `pkg-config` (see notes) |
| `nalgebra` | 0.35.0 | | 3×3 SVD for plane fits |
| `chacha20` | 0.10.2 | `rng` (default features off) | A seeded RNG with a specified stream, for reproducible RANSAC |
| `tokenizers` | 0.23.2 | `fancy-regex` (default features off) | The text embedder's WordPiece tokenizer. The default `onig` feature needs the oniguruma C library, which the design avoids ([backend vision 2](../../../docs/backend/vision.md#2-dependencies)). `1.0.0-rc.2` exists; 0.23.2 is the newest stable |
| `toml` | 1.1.8 | | The model manifest. crates.io lists it as `1.1.8+spec-1.1.0`; Cargo ignores version metadata, so `=1.1.8` matches it ([Cargo, version metadata](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#version-metadata)); ✓ the probe crate resolved `toml = "=1.1.8"` |
| `clap` | 4.6.7 | `derive`, `env` | The worker's `--caps` flag and its settings from the environment ([backend worker 2](../../../docs/backend/worker.md#2-dependencies)) |
| `wiremock` | 0.6.5 | | Dev-dependency: mock HTTP servers for the outbound clients ([backend integrations 2](../../../docs/backend/integrations.md#2-dependencies)) |
| `svix` | 2.8.0 | | Dev-dependency: the test oracle for the hand-written webhook verifier; as a normal dependency it adds about 19 crates ([backend integrations 2](../../../docs/backend/integrations.md#2-dependencies)) |

## Compatibility checks

What was checked, when, and how.

| Question | Finding | How |
|---|---|---|
| Do the crates resolve together? | Yes. On 2026-10-16 a probe crate with **every** pin in the three tables (exact `=x.y.z`, the features above, `kv-mem` and the two dev-only crates under `[dev-dependencies]`) resolved to 754 packages with `cargo generate-lockfile`, and `cargo build --all-targets` compiled all of them on Rust 1.99.1, `aarch64-apple-darwin`, in 1 min 39 s from an empty target directory (621 crates compiled; libheif 1.23.1 from Homebrew). The 2026-10-15 check did the same for the first two tables as they were then | A throwaway crate outside the repository, deleted afterwards. `check_examples.py --versions` now builds the same probe from this file: ✓ on 2026-10-16, 52 dependencies and 7 dev-dependencies, 754 packages, `cargo check --all-targets` in 51 s with a warm cache |
| Does `utoipa-axum` match `axum`? | Yes. `utoipa-axum` 0.3.0 depends on `axum = "0.8.4"` (so 0.8.9 is used) and `utoipa = "6.0.0"`. `cargo tree -i axum-core` shows one `axum` | `utoipa-axum-0.3.0/Cargo.toml` in `~/.cargo/registry` |
| What changed in utoipa 6? | OpenAPI 3.2 support, MSRV 1.88, expression `ignore` removed from `ToSchema` and `IntoParams`, YAML moved to `yaml_serde` | [utoipa CHANGELOG at tag `utoipa-6.0.0`](https://github.com/juhaku/utoipa/blob/utoipa-6.0.0/utoipa/CHANGELOG.md) |
| Does the `surrealdb` 3.3.4 crate support server 3.3.4? | Yes. The SDK checks the server's version on connect against `SUPPORTED_VERSIONS = ">=3.0.0-alpha.1, <4.0.0"` and refuses anything outside it. The database doc pins both at 3.3.4 and moves them together ([database 1](../../../docs/database/README.md#1-version-and-setup)) | `surrealdb-3.3.4/src/lib.rs`, constant `SUPPORTED_VERSIONS` and `check_server_version` |
| What does `kv-mem` cost? | It adds the embedded engine (`surrealdb-core`, `surrealdb-kvs-mem` and their tree). Because Cargo applies dev-dependency features to every test and example target, the check builds the engine for all of them; the measured time is in [rust-testing, database](../rust-testing/database.md) | `cargo tree -e features -i surrealdb` |
| Does `aws-sdk-s3` need its default features? | No. The default `rustls` feature is the legacy client: it pulls `hyper` 0.14 and `rustls` 0.21 as a second HTTP stack. With default features off and `default-https-client`, `rt-tokio`, `http-1x`, the client in [storage 5](../../../docs/storage/README.md#5-rust-client) compiles and `hyper` 0.14 leaves the tree | `cargo tree -i hyper@0.14.32 -e features` before and after |
| Is `aws-config` needed? | No. Storage 5 builds the client from `aws_sdk_s3::config::Builder` with explicit credentials, so the examples don't use `aws-config`. If it is ever needed, the matching version is 1.12.0 | crates.io |
| Duplicate crates worth knowing about | `reqwest` 0.13.5 uses `tower-http` 0.6.11 internally, beside our 0.7.1; no type crosses between them. `surrealdb-types` 3.3.4 uses `ulid` 1.2.1, beside our 3.0.0; the store converts ULIDs to strings for `RecordId::new`, so no type crosses either. `surrealdb-iam` (through `pbkdf2` too) brings `hmac` 0.12.1 and `sha2` 0.10.9 into the **normal** tree, beside our 0.13.0 and 0.11.1; `base64` comes in three versions (0.23.1 from `reqwest` and `hyper-util`, 0.22.1 from `jsonwebtoken`, `pem` and `surrealdb-iam`, 0.13.1 from `tokenizers`' `spm_precompiled`), none of them ours. Only the `kv-mem` engine's `jsonwebtoken` 10.4.0 (from `surrealdb-core`) and `reqwest` 0.12.28 (from `object_store`) are dev-only. No type crosses between any of these | `cargo tree -d`, `cargo tree -e normal -i <crate>@<version>` on the probe crate |

## Notes and things not confirmed

- **`ort` is a release candidate.** `ort` has no stable 2.x release; `2.0.0-rc.14` (2026-10-12, MSRV 1.92) is the newest and describes itself as a wrapper for ONNX Runtime 1.31. Pin it exactly: release candidates may break between each other. In 2.0.0-rc.14, `Session::builder` takes an `&Environment` (from `ort::init().build()`), so code written for other release candidates may not compile.
- **`load-dynamic` instead of `download-binaries`.** The design turns `ort`'s default features off (`std`, `ndarray`, `tracing`, `download-binaries`, `tls-native`, `copy-dylibs` and `api-31`; `ort-2.0.0-rc.14/Cargo.toml`) and enables `load-dynamic` and `api-29`: nothing is linked at build time, and the process opens the vendor's `libonnxruntime` from `ORT_DYLIB_PATH` at its first session. One binary then serves Microsoft's ONNX Runtime 1.31 builds and AMD's 1.29 MIGraphX build, and every build links on any platform, including macOS. The execution providers `cuda`, `migraphx` and `webgpu` are cargo features, picked at run time by `ORT_EP`. With `download-binaries`, `cuda` and `migraphx` can't be enabled together, because pyke publishes no such build ([backend vision 2](../../../docs/backend/vision.md#2-dependencies); [ort docs at tag `v2.0.0-rc.14`, linking](https://github.com/pykeio/ort/blob/v2.0.0-rc.14/docs/content/setup/linking.mdx); `ort-2.0.0-rc.14/Cargo.toml`, `[features]`). So the check builds and links examples and tests that use `ort`, but none of them may build a session, which needs the library at run time. ✓ The probe crate built `ort` with these features on 2026-10-16.
- **ONNX Runtime on the rig.** Microsoft's 1.31 CUDA build needs CUDA 13 and a matching cuDNN ([ort docs at tag `v2.0.0-rc.14`, execution providers, CUDA](https://github.com/pykeio/ort/blob/v2.0.0-rc.14/docs/content/perf/execution-providers.mdx)); architecture 10 asks for "CUDA 12.8+" builds, which 13 satisfies. Whether the libraries run on the rig's cards is a day-1 check on the rig and is *not confirmed* here ([rust-workers, ort.md](../rust-workers/ort.md#2-cuda-on-the-rig)).
- **libheif and libwebp.** `libheif-rs` 3.0.0 with `v1_19` links the system libheif through `pkg-config` and needs version 1.19 or newer; the design's images use `debian:trixie-slim` for it ([backend README 12](../../../docs/backend/README.md#12-builds-and-deployment)). `webp` builds libwebp from source. ✓ Both built here on 2026-10-16 (libheif 1.23.1 from Homebrew), but no example uses them, so the check stays free of native libraries.
- The check resolves a fresh lock file each run. Direct dependencies are exact pins; transitive ones may move to newer semver-compatible releases between runs.
