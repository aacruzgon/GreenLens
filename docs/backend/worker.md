# PDM Backend: `worker` binary

> **Status:** Draft v0.5.3, 2026-10-16. v0.5.3: the perception worker reads its thresholds from the manifest with `PerceptionConfig::from_manifest`, the environment sets only the batch caps and never above the manifest's, and start-up refuses an fp16 model on `ORT_EP=cpu` or a file whose tensor names differ from the manifest (gaps G4, G6 and G11); `VLM_BASE_URL` defaults to `http://vllm-nv:8000`, the compose service's name (G7), all from [issue #12](https://github.com/aacruzgon/GreenLens/issues/12). v0.5.2: open question 3 is settled: xgrammar enforces `anyOf` with `null`. v0.5.1: section 12's image path is the registry's, `ghcr.io/aacruzgon/greenlens/worker` ([infra 3](../infra/README.md#3-images)). v0.5: section 15.1 designs stub vision (`--stub-vision`, refused in a release build without `PDM_ALLOW_STUB_VISION`), the canned `perception` result the API accepts, its marks in `model_versions` and logs, the tests, and seven work items for the roadmap's slice S1; section 17 has six new questions; no section number changes. v0.4.2: the rig's compose listing gives each worker an `env_file` with its own `WORKER_TOKEN` (the worker reads no `WORKER_TOKEN_FILE`). v0.4.1: section 17 keeps only the four questions still open, since docs/vision v0.2 and contracts 5–6 now agree on the rest; section 1 drops the image-fetch difference (the skill uses 30 s too); section 12 lists the fallback service with its `stop_grace_period`. v0.4: the SIGTERM shutdown ends within `DRAIN_SECONDS`, dropping a `complete` or heartbeat call in flight (it could take 20 + 10 + 5 = 35 s, past the 30 s grace); the worker API client retries by the error body's `retryable` and `retry_after_s` (contracts 2), not by status, and waits `retry_after_s`; section 1 states the HTTP timeouts and only the two skill differences that remain. v0.3.1: why the immediate re-claim after a `204` can't spin while the API shuts down (probed). v0.3: the hosted fallback gives the guided check the same 8,000-token budget as the other calls (docs/vision README 12 #27), and the fallback row points to the full list of stripped schema keywords. v0.2: the fallback worker reads its hosted-LLM key from `LLM_API_KEY`, the name `api` and the backend README §9 use (it was `ANTHROPIC_API_KEY`). v0.1: the worker process (`--caps perception|vlm|fallback`): configuration, the heartbeat and claim loops with slots, leases and the SIGTERM drain, cancellation by drop, the VLM adapter with its per-call schema builder and decoding, the hosted fallback, per-card settings for the RX 9070 XT, and the rig's compose services; compiled on Rust 1.99.1.

Companion to the [backend architecture](README.md), the [`vision` crate](vision.md) (the perception models and image code it uses), the [`integrations` crate](integrations.md) (its HTTP clients for the VLM server and the LLM API) and the [`domain` crate](domain.md) (the job types and the `VisionModel` port). Every listing is copied from the compiled source of `bins/worker` by the docgen tool. `cargo check -p worker` (also `--features cuda`), `cargo clippy -p worker --all-targets` and `cargo test -p worker` pass: 34 tests, among them the loop on paused time against a fake broker and the VLM adapter against a scripted backend.

## Contents

1. [Purpose and boundaries](#1-purpose-and-boundaries)
2. [Dependencies](#2-dependencies)
3. [Module layout](#3-module-layout)
4. [Configuration](#4-configuration)
5. [The lifecycle state machine](#5-the-lifecycle-state-machine)
6. [Leases, heartbeats and completion](#6-leases-heartbeats-and-completion)
7. [Cancellation](#7-cancellation)
8. [The VLM adapters](#8-the-vlm-adapters)
9. [The per-call schema and decoding](#9-the-per-call-schema-and-decoding)
10. [The fallback worker](#10-the-fallback-worker)
11. [Multi-vendor GPUs](#11-multi-vendor-gpus)
12. [Deployment](#12-deployment)
13. [Concurrency and lifetimes](#13-concurrency-and-lifetimes)
14. [Errors](#14-errors)
15. [Testing](#15-testing)
16. [Decisions on the contracts' open questions](#16-decisions-on-the-contracts-open-questions)
17. [Open questions](#17-open-questions)
18. [Sources](#18-sources)

---

## 1. Purpose and boundaries

One binary, three roles ([architecture 3](../architecture/README.md#rust-crates); [architecture 8.4](../architecture/README.md#84-job-leasing-and-fallback)):

| `--caps` | Runs on | Claims | Model (`Arc<dyn VisionModel>`) | `slots` |
|---|---|---|---|---|
| `perception` | the rig, RTX 3070 | `perception` | `vision::perception::PerceptionPipeline` (ONNX Runtime, CUDA) | 1, batching inside the job |
| `vlm` | the rig, RTX 5070 Ti and RX 9070 XT (one process per card) | `vlm`, `verify`, `guided` | `VlmAdapter<OpenAiBackend>` over its own vLLM or llama.cpp sidecar | `VLM_SLOTS`, 1 or 2, per card |
| `fallback` | the control plane | every kind, 15 s after `queued_at` | `FallbackModel` (`VlmAdapter<AnthropicBackend>` + perception by the LLM) | `FALLBACK_SLOTS`, default 4 |

**What it owns:** the worker side of [contracts 5](../architecture/contracts.md#5-worker-api-and-jobs) (heartbeat, claim, events, complete, image fetch), the job lifecycle, and the VLM side of [contracts 6](../architecture/contracts.md#6-vlm-structured-output) (image preparation, per-call schemas, prompts, validation, alias mapping, box conversion, certainty).

**What it never does:** no database, no object storage, no depth maps (payloads never name one, and no route serves one). It talks only to the API over outbound HTTPS and to its own sidecar or the LLM API, so the rig needs no open ports.

**Dependency direction:** `worker → vision, integrations, domain` ([crate graph](README.md#5-crate-graph-and-dependency-rules)). The VLM adapters live here, not in `vision`, because they are HTTP clients over `integrations`; `vision` stays free of networking.

**HTTP timeouts.** The process has one `reqwest::Client`, built by `integrations::http::client` with a 600 s read timeout instead of its 60 s default ([integrations 4](integrations.md#4-shared-http-client-retries-pacing-sse)). The read timeout also covers the wait for the response headers, and a VLM call isn't streamed: the server sends nothing until the answer is complete. Each call sets its own total deadline, shorter than 600 s, and that deadline is what ends it: 240 s for a VLM call, 300 s for a hosted LLM call, `wait_s + 10` = 35 s for the claim, 30 s for an image, 10 s for `events` and `complete`, 5 s for the heartbeat.

**Where it departs from the project skills.** The `rust-workers` skill's loop (`loop.md`) is the model for `lifecycle` and `job`, with the domain's real types instead of its sketches. The skill follows this document on the `integrations` clients, the 600 s read timeout, the progress sink that cancels a job on `409 lease_lost`, the settled job error codes and the retry rules of section 6. It gives an image fetch 30 s, as `HttpWorkerApi::image` does. One difference remains: the skill backs off after a `204` that comes back in under half of `wait_s`, while the claim loop here claims again at once, because the API stops accepting connections when it shuts down (section 5, with the probe).

---

## 2. Dependencies

```toml
[package]
name = "worker"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

[features]
default = []
# Execution providers of the perception worker's ONNX Runtime, forwarded to `vision`.
cuda = ["vision/cuda"]
migraphx = ["vision/migraphx"]
webgpu = ["vision/webgpu"]
heic = ["vision/heic"]

[dependencies]
domain.workspace = true
integrations.workspace = true
vision.workspace = true

anyhow.workspace = true
async-trait.workspace = true
bytes.workspace = true
clap.workspace = true
rand.workspace = true
reqwest.workspace = true
secrecy.workspace = true
serde.workspace = true
serde_json = { workspace = true, features = ["preserve_order"] }
thiserror.workspace = true
tokio.workspace = true
tokio-util.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true

[dev-dependencies]
domain = { workspace = true, features = ["test-util", "mock"] }
tokio = { workspace = true, features = ["test-util"] }
image.workspace = true
```

| Crate | Why |
|---|---|
| `vision` | The perception pipeline, `OrtRuntime`, the manifest, image decode, resize and JPEG for VLM preparation |
| `integrations` | `openai_compat::OpenAiCompatClient` (vLLM, llama.cpp) and `anthropic::AnthropicClient` with `sanitize_schema`; the shared `reqwest` client builder |
| `domain` | Jobs, results, events, `VisionModel`, `ProgressSink`, the routing and job rules |
| `serde_json` with `preserve_order` | **Property order is generation order** under xgrammar ([vision vlm 8](../vision/vlm.md#8-schemas-from-the-models-side)): `reason` before `answer`, `certainty` last. Without `preserve_order`, `serde_json::Map` is a `BTreeMap` and would sort the keys alphabetically (`answer` before `reason`). Feature unification turns it on for every crate in a build that includes the worker; a test checks the order |
| `reqwest` 0.13.5 | The worker API client |
| `tokio`, `tokio-util` | Tasks, `CancellationToken`, `TaskTracker`, semaphores, paused-time tests |
| `clap` 4.6.7 (`derive`, `env`) | `--caps` and every setting from the environment |
| `secrecy` | Tokens and keys never printed (`WORKER_TOKEN`, `VLM_API_KEY`, `LLM_API_KEY`) |
| `rand` 0.10.3 | Backoff jitter |
| `anyhow` | Start-up errors in `main` only |

---

## 3. Module layout

```
bins/worker/src/
├─ main.rs               tracing, CLI, the shared HTTP client, build_model for --caps, run
├─ config.rs             Cli and its flattened groups, Caps, Timings
├─ api.rs                WorkerApi trait, HttpWorkerApi, JobLease, CallError, classify_body, decode_claim
├─ api/tests.rs          classify_body against contracts 2's error bodies
├─ lifecycle.rs          Backoff, retry_delay, Running registry, Worker<A>: heartbeat, claim loop, drain; run_until_sigterm, run_until
├─ lifecycle/tests.rs    the loop on paused time
├─ job.rs                JobRunner, run_job, complete retries, image fetch, ApiProgress, job_error
├─ vlm.rs                VlmAdapter<B> (VisionModel): analyse, verify, read_guided_check, call_validated
├─ vlm/profile.rs        AdapterProfile (box format, cell, pixel budget), CertaintyMap, CallBudget, model_versions
├─ vlm/backend.rs        VlmBackend, VlmCall<'a>, OpenAiBackend, AnthropicBackend
├─ vlm/prepare.rs        target_size, prepare, prepare_all
├─ vlm/schema.rs         the per-call JSON Schema builder
├─ vlm/prompt.rs         system, findings, indicators, verify and guided prompts
├─ vlm/decode.rs         validation, alias mapping, box conversion, truncation
├─ vlm/tests.rs          the adapter against a scripted backend
└─ fallback.rs           FallbackModel, FallbackLabels: perception on the hosted LLM
```

---

## 4. Configuration

`--caps` picks the role; everything else comes from the environment, so one image serves every card ([architecture 12](../architecture/README.md#12-deployment)).

```rust
/// What this process claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Caps {
    /// `perception` jobs on the ONNX models (RTX 3070).
    Perception,
    /// `vlm`, `verify` and `guided` jobs on a VLM server (RTX 5070 Ti, RX 9070 XT).
    Vlm,
    /// Every kind, 15 seconds after it was queued, on the LLM API (control plane).
    Fallback,
}

/// `worker --caps …`, with the rest from the environment.
#[derive(Debug, Parser)]
#[command(name = "worker", about = "PDM GPU and fallback worker")]
pub struct Cli {
    /// The capability this process claims.
    #[arg(long, value_enum, env = "WORKER_CAPS")]
    pub caps: Caps,
    /// Stable per process, naming the card: `rig-9070xt-vlm-1`.
    #[arg(long, env = "WORKER_ID")]
    pub worker_id: String,
    /// The control plane: `https://api.example.org`.
    #[arg(long, env = "API_BASE_URL")]
    pub api_base_url: String,
    /// The worker's bearer token (`Authorization: Bearer`).
    #[arg(long, env = "WORKER_TOKEN", hide_env_values = true)]
    pub worker_token: SecretString,
    /// Seconds running jobs get to finish after SIGTERM before they are abandoned.
    #[arg(long, env = "DRAIN_SECONDS", default_value_t = 20)]
    pub drain_seconds: u64,
    /// Blocking-pool permits for CPU work (decode, resize, pre-processing).
    #[arg(long, env = "CPU_PERMITS", default_value_t = 2)]
    pub cpu_permits: usize,
    /// Perception settings.
    #[command(flatten)]
    pub perception: PerceptionArgs,
    /// VLM settings.
    #[command(flatten)]
    pub vlm: VlmArgs,
    /// Fallback settings.
    #[command(flatten)]
    pub fallback: FallbackArgs,
}

/// `--caps perception` settings ([vision 5], ONNX Runtime on the 3070).
///
/// [vision 5]: ../../../docs/vision/README.md#5-runtimes-and-versions
#[derive(Debug, Args)]
pub struct PerceptionArgs {
    /// `ml/models/manifest.toml`. Also read by the fallback for its label lists.
    #[arg(long, env = "MODEL_MANIFEST", default_value = "/models/manifest.toml")]
    pub manifest: PathBuf,
    /// `cuda`, `migraphx`, `webgpu` or `cpu`.
    #[arg(long, env = "ORT_EP", default_value = "cuda")]
    pub ort_ep: OrtEp,
    /// The GPU inside the container.
    #[arg(long, env = "ORT_DEVICE_ID", default_value_t = 0)]
    pub ort_device_id: i32,
    /// The `libonnxruntime` to open (`load-dynamic` builds).
    #[arg(long, env = "ORT_DYLIB_PATH")]
    pub ort_dylib_path: Option<PathBuf>,
    /// Threads of the shared intra-op pool.
    #[arg(long, env = "ORT_INTRA_THREADS", default_value_t = 2)]
    pub ort_intra_threads: usize,
    /// The job-wide batch cap.
    #[arg(long, env = "PERCEPTION_MAX_BATCH", default_value_t = 16)]
    pub max_batch: usize,
    /// The leaf detector's batch cap.
    #[arg(long, env = "DETECTOR_MAX_BATCH", default_value_t = 8)]
    pub detector_max_batch: usize,
    /// The SAM encoder's batch cap.
    #[arg(long, env = "SAM_ENCODER_MAX_BATCH", default_value_t = 2)]
    pub sam_encoder_max_batch: usize,
    /// Boxes per SAM decoder call.
    #[arg(long, env = "SAM_MAX_BOXES", default_value_t = 16)]
    pub sam_max_boxes: usize,
    /// `BioCLIP`'s batch cap.
    #[arg(long, env = "BIOCLIP_MAX_BATCH", default_value_t = 16)]
    pub bioclip_max_batch: usize,
}

/// `--caps vlm` settings ([contracts 6]; [vision vlm 4–5]): per worker, so the RTX 5070 Ti and
/// the RX 9070 XT each set their own.
///
/// [vision vlm 4–5]: ../../../docs/vision/vlm.md#4-serving
#[derive(Debug, Args)]
pub struct VlmArgs {
    /// The sidecar, without `/v1`: `http://vllm-nv:8000`.
    #[arg(long, env = "VLM_BASE_URL", default_value = "http://vllm-nv:8000")]
    pub base_url: String,
    /// The sidecar's `--api-key`.
    #[arg(long, env = "VLM_API_KEY", hide_env_values = true)]
    pub api_key: Option<SecretString>,
    /// `vllm` or `llamacpp`.
    #[arg(long, env = "VLM_ADAPTER", value_enum, default_value = "vllm")]
    pub adapter: VlmServerKind,
    /// The served model name (`--served-model-name vlm`).
    #[arg(long, env = "VLM_SERVED_NAME", default_value = "vlm")]
    pub served_name: String,
    /// `vlm_model`: repository@revision of the checkpoint the sidecar serves.
    #[arg(
        long,
        env = "VLM_MODEL",
        default_value = "Qwen/Qwen3-VL-8B-Instruct-FP8"
    )]
    pub model: String,
    /// `vlm_quant`: `fp8_block`, `awq_w4a16`, `gguf_q8_0`.
    #[arg(long, env = "VLM_QUANT", default_value = "fp8_block")]
    pub quant: String,
    /// `vlm_server`: `vllm@v0.31.0`, `vllm-rocm@v0.31.0`, `llama.cpp@b11669`.
    #[arg(long, env = "VLM_SERVER", default_value = "vllm@v0.31.0")]
    pub server: String,
    /// `norm_1000` for the chosen Qwen3-VL-class models.
    #[arg(long, env = "VLM_BOX_FORMAT", value_enum, default_value = "norm1000")]
    pub box_format: BoxFormatArg,
    /// The adapter's cell size: 32 for Qwen3-VL and Qwen3.5, 28 for Qwen2.5-VL.
    #[arg(long, env = "VLM_CELL_PX", default_value_t = 32)]
    pub cell_px: u32,
    /// The largest prepared image, pixels; the same value the server was started with.
    #[arg(long, env = "VLM_MAX_PIXELS", default_value_t = 1_003_520)]
    pub max_pixels: u32,
    /// Jobs held at once; the sidecar's `--max-num-seqs` (or `-np`) has the same value.
    #[arg(long, env = "VLM_SLOTS", default_value_t = 1)]
    pub slots: u32,
}

/// `--caps fallback` settings ([vision vlm 12]).
///
/// [vision vlm 12]: ../../../docs/vision/vlm.md#12-the-fallback-worker
#[derive(Debug, Args)]
pub struct FallbackArgs {
    /// The LLM API key.
    #[arg(long, env = "LLM_API_KEY", hide_env_values = true)]
    pub api_key: Option<SecretString>,
    /// The hosted model.
    #[arg(long, env = "FALLBACK_VISION_MODEL", default_value = "claude-opus-5-5")]
    pub model: String,
    /// Jobs held at once, bounded by the LLM API's rate limit.
    #[arg(long, env = "FALLBACK_SLOTS", default_value_t = 4)]
    pub slots: u32,
    /// Long side of a prepared image: the size the API doesn't resize.
    #[arg(long, env = "FALLBACK_MAX_LONG_SIDE", default_value_t = 1568)]
    pub max_long_side: u32,
    /// Pixels of a prepared image (about 1.15 megapixels, the standard tier).
    #[arg(long, env = "FALLBACK_MAX_PIXELS", default_value_t = 1_150_000)]
    pub max_pixels: u32,
}

/// Fixed timings of the worker loop ([contracts 5]; the `rust-workers` skill).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timings {
    /// Heartbeat period.
    pub heartbeat_every: Duration,
    /// Heartbeat request timeout, far below the 30 s after which the broker requeues.
    pub heartbeat_timeout: Duration,
    /// Claim long poll.
    pub claim_wait_s: u32,
    /// Stop a job this long before its `max_runtime_s`, keeping time for `complete`.
    pub stop_before_max_runtime: Duration,
    /// Bound of the SIGTERM drain.
    pub drain: Duration,
}
```

| Variable | Default | Role | Meaning |
|---|---|---|---|
| `WORKER_ID` | | all | Names the card: `rig-3070-perception-1`, `rig-5070ti-vlm-1`, `rig-9070xt-vlm-1`, `cp-fallback-1`. Sent as `worker_id` everywhere |
| `API_BASE_URL`, `WORKER_TOKEN` | | all | The control plane and the bearer token |
| `DRAIN_SECONDS` | 20 | all | SIGTERM drain bound, and so the bound of the whole shutdown (section 5); keep `stop_grace_period` at least 10 s above it |
| `CPU_PERMITS` | 2 | all | Blocking-pool permits for decode, resize, pre-processing |
| `MODEL_MANIFEST` | `/models/manifest.toml` | perception, fallback | The manifest; the fallback reads its label lists |
| `ORT_EP`, `ORT_DEVICE_ID`, `ORT_DYLIB_PATH`, `ORT_INTRA_THREADS` | `cuda`, 0, —, 2 | perception | Section 11 |
| `PERCEPTION_MAX_BATCH`, `DETECTOR_MAX_BATCH`, `SAM_ENCODER_MAX_BATCH`, `SAM_MAX_BOXES`, `BIOCLIP_MAX_BATCH` | 16, 8, 2, 16, 16 | perception | Caps ([contracts 5, batching](../architecture/contracts.md#results)). They change speed, not results, and none may exceed its model's `max_batch` (`max_boxes`) in the manifest; every threshold comes from the manifest's `[model.config]` ([vision 5](vision.md#5-the-model-manifest)) |
| `VLM_BASE_URL`, `VLM_API_KEY`, `VLM_SERVED_NAME` | `http://vllm-nv:8000`, —, `vlm` | vlm | The sidecar |
| `VLM_ADAPTER` | `vllm` | vlm | `vllm` or `llamacpp` |
| `VLM_MODEL`, `VLM_QUANT`, `VLM_SERVER` | | vlm | `vlm_model`, `vlm_quant`, `vlm_server` in `model_versions` |
| `VLM_BOX_FORMAT`, `VLM_CELL_PX`, `VLM_MAX_PIXELS` | `norm1000`, 32, 1,003,520 | vlm | The adapter instance ([contracts 6](../architecture/contracts.md#box-formats-per-model-adapter)) |
| `VLM_SLOTS` | 1 | vlm | Jobs held at once; the sidecar's `--max-num-seqs` (or `-np`) is the same |
| `LLM_API_KEY`, `FALLBACK_VISION_MODEL` | —, `claude-opus-5-5` | fallback | The hosted model |
| `FALLBACK_SLOTS`, `FALLBACK_MAX_LONG_SIDE`, `FALLBACK_MAX_PIXELS` | 4, 1568, 1,150,000 | fallback | Slots, and the size the API won't resize |
| `WORKER_STUB_VISION`, `PDM_ALLOW_STUB_VISION`, `STUB_*` | off, unset | perception | Canned results for the walking skeleton; refused in a release build without the allow variable ([section 15.1](#151-stub-vision), proposed) |

Start-up fails before the first claim on a missing key, a missing `[model.config]` threshold, a batch cap above the manifest's, an ONNX file whose tensor names differ from its entry, an fp16 model with `ORT_EP=cpu`, a file whose hash differs from the manifest, a provider that isn't compiled in or doesn't register, or a VLM server that doesn't serve `VLM_SERVED_NAME` (`GET /v1/models`).

```rust
/// The model for `--caps`, loaded before the first claim so a missing driver, file or key fails
/// start-up instead of a job.
async fn build_model(
    cli: &Cli,
    http: reqwest::Client,
    cpu: Arc<Semaphore>,
) -> anyhow::Result<Arc<dyn VisionModel>> {
    Ok(match cli.caps {
        Caps::Perception => {
            let p = &cli.perception;
            let manifest = Manifest::load(&p.manifest).context("MODEL_MANIFEST")?;
            let runtime = OrtRuntime::init(&RuntimeConfig {
                ep: p.ort_ep,
                device_id: p.ort_device_id,
                dylib_path: p.ort_dylib_path.clone(),
                intra_threads: p.ort_intra_threads,
                name: "perception-worker",
            })?;
            // Thresholds come from the manifest's [model.config] (PerceptionPipeline::load reads
            // them); the environment sets only the caps, never above the manifest's.
            let caps = BatchCaps {
                job: p.max_batch,
                detector: p.detector_max_batch,
                sam_encoder: p.sam_encoder_max_batch,
                sam_boxes: p.sam_max_boxes,
                bioclip: p.bioclip_max_batch,
            };
            Arc::new(PerceptionPipeline::load(runtime, &manifest, caps, cli.cpu_permits).await?)
        }
        Caps::Vlm => {
            let v = &cli.vlm;
            let client = OpenAiCompatClient::new(
                http,
                OpenAiCompatConfig {
                    base_url: v.base_url.clone(),
                    api_key: v.api_key.clone(),
                    timeout: Duration::from_secs(240),
                    flavour: v.adapter.flavour(),
                    retry: RetryPolicy::NONE,
                },
            );
            let served = client
                .models()
                .await
                .context("the VLM server's /v1/models")?;
            anyhow::ensure!(
                served.iter().any(|m| m == &v.served_name),
                "the VLM server serves {served:?}, not {}",
                v.served_name
            );
            let profile = AdapterProfile {
                box_format: v.box_format.format(),
                cell_px: v.cell_px,
                max_pixels: v.max_pixels,
                max_long_side: None,
                jpeg_quality: 90,
                budget: CallBudget::LOCAL,
                certainty: CertaintyMap::default(),
                identity: VlmIdentity {
                    model: v.model.clone(),
                    quant: Some(v.quant.clone()),
                    server: v.server.clone(),
                },
            };
            Arc::new(VlmAdapter::new(
                OpenAiBackend::new(client, v.served_name.clone()),
                profile,
                cpu,
            ))
        }
        Caps::Fallback => {
            let f = &cli.fallback;
            let key = f.api_key.clone().context("LLM_API_KEY")?;
            let mut config = AnthropicConfig::new(key);
            config.retry = RetryPolicy::NONE;
            config.request_timeout = Duration::from_secs(300);
            config.models.vision_fallback.clone_from(&f.model);
            let client = AnthropicClient::new(http, config);
            let profile = AdapterProfile {
                box_format: domain::vlm::BoxFormat::AbsPxPrepared,
                cell_px: 1,
                max_pixels: f.max_pixels,
                max_long_side: Some(f.max_long_side),
                jpeg_quality: 90,
                budget: CallBudget::HOSTED,
                certainty: CertaintyMap::default(),
                identity: VlmIdentity {
                    model: f.model.clone(),
                    quant: None,
                    server: "anthropic".to_owned(),
                },
            };
            let manifest = Manifest::load(&cli.perception.manifest).context("MODEL_MANIFEST")?;
            let labels = fallback::FallbackLabels::load(&manifest)?;
            let adapter =
                VlmAdapter::new(AnthropicBackend::new(client, f.model.clone()), profile, cpu);
            Arc::new(fallback::FallbackModel::new(adapter, labels))
        }
    })
}
```

---

## 5. The lifecycle state machine

```
             ┌──────────── heartbeat task (own token, stops last) ─────────────┐
start ──▶ claiming ──claim 200──▶ job task: fetching ──▶ running ──▶ completing ──▶ done
  │        ▲  │204: poll again        │cancel_job_ids / lease_lost ──▶ dropped (nothing sent)
  │        │  └─error: back off ─┘    │soft deadline (max_runtime − 15 s) ──▶ failed, retryable
  │        │                          │unsupported v / unreadable job ──▶ failed, not retryable
  │  SIGTERM ─▶ draining: no claims, jobs get DRAIN_SECONDS ──▶ abandoned (nothing sent) ──▶ exit
  └─ load model (fail fast)
```

`Worker<A>` holds the API client, the `JobRunner`, the registry of held jobs, the slot semaphore, a `TaskTracker` for job tasks and the `abandon` token that parents every job's token.

```rust
/// Who this worker is, sent in every heartbeat and claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// `worker_id`.
    pub worker_id: WorkerId,
    /// `caps`.
    pub caps: Vec<Capability>,
    /// `slots`.
    pub slots: u32,
}

/// The worker. Generic over the API (`HttpWorkerApi`, or a fake in tests) for static dispatch;
/// the model is a trait object chosen at start from `--caps`.
pub struct Worker<A: WorkerApi> {
    api: Arc<A>,
    runner: Arc<JobRunner>,
    identity: Arc<Identity>,
    running: Running,
    slots: Arc<Semaphore>,
    tracker: TaskTracker,
    abandon: CancellationToken,
    timings: Timings,
}
```
```rust
impl<A: WorkerApi> Worker<A> {
    /// A worker that runs jobs on `model`.
    pub fn new(api: A, model: Arc<dyn VisionModel>, identity: Identity, timings: Timings) -> Self {
        let slots = Arc::new(Semaphore::new(identity.slots as usize));
        let runner = Arc::new(JobRunner::new(model, identity.worker_id.clone()));
        Self {
            api: Arc::new(api),
            runner,
            identity: Arc::new(identity),
            running: Running::default(),
            slots,
            tracker: TaskTracker::new(),
            abandon: CancellationToken::new(),
            timings,
        }
    }

    /// The heartbeat's `versions`: the model's keys plus `worker_id`.
    fn versions(&self) -> BTreeMap<String, String> {
        self.runner.versions()
    }

    /// Every 10 s until `stop`: list the running jobs, cancel what the broker says to cancel.
    /// `Delay` sends one catch-up beat after a slow call instead of a burst of stale ones.
    /// `stop` also drops a call in flight, so the loop ends at once. A failed beat is logged,
    /// never retried early: the next one is 10 s away, and the broker waits 30 s.
    pub async fn heartbeat_loop(&self, stop: CancellationToken) {
        let mut every = tokio::time::interval(self.timings.heartbeat_every);
        every.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                () = stop.cancelled() => return,
                _ = every.tick() => {}
            }
            let hb = Heartbeat {
                worker_id: self.identity.worker_id.clone(),
                caps: self.identity.caps.clone(),
                slots: self.identity.slots,
                gpu: None,
                current_jobs: self.running.current_jobs(),
                versions: self.versions(),
            };
            let call =
                tokio::time::timeout(self.timings.heartbeat_timeout, self.api.heartbeat(&hb));
            let answer = tokio::select! {
                () = stop.cancelled() => return,
                answer = call => answer,
            };
            match answer {
                Ok(Ok(resp)) => self.running.cancel(&resp.cancel_job_ids),
                Ok(Err(e)) => tracing::warn!(error = %e, "heartbeat failed"),
                Err(_) => tracing::warn!("heartbeat timed out"),
            }
        }
    }

    /// Claims while a slot is free, until `shutdown`. A slot is taken before each claim, so
    /// the worker never polls while full and never holds more than `slots` jobs.
    pub async fn claim_loop(&self, shutdown: CancellationToken) {
        let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(30));
        loop {
            let slot = tokio::select! {
                biased;
                () = shutdown.cancelled() => return,
                slot = Arc::clone(&self.slots).acquire_owned() => match slot {
                    Ok(slot) => slot,
                    Err(_) => return, // the semaphore is never closed; stop if it ever is
                },
            };
            let req = ClaimRequest {
                worker_id: self.identity.worker_id.clone(),
                caps: self.identity.caps.clone(),
                slots: self.identity.slots,
                wait_s: Some(self.timings.claim_wait_s),
            };
            let claimed = tokio::select! {
                biased;
                () = shutdown.cancelled() => return, // drops the open long poll
                claimed = self.api.claim(&req) => claimed,
            };
            match claimed {
                Ok(Some(job)) => {
                    backoff.reset();
                    self.start(job, slot);
                }
                Ok(None) => backoff.reset(),
                Err(e) => {
                    drop(slot);
                    let wait = retry_delay(&e, &mut backoff);
                    tracing::warn!(error = %e, wait_ms = wait.as_millis(), "claim failed");
                    tokio::select! {
                        () = shutdown.cancelled() => return,
                        () = tokio::time::sleep(wait) => {}
                    }
                }
            }
        }
    }

    /// Registers the job and spawns its task. The task owns everything it uses (`Arc` clones,
    /// the job, the permit, the registry guard): `TaskTracker::spawn` needs a `'static + Send`
    /// future, because the task may outlive this call and run on any runtime thread.
    fn start(&self, claimed: ClaimedJob, slot: OwnedSemaphorePermit) {
        let lease = match &claimed {
            ClaimedJob::Job(job) => JobLease::from(job.as_ref()),
            ClaimedJob::Unsupported(head) => JobLease {
                job_id: head.job_id,
                lease_token: head.lease_token.clone(),
            },
        };
        let cancel = self.abandon.child_token();
        let registered = self.running.register(lease, cancel.clone());
        let (api, runner, timings) = (
            Arc::clone(&self.api),
            Arc::clone(&self.runner),
            self.timings,
        );
        self.tracker.spawn(async move {
            run_job(&api, &runner, claimed, &cancel, timings).await;
            drop((registered, slot)); // leaves the heartbeat list, then frees the slot
        });
    }

    /// After `shutdown`: wait up to the drain bound for running jobs, then abandon the rest
    /// (nothing is sent; the broker requeues them 30 s after the last heartbeat). Every await
    /// in a job task, a `complete` call in flight included, selects on the job's token, so
    /// the second wait ends at once: the drain never outlasts its bound.
    pub async fn drain(&self) {
        self.tracker.close();
        if tokio::time::timeout(self.timings.drain, self.tracker.wait())
            .await
            .is_err()
        {
            tracing::warn!(jobs = self.tracker.len(), "abandoning jobs at shutdown");
            self.abandon.cancel();
            self.tracker.wait().await;
        }
    }

    /// The registry, for tests.
    #[cfg(test)]
    pub fn running(&self) -> &Running {
        &self.running
    }
}
```
```rust
/// The process: heartbeat and claims until SIGTERM or Ctrl-C, then [`run_until`]'s shutdown.
pub async fn run_until_sigterm<A: WorkerApi>(worker: Arc<Worker<A>>) -> std::io::Result<()> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut sigterm = signal(SignalKind::terminate())?;
    let stopped = async move {
        tokio::select! {
            _ = sigterm.recv() => Ok(()),
            r = tokio::signal::ctrl_c() => r,
        }
    };
    run_until(worker, stopped).await
}

/// Heartbeat and claims until `stopped`, then stop claiming, drain, stop heartbeating. The
/// heartbeat outlives the claim loop so draining jobs keep their leases. From `stopped` to the
/// return is at most the drain bound: the claim loop and the heartbeat drop a call in flight,
/// and the drain cuts every job at its bound.
pub async fn run_until<A, S>(worker: Arc<Worker<A>>, stopped: S) -> std::io::Result<()>
where
    A: WorkerApi,
    S: Future<Output = std::io::Result<()>>,
{
    let (shutdown, stop_heartbeat) = (CancellationToken::new(), CancellationToken::new());
    let heartbeat = tokio::spawn({
        let (worker, stop) = (Arc::clone(&worker), stop_heartbeat.clone());
        async move { worker.heartbeat_loop(stop).await }
    });
    let claims = tokio::spawn({
        let (worker, shutdown) = (Arc::clone(&worker), shutdown.clone());
        async move { worker.claim_loop(shutdown).await }
    });
    stopped.await?;
    tracing::info!("shutting down: no new claims");
    shutdown.cancel();
    if let Err(e) = claims.await {
        tracing::error!(error = %e, "claim loop panicked");
    }
    worker.drain().await;
    stop_heartbeat.cancel();
    if let Err(e) = heartbeat.await {
        tracing::error!(error = %e, "heartbeat panicked");
    }
    Ok(())
}
```

The claim loop takes a slot **before** each claim, so the worker never polls while full and never holds more than `slots` jobs; the permit moves into the job task and is dropped when it ends. `204` loops at once (the server already waited `wait_s`); an error backs off 1 s doubling to 30 s, each delay drawn from `[d/2, d]`. The one early `204` is the open poll when the API shuts down, and it can't turn into a spin: on the signal `axum::serve` drops its listener and disables keep-alive on open connections, so the next claim fails to connect, or gets `502` from Caddy, and backs off (✓ probe on axum 0.8.9 and reqwest 0.13.5: during a 1.7 s graceful shutdown the re-claiming client saw one `204`, then a connection error, and no claim reached the handler after the signal).

```rust
/// Exponential backoff with jitter that never goes below half the delay.
#[derive(Debug, Clone)]
pub struct Backoff {
    attempt: u32,
    base: Duration,
    cap: Duration,
}

/// The wait before retrying after `e`: the backoff's next delay, or the API's `retry_after_s`
/// when it asked for longer ([contracts 2]).
pub fn retry_delay(e: &CallError, backoff: &mut Backoff) -> Duration {
    let delay = backoff.next_delay();
    e.retry_after().map_or(delay, |after| delay.max(after))
}
```

**Shutdown time.** From SIGTERM to exit takes at most `DRAIN_SECONDS`, 20 s. The claim loop returns at once and drops an open long poll. Running jobs get the 20 s. At the bound, `abandon` cancels every job's token, and every await in a job task selects on that token: the model, an image fetch, a `complete` call in flight and the backoff between `complete` calls. So the tasks end at once. Then the heartbeat stops, and its `stop` token drops a call in flight too. What is left when `main` returns is a decode or resize already started on the blocking pool, which the runtime waits for, and an ONNX Runtime run, which `TerminateOnDrop` has already told to stop. Compose's `stop_grace_period: 30s` leaves 10 s for these and for Docker (section 12). Without the two cuts, a `complete` in flight at the bound (10 s deadline) and then a heartbeat in flight (5 s deadline) would run on after the drain: 20 + 10 + 5 = 35 s, past the grace, and Docker would SIGKILL the worker. A `complete` cut at the bound is either committed by the API or not. If not, the job is requeued 30 s after the last heartbeat, like any abandoned job, and the requeue is free for the job's first two. `the_whole_shutdown_ends_within_the_drain_bound` shows exactly 20 s on paused time.

---

## 6. Leases, heartbeats and completion

**The API boundary.** `WorkerApi` is used generically (`Worker<A: WorkerApi>`), never as `dyn`, so its methods return `impl Future + Send` without boxing; the explicit `Send` lets job tasks that await them run on the multi-thread runtime.

```rust
/// What a call about a job needs: its id and lease token. Owned and cheap to clone (the token
/// is an `Arc<str>`), so the job task, its progress sink and the registry each hold one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobLease {
    /// The job.
    pub job_id: JobId,
    /// `X-Lease-Token`.
    pub lease_token: LeaseToken,
}

/// A failed call, classified by the error body's `retryable` ([contracts 2]): the server's
/// word on whether the same request may succeed later.
#[derive(Debug, thiserror::Error)]
pub enum CallError {
    /// `409` with `code: "lease_lost"`: the job was requeued, expired or cancelled. Drop it
    /// silently.
    #[error("lease lost")]
    LeaseLost,
    /// The same call may succeed later: an error body with `retryable: true`, a `429` or `5xx`
    /// without an error body (a proxy's own answer), a connect error, a timeout, or a body
    /// that couldn't be read. Retry with backoff, and not before `retry_after` when the body
    /// had `retry_after_s`.
    #[error("worker API unavailable: {reason}")]
    Unavailable {
        /// The status and body, or the transport error.
        reason: String,
        /// The body's `retry_after_s`.
        retry_after: Option<Duration>,
    },
    /// An error body with `retryable: false` (`500 internal` among them), or a `4xx` without
    /// one: the same call would fail again.
    #[error("worker API rejected the call: {0}")]
    Rejected(String),
}
```
```rust
impl CallError {
    /// `Unavailable` without a `retry_after`.
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
            retry_after: None,
        }
    }

    /// The wait the API asked for, if any.
    pub const fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::Unavailable { retry_after, .. } => *retry_after,
            Self::LeaseLost | Self::Rejected(_) => None,
        }
    }
}
```
```rust
/// The API boundary. Methods return `impl Future + Send`: the trait is used generically
/// (`Worker<A: WorkerApi>`), never as `dyn`, so no boxing is needed, and the explicit `Send`
/// lets job tasks that await these futures be spawned on the multi-thread runtime. Under
/// edition 2024 the returned future captures every lifetime in scope (`&self` and the borrowed
/// arguments), which is what a future that reads them needs.
pub trait WorkerApi: Send + Sync + 'static {
    /// `POST /internal/workers/heartbeat`.
    fn heartbeat(
        &self,
        hb: &Heartbeat,
    ) -> impl Future<Output = Result<HeartbeatResponse, CallError>> + Send;
    /// `POST /internal/jobs/claim`; `Ok(None)` is the `204` of a long poll that found nothing.
    fn claim(
        &self,
        req: &ClaimRequest,
    ) -> impl Future<Output = Result<Option<ClaimedJob>, CallError>> + Send;
    /// `POST /internal/jobs/{id}/events`.
    fn events(
        &self,
        lease: &JobLease,
        events: &WorkerEvents,
    ) -> impl Future<Output = Result<(), CallError>> + Send;
    /// `POST /internal/jobs/{id}/complete`; a repeat for a completed job is `204` too.
    fn complete(
        &self,
        lease: &JobLease,
        body: &CompleteJob,
    ) -> impl Future<Output = Result<(), CallError>> + Send;
    /// `GET /internal/jobs/{id}/images/{image_id}`: the WebP bytes.
    fn image(
        &self,
        lease: &JobLease,
        image: &JobImage,
    ) -> impl Future<Output = Result<Bytes, CallError>> + Send;
}

/// The reqwest implementation. One `Client` per process (clones share its pool), whose 600 s
/// read gap (`main`) is longer than every deadline here: each call sets its own, so the claim
/// long poll and large image downloads end at that deadline, never at a read gap.
#[derive(Debug, Clone)]
pub struct HttpWorkerApi {
    http: Client,
    base: String,
    token: SecretString,
    wait_s: u32,
}

/// The fields of [contracts 2]'s error body the worker reads; serde ignores the others.
#[derive(Deserialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Deserialize)]
struct ErrorDetail {
    code: String,
    retryable: bool,
    #[serde(default)]
    retry_after_s: Option<u32>,
}

/// Reads a non-2xx response. Only the body says `lease_lost`, `retryable` and
/// `retry_after_s`, so it is read before anything consumes the response; a body that can't be
/// read is a transport error.
async fn classify(response: Response) -> CallError {
    let status = response.status();
    match response.text().await {
        Ok(body) => classify_body(status, &body),
        Err(e) => transport(&e),
    }
}

/// Classifies an error answer by its body's `retryable`, the server's word ([contracts 2]).
/// `lease_lost` is the one code with its own meaning. Without an error body (a proxy's `502`),
/// a `429` or `5xx` is worth another try and anything else is not.
pub fn classify_body(status: StatusCode, body: &str) -> CallError {
    let what = format!("{status}: {body}");
    match serde_json::from_str::<ErrorBody>(body)
        .ok()
        .map(|b| b.error)
    {
        Some(e) if status == StatusCode::CONFLICT && e.code == "lease_lost" => CallError::LeaseLost,
        Some(e) if e.retryable => CallError::Unavailable {
            reason: what,
            retry_after: e.retry_after_s.map(|s| Duration::from_secs(u64::from(s))),
        },
        None if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS => {
            CallError::unavailable(what)
        }
        // `retryable: false` (`500 internal` among them), or a 4xx without an error body.
        Some(_) | None => CallError::Rejected(what),
    }
}

/// Decodes a claimed job. `ClaimedJob::decode` reads `v`, `job_id` and `lease_token` first and
/// keeps a job of another `v` as `Unsupported`; a `v: 1` job whose `kind` or payload this
/// build can't read (an older worker against a newer API, contracts open question 21) is kept
/// as `Unsupported` too, so it is failed with `unsupported_version` instead of being left to
/// expire. Only a body without even the head is an error.
pub fn decode_claim(body: &[u8]) -> Result<ClaimedJob, CallError> {
    match ClaimedJob::decode(body) {
        Ok(job) => Ok(job),
        Err(e) => match serde_json::from_slice::<domain::jobs::JobHead>(body) {
            Ok(head) => {
                tracing::error!(error = %e, job_id = %head.job_id, "unreadable job");
                Ok(ClaimedJob::Unsupported(head))
            }
            Err(_) => Err(CallError::Rejected(format!("undecodable job: {e}"))),
        },
    }
}
```

| Rule ([contracts 5](../architecture/contracts.md#leases-runs-and-attempts)) | Where |
|---|---|
| Heartbeat every 10 s on its own task, `MissedTickBehavior::Delay`, a 5 s timeout, every held job with its lease token | `Worker::heartbeat_loop` |
| `versions` = the model's keys + `worker_id` | `JobRunner::versions` |
| A slot before each claim; `slots` the same in claim and heartbeat | `Worker::claim_loop` |
| Claim long poll `wait_s` 25 with a request timeout of `wait_s + 10` s | `HttpWorkerApi::claim` |
| `X-Lease-Token` on events, complete and image calls | `HttpWorkerApi::leased` |
| Every error body is read before anything consumes the response: its `code` decides `lease_lost`, its `retryable` and `retry_after_s` decide the retry (table below) | `classify`, `classify_body` |
| Decode `v`, `job_id`, `lease_token` first; another `v` → `failed`, `unsupported_version`, not retryable | `ClaimedJob::decode`, `decode_claim`, `run_job` |
| Images only through `GET /internal/jobs/{id}/images/{image_id}`, each retried twice after an `Unavailable` answer | `fetch_images` |
| `complete` with `model_versions` and `timings_ms`, retried with backoff (at least `retry_after_s`) until `max_runtime_s`; a repeat is `204`; the job's token cuts it, a call in flight included | `complete_with_retry` |

**What an answer means** ([contracts 2](../architecture/contracts.md#2-errors)). Every error body has `retryable`, the server's word on whether the same request may succeed later, so the worker follows it whatever the status. Only `lease_lost` has a meaning of its own. A status alone decides only when there is no error body, as with Caddy's own `502` while the API is down.

| Answer | `CallError` | Heartbeat | Claim | Image fetch | `complete` |
|---|---|---|---|---|---|
| `409` with `code: "lease_lost"` | `LeaseLost` | — | — | drops the job | drops the job; nothing more is sent |
| An error body with `retryable: true` (`429 rate_limited`, `503 upstream_unavailable`) | `Unavailable`, with `retry_after_s` | logged; the next beat is 10 s away | backs off, at least `retry_after_s` | retried twice, at least `retry_after_s` apart | retried with backoff, at least `retry_after_s`, until `max_runtime_s` |
| A `429` or `5xx` without an error body, a connect error, a timeout, a body that couldn't be read | `Unavailable` | logged | backs off | retried twice | retried with backoff |
| An error body with `retryable: false` (`500 internal`, `429 budget_exceeded`, `400 invalid_request`, `404 not_found`), or a `4xx` without one | `Rejected` | logged | backs off like any error | fails the job with `image_fetch_failed` | logged, not retried; the job leaves the heartbeat, so its lease expires and the broker requeues it |
| `204` on a repeat `complete` | success | | | | done |

The backoffs are 1 s doubling to 30 s for the claim, 0.5 s doubling to 8 s for `complete` and 0.25 s doubling to 2 s for an image, each delay drawn from `[d/2, d]`; `retry_delay` waits the longer of that delay and `retry_after_s`.

```rust
/// Runs jobs on the process's model and labels results with its versions.
pub struct JobRunner {
    model: Arc<dyn VisionModel>,
    worker_id: WorkerId,
}

/// The job was lost while it ran; nothing more may be sent about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseLost;

/// The images a job's payload names, each once, in payload order. Depth maps are never in a
/// payload, so the worker never sees one.
pub fn job_images(spec: &JobSpec) -> Vec<JobImage> {
    let mut out: Vec<JobImage> = Vec::new();
    let mut add = |images: &[JobImage]| {
        for i in images {
            if !out.iter().any(|o| o.image_id == i.image_id) {
                out.push(i.clone());
            }
        }
    };
    match spec {
        JobSpec::Perception(p) => add(&p.images),
        JobSpec::Vlm(p) => {
            add(&p.findings_images);
            add(&p.indicator_images);
        }
        JobSpec::Verify(p) => add(&p.images),
        JobSpec::Guided(p) => add(std::slice::from_ref(&p.image)),
    }
    out
}

/// The job task's body: version check, the model under the soft deadline (15 s before
/// `max_runtime_s`, on the monotonic clock), then `complete` with retries. A cancelled token
/// (`cancel_job_ids`, `lease_lost`, or the drain running out) returns at once without sending
/// anything more, whichever of these it is in.
pub async fn run_job<A: WorkerApi>(
    api: &Arc<A>,
    runner: &JobRunner,
    claimed: ClaimedJob,
    cancel: &CancellationToken,
    timings: Timings,
) {
    let received = Instant::now();
    let (job, lease, max_runtime) = match claimed {
        ClaimedJob::Unsupported(head) => {
            let lease = JobLease {
                job_id: head.job_id,
                lease_token: head.lease_token,
            };
            let body = CompleteJob {
                v: V1,
                outcome: JobOutcome::Failed {
                    error: JobError::new(
                        JobErrorCode::UnsupportedVersion,
                        format!(
                            "job v {} (or its kind or payload) is not supported by this worker",
                            head.v
                        ),
                    ),
                },
                model_versions: runner.versions(),
                timings_ms: BTreeMap::new(),
            };
            // No max_runtime_s without a v1 job: give `complete` the shortest one (2 minutes).
            let until = received + Duration::from_mins(2);
            complete_with_retry(api.as_ref(), &lease, &body, until, cancel).await;
            return;
        }
        ClaimedJob::Job(job) => {
            let lease = JobLease::from(job.as_ref());
            let max = Duration::from_secs(u64::from(job.max_runtime_s));
            (job, lease, max)
        }
    };
    let hard_deadline = received + max_runtime;
    let soft_deadline = received + max_runtime.saturating_sub(timings.stop_before_max_runtime);
    let progress: Arc<dyn ProgressSink> = Arc::new(ApiProgress::new(
        Arc::clone(api),
        lease.clone(),
        cancel.clone(),
    ));
    let run = tokio::time::timeout_at(soft_deadline, runner.execute(api.as_ref(), &job, progress));
    let outcome = tokio::select! {
        () = cancel.cancelled() => return, // drops the model's future: cancellation by drop
        outcome = run => outcome,
    };
    let body = match outcome {
        Ok(Ok(body)) => body,
        Ok(Err(LeaseLost)) => return,
        Err(_elapsed) => CompleteJob {
            v: V1,
            outcome: JobOutcome::Failed {
                error: JobError::new(
                    JobErrorCode::MaxRuntimeExceeded,
                    format!("stopped before max_runtime_s {}", job.max_runtime_s),
                ),
            },
            model_versions: runner.versions(),
            timings_ms: BTreeMap::new(),
        },
    };
    complete_with_retry(api.as_ref(), &lease, &body, hard_deadline, cancel).await;
}

/// Repeating `complete` with the same lease token is safe: a job that already completed gets
/// `204`. Retries an `Unavailable` answer with backoff, not before its `retry_after_s`; stops at
/// the hard deadline, since the lease can't outlive `max_runtime`. The job's token cuts a call
/// in flight too, so a drain that runs out isn't held up by a slow `complete`: if the API
/// didn't commit it, the job is requeued 30 s after the last heartbeat, like any abandoned job.
async fn complete_with_retry<A: WorkerApi>(
    api: &A,
    lease: &JobLease,
    body: &CompleteJob,
    until: Instant,
    cancel: &CancellationToken,
) {
    let mut backoff = Backoff::new(Duration::from_millis(500), Duration::from_secs(8));
    loop {
        let answer = tokio::select! {
            () = cancel.cancelled() => return,
            answer = api.complete(lease, body) => answer,
        };
        let e = match answer {
            Ok(()) | Err(CallError::LeaseLost) => return,
            Err(CallError::Rejected(e)) => {
                tracing::error!(error = %e, job_id = %lease.job_id, "complete rejected");
                return;
            }
            Err(e @ CallError::Unavailable { .. }) => e,
        };
        let wait = retry_delay(&e, &mut backoff);
        if Instant::now() + wait >= until {
            tracing::warn!(error = %e, job_id = %lease.job_id, "giving up on complete");
            return;
        }
        tokio::select! {
            () = cancel.cancelled() => return,
            () = tokio::time::sleep(wait) => {}
        }
    }
}
```
```rust
impl JobRunner {
    /// A runner for `model`.
    pub fn new(model: Arc<dyn VisionModel>, worker_id: WorkerId) -> Self {
        Self { model, worker_id }
    }

    /// The model's version keys plus `worker_id` ([contracts 5], `model_versions` keys).
    pub fn versions(&self) -> BTreeMap<String, String> {
        let mut v = self.model.model_versions();
        v.insert(
            ModelVersionKey::WORKER_ID.to_owned(),
            self.worker_id.to_string(),
        );
        v
    }

    /// Runs one decoded job: fetch, model, result. `Err(LeaseLost)` means "send nothing".
    pub async fn execute<A: WorkerApi>(
        &self,
        api: &A,
        job: &Job,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<CompleteJob, LeaseLost> {
        let lease = JobLease::from(job);
        let mut timings = BTreeMap::new();
        let started = StdInstant::now();
        let images = match fetch_images(api, &lease, job_images(&job.spec)).await {
            Ok(images) => images,
            Err(CallError::LeaseLost) => return Err(LeaseLost),
            Err(e) => {
                return Ok(self.failed(
                    job,
                    JobError::new(JobErrorCode::ImageFetchFailed, e.to_string()),
                ));
            }
        };
        timings.insert("image_fetch".to_owned(), elapsed_ms(started));
        let ctx = JobContext {
            job_id: job.job_id,
            scan_id: job.scan_id,
            run: job.run,
            attempt: job.attempt,
        };
        let model_started = StdInstant::now();
        let outcome = match &job.spec {
            JobSpec::Perception(p) => self
                .model
                .perceive(ctx, p, &images, progress)
                .await
                .map(JobResult::Perception),
            JobSpec::Vlm(p) => self
                .model
                .analyse(ctx, p, &images, progress)
                .await
                .map(JobResult::Vlm),
            JobSpec::Verify(p) => self
                .model
                .verify(ctx, p, &images, progress)
                .await
                .map(JobResult::Verify),
            JobSpec::Guided(p) => match images.first() {
                Some(image) => self
                    .model
                    .read_guided_check(ctx, p, image)
                    .await
                    .map(JobResult::Guided),
                None => Err(ModelError::Unsupported {
                    model: "worker",
                    what: "a guided job without its image",
                }),
            },
        };
        timings.insert(
            job.spec.kind().as_str().to_owned(),
            elapsed_ms(model_started),
        );
        let mut body = match outcome.map(|r| r.to_value()) {
            Ok(Ok(result)) => self.complete(job, JobOutcome::Done { result }),
            Ok(Err(e)) => self.failed(
                job,
                JobError::new(
                    JobErrorCode::ModelError,
                    format!("result didn't serialise: {e}"),
                ),
            ),
            Err(e) => self.failed(job, job_error(&e)),
        };
        body.timings_ms.extend(timings);
        Ok(body)
    }

    fn complete(&self, job: &Job, outcome: JobOutcome) -> CompleteJob {
        let mut model_versions = self.versions();
        if let JobSpec::Perception(p) = &job.spec {
            // The versions this job used, not every loaded one ([vision 6], versions in results).
            model_versions.insert(
                ModelVersionKey::LABEL_SET_VERSION.to_owned(),
                p.label_set_version.clone(),
            );
            model_versions.insert(
                ModelVersionKey::CLASSIFIER_LABELS_VERSION.to_owned(),
                p.classifier_labels_version.clone(),
            );
        }
        CompleteJob {
            v: V1,
            outcome,
            model_versions,
            timings_ms: BTreeMap::new(),
        }
    }

    fn failed(&self, job: &Job, error: JobError) -> CompleteJob {
        self.complete(job, JobOutcome::Failed { error })
    }
}
```

**`model_versions` keys** ([contracts 5](../architecture/contracts.md#post-internaljobsidcomplete)), as `domain::jobs::ModelVersionKey` constants:

| Key | Perception | VLM | Fallback |
|---|---|---|---|
| `worker_id` | ✓ | ✓ | ✓ |
| `vlm_model`, `vlm_server`, `vlm_box_format`, `vlm_max_pixels`, `vlm_prompts`, `vlm_certainty` | | ✓ | ✓ (`vlm_server = anthropic`) |
| `vlm_quant` | | ✓ | |
| `perception_config`, `ort_ep` | ✓ | | |
| `label_set_version`, `classifier_labels_version` | ✓: the job's in `complete`; every loaded one, comma-separated, in the heartbeat | | |
| `<model name>` (manifest entry names: `leaf_detector`, `sam_encoder`, `sam_decoder`, `bioclip`, `disease_head…`, `plant_probe`) | ✓ | | |

`vlm_prompts` is `system@1,findings@2,indicators@1,verify@1,guided@1`, in the contract's `name@n` form with the prompt versions of [vision vlm 7](../vision/vlm.md#7-prompts); `vlm_certainty` is `0.4/0.65/0.85`.

---

## 7. Cancellation

Three things end a job early, all through the job's `CancellationToken` (a child of `abandon`):

| Cause | How it arrives | What is sent |
|---|---|---|
| `cancel_job_ids` in a heartbeat response (a new run, a deleted scan) | `Running::cancel` | nothing |
| `409 lease_lost` on an `events` call | `ApiProgress::send` cancels the token | nothing |
| `409 lease_lost` on an image fetch | `execute` returns `LeaseLost` | nothing |
| The drain runs out at SIGTERM | `abandon.cancel()` | nothing more, and a `complete` in flight is dropped; the broker requeues after the heartbeat goes stale |
| The soft deadline (`max_runtime_s − 15 s`) | `timeout_at` around the model | `failed`, `max_runtime_exceeded`, retryable |

`run_job` `select!`s on the token and the model's future; when the token fires it returns, and **dropping the model's future is the cancellation** ([domain 9](domain.md#models)). Each adapter makes that drop effective: the perception pipeline's `TerminateOnDrop` guard terminates a running ONNX Runtime batch ([vision 6](vision.md#6-perception)); dropping a `reqwest` future closes the VLM server's connection, and vLLM aborts a request whose client disconnected (*unverified* for llama.cpp, which finishes the slot).

```rust
/// The held jobs: what the heartbeat lists in `current_jobs` and cancels on `cancel_job_ids`.
/// A `std` mutex, never held across an `.await`: each critical section is one insert, remove or
/// read.
#[derive(Clone, Default)]
pub struct Running(Arc<Mutex<HashMap<JobId, Entry>>>);

/// Removes the job from the heartbeat list when the job task ends, however it ends.
pub struct Registered {
    running: Running,
    job_id: JobId,
}
```
```rust
/// The job's progress sink: posts events with the lease token. On `409 lease_lost` it cancels
/// the job's token, so the job task stops at once and sends nothing more; other failures are
/// returned and the model goes on (`complete` repeats every stage).
pub struct ApiProgress<A: WorkerApi> {
    api: Arc<A>,
    lease: JobLease,
    cancel: CancellationToken,
}
```

---

## 8. The VLM adapters

`VlmAdapter<B: VlmBackend>` implements `VisionModel` for `vlm`, `verify` and `guided` jobs ([contracts 6](../architecture/contracts.md#6-vlm-structured-output); [vision vlm](../vision/vlm.md)). `B` is fixed per worker kind at compile time; only the `VisionModel` is a trait object.

```rust
/// One image of a call: its label line (`img1: whole plant`) and its JPEG bytes. `'a` borrows
/// the prepared image the adapter keeps for the whole job.
#[derive(Debug, Clone, Copy)]
pub struct VlmImage<'a> {
    /// `img1: whole plant`.
    pub label: &'a str,
    /// JPEG.
    pub jpeg: &'a [u8],
}

/// One structured-output call. Everything is borrowed for `'a`: the prompts and schema the
/// adapter built for this call and the images it prepared for the job; the backend serialises
/// the request once and keeps none of it.
#[derive(Debug, Clone)]
pub struct VlmCall<'a> {
    /// The system prompt.
    pub system: &'a str,
    /// The images, first in the user message.
    pub images: Vec<VlmImage<'a>>,
    /// The call's instructions, after the images.
    pub instructions: &'a str,
    /// The schema's name (`findings`, `indicators`, `guided`).
    pub schema_name: &'a str,
    /// The per-call JSON Schema, with its limits.
    pub schema: &'a Value,
    /// The output budget.
    pub max_output_tokens: u32,
}

/// Why the model stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// A complete answer.
    Complete,
    /// The token limit: a validation failure, never a truncated success.
    Length,
    /// A safety refusal (HTTP 200 with `stop_reason: refusal`): a validation failure.
    Refusal,
}

/// The answer's text (JSON) and how it ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VlmReply {
    /// The JSON text.
    pub text: String,
    /// Why it stopped.
    pub stop: Stop,
}

/// A VLM backend. The adapter is generic over it (`VlmAdapter<B>`), so the trait is never a
/// `dyn` and can return `impl Future`. The named lifetime `'a` ties the future to both `&self`
/// and the borrowed call; `+ Send` is spelled out so the adapter's own `#[async_trait]` future,
/// which awaits this one, stays `Send`.
pub trait VlmBackend: Send + Sync + 'static {
    /// The model name in `ModelError`s.
    fn name(&self) -> &'static str;
    /// One call.
    fn call<'a>(
        &'a self,
        call: &'a VlmCall<'a>,
    ) -> impl Future<Output = Result<VlmReply, ModelError>> + Send + 'a;
}

/// vLLM or llama.cpp through `integrations::openai_compat`.
#[derive(Debug, Clone)]
pub struct OpenAiBackend {
    client: OpenAiCompatClient,
    served_name: String,
}

/// The hosted LLM through `integrations::anthropic`. The schema is sanitised first: the API
/// rejects length, item-count and numeric-bound keywords with a 400 and needs
/// `additionalProperties: false` on every object (contracts 6); the adapter's decoding truncates
/// and clamps afterwards for every backend.
#[derive(Debug, Clone)]
pub struct AnthropicBackend {
    client: AnthropicClient,
    model: String,
}
```

**The adapter instance.** Box format, cell size and pixel budget belong to the adapter instance, the model, server and preparation together ([contracts 6, box formats](../architecture/contracts.md#box-formats-per-model-adapter)):

| Instance | `box_format` | Cell | Pixels | Long side |
|---|---|---|---|---|
| Qwen3-VL-8B FP8 on vLLM (5070 Ti) or vLLM ROCm (9070 XT) | `norm_1000` | 32 | `VLM_MAX_PIXELS` (1,003,520) | — |
| The same model on llama.cpp | `norm_1000` | 32 | `VLM_MAX_PIXELS` | — |
| Qwen2.5-VL baseline | `abs_px_prepared` | 28 | `VLM_MAX_PIXELS` | — |
| The hosted LLM (fallback) | `abs_px_prepared` | 1 | 1,150,000 | 1568 |

```rust
/// The prompt versions ([vision vlm 7]), in the contract's `name@n` form.
///
/// [vision vlm 7]: ../../../docs/vision/vlm.md#7-prompts
pub const PROMPTS: &str = "system@1,findings@2,indicators@1,verify@1,guided@1";

/// Certainty words to confidences ([contracts 6], after decoding): 0.4 / 0.65 / 0.85 until
/// calibration moves each value within its band, never above 0.85.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CertaintyMap {
    /// `low`.
    pub low: f64,
    /// `medium`.
    pub medium: f64,
    /// `high`.
    pub high: f64,
}

/// Output budgets per call (`max_completion_tokens` on vLLM, `max_tokens` elsewhere).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallBudget {
    /// The findings call.
    pub findings: u32,
    /// The indicators (and verify) call.
    pub indicators: u32,
    /// The guided-check call.
    pub guided: u32,
}

/// What `model_versions` says about the VLM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VlmIdentity {
    /// `vlm_model`: repository@revision, or the hosted model id.
    pub model: String,
    /// `vlm_quant`, for local checkpoints.
    pub quant: Option<String>,
    /// `vlm_server`: `vllm@v0.31.0`, `vllm-rocm@v0.31.0`, `llama.cpp@b11669`, `anthropic`.
    pub server: String,
}

/// One adapter instance.
#[derive(Debug, Clone, PartialEq)]
pub struct AdapterProfile {
    /// How the model writes `bbox_2d`.
    pub box_format: BoxFormat,
    /// Each prepared side is a multiple of this (32 for Qwen3-VL; 1 for the hosted model).
    pub cell_px: u32,
    /// `VLM_MAX_PIXELS`.
    pub max_pixels: u32,
    /// A long-side cap (1568 for the hosted model, which resizes larger images itself).
    pub max_long_side: Option<u32>,
    /// JPEG quality of the prepared images.
    pub jpeg_quality: u8,
    /// Output budgets.
    pub budget: CallBudget,
    /// The certainty mapping.
    pub certainty: CertaintyMap,
    /// For `model_versions`.
    pub identity: VlmIdentity,
}
```

**Image preparation.** Each image is scaled (never up) to at most `VLM_MAX_PIXELS` and the long-side cap, each side rounded **down** to a multiple of the cell, then JPEG at quality 90; a 2048 × 1536 stored photo becomes 1152 × 864 (972 tokens on Qwen3). The server is started so prepared images fall inside its processor's range, so "the image as sent" is "the image as seen" ([contracts 6, image preparation](../architecture/contracts.md#image-preparation)). Preparation runs once per job on the blocking pool, so the findings and indicators calls share the same bytes, which also lets vLLM's prefix cache reuse the image tokens.

```rust
/// One prepared image: what was sent, and its size, which `abs_px_prepared` boxes divide by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedImage {
    /// The stored image it came from.
    pub image_id: ImageId,
    /// Its kind.
    pub kind: ImageKind,
    /// Prepared width.
    pub width: u32,
    /// Prepared height.
    pub height: u32,
    /// JPEG bytes.
    pub jpeg: Vec<u8>,
}

/// The prepared size: scaled (never up) so the pixel count is at most `max_pixels` and the long
/// side at most `max_long_side`, then each side rounded **down** to a multiple of `cell`, never
/// below one cell. Rounding down keeps the count under the budget, as Qwen's `smart_resize` does
/// when it must shrink.
pub fn target_size(
    size: (u32, u32),
    cell: u32,
    max_pixels: u32,
    max_long_side: Option<u32>,
) -> (u32, u32) {
    let (w, h) = (f64::from(size.0), f64::from(size.1));
    let cell = cell.max(1);
    let mut scale = (f64::from(max_pixels) / (w * h)).sqrt().min(1.0);
    if let Some(long) = max_long_side {
        scale = scale.min(f64::from(long) / w.max(h));
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a positive pixel count far below u32::MAX"
    )]
    let snap = |len: f64| -> u32 {
        let scaled = (len * scale).floor() as u32;
        (scaled / cell * cell).max(cell)
    };
    (snap(w), snap(h))
}

/// Prepares one image. Blocking.
pub fn prepare(
    image: &LoadedImage,
    profile: &AdapterProfile,
) -> Result<PreparedImage, VisionError> {
    let pixels = imaging::decode_stored(&image.bytes)?;
    let (w, h) = target_size(
        pixels.dimensions(),
        profile.cell_px,
        profile.max_pixels,
        profile.max_long_side,
    );
    let resized = imaging::resize(ImageView::from(&pixels), w, h)?;
    Ok(PreparedImage {
        image_id: image.image.image_id,
        kind: image.image.kind,
        width: w,
        height: h,
        jpeg: imaging::encode_jpeg(ImageView::from(&resized), profile.jpeg_quality)?,
    })
}

/// Prepares every image on the blocking pool, behind the CPU semaphore. The closure is
/// `'static + Send`: it owns clones of the images (`Bytes` clones are reference counts) and
/// of the profile.
pub async fn prepare_all(
    images: &[LoadedImage],
    profile: &AdapterProfile,
    cpu: &Arc<Semaphore>,
) -> Result<Vec<PreparedImage>, VisionError> {
    let owned = images.to_vec();
    let profile = profile.clone();
    let permit = Arc::clone(cpu)
        .acquire_owned()
        .await
        .map_err(|_| VisionError::InferenceGone)?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        owned.iter().map(|i| prepare(i, &profile)).collect()
    })
    .await?
}
```

**The calls.**

| Job | Calls | Images | Events |
|---|---|---|---|
| `vlm` | findings, then indicators | findings: the payload's `findings_images` in order; indicators: its `indicator_images` | `vlm_findings` started, partial result, completed; `vlm_indicators` started and completed, or completed with `skipped: all_measured` when no candidate has an indicator left |
| `verify` | indicators | the payload's `images`; for `new_photo` the first is labelled "new photo, taken to answer a question" | `verify` started and completed |
| `guided` | guided | the one image | none (the port has no progress sink) |

Photo routing ([contracts 5](../architecture/contracts.md#payloads)) is the API's: the payload already holds the chosen images in order (`domain::rules::routing`), and the worker keeps that order and the cap of 4 (`pick`).

```rust
impl<B: VlmBackend> VlmAdapter<B> {
    /// An adapter on `backend` with the instance's profile.
    pub fn new(backend: B, profile: AdapterProfile, cpu: Arc<Semaphore>) -> Self {
        Self {
            system: prompt::system(profile.box_format),
            backend,
            profile,
            limits: Limits::CONTRACT,
            cpu,
        }
    }

    /// The profile.
    pub const fn profile(&self) -> &AdapterProfile {
        &self.profile
    }

    /// Prepares the job's images (each once, even when both calls use it).
    pub async fn prepare(&self, images: &[LoadedImage]) -> Result<Vec<PreparedImage>, ModelError> {
        prepare::prepare_all(images, &self.profile, &self.cpu)
            .await
            .map_err(|e| e.into_model_error(self.backend.name()))
    }

    /// One structured call, validated, retried once on any validation failure (including
    /// `finish_reason: length` and a refusal), then `InvalidOutput` (`model_error`, retryable).
    pub async fn call_validated<T>(
        &self,
        call: &VlmCall<'_>,
        decode: impl Fn(&str) -> Result<T, Invalid> + Send + Sync,
    ) -> Result<T, ModelError> {
        let mut last = None;
        for attempt in 1..=2u32 {
            let reply = self.backend.call(call).await?;
            let decoded = match reply.stop {
                Stop::Length => Err(Invalid::Truncated),
                Stop::Refusal => Err(Invalid::Refused),
                Stop::Complete => decode(&reply.text),
            };
            match decoded {
                Ok(value) => return Ok(value),
                Err(e) => {
                    tracing::warn!(attempt, error = %e, schema = call.schema_name, "invalid VLM output");
                    last = Some(e);
                }
            }
        }
        Err(ModelError::InvalidOutput {
            model: self.backend.name(),
            source: Box::new(last.unwrap_or(Invalid::Truncated)),
        })
    }

    /// The call's labelled images; `'c` borrows the prepared images.
    fn labelled<'c>(
        images: &[&'c PreparedImage],
        first_is_new: bool,
    ) -> (Vec<String>, CallImages<'c>) {
        let labels = images
            .iter()
            .enumerate()
            .map(|(i, p)| prompt::image_label(i, p.kind, first_is_new && i == 0))
            .collect();
        (labels, CallImages::new(images.to_vec()))
    }

    fn box_range(&self, images: &[&PreparedImage]) -> BoxRange {
        let largest = images
            .iter()
            .map(|p| p.width.max(p.height))
            .max()
            .unwrap_or(1000);
        BoxRange::for_format(self.profile.box_format, largest)
    }

    /// The findings call on the payload's findings images, in the payload's order.
    pub async fn findings(
        &self,
        payload: &VlmPayload,
        prepared: &[PreparedImage],
    ) -> Result<VlmFindings, ModelError> {
        let images = pick(prepared, payload.findings_images.iter().map(|i| i.image_id));
        let (labels, call_images) = Self::labelled(&images, false);
        let schema: Value = schema::findings(
            images.len(),
            &payload.species_options,
            &payload.organism_options,
            self.box_range(&images),
            self.limits,
        );
        let instructions = prompt::findings(
            payload.working_species.as_ref(),
            &payload.species_options,
            &payload.organism_options,
        );
        let call = VlmCall {
            system: &self.system,
            images: vlm_images(&labels, &images),
            instructions: &instructions,
            schema_name: "findings",
            schema: &schema,
            max_output_tokens: self.profile.budget.findings,
        };
        self.call_validated(&call, |text| {
            decode::findings(
                text,
                &call_images,
                &payload.species_options,
                &payload.organism_options,
                self.profile.box_format,
                &self.profile.certainty,
                self.limits,
            )
        })
        .await
    }

    /// The indicators call (also the verify call) on the given images, for every indicator of
    /// every candidate, in payload order.
    pub async fn indicators(
        &self,
        candidates: &[CandidatePrompt],
        images: &[&PreparedImage],
        new_photo: bool,
    ) -> Result<VlmIndicatorAnswers, ModelError> {
        let all: Vec<&Indicator> = candidates.iter().flat_map(|c| &c.indicators).collect();
        let kinds: Vec<_> = images.iter().map(|p| p.kind).collect();
        let questions = schema::questions(all, &kinds);
        let expected: Vec<Expected> = questions
            .iter()
            .map(|q| Expected {
                key: q.key.clone(),
                indicator_id: q.indicator.indicator_id.clone(),
                unsure_only: q.images.is_empty(),
            })
            .collect();
        let (labels, call_images) = Self::labelled(images, new_photo);
        let schema = schema::indicators(images.len(), &questions, self.limits);
        let instructions = prompt::indicators(&questions, new_photo);
        let call = VlmCall {
            system: &self.system,
            images: vlm_images(&labels, images),
            instructions: &instructions,
            schema_name: "indicators",
            schema: &schema,
            max_output_tokens: self.profile.budget.indicators,
        };
        self.call_validated(&call, |text| {
            decode::indicators(text, &call_images, &expected, self.limits)
        })
        .await
    }
}
```
```rust
/// The prepared images with these ids, in that order, at most 4.
pub fn pick(
    prepared: &[PreparedImage],
    ids: impl IntoIterator<Item = domain::id::ImageId>,
) -> Vec<&PreparedImage> {
    ids.into_iter()
        .filter_map(|id| prepared.iter().find(|p| p.image_id == id))
        .take(MAX_VLM_IMAGES)
        .collect()
}
```

**Requests.** Greedy decoding (temperature 0, `top_p` 1, no penalties, seed 0) and `response_format` with the strict per-call schema, which vLLM and llama.cpp both take; `max_completion_tokens` on vLLM, `max_tokens` on llama.cpp (the `integrations` client's flavour decides) ([vision vlm 10](../vision/vlm.md#10-sampling-and-the-request)). Budgets: 1,500 tokens for findings and indicators, 100 for guided; 8,000 for every call on the hosted model, guided checks included, because its thinking counts toward `max_tokens`.

**Validation and the one retry.** `call_validated` treats a parse failure, a validation failure, `finish_reason: length` and a refusal alike: the call is repeated once with the same request, then the job fails with `model_error`, retryable ([contracts 6, after decoding](../architecture/contracts.md#after-decoding)). Greedy decoding makes the retry mostly deterministic, so each failure is logged with the schema name.

---

## 9. The per-call schema and decoding

The worker builds each call's JSON Schema from the payload ([contracts 6, per-job schema](../architecture/contracts.md#per-job-schema)):

| In the schema | Built as | Mapped back by |
|---|---|---|
| Images | an enum `img1` … `imgN` of the call's images | `CallImages::get` → `image_id` |
| Indicator answers | one required key per indicator, `q<n>_<slug cut to 24>` (`domain::vlm::indicator_key`), `reason` (`maxLength` 60) before `answer` | `Expected` → `indicator_id` |
| A question none of whose `image_kinds` is in the call | `answer` enum `["unsure"]` only | rejected if answered otherwise |
| Species guess | enum of `species_options` slugs + `other` | `SpeciesId` or `null` |
| Organism labels | enum of `organism_options` slugs + `other` | `OrganismId` or `null` |
| Boxes | arrays of exactly 4 integers, 0–1000 for `norm_1000`, 0–the largest prepared side for `abs_px_prepared` | `BoxFormat::to_bbox` (corners → `[x, y, w, h]`, clamped) |
| Soil readings, `other_name`, a damage box | `anyOf [schema, null]` | `None` |
| Guided | only the branch of the image's kind | rejected if the other branch comes back |

Every object has `additionalProperties: false`; length-limited strings have no `pattern` (xgrammar drops length bounds next to one, and vLLM rejects the combination ✓ per the vision research); `organisms` and `damage` have `maxItems` 10.

```rust
/// Text and list limits. They go into the schema, and decoding enforces them again for servers
/// that ignore them and for the hosted API, whose schema loses them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// `notes`.
    pub notes: usize,
    /// `other_description`.
    pub other_description: usize,
    /// `other_name`.
    pub other_name: usize,
    /// An indicator's `reason`.
    pub reason: usize,
    /// `organisms` and `damage`.
    pub max_items: usize,
}

/// The range of `bbox_2d` values for a box format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxRange {
    /// Integers from 0 to `max`: 1000 for `norm_1000`, the largest prepared side for pixels.
    Integers {
        /// The upper bound.
        max: u32,
    },
    /// Numbers from 0 to 1 (`norm_1`).
    Unit,
}

/// One question of an indicators call, with its key and the aliases of the call's images whose
/// kind can show it. `'p` borrows the payload's indicator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question<'p> {
    /// `q3_concentric_rings`.
    pub key: String,
    /// The payload's indicator.
    pub indicator: &'p Indicator,
    /// Aliases of the images that can show it; empty means the answer is `unsure` only.
    pub images: Vec<ImageAlias>,
}

/// The questions of a call, numbered in payload order, with their images by kind.
pub fn questions<'p>(
    indicators: impl IntoIterator<Item = &'p Indicator>,
    image_kinds: &[domain::scan::ImageKind],
) -> Vec<Question<'p>> {
    indicators
        .into_iter()
        .enumerate()
        .map(|(i, indicator)| Question {
            key: indicator_key(i + 1, &indicator.indicator_id),
            indicator,
            images: image_kinds
                .iter()
                .enumerate()
                .filter(|(_, k)| indicator.image_kinds.contains(k))
                .filter_map(|(j, _)| ImageAlias::for_index(j))
                .collect(),
        })
        .collect()
}

/// The findings call's schema for `n_images` images.
pub fn findings(
    n_images: usize,
    species: &[SpeciesOption],
    organisms: &[OrganismOption],
    boxes: BoxRange,
    limits: Limits,
) -> Value {
    let alias_list = aliases(n_images);
    let images = string_enum(alias_list.iter().map(String::as_str));
    let species_enum = string_enum(
        species
            .iter()
            .map(|s| s.id.as_str())
            .chain(std::iter::once(OTHER)),
    );
    let organism_enum = string_enum(
        organisms
            .iter()
            .map(|o| o.id.as_str())
            .chain(std::iter::once(OTHER)),
    );
    let damage_enum = damage_enum();
    let soil = soil_schema(&images);
    let organism = object(vec![
        ("label", organism_enum),
        ("other_name", nullable(text(limits.other_name))),
        ("count", string_enum(["one", "few", "many"])),
        ("image", images.clone()),
        ("bbox_2d", boxes.schema()),
        ("certainty", certainty()),
    ]);
    let damage = object(vec![
        ("pattern", damage_enum),
        (
            "other_description",
            nullable(text(limits.other_description)),
        ),
        ("image", images.clone()),
        ("bbox_2d", nullable(boxes.schema())),
        ("certainty", certainty()),
    ]);
    object(vec![
        (
            "species_guess",
            object(vec![
                ("species", species_enum),
                ("other_name", nullable(text(limits.other_name))),
                ("certainty", certainty()),
            ]),
        ),
        ("soil", soil),
        (
            "organisms",
            json!({ "type": "array", "items": organism, "maxItems": limits.max_items }),
        ),
        (
            "damage",
            json!({ "type": "array", "items": damage, "maxItems": limits.max_items }),
        ),
        ("notes", text(limits.notes)),
    ])
}

/// The indicators call's schema: one required key per question, `reason` first; a question no
/// image can show allows only `unsure`.
pub fn indicators(n_images: usize, questions: &[Question<'_>], limits: Limits) -> Value {
    let alias_list = aliases(n_images);
    let images = nullable(string_enum(alias_list.iter().map(String::as_str)));
    let props: Vec<(&str, Value)> = questions
        .iter()
        .map(|q| {
            let answers = if q.images.is_empty() {
                string_enum(["unsure"])
            } else {
                string_enum(["yes", "no", "unsure"])
            };
            (
                q.key.as_str(),
                object(vec![
                    ("reason", text(limits.reason)),
                    ("answer", answers),
                    ("image", images.clone()),
                ]),
            )
        })
        .collect();
    object(props)
}

/// The guided-check call's schema: only the branch for the image's kind.
pub fn guided(kind: GuidedKind) -> Value {
    match kind {
        GuidedKind::SqueezeTest => object(vec![
            ("kind", string_enum(["squeeze_test"])),
            (
                "result",
                string_enum(["crumbles", "holds_ball", "sticky_ribbon", "unclear"]),
            ),
            ("certainty", certainty()),
        ]),
        GuidedKind::JarTest => object(vec![
            ("kind", string_enum(["jar_test"])),
            ("settled", json!({ "type": "boolean" })),
            (
                "texture",
                string_enum(["sandy", "loamy", "silty", "clayey", "unclear"]),
            ),
            ("certainty", certainty()),
        ]),
    }
}
```
```rust
/// The system prompt, with the box line for the adapter's format.
pub fn system(format: BoxFormat) -> String {
    let boxes = match format {
        BoxFormat::Norm1000 => {
            "- Boxes are [x1, y1, x2, y2]: the top-left and bottom-right corners, as integers from 0 to 1000 relative to the image's width and height."
        }
        BoxFormat::AbsPxPrepared => {
            "- Boxes are [x1, y1, x2, y2]: the top-left and bottom-right corners, as integer pixel coordinates in the image as you see it."
        }
        BoxFormat::Norm1 => {
            "- Boxes are [x1, y1, x2, y2]: the top-left and bottom-right corners, as numbers from 0 to 1 relative to the image's width and height."
        }
    };
    format!("{SYSTEM}{boxes}")
}

/// The label line of an image in a call: `img1: whole plant`.
pub fn image_label(alias_index: usize, kind: ImageKind, new_photo: bool) -> String {
    let what = if new_photo {
        "new photo, taken to answer a question"
    } else {
        match kind {
            ImageKind::WholePlant => "whole plant",
            ImageKind::LeafCloseup => "close-up of a leaf",
            ImageKind::LeafUnderside => "underside of a leaf",
            ImageKind::SoilSurface => "soil around the base of the plant",
            ImageKind::FlowerFruit => "flower or fruit",
            ImageKind::SqueezeTest => "a soil squeeze test",
            ImageKind::JarTest => "a soil jar test",
        }
    };
    format!("img{}: {what}", alias_index + 1)
}

/// The findings call's instructions (after the image labels).
pub fn findings(
    working: Option<&WorkingSpecies>,
    species: &[SpeciesOption],
    organisms: &[OrganismOption],
) -> String {
    let mut s = String::new();
    match working {
        Some(w) => {
            let _ = writeln!(
                s,
                "The plant is believed to be {}. Give your own opinion below; don't just repeat this.\n",
                w.name
            );
        }
        None => s.push_str("The plant's species is not known.\n\n"),
    }
    s.push_str("1. species_guess: the species you see, from this list, or \"other\" with its common name in other_name.\n");
    for o in species {
        let _ = writeln!(s, "- {}: {}", o.id, o.name);
    }
    s.push('\n');
    s.push_str(SOIL);
    s.push_str("\n\n3. organisms: every insect, mite, spider, slug, snail, worm, bird or other animal you can see, up to 10. Choose the label from this list, or \"other\" with a short name in other_name. Box each one, or the group when there are many of the same kind close together. count is one, few (2 to 10) or many.\n");
    for o in organisms {
        let _ = writeln!(s, "- {}: {}", o.id, o.name);
    }
    s.push('\n');
    s.push_str(BENEFICIALS);
    s.push_str("\n\n");
    s.push_str(DAMAGE);
    s
}

/// The indicators (or verify) call's instructions. No candidate names: only the questions,
/// each with where to look and which images can show it.
pub fn indicators(questions: &[Question<'_>], new_photo: bool) -> String {
    let mut s = String::from(
        "Answer each question about this plant from the photos. For each one:
- reason: first, at most 60 characters on what you see that decides it.
- answer: yes, no, or unsure. Answer unsure when the photos don't show it clearly; that is better than a guess.
- image: the image that decided it, or null.
",
    );
    if new_photo {
        s.push_str("The first image was taken to answer these questions. Look at it first.\n");
    }
    s.push_str("\nQuestions (look first at the part of the plant named in brackets):\n");
    for q in questions {
        let images = if q.images.is_empty() {
            "no image shows this, answer unsure".to_owned()
        } else {
            q.images
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        };
        let _ = writeln!(
            s,
            "{} [{}; {}]: {}",
            q.key,
            where_text(q.indicator.r#where),
            images,
            q.indicator.question
        );
    }
    s
}

/// The guided-check call's instructions.
pub const fn guided(kind: GuidedKind) -> &'static str {
    match kind {
        GuidedKind::SqueezeTest => "The person took a handful of moist soil and squeezed it in their hand. Look at the result:
- crumbles: it falls apart when the hand opens
- holds_ball: it holds a ball shape but breaks when pressed
- sticky_ribbon: it is sticky and can be pushed into a ribbon between the fingers
- unclear: the photo doesn't show it clearly",
        GuidedKind::JarTest => "The person shook soil with water in a clear jar and let it settle. Sand settles first at the bottom, then silt, then clay on top.
- settled: true when the water above the soil is mostly clear and the layers are visible; false when it is still cloudy.
- texture: sandy (mostly the bottom layer), loamy (similar layers), silty (mostly the middle layer), clayey (a thick top layer), or unclear.",
    }
}
```

**After decoding.** Strings are truncated to their limits by characters and boxes clamped for every backend, because the hosted API's schema loses the limits and a server may not enforce them. A box with no area after clamping drops an organism (which needs one) and leaves a damage finding without a region ([vision vlm 9](../vision/vlm.md#9-box-formats)). Certainty words map through the profile's `CertaintyMap`.

```rust
/// Why an answer failed validation. Each is retried once, then fails the job with
/// `model_error`, `retryable: true`.
#[derive(Debug, thiserror::Error)]
pub enum Invalid {
    /// Not JSON of the expected shape.
    #[error("unparseable: {0}")]
    Json(#[from] serde_json::Error),
    /// The model stopped at its token limit.
    #[error("cut off at the token limit")]
    Truncated,
    /// The model refused.
    #[error("refused")]
    Refused,
    /// An alias outside the call's images.
    #[error("unknown image alias {0}")]
    Alias(String),
    /// A species outside `species_options` and not `other`.
    #[error("unknown species {0}")]
    Species(String),
    /// An organism outside `organism_options` and not `other`.
    #[error("unknown organism {0}")]
    Organism(String),
    /// An indicator key is missing.
    #[error("missing answer {0}")]
    MissingKey(String),
    /// A key that isn't one of the call's questions.
    #[error("unexpected answer {0}")]
    ExtraKey(String),
    /// `yes` or `no` to a question no image of the call can show.
    #[error("{0} may only be unsure")]
    UnsureOnly(String),
    /// The guided answer's branch isn't the image's kind.
    #[error("guided answer for the wrong check")]
    WrongBranch,
}

/// The images of one call, by alias. `'c` borrows the job's prepared images.
#[derive(Debug, Clone)]
pub struct CallImages<'c> {
    images: Vec<&'c PreparedImage>,
}

/// Cuts a string to `max` characters (not bytes, so a multi-byte character is never split).
pub fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// Converts a raw `bbox_2d` to the stored image's `[x, y, w, h]` by the adapter's format,
/// clamped to the image. A box with no area after clamping is `None`.
pub fn convert_box(raw: [i64; 4], format: BoxFormat, prepared: (u32, u32)) -> Option<BBox> {
    #[allow(
        clippy::cast_precision_loss,
        reason = "box coordinates are at most a few thousand"
    )]
    let corners = raw.map(|v| v as f64);
    format
        .to_bbox(corners, prepared)
        .ok()
        .filter(|b| b.to_array()[2] > 0.0 && b.to_array()[3] > 0.0)
}

/// The findings call's answer.
pub fn findings(
    text: &str,
    images: &CallImages<'_>,
    species: &[SpeciesOption],
    organisms: &[OrganismOption],
    format: BoxFormat,
    certainty: &CertaintyMap,
    limits: Limits,
) -> Result<VlmFindings, Invalid> {
    let raw: VlmFindingsRaw = serde_json::from_str(text)?;
    let species_id: Option<SpeciesId> = if raw.species_guess.species == OTHER {
        None
    } else {
        Some(
            known(
                &raw.species_guess.species,
                species.iter().map(|s| (&s.id, s.id.clone())),
                |i| i.as_str(),
            )
            .ok_or_else(|| Invalid::Species(raw.species_guess.species.clone()))?,
        )
    };
    let soil_findings = soil_findings(raw.soil, images, certainty)?;
    let organism_findings =
        organism_findings(raw.organisms, organisms, images, format, certainty, limits)?;
    let damage_findings = damage_findings(raw.damage, images, format, certainty, limits)?;
    Ok(VlmFindings {
        species_guess: SpeciesGuess {
            other_name: species_id
                .is_none()
                .then(|| {
                    raw.species_guess
                        .other_name
                        .as_deref()
                        .map(|n| truncate(n, limits.other_name))
                })
                .flatten(),
            species_id,
            confidence: certainty.confidence(raw.species_guess.certainty),
        },
        soil: soil_findings,
        organisms: organism_findings,
        damage: damage_findings,
        notes: truncate(&raw.notes, limits.notes),
    })
}

/// One question as decoding needs it: its key, its indicator, whether only `unsure` is allowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expected {
    /// `q1_concentric_rings`.
    pub key: String,
    /// The indicator.
    pub indicator_id: IndicatorId,
    /// No image of the call can show it.
    pub unsure_only: bool,
}

/// The indicators call's answer: exactly one answer per question, in question order.
pub fn indicators(
    text: &str,
    images: &CallImages<'_>,
    expected: &[Expected],
    limits: Limits,
) -> Result<VlmIndicatorAnswers, Invalid> {
    let mut raw: VlmIndicatorsRaw = serde_json::from_str(text)?;
    let mut answers = Vec::with_capacity(expected.len());
    for e in expected {
        let a = raw
            .remove(&e.key)
            .ok_or_else(|| Invalid::MissingKey(e.key.clone()))?;
        if e.unsure_only && a.answer != Answer::Unsure {
            return Err(Invalid::UnsureOnly(e.key.clone()));
        }
        let image_id: Option<ImageId> = a
            .image
            .map(|al| images.get(al))
            .transpose()?
            .map(|p| p.image_id);
        answers.push(IndicatorAnswer {
            indicator_id: e.indicator_id.clone(),
            answer: a.answer,
            reason: truncate(&a.reason, limits.reason),
            image_id,
        });
    }
    if let Some(extra) = raw.keys().next() {
        return Err(Invalid::ExtraKey(extra.clone()));
    }
    Ok(VlmIndicatorAnswers { answers })
}

/// The guided call's answer; its branch must be the image's kind.
pub fn guided(
    text: &str,
    kind: GuidedKind,
    certainty: &CertaintyMap,
) -> Result<GuidedTexture, Invalid> {
    let raw: VlmGuidedRaw = serde_json::from_str(text)?;
    match (kind, raw) {
        (
            GuidedKind::SqueezeTest,
            VlmGuidedRaw::SqueezeTest {
                result,
                certainty: c,
            },
        ) => Ok(GuidedTexture::SqueezeTest {
            result,
            confidence: certainty.confidence(c),
        }),
        (
            GuidedKind::JarTest,
            VlmGuidedRaw::JarTest {
                settled,
                texture,
                certainty: c,
            },
        ) => Ok(GuidedTexture::JarTest {
            settled,
            texture,
            confidence: certainty.confidence(c),
        }),
        _ => Err(Invalid::WrongBranch),
    }
}
```

---

## 10. The fallback worker

The fallback worker takes any job still unclaimed 15 s after `queued_at`, with doubled `max_runtime` (the job's own `max_runtime_s` already says so), and runs it on the hosted LLM ([architecture 8.4](../architecture/README.md#84-job-leasing-and-fallback); [vision 8](../vision/README.md#8-the-fallback-path); [vision vlm 12](../vision/vlm.md#12-the-fallback-worker)).

| Kind | What it does |
|---|---|
| `vlm`, `verify`, `guided` | The same `VlmAdapter` with the hosted profile: `abs_px_prepared` boxes on images it won't resize (≤ 1568 px a side and ≤ 1.15 MP), the same prompts with the pixel box line, `output_config.format` with the schema **sanitised** by `integrations::anthropic::sanitize_schema` (it strips every keyword the hosted API rejects, such as `maxLength`, `maxItems` and the numeric bounds, and adds `additionalProperties: false` to every object; the full list is in [contracts 6](../architecture/contracts.md#6-vlm-structured-output) and [integrations](integrations.md)), effort `low`, no `temperature`; decoding truncates and clamps afterwards. `stop_reason: max_tokens` and `refusal` are validation failures |
| `perception` with `question_id` | Completed at once with no regions and no LLM call: its only purpose is masks ([contracts 5, results](../architecture/contracts.md#results)) |
| `perception` | Up to 4 images in the findings order. One survey call (plant present, which images show leaves, the species top 3 from the payload's `label_set_version`), then one disease call over the leaf close-ups restricted to the working species' classes of the payload's `classifier_labels_version`. One whole-image `leaf` region per close-up that shows leaves (`<image_id>.1`, no mask); confidences from certainty words, never a softmax; no `image_embeddings` |

```rust
/// The label lists the fallback needs to answer `perception` jobs in the payload's terms.
#[derive(Debug, Default)]
pub struct FallbackLabels {
    species: BTreeMap<String, Arc<LabelMatrix>>,
    disease: BTreeMap<String, Arc<DiseaseHead>>,
}

/// The fallback model.
pub struct FallbackModel {
    vlm: VlmAdapter<AnthropicBackend>,
    labels: FallbackLabels,
}
```

The label lists come from the same manifest files the perception worker loads, because the payload carries only their versions; a version the fallback hasn't loaded fails with `unknown_label_version`, as on the rig.

The fallback's box format is confirmed with one live call on day 2 ([contracts 12](../architecture/contracts.md#12-open-questions), question 26).

---

## 11. Multi-vendor GPUs

The rig has three cards from two vendors ([architecture 10](../architecture/README.md#10-models-and-gpu-placement); [vision 4](../vision/README.md#4-placement-and-vram-budgets)):

| Card | Worker | Server | Settings |
|---|---|---|---|
| RTX 3070 (8 GB, sm_86) | `--caps perception`, `WORKER_ID=rig-3070-perception-1` | ONNX Runtime 1.31, CUDA 13 EP, in the worker | `ORT_EP=cuda`, `ORT_DYLIB_PATH` to the image's `libonnxruntime.so.1.31.0` |
| RTX 5070 Ti (16 GB, sm_120) | `--caps vlm`, `WORKER_ID=rig-5070ti-vlm-1` | vLLM v0.31.0 (CUDA 13) sidecar | `VLM_ADAPTER=vllm`, `VLM_SERVER=vllm@v0.31.0`, `VLM_BOX_FORMAT=norm1000`, `VLM_CELL_PX=32`, `VLM_SLOTS` from its benchmark |
| RX 9070 XT (16 GB, gfx1201) | `--caps vlm`, `WORKER_ID=rig-9070xt-vlm-1` | vLLM ROCm v0.31.0 sidecar, or llama.cpp Vulkan | `VLM_ADAPTER=vllm` (or `llamacpp`), `VLM_SERVER=vllm-rocm@v0.31.0` (or `llama.cpp-vulkan@<build>`), same box format and cell, its own `VLM_SLOTS` |

- **The VLM workers need no GPU**: they are HTTP clients of their sidecars, and one worker image serves both vendors. A scan's `vlm` job and a later `verify` or `guided` job may land on different cards; every VLM worker serves the same checkpoint, so results are comparable, not identical, and `model_versions` says which card and server produced each ([contracts 5](../architecture/contracts.md#leases-runs-and-attempts)). A worker whose `vlm_model` differs from the others is a configuration error shown on `/status`.
- **Two VLM workers race for jobs**: the claim transaction gives each job to exactly one, oldest first; nothing in the worker changes. If the 9070 XT is slower, `VLM_SLOTS=1` there lets it take the overflow.
- **ONNX Runtime on AMD** is possible but not used: perception stays on the 3070. Should it move, the same worker builds with `--features migraphx` (AMD's ORT 1.29 from `rocm/onnxruntime`, which the default `api-29` feature accepts) or `--features webgpu` (Dawn on Vulkan, no ROCm), and `ORT_EP` picks it at start ([vision 4](vision.md#4-onnx-runtime-environment-execution-providers-session-pool)). MIGraphX compiles a program per input shape, so batches would be padded to fixed sizes.

Which server runs on the 9070 XT is open ([contracts 12](../architecture/contracts.md#12-open-questions), question 25): on day 2 its boxes and indicator answers on 10 evaluation photos are compared with the 5070 Ti's.

---

## 12. Deployment

`infra/rig.compose.yml` ([architecture 12](../architecture/README.md#12-deployment)). NVIDIA services are pinned by **GPU UUID**, never by index, through the NVIDIA Container Toolkit; the AMD sidecar gets only `/dev/kfd` and **its own render node by PCI path**, never all of `/dev/dri` (which also holds the NVIDIA nodes), plus the host's numeric `render` and `video` group ids. Images are pinned by digest; the ROCm digest is from the AMD research, the CUDA image's tag and digest are confirmed on day 1.

```yaml
x-worker: &worker
  image: ghcr.io/aacruzgon/greenlens/worker@sha256:<digest>
  restart: unless-stopped
  stop_grace_period: 30s            # DRAIN_SECONDS (20), the shutdown's bound, + 10 s (section 5)
  environment: &worker-env
    API_BASE_URL: https://api.example.org

services:
  perception-worker:                # RTX 3070
    <<: *worker
    command: ["--caps", "perception"]
    env_file: rig-3070-perception-1.env   # WORKER_TOKEN (never committed)
    environment:
      <<: *worker-env
      WORKER_ID: rig-3070-perception-1
      ORT_EP: cuda
      ORT_DYLIB_PATH: /opt/onnxruntime/lib/libonnxruntime.so.1.31.0
      MODEL_MANIFEST: /models/manifest.toml
    volumes: ["models:/models:ro"]
    deploy:
      resources:
        reservations:
          devices: [{ driver: nvidia, device_ids: ["GPU-<3070 uuid>"], capabilities: [gpu] }]

  vllm-nv:                          # RTX 5070 Ti
    image: vllm/vllm-openai@sha256:<digest of v0.31.0>
    command: >
      Qwen/Qwen3-VL-8B-Instruct-FP8 --revision <sha> --served-model-name vlm
      --max-model-len 16384 --max-num-seqs 2 --max-num-batched-tokens 8192
      --gpu-memory-utilization 0.90 --enable-prefix-caching
      --limit-mm-per-prompt '{"image":{"count":4,"width":1152,"height":864},"video":0}'
      --mm-processor-kwargs '{"size":{"longest_edge":1003520,"shortest_edge":65536}}'
      --structured-outputs-config.backend xgrammar
      --structured-outputs-config.disable_any_whitespace true
      --generation-config vllm --api-key ${VLM_API_KEY}
    volumes: ["hf-cache:/root/.cache/huggingface"]
    ipc: host
    deploy:
      resources:
        reservations:
          devices: [{ driver: nvidia, device_ids: ["GPU-<5070 Ti uuid>"], capabilities: [gpu] }]

  vlm-worker-nv:
    <<: *worker
    command: ["--caps", "vlm"]
    env_file: rig-5070ti-vlm-1.env  # WORKER_TOKEN, VLM_API_KEY (never committed)
    depends_on: [vllm-nv]
    environment:
      <<: *worker-env
      WORKER_ID: rig-5070ti-vlm-1
      VLM_BASE_URL: http://vllm-nv:8000
      VLM_ADAPTER: vllm
      VLM_MODEL: Qwen/Qwen3-VL-8B-Instruct-FP8@<sha>
      VLM_QUANT: fp8_block
      VLM_SERVER: vllm@v0.31.0
      VLM_BOX_FORMAT: norm1000
      VLM_CELL_PX: "32"
      VLM_MAX_PIXELS: "1003520"
      VLM_SLOTS: "2"                # = --max-num-seqs; from the day-2 benchmark

  vllm-amd:                         # RX 9070 XT
    image: vllm/vllm-openai-rocm@sha256:749f6f3f944f12af49966ac523c1f8573e4b229594b541954bd5f879b4496b1f
    command: >                      # the same checkpoint, revision and flags as vllm-nv
      Qwen/Qwen3-VL-8B-Instruct-FP8 --revision <sha> --served-model-name vlm
      --max-num-seqs 1 ...
    devices:
      - /dev/kfd
      - /dev/dri/by-path/pci-0000:<bus>:00.0-render
    group_add: ["<host render gid>", "<host video gid>"]
    security_opt: [seccomp=unconfined]
    cap_add: [SYS_PTRACE]
    ipc: host
    environment:
      FLASH_ATTENTION_TRITON_AMD_ENABLE: "TRUE"   # no HSA_OVERRIDE_GFX_VERSION: gfx1201 is supported

  vlm-worker-amd:
    <<: *worker
    command: ["--caps", "vlm"]
    env_file: rig-9070xt-vlm-1.env  # WORKER_TOKEN, VLM_API_KEY (never committed)
    depends_on: [vllm-amd]
    environment:
      <<: *worker-env
      WORKER_ID: rig-9070xt-vlm-1
      VLM_BASE_URL: http://vllm-amd:8000
      VLM_ADAPTER: vllm
      VLM_MODEL: Qwen/Qwen3-VL-8B-Instruct-FP8@<sha>
      VLM_QUANT: fp8_block           # awq_w4a16 on both cards if FP8 doesn't run on gfx1201
      VLM_SERVER: vllm-rocm@v0.31.0
      VLM_BOX_FORMAT: norm1000
      VLM_CELL_PX: "32"
      VLM_MAX_PIXELS: "1003520"
      VLM_SLOTS: "1"

volumes: { models: {}, hf-cache: {} }
```

- **If llama.cpp serves the 9070 XT**: `ghcr.io/ggml-org/llama.cpp:server-vulkan` at a pinned build and digest, with only the render node; `-np` equal to `VLM_SLOTS`, `--image-max-tokens` at least `VLM_MAX_PIXELS / 32²` and no `--image-min-tokens`; `VLM_ADAPTER=llamacpp`; and both cards then serve the same GGUF files ([vision vlm 4](../vision/vlm.md#4-serving)). Port 8080 inside the image.
- **A second host**: if the 9070 XT goes in another machine, `infra/rig-amd.compose.yml` holds only `vllm-amd` and `vlm-worker-amd`; nothing else changes, since workers only pull.
- **The fallback worker** runs on the control plane (`infra/control-plane.compose.yml`) with `--caps fallback`, `LLM_API_KEY`, `MODEL_MANIFEST` pointing at the label files, and no GPU. Its grace period follows the same rule as the rig's services ([architecture 12](../architecture/README.md#12-deployment)): `DRAIN_SECONDS` plus 10 s, since its shutdown has the same bound.

  ```yaml
  fallback-worker:                  # infra/control-plane.compose.yml
    image: ghcr.io/aacruzgon/greenlens/worker@sha256:<digest>
    restart: unless-stopped
    stop_grace_period: 30s          # DRAIN_SECONDS (20), the shutdown's bound, + 10 s (section 5)
    env_file: fallback.env          # WORKER_TOKEN, LLM_API_KEY (never committed)
    environment:
      WORKER_CAPS: fallback
      WORKER_ID: cp-fallback-1
      API_BASE_URL: http://api:8080
      MODEL_MANIFEST: /models/manifest.toml
  ```
- **Images**: the worker image installs CA certificates (reqwest 0.13's rustls verifier fails without them) and, for perception, the CUDA 13 runtime with cuDNN 9 and ONNX Runtime 1.31's libraries next to `ORT_DYLIB_PATH` (`nvidia/cuda:13.0.3-cudnn-runtime-ubuntu24.04` as the base); the host has NVIDIA driver 580.65.06 or later ([vision 5](../vision/README.md#5-runtimes-and-versions)).

---

## 13. Concurrency and lifetimes

| Task | Lives | Token |
|---|---|---|
| Heartbeat | until after the drain | `stop_heartbeat` (not a child of `shutdown`, so draining jobs keep their leases); it also drops a call in flight |
| Claim loop | until SIGTERM | `shutdown`; `biased` `select!` drops an open long poll |
| One task per job, on the `TaskTracker` | until `complete` or cancellation | a child of `abandon` |
| CPU work (decode, resize, JPEG, pre-processing) | one blocking task per batch | an `OwnedSemaphorePermit` moved into the closure |
| ONNX Runtime | the inference thread (perception) | `TerminateOnDrop` |

**`'static` job tasks.** `TaskTracker::spawn` requires `F: Future + Send + 'static`: the task may outlive `Worker::start` and runs on any runtime thread. So the task's future owns everything it touches: `Arc` clones of the API and runner, the owned `ClaimedJob`, the slot permit, the `Registered` guard and the token. Inside it, `run_job(&api, &runner, …)` borrows those owned values for the task's own duration, which is fine: the borrow is of the task's state, not of the caller's.

**`impl Future + Send` in `WorkerApi` and `VlmBackend`.** Both traits are used generically, never as `dyn`, so they return `impl Future` (return-position `impl Trait` in traits) instead of a boxed future, and spell out `+ Send` because callers' futures must be `Send` to be spawned or awaited inside `#[async_trait]` methods (which box a `Send` future). Under edition 2024 the returned future captures every in-scope lifetime, here `&self` and the borrowed arguments, which is what a future that reads them needs; `VlmBackend::call<'a>` names `'a` because the future borrows both `&'a self` and the `&'a VlmCall<'a>`.

**Named lifetimes.**

| Lifetime | Where | What it borrows |
|---|---|---|
| `'a` | `VlmCall<'a>`, `VlmImage<'a>` | the call's prompts and schema and the job's prepared JPEG bytes; the backend serialises the request once and keeps none of it |
| `'c` | `CallImages<'c>` | the job's prepared images, to map aliases back while decoding |
| `'p` | `schema::Question<'p>` | the payload's indicators, to build the schema, the prompt and the expected keys without copying them |

**`Send` across `spawn_blocking`.** `prepare_all` moves owned clones of the images (each `Bytes` clone is a reference count) and of the profile into the closure; the result (`Vec<PreparedImage>`) is owned and `Send`. Nothing borrowed crosses.

**Shared state.** The registry of held jobs is a `std::sync::Mutex<HashMap<JobId, Entry>>`, never held across an `.await`; each critical section is one insert, remove or read, so a poisoned lock is still consistent (`PoisonError::into_inner`). `Registered`'s `Drop` removes a job from the heartbeat list however its task ends, before its slot is freed.

---

## 14. Errors

| Failure | `complete` | Retryable |
|---|---|---|
| `v` other than 1, or a `v: 1` job whose kind or payload this build can't read | `unsupported_version` | no |
| A label list or classifier mapping not loaded (`vision::VisionError::UnknownLabelVersion`, found by downcasting the model error's source) | `unknown_label_version` | no |
| A model failure, a VLM server or LLM API error, output invalid twice | `model_error` | yes (`ModelError::retryable`) |
| The adapter can't do the kind (`ModelError::Unsupported`) | `model_error` | no |
| An image fetch that fails after its retries | `image_fetch_failed` | yes |
| The soft deadline | `max_runtime_exceeded` | yes |
| `409 lease_lost` anywhere, `cancel_job_ids`, the drain | nothing is sent | |

```rust
/// Maps a model's error to the `complete` error ([contracts 5], `JobErrorCode`):
/// `unknown_label_version` (not retryable) when the source is a `vision` label-version error,
/// `model_error` otherwise, retryable unless the adapter doesn't do the job at all.
pub fn job_error(e: &ModelError) -> JobError {
    let source = std::error::Error::source(e).and_then(|s| s.downcast_ref::<vision::VisionError>());
    if source.is_some_and(vision::VisionError::is_unknown_label_version) {
        return JobError::new(JobErrorCode::UnknownLabelVersion, chain(e));
    }
    JobError {
        code: JobErrorCode::ModelError,
        message: chain(e),
        retryable: e.retryable(),
    }
}
```

The message is the error and its `source()` chain, `": "`-joined ([rust-errors](#18-sources)). `JobErrorCode` keeps unknown codes as `Other(String)`, so an older API reading a newer worker's code doesn't fail to decode.

---

## 15. Testing

| Test | Shows |
|---|---|
| `holds_at_most_slots_jobs` | With 2 slots and hung models, exactly 2 claims in 60 s |
| `a_finished_job_completes_with_versions_and_timings` | `done` with `worker_id` in `model_versions` and the kind in `timings_ms` |
| `lease_lost_from_a_progress_event_drops_the_job` | A `409 lease_lost` on `events` cancels the job's token |
| `heartbeat_lists_jobs_and_cancels_the_listed_ones` | `current_jobs` carries the lease; `cancel_job_ids` cancels the token |
| `another_version_fails_with_unsupported_version` | `failed`, `unsupported_version`, not retryable, model never runs |
| `an_image_that_cannot_be_fetched_fails_the_job_retryably` | `image_fetch_failed`, retryable |
| `complete_is_retried_after_a_timeout` | Two failed `complete`s, then one lands |
| `a_hung_model_is_stopped_before_max_runtime` | `max_runtime_exceeded` at 105 s for `max_runtime_s: 120` |
| `shutdown_abandons_jobs_after_the_drain_bound` | The drain waits exactly 20 s, sends nothing, frees the slot |
| `claim_errors_back_off_instead_of_spinning` | 6–8 claims in 60 s against an unreachable API |
| `claim_errors_wait_for_retry_after` | Claims at 0, 20 and 40 s after errors with `retry_after_s: 20` |
| `complete_waits_for_retry_after` | After a `503` with `retry_after_s: 7`, the next `complete` goes out 7 s later, not after the 0.25–0.5 s backoff |
| `a_complete_in_flight_does_not_hold_up_the_drain` | A `complete` that never answers is dropped at the 20 s bound, and the slot is freed |
| `stopping_the_heartbeat_drops_a_call_in_flight` | The heartbeat loop ends at once, not after the call's 5 s |
| `the_whole_shutdown_ends_within_the_drain_bound` | `run_until` with a hung `complete` and a hung heartbeat returns exactly 20 s after the signal |
| `classify_body` tests (5) | `lease_lost` only from a `409` with that code; `retryable: true` → `Unavailable` with `retry_after_s`; `retryable: false` → `Rejected` whatever the status (`500 internal`, `429 budget_exceeded`); without an error body only `429` and `5xx` are retried |
| `a_cut_off_answer_is_retried_once_then_succeeds` | `finish_reason: length`, then a valid answer; `vlm_indicators` skipped with `all_measured` |
| `two_invalid_answers_fail_with_a_retryable_model_error` | Unparseable, then a refusal → `InvalidOutput`, retryable |
| `images_are_prepared_on_the_cell_grid` | A 64 × 48 image becomes 64 × 32 on the 32-pixel grid |
| schema, decode, prepare unit tests | `reason` before `answer` in the serialised schema; `unsure`-only questions; payload enums and box bounds; one guided branch; aliases, boxes, truncation and certainty mapped; a zero-area organism box drops the organism; unknown species and missing keys fail; Qwen3 cells and the pixel budget; the hosted 1568-pixel cap |

The stub's tests are in [section 15.1.5](#1515-tests) (proposed). All loop tests run on paused time (`start_paused = true`) against a fake `WorkerApi`; the fake answers an empty claim like a 25 s long poll, so the clock can advance. Contract tests of `HttpWorkerApi` against a live `api` belong to the end-to-end tests ([api](api.md)). The day-2 benchmark ([vision evaluation](../vision/evaluation.md)) checks the adapters against the real servers: valid output, xgrammar's enforcement of `maxLength` and `maxItems`, boxes drawn back on the prepared images, and the 9070 XT against the 5070 Ti.

### 15.1 Stub vision

The roadmap's first slice (S1, [roadmap 5](roadmap.md#5-s1-walking-skeleton)) runs a scan from the app through the API and the broker to a worker and back over SSE, before any GPU or model file exists. The worker has only the real perception path and the hosted fallback, so S1 has nothing to claim a `perception` job ([roadmap gap 2](roadmap.md#12-gaps-found-while-writing-this-roadmap)). **Stub vision** is a `VisionModel` that answers `perception` jobs with canned, valid results. It is a development tool. It is off by default, it is refused in a release build unless the operator says so twice, and it marks everything it produces.

Nothing in this section is compiled yet. The listings are labelled "Proposed (not compiled)", and the numbers in section 15.1.7 are the work items.

#### 15.1.1 Turning it on

| Setting | Name | Default | Meaning |
|---|---|---|---|
| Flag | `--stub-vision` (env `WORKER_STUB_VISION`) | off | Use `StubModel` instead of the real model for `--caps perception` |
| Allow variable | `PDM_ALLOW_STUB_VISION=1` | unset | Required in a release build (`cfg!(debug_assertions)` is false). Debug builds and `cargo test` don't need it |
| `STUB_LABEL_SET_VERSION`, `STUB_CLASSIFIER_LABELS_VERSION` | env only | none, **required** | The label versions the stub reports in its heartbeat (comma-separated when several). Set them to the values the API queues, which are the knowledge base's `label_set_version` and the classifier mapping's version |
| `STUB_SPECIES` | env only | `tomato` | Comma-separated species slugs used as candidates (section 15.1.2) |
| `STUB_PROBLEMS` | env only | empty | Comma-separated problem slugs the disease stage may add below `healthy` |
| `STUB_PLANT_PRESENT` | env only | `true` | `false` makes the plant check fail, to exercise `no_plant_found` |
| `STUB_STAGE_DELAY_MS` | env only | 150 | Pause inside each stage, so the SSE stream shows the stages in order and a test can kill the worker mid-job. `10000` leaves a window of about a minute for the lease-expiry check |

**The rules, all checked before the first claim** (a refusal exits with a message that names the rule; nothing is claimed):

1. `--stub-vision` is valid only with `--caps perception`. With `vlm` or `fallback` it is refused: a VLM job is the real VLM's or the hosted model's, and a canned `VlmResult` would put invented findings into a diagnosis.
2. In a release build it is refused unless `PDM_ALLOW_STUB_VISION=1`. The flag alone isn't enough, so a flag left in a compose file by mistake can't start it, and the rig's `infra/rig.compose.yml` never sets the variable.
3. Both `STUB_*_VERSION` variables must be set. The API queues a `perception` job only when a live perception or fallback worker reports the job's label versions in its heartbeat; with live workers of which none has both, submit fails with `503 upstream_unavailable` ([contracts 5](../architecture/contracts.md#post-internaljobsidcomplete)). The stub can't read a manifest (section 15.1.4), so the operator names the versions.
4. `MODEL_MANIFEST`, `ORT_*` and the `*_MAX_BATCH` variables are ignored. The manifest isn't loaded, no ONNX Runtime is initialised, and no GPU or model file is needed.

```text
// Proposed (not compiled): bins/worker/src/config.rs
#[derive(Debug, Args)]
pub struct StubArgs {
    #[arg(long, env = "WORKER_STUB_VISION")]
    pub stub_vision: bool,
    #[arg(long, env = "PDM_ALLOW_STUB_VISION")]
    pub allow_stub_vision: bool,
    #[arg(long, env = "STUB_LABEL_SET_VERSION")]
    pub label_set_version: Option<String>,
    #[arg(long, env = "STUB_CLASSIFIER_LABELS_VERSION")]
    pub classifier_labels_version: Option<String>,
    #[arg(long, env = "STUB_SPECIES", value_delimiter = ',', default_value = "tomato")]
    pub species: Vec<String>,
    #[arg(long, env = "STUB_PROBLEMS", value_delimiter = ',')]
    pub problems: Vec<String>,
    #[arg(long, env = "STUB_PLANT_PRESENT", default_value_t = true)]
    pub plant_present: bool,
    #[arg(long, env = "STUB_STAGE_DELAY_MS", default_value_t = 150)]
    pub stage_delay_ms: u64,
}

/// A pure function, so a test can pass `release` instead of changing its own build.
pub fn check_stub_allowed(cli: &Cli, release: bool) -> Result<(), StubRefused> { /* rules 1 to 3 */ }
```

`build_model` calls `check_stub_allowed(&cli, !cfg!(debug_assertions))` first and returns `StubModel::new(StubConfig::from(&cli.stub))` when the flag is on. It logs one `warn!` line, `stub vision is on: results are canned`, and the worker's tracing span carries `stub_vision = true` for the life of the process.

#### 15.1.2 What a stub result contains

`StubModel::perceive` reads only the payload: image ids, kinds and sizes, the stage list, `species_id`, `skip_species_identification` and `regions`. It never looks at pixels. It runs the stages in the payload's `stages`, in the contract's order, and for each one sends `stage_started`, a `partial_result` and `stage_completed`, with a pause of `STUB_STAGE_DELAY_MS` before the last of the three. `complete` then repeats every stage's output, so the API finds nothing to change ([contracts 5](../architecture/contracts.md#post-internaljobsidcomplete)). Every number goes through `Confidence::new` and `BBox::new`, which reject anything the API's decoding would.

| Field | Stub value |
|---|---|
| `plant_present` | Set only when `plant_check` is in `stages`: `STUB_PLANT_PRESENT` (default `true`) |
| `regions` | Only with `segmentation`. Per image, numbered from 1 as `<image_id>.<n>`: `whole_plant` gives one `plant` region, box `x 0.10, y 0.10, w 0.80, h 0.80`; `leaf_closeup` and `leaf_underside` give one `leaf` region, box `0.15, 0.15, 0.70, 0.70`; `soil_surface` gives one `soil` region, box `0.00, 0.50, 1.00, 0.50`; `flower_fruit` gives none. Confidence 0.90. **No `mask`**: masks are optional, and the API measures only what needs no mask. A question photo's job (`question_id` set) gets the same rule for its one image |
| `species` | Only with `species`, and only when the payload has no `species_id` and doesn't set `skip_species_identification`. Up to three candidates from `STUB_SPECIES`, in order, with scores 0.62, 0.21 and 0.09 (the top one is a medium confidence, so a plant with only this stage counts as readable, [contracts 3](../architecture/contracts.md#3-scan-rest-api)). `species_id` is the slug, `label` is `"<slug> (stub)"`. A slug the knowledge base doesn't know is the operator's mistake: the API keeps `knowledge_ref` and the candidate stays unmatched |
| `disease` | Only with `disease`. For each leaf region of this job (or of the payload's `regions` when segmentation isn't re-run) on a `leaf_closeup` or `leaf_underside` image, one entry with its `region_id`. `top` is `healthy` at 0.70, then the first two `STUB_PROBLEMS` at 0.20 and 0.10, in that order, so it is sorted and has at most three entries. The working species is the payload's `species_id` or the first candidate; the stub doesn't restrict `top` to its labels, because it has no label file |
| `disease_skipped` | `no_leaves_found` when `disease` is requested and there is no leaf region and no close-up. Never `species_not_supported` |
| `image_embeddings` | **Absent**, always. The field is optional. A made-up vector stored with an embedding model name would corrupt the similar-case search the day a real one is stored beside it. The `image_embedding` stage is reported complete with `skipped: { reason: "stub" }` |

`analyse`, `verify` and `read_guided_check` return `ModelError::Unsupported`, so a `vlm`, `verify` or `guided` job that reaches the stub (it can't: the claim filters by `caps`) would complete `failed`, `model_error`, not retryable.

**What the API checks, and why a stub result passes.** The API decodes `result` as the job's recorded kind (`JobResult::decode`), and a result that doesn't decode is stored as a failure with `model_error` ([contracts 5](../architecture/contracts.md#post-internaljobsidcomplete)). Decoding rejects a confidence outside 0 to 1 and a box outside the image. Nothing else is checked: `perception_writes` copies regions, candidates and `top` entries into observations, and no code reads `image_embeddings` yet. The one check that depends on the worker is the label-version gate in rule 3 above. The stub passes it only because the operator set the versions.

#### 15.1.3 How a stub result is recognised

A stub result must never be taken for a real one, in a log or in stored data.

| Where | Mark |
|---|---|
| `complete.model_versions` and the heartbeat's `versions` | `stub: "1"`, `perception_config: "stub"`, and `stub` as the value of `leaf_detector`, `bioclip` and `disease_head`. `worker_id` is added by `JobRunner` as for every worker. `label_set_version` and `classifier_labels_version` are the job's own, as for a real worker. No `ort_ep` |
| Stored observations | `model_version` is `stub`: the API takes it from `vlm_model`, then from `perception_config` (`complete.model_versions`) |
| Candidate labels | `"<slug> (stub)"` |
| `WORKER_ID` | By convention `dev-stub-perception-1`; the compose file for S1 uses it |
| Logs | The `warn!` at start-up, and `stub_vision = true` on the worker's span, so every line has it |
| Stage event | The `image_embedding` skip reason `stub` |

The API can't tell a stub from a real worker by anything but `model_versions`. Whether it should refuse them in production is open (section 17).

#### 15.1.4 What the stub never does

- **No network.** `StubModel` holds no `reqwest` client, no `integrations` client and no `ort` session. The worker loop's own calls to the API (heartbeat, claim, events, image fetch, `complete`) are unchanged, and that includes the image fetch: it exercises the API's worker image route, which S1 wants. The stub ignores the bytes and never decodes them.
- **No files.** It reads no manifest, no label file and no model file, and writes nothing. Its only inputs are the payload and the environment.
- **No randomness and no clock.** Equal payloads give equal results, so a test can compare them and the smoke script can assert on them. The only wait is `tokio::time::sleep`, which a cancellation drops like any other `.await`.
- **No other kind.** It answers `perception` and nothing else (section 15.1.1, rule 1).

#### 15.1.5 Tests

| Test | Shows |
|---|---|
| `stub_is_refused_in_release_without_the_allow_variable` | `check_stub_allowed(cli, true)` fails; with `allow_stub_vision` it passes; with `release = false` it passes without it |
| `stub_is_refused_with_vlm_and_fallback_caps` | Rule 1, for both caps |
| `stub_needs_both_label_versions` | A missing `STUB_LABEL_SET_VERSION` or `STUB_CLASSIFIER_LABELS_VERSION` is refused, naming the variable |
| `stub_regions_follow_the_image_kinds` | A full scan's five photo kinds give the plant, leaf, leaf, soil regions and none for `flower_fruit`, ids `<image>.1`, no mask |
| `stub_result_decodes_as_the_jobs_kind` | The serialised result passes `JobResult::decode(JobKind::Perception, …)` for every subset of `stages`, and every box and confidence is in range |
| `stub_follows_the_payloads_species_choice` | With `species_id` or `skip_species_identification` there is no `species`; disease uses the given species; without either, three candidates in descending score |
| `stub_disease_is_sorted_and_skips_without_leaves` | `top` has at most three entries in descending order; no leaf regions and no close-up give `disease_skipped: no_leaves_found` |
| `stub_never_sends_embeddings` | `image_embeddings` is `None`, and the `image_embedding` stage is skipped with reason `stub` |
| `stub_marks_every_version` | `model_versions` has `stub: "1"`, `perception_config: "stub"` and the three model keys as `stub` |
| `stub_module_has_no_io` | The module's source (`include_str!`) contains none of `reqwest`, `std::fs`, `tokio::fs`, `std::net`, `ort`, `rand` or `SystemTime` |
| `a_stub_job_completes_through_the_loop` | The existing loop test with a fake `WorkerApi` and a `StubModel`: `done`, `stub` in `model_versions`, the stage events in order |
| `a_cancelled_stub_job_stops_at_its_delay` | With `STUB_STAGE_DELAY_MS = 10_000` on paused time, a cancel after 3 s ends the job and sends nothing |

One more test belongs to `pipeline`, since the stub's output is its input: `a_stub_perception_result_stores_its_observations`. A fixture of the stub's full result for a five-photo scan goes through `job_completed`, and the stored observations carry `model_version: "stub"` and the `Retrieve` claim.

**The S1 exit check these enable.** With a stub worker on the control plane, the smoke script ([roadmap 5](roadmap.md#5-s1-walking-skeleton)) creates a garden, uploads photos, submits and follows the stream through the plant check, segmentation, species, disease and `image_embedding` stages. It then drops the connection and resumes with `Last-Event-ID`. With `STUB_STAGE_DELAY_MS=10000`, killing the worker mid-job requeues the job once its lease runs out, and a second stub worker finishes it. Restarting `api` during the scan and resuming replays each missed event once. These are the first two exit checks and the third.

#### 15.1.6 Where the stub stops

The scan goes on to the `retrieve` step and queues a `vlm` job. On the control plane, the fallback worker takes that job 15 s after `queued_at`, with a real `LLM_API_KEY` and a real cost. Without a key the scan waits at `analysing` with the `vlm` job queued. S1's exit checks hold either way, because they end at the perception stages and the requeue. A canned VLM result is not offered (rule 1); open question 7 asks whether the smoke test needs one.

#### 15.1.7 Work items

All are part of S1's work item 1 ([roadmap 5](roadmap.md#5-s1-walking-skeleton)). Each item's exit check is a test or a command.

| # | Work | Exit check |
|---|---|---|
| ST1 | `StubArgs`, `check_stub_allowed`, the `build_model` branch, the start-up `warn!` and span field | The three refusal tests pass; `worker --caps perception --stub-vision` with the allow variable starts in a release build on a machine with no GPU and no `/models` |
| ST2 | `StubModel` and its result builder | `stub_regions_follow_the_image_kinds`, `stub_result_decodes_as_the_jobs_kind`, `stub_follows_the_payloads_species_choice`, `stub_disease_is_sorted_and_skips_without_leaves` and `stub_never_sends_embeddings` pass; `cargo clippy -p worker --all-targets` is clean |
| ST3 | The marks of section 15.1.3 and `stub_module_has_no_io` | `stub_marks_every_version` and `stub_module_has_no_io` pass; the heartbeat of a started stub lists both label versions |
| ST4 | The stage events and the delay | `a_stub_job_completes_through_the_loop` and `a_cancelled_stub_job_stops_at_its_delay` pass on paused time |
| ST5 | `pipeline`'s fixture test, and a check that the stub's output decodes against `contracts.md` | `a_stub_perception_result_stores_its_observations` passes |
| ST6 | A `perception-stub-worker` service in S1's `control-plane.compose.yml` (`--caps perception --stub-vision`, `PDM_ALLOW_STUB_VISION=1`, the `STUB_*` variables, its own `env_file` with `WORKER_TOKEN`) | The worker's first heartbeat shows on the API; a submitted scan's perception job is claimed within a second and the stage events reach the stream |
| ST7 | The kill, restart and resume checks of section 15.1.5 in the S1 smoke script | S1's first three exit checks pass against the deployed control plane |

---

## 16. Decisions on the contracts' open questions

The questions of [contracts 12](../architecture/contracts.md#12-open-questions) that the worker settles. Contracts.md isn't edited here; the coordinator applies them.

| Question | Decision | Where |
|---|---|---|
| 18, job error codes | `unsupported_version` (no), `unknown_label_version` (no), `model_error` (yes; no when the adapter can't do the kind), `image_fetch_failed` (yes), and **`max_runtime_exceeded`** (yes) for a job the worker stopped at its soft deadline, as the skills assumed. Added to `domain::jobs::JobErrorCode`, which keeps unknown strings as `Other` | sections 6, 14; `domain::jobs` |
| 19, is `unsupported_version` retryable? | No: the same worker can't succeed on a retry, and a newer worker gets the job only when it is requeued after its attempts | `run_job` |
| 20, lease hand-back at shutdown | Confirm the wait: no hand-back call. At SIGTERM the claim loop drops an open long poll; a lease the server granted meanwhile is requeued when the heartbeat goes stale (30 s) | section 5 |
| 21, an unknown job `kind` | The claim already filters by the declared `caps`. If one slips through (an older worker, a newer API), the worker reads the head (`v`, `job_id`, `lease_token`) and completes `failed` with `unsupported_version`, not retryable, naming the problem in the message, instead of leaving it to expire | `decode_claim`, `run_job` |
| 23, a missing `X-Lease-Token` | The worker always sends it on every call about a job; whatever the API answers for a missing one (`400 invalid_request` in the skills) is a bug the worker logs and never retries (`CallError::Rejected`). The status itself is the API's decision | `HttpWorkerApi::leased`, `classify` |

---

## 17. Open questions

1. **`organism_options` without recognition text** ([vision vlm 7](../vision/vlm.md#7-prompts)) is still a proposal ([contracts open question 36](../architecture/contracts.md#12-open-questions)).
2. **The fallback's runtime.** Opus 5.5 always thinks, so every hosted call gets 8,000 output tokens ([contracts 6](../architecture/contracts.md#after-decoding)). Whether calls that long finish within the fallback's doubled `max_runtime` is measured on day 2 ([contracts open question 26](../architecture/contracts.md#12-open-questions)).
3. **`anyOf` with `null`** for nullable values in the schema. Settled on 2026-10-16 ([contracts open question 31](../architecture/contracts.md#12-open-questions), [vision vlm 4](../vision/vlm.md#4-serving)): xgrammar in vLLM v0.31.0 produces both branches and blocks values outside the schema. The non-null soil branch needs the prompt to show a reading's shape.
4. **Cancellation on llama.cpp**: whether a dropped connection frees the slot is *unverified* ([contracts open question 33](../architecture/contracts.md#12-open-questions)).
5. **Should `api` refuse stub results in production?** Today only `model_versions.stub` tells them apart. A setting such as `API_REJECT_STUB_VISION=true` on the deployed control plane, once S1 is over, would fail a `complete` that carries the key. The alternative is to rely on the release guard and the compose files alone (section 15.1.3).
6. **Is the `stub` key a contracts change?** The `model_versions` table in [contracts 5](../architecture/contracts.md#post-internaljobsidcomplete) lists keys by model entry. Adding `stub` and the value `stub` for `perception_config` needs a row there. The coordinator applies it.
7. **Does S1 need a canned `vlm` result?** Section 11.6 stops at the `vlm` job, and the fallback worker would answer it for a real LLM key. If the smoke test must reach `planned` without a key, a stub for `vlm`, `verify` and `guided` is a second flag with the same guard. It would need findings that name only knowledge-base ids, and a plan the scoring step accepts. It stays out until S1 shows the need.
8. **Where the label versions come from.** The stub gets them from two variables because the heartbeat needs them before any job, and it reads no file. Reading only the two version strings from the manifest's header would be a smaller burden for the operator, but it is a file read. They stay variables unless the S1 compose file shows them going stale after a knowledge reseed.
9. **Does the skeleton build need ONNX Runtime at all?** If the S1 worker image is built without the `cuda` feature and with `load-dynamic`, the stub starts with no `libonnxruntime`. This is *unverified*: ST1's exit check on a machine with no GPU settles it.
10. **Species and `image_embedding` events.** Section 11.2 sends no events for `species` when the payload names the species, and a `skipped: stub` for `image_embedding`. Whether `PerceptionPipeline` does the same is not compared yet; ST4 compares the event order of both.

**Resolved with docs/vision v0.2.** Contracts 5 and 6 and [vision 6](../vision/README.md#6-model-files-versions-and-pinning) and [vision vlm 11–12](../vision/vlm.md#11-validation-retries-and-versions) now agree on what this section used to list: the `model_versions` value formats (underscores, `@` versions, commas between prompts, slashes between certainty values); per-model keys named by their manifest entry, with SAM as `sam_encoder` and `sam_decoder`; `label_set_version` and `classifier_labels_version` as keys, comma-separated in the heartbeat; boxes with no area; box bounds for pixel formats; the hosted model's 1,568-pixel and 1.15-megapixel caps; the fallback loading its label files from the manifest; JPEG at quality 90 with each side rounded down; the 8,000-token hosted budget; and `BoxFormat`'s comment, which names `norm_1000` as the Qwen3-VL format and `abs_px_prepared` as the fallback's.

---

## 18. Sources

**Rust language**
- The Book: [16.4 `Send` and `Sync`](https://doc.rust-lang.org/book/ch16-04-extensible-concurrency-sync-and-send.html) · [10.3 lifetimes](https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html) · [17 async and await](https://doc.rust-lang.org/book/ch17-00-async-await.html) · [15.3 `Drop`](https://doc.rust-lang.org/book/ch15-03-drop.html)
- The Reference: [`impl Trait` in traits](https://doc.rust-lang.org/reference/types/impl-trait.html) · [dyn compatibility](https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility)
- [Edition Guide, RPIT lifetime capture](https://doc.rust-lang.org/edition-guide/rust-2024/rpit-lifetime-capture.html) · [Async Book](https://rust-lang.github.io/async-book/)
- tokio 1.53.3: [`spawn_blocking`](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html) · [`select!` fairness](https://docs.rs/tokio/1.53.3/tokio/macro.select.html#fairness) · [`MissedTickBehavior`](https://docs.rs/tokio/1.53.3/tokio/time/enum.MissedTickBehavior.html) · [`timeout_at`](https://docs.rs/tokio/1.53.3/tokio/time/fn.timeout_at.html) · [`Semaphore::acquire_owned`](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Semaphore.html#method.acquire_owned) · [paused time](https://docs.rs/tokio/1.53.3/tokio/attr.test.html#configure-the-runtime-to-start-with-time-paused)
- tokio-util 0.7.20: [`CancellationToken`](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html) · [`TaskTracker`](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html)

**Crates and servers**
- [reqwest 0.13.5](https://docs.rs/reqwest/0.13.5/reqwest/) · [clap 4.6.7 derive](https://docs.rs/clap/4.6.7/clap/_derive/index.html) · [serde_json `preserve_order`](https://docs.rs/serde_json/1.0.154/serde_json/#features) · [secrecy 0.10.3](https://docs.rs/secrecy/0.10.3/secrecy/)
- [vLLM structured outputs](https://docs.vllm.ai/en/stable/features/structured_outputs.html) · [vLLM Docker deployment (ROCm)](https://docs.vllm.ai/en/latest/deployment/docker.html) · [llama.cpp server](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md) · [Anthropic structured outputs](https://platform.claude.com/docs/en/build-with-claude/structured-outputs) · [Anthropic vision coordinates](https://platform.claude.com/docs/en/build-with-claude/vision-coordinates)
- [ROCm Docker](https://rocm.docs.amd.com/projects/install-on-linux/en/latest/how-to/docker.html) · [NVIDIA Container Toolkit with Compose](https://docs.docker.com/compose/how-tos/gpu-support/)

**Project documents, research and skills**
- [Data contracts](../architecture/contracts.md) · [architecture](../architecture/README.md) · [domain](domain.md) · [vision](vision.md) · [integrations](integrations.md) · [api](api.md)
- [Computer-vision design](../vision/README.md): [VLM](../vision/vlm.md), [models](../vision/models.md), [evaluation](../vision/evaluation.md)
- The backend research notes (APIs, AMD RX 9070 XT, model decisions)
- Skills in the main checkout's `.claude/skills/`: `rust-workers` (`SKILL.md`, `loop.md`, `reqwest.md`, `ort.md`), `rust-async`, `rust-errors` (`jobs.md`), `rust-testing`, `rust` (`versions.md`)

External links were taken from the research notes, which opened them on 2026-10-15; facts marked ✓ were compiled or run there or in the scratch workspace.
