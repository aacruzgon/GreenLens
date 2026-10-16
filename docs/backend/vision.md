# PDM Backend: `vision` crate

> **Status:** Draft v0.2.3, 2026-10-16. v0.2.3: open question 5 is open again: `ml/models/FORMATS.md` is the format of record, and the loader doesn't read it yet. v0.2.2: open question 5 is resolved: vision 6 now fixes the head and label-matrix JSON. v0.2.1: the section 1 note says the `rust-workers` skill follows this crate: `ort` with `load-dynamic` and `api-29`, providers as cargo features chosen by `ORT_EP`, one inference thread and the session pool. v0.2: the text embedder takes `domain::ports::EmbedMode` on each `embed` call instead of offering two pre-set instances (`queries()` and `documents()` are gone). v0.1: ONNX Runtime sessions with the execution provider chosen at run time, the perception models as one `VisionModel`, the CPU upload pipeline (decode, quality, two-pass face blurring, WebP, depth), LiDAR measurement as deterministic geometry, the text embedder and COCO RLE, compiled on Rust 1.99.1 with `ort` 2.0.0-rc.14.

Companion to the [backend architecture](README.md), the [`domain` crate](domain.md) whose ports this crate implements, and the [computer-vision design](../vision/README.md), whose model choices it follows. Every listing below is copied from the compiled source of `crates/vision` by the docgen tool. `cargo check -p vision` (also with `--features cuda`, `migraphx`, `webgpu`), `cargo clippy -p vision --all-targets` and `cargo test -p vision` pass: 53 unit tests, among them an end-to-end measurement of a synthetic leaf and byte-for-byte RLE fixtures from pycocotools. ONNX Runtime itself never runs in the tests: the default `load-dynamic` feature links nothing, and no test builds a session.

## Contents

1. [Purpose and boundaries](#1-purpose-and-boundaries)
2. [Dependencies](#2-dependencies)
3. [Module layout](#3-module-layout)
4. [ONNX Runtime: environment, execution providers, session pool](#4-onnx-runtime-environment-execution-providers-session-pool)
5. [The model manifest](#5-the-model-manifest)
6. [Perception](#6-perception)
7. [The upload pipeline](#7-the-upload-pipeline)
8. [Depth](#8-depth)
9. [Measurement](#9-measurement)
10. [The text embedder](#10-the-text-embedder)
11. [Masks and COCO RLE](#11-masks-and-coco-rle)
12. [Concurrency and lifetimes](#12-concurrency-and-lifetimes)
13. [Errors](#13-errors)
14. [Testing](#14-testing)
15. [Open questions](#15-open-questions)
16. [Sources](#16-sources)

---

## 1. Purpose and boundaries

**What it owns.**

| Area | Module | Implements | Called by |
|---|---|---|---|
| ONNX Runtime environment, providers, session pool | `runtime` | | everything below |
| Model files, hashes, configuration | `manifest` | | `api`, the perception and fallback workers |
| Perception stages on the RTX 3070 | `perception` | `VisionModel::perceive` | the `perception` worker |
| Decode, quality gate, faces, resize, WebP | `imaging`, `upload` | | `api` (`POST /scans/{id}/images`, chat images) |
| Depth validation, rotation, intrinsics, face zeroing | `depth` | | `upload`, the measurement stage |
| LiDAR measurement | `measure` | `SpotSegmenter` (the colour default) | `pipeline`, inside `api` |
| Text embeddings | `embed` | `TextEmbedder` | `api` (`agent`, `knowledge`) |
| Masks, COCO RLE | `mask`, `rle` | | perception (encode), `api` (decode to PNG), measurement |

**What it never does.** No network, no database, no object storage, no job protocol: callers hand in bytes and get bytes and records back. The VLM adapters are not here: they are HTTP clients and live in the [worker](worker.md). Answering measurable indicators from measurements belongs to `pipeline` ([pipeline](pipeline.md)); this crate only measures.

**Synchronous core, async edges.** Every CPU-heavy function is synchronous and borrows its buffers (`&[u8]`, `ImageView<'a>`, `DepthMap<'a>`); the caller runs it on a blocking thread. Only the port implementations (`PerceptionPipeline`, `ArcticEmbedder`) are async, and they move owned data onto a blocking thread or the inference thread themselves (section 12).

**The project skills.** The `rust-workers` skill (`ort.md`) follows this crate: one inference thread owns the perception models on the worker (section 6), `api`'s CPU models sit in a session pool (section 4), and `ort` is built with `load-dynamic` and `api-29`, not a fixed `cuda` build: the execution providers' bindings are cargo features (`cuda`, `migraphx`, `webgpu`), `ORT_EP` picks one at run time, and `ORT_DYLIB_PATH` names the vendor's ONNX Runtime, so one source builds the NVIDIA, AMD and CPU binaries ([AMD research](#16-sources)).

---

## 2. Dependencies

```toml
[package]
name = "vision"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

[features]
# One binary for every GPU vendor: nothing is linked at build time, and libonnxruntime is opened at
# start from ORT_DYLIB_PATH (Microsoft's 1.31 build on NVIDIA and the CPU, AMD's 1.29 on ROCm).
# api-29 is the lowest ONNX Runtime API the binary asks for, so AMD's 1.29 loads too.
default = ["load-dynamic", "api-29"]
load-dynamic = ["ort/load-dynamic"]
api-29 = ["ort/api-29"]
# Alternative to load-dynamic: link pyke's prebuilt ORT 1.31 (one execution provider family per build).
download-binaries = ["ort/download-binaries", "ort/tls-native", "ort/copy-dylibs"]
# Execution providers. With download-binaries, never enable cuda and migraphx together (no such dist).
cuda = ["ort/cuda"]
migraphx = ["ort/migraphx"]
webgpu = ["ort/webgpu"]
# HEIC uploads through the system libheif (>= 1.19, found by pkg-config).
heic = ["dep:libheif-rs"]

[dependencies]
domain.workspace = true

async-trait.workspace = true
bytes.workspace = true
chacha20.workspace = true
data-encoding.workspace = true
fast_image_resize.workspace = true
image.workspace = true
imageproc.workspace = true
libheif-rs = { workspace = true, optional = true }
nalgebra.workspace = true
ndarray.workspace = true
ort.workspace = true
png.workspace = true
rand.workspace = true
serde.workspace = true
serde_json.workspace = true
sha2.workspace = true
thiserror.workspace = true
tokenizers.workspace = true
tokio.workspace = true
tokio-util.workspace = true
toml.workspace = true
tracing.workspace = true
webp.workspace = true

[dev-dependencies]
domain = { workspace = true, features = ["test-util", "mock"] }
tokio = { workspace = true, features = ["test-util"] }
```

| Crate | Version | Why |
|---|---|---|
| `ort` | =2.0.0-rc.14, `std`, `ndarray`, `tracing` (+ `load-dynamic`, `api-29`) | ONNX Runtime 1.31 bindings. A release candidate, pinned exactly. `load-dynamic` opens the vendor's `libonnxruntime` at start (`ORT_DYLIB_PATH`), so one binary serves Microsoft's 1.31 build and AMD's 1.29 build; `api-29` is the lowest API level asked for ✓ (AMD research 2.2) |
| `ndarray` | 0.17.2 | Tensors; must be the minor `ort` uses |
| `image` | 0.25.10, `jpeg`, `png`, `webp` | Decode with EXIF orientation, JPEG encode for the VLM, crops and blurs |
| `imageproc` | 0.27.0, no default features | Laplacian and histogram for the quality gate |
| `fast_image_resize` | 6.1.0 | SIMD Lanczos3 resizing of 12-megapixel photos |
| `webp` | 0.3.1 | Lossy WebP through libwebp (`image` writes only lossless) ✓ |
| `png` | 0.18.1 | Exact 16-bit and 8-bit grayscale PNGs for depth and confidence ✓ |
| `libheif-rs` | 3.0.0, `v1_19` (feature `heic`) | HEIC uploads; needs a system libheif ≥ 1.19 ✓ |
| `nalgebra` | 0.35.0 | 3×3 SVD for plane fits, vectors |
| `chacha20`, `rand` | 0.10.2 (`rng`), 0.10.3 | A seeded RNG whose stream is specified, for reproducible RANSAC ✓ |
| `tokenizers` | 0.23.2, `fancy-regex` | The embedder's WordPiece tokenizer; `fancy-regex` instead of the default `onig`, so no C toolchain for oniguruma ✓ |
| `toml`, `sha2`, `data-encoding` | 1.1.8, 0.11.1, 2.11.1 | The manifest, its SHA-256 checks, base64 head matrices |
| `tokio`, `tokio-util`, `async-trait` | 1.53.3, 0.7.20, 0.1.92 | `spawn_blocking`, the inference thread's channels, the async ports |

The `fastembed` crate is not used: 7.1.1 pins `ort =2.0.0-rc.13`, and `ort-sys`'s `links = "onnxruntime"` forbids two versions in one build ✓ ([vision models 10](../vision/models.md#10-text-embedder)).

---

## 3. Module layout

```
crates/vision/src/
├─ lib.rs                  modules; the crate-wide cast allowance for pixel arithmetic
├─ error.rs                VisionError and its mapping to ModelError
├─ runtime.rs              OrtEp, RuntimeConfig, OrtRuntime: the environment and session building
├─ runtime/pool.rs         Pool<T> / SessionPool and the Pooled<'p, T> guard
├─ manifest.rs             ml/models/manifest.toml: entries, hashes, perception_config
├─ imaging.rs              ImageView<'a>, format sniffing, decode with EXIF orientation, resize, WebP, JPEG
├─ upload.rs               UploadProcessor: the scan and chat upload pipeline
├─ upload/quality.rs       blur score and exposure, the quality verdict
├─ upload/faces.rs         YuNet two-pass detection, union merge, growth, blurring
├─ depth.rs                depth and confidence maps, PNG, rotation, intrinsics, face zeroing, process_depth
├─ mask.rs                 MaskBuf, resampling, erosion, components, holes, outlines
├─ rle.rs                  COCO RLE (port of maskApi.c)
├─ measure.rs              Measurer: inputs, quantities, aggregation, organism length
├─ measure/config.rs       MeasurementConfig: every constant, contract defaults
├─ measure/camera.rs       Camera: rays through K⁻¹, undistortion; stored_fx
├─ measure/plane.rs        planes, seeded RANSAC, nearest-rank percentiles
├─ measure/scene.rs        DepthScene<'a>: confident depth points
├─ measure/leaf.rs         leaf plane, length, area, spots, holes
├─ measure/plant.rs        ground plane, plant points, height, canopy width
├─ measure/confidence.rs   the criteria table
├─ measure/spots.rs        ColourSpotSegmenter (SpotSegmenter)
├─ perception.rs           PerceptionPipeline (VisionModel): stages, batching, events
├─ perception/inference.rs the inference thread; TerminateOnDrop
├─ perception/preprocess.rs image → tensor from the manifest recipe; batches
├─ perception/detector.rs  RF-DETR post-processing
├─ perception/sam.rs       SAM 2.1 prompts and mask upsampling
├─ perception/heads.rs     label matrix, disease head, plant probe, species combination
└─ embed.rs                ArcticEmbedder (TextEmbedder)
```

---

## 4. ONNX Runtime: environment, execution providers, session pool

**One environment per process.** `ort` rc.14 has no implicit global environment: `Session::builder(&Environment)` takes one, and a second `build()` in the process is an error ✓. `OrtRuntime::init` builds it once with one shared intra-op pool (spinning off), so four or five sessions don't each start a core-sized pool. Every session keeps a clone of the environment alive.

**The provider is chosen at run time.** `ORT_EP=cuda|migraphx|webgpu|cpu` picks it; each GPU provider exists only when its cargo feature is on (`cuda`, `migraphx`, `webgpu`), so the CUDA and MIGraphX bindings never have to link into one binary. With `download-binaries` they can't: pyke publishes no dist with both, and the build script sets a link-error cfg ✓ (AMD research 2.2). With the default `load-dynamic`, the image ships its vendor's `libonnxruntime` and `ORT_DYLIB_PATH` names it. Asking for a provider that isn't compiled in fails `OrtRuntime::init`.

```rust
/// The execution provider, from `ORT_EP=cuda|migraphx|webgpu|cpu`. Each GPU provider exists only
/// when its cargo feature is on; asking for one that isn't compiled in fails at start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrtEp {
    /// NVIDIA, CUDA 13 (the RTX 3070 perception worker).
    Cuda,
    /// AMD, `MIGraphX` on `ROCm` (needs AMD's ORT 1.29 and the `api-29` build).
    MiGraphX,
    /// `WebGPU` through Dawn's Vulkan backend: any vendor, no `ROCm`.
    WebGpu,
    /// The CPU (`api`'s face detector and text embedder, and development machines).
    Cpu,
}

/// How the process starts ONNX Runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    /// The provider every session registers.
    pub ep: OrtEp,
    /// `ORT_DEVICE_ID`: the GPU inside the container (0 when one GPU is passed in).
    pub device_id: i32,
    /// `ORT_DYLIB_PATH` with `load-dynamic`: the `libonnxruntime` to open. `None` uses ort's
    /// default lookup (`libonnxruntime.so` on the loader path).
    pub dylib_path: Option<std::path::PathBuf>,
    /// Threads of the one intra-op pool every session shares. 2 on a GPU worker (a starting
    /// value, measured on day 2); `api` sizes it to its blocking-pool budget.
    pub intra_threads: usize,
    /// The environment's name in ONNX Runtime's logs.
    pub name: &'static str,
}

/// Per-session settings ([vision 4], the 3070's budgets).
///
/// [vision 4]: ../../../docs/vision/README.md#4-placement-and-vram-budgets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SessionSettings {
    /// `gpu_mem_limit` of the session's CUDA arena, from `[model.config] gpu_mem_limit_mb`.
    pub gpu_mem_limit_mb: Option<usize>,
}

/// The process's ONNX Runtime: the environment and the provider choice. Build one at start.
pub struct OrtRuntime {
    env: Environment,
    ep: OrtEp,
    device_id: i32,
}
```

**`error_on_failure` on every provider.** Without it a provider that fails to register is logged and skipped, and the session runs on the CPU: a silent slowdown on the rig ✓ (ort source, `ep/mod.rs`). The CUDA provider gets the settings of [vision 5](../vision/README.md#5-runtimes-and-versions): `kSameAsRequested` arena growth, `HEURISTIC` cuDNN search, no maximum workspace, and each session's `gpu_mem_limit` from its manifest entry.

```rust
impl OrtRuntime {
    /// Opens ONNX Runtime and builds the environment with one shared intra-op pool, spinning off.
    /// Fails when the provider isn't compiled in, so a misconfigured worker never starts.
    pub fn init(config: &RuntimeConfig) -> Result<Self, VisionError> {
        if !config.ep.is_compiled() {
            return Err(VisionError::EpNotCompiled(config.ep));
        }
        let pool = GlobalThreadPoolOptions::default()
            .with_intra_threads(config.intra_threads)?
            .with_spin_control(false)?;
        let builder = Self::builder(config)?;
        let env = builder
            .with_name(config.name)
            .with_global_thread_pool(pool)
            .build()?;
        tracing::info!(ep = %config.ep, device = config.device_id, "ONNX Runtime ready");
        Ok(Self {
            env,
            ep: config.ep,
            device_id: config.device_id,
        })
    }

    #[cfg(feature = "load-dynamic")]
    fn builder(
        config: &RuntimeConfig,
    ) -> Result<ort::environment::EnvironmentBuilder, VisionError> {
        match &config.dylib_path {
            Some(path) => ort::init_from(path).map_err(|e| {
                VisionError::Ort(ort::Error::new(format!("loading {}: {e}", path.display())))
            }),
            None => Ok(ort::init()),
        }
    }

    #[cfg(not(feature = "load-dynamic"))]
    fn builder(
        _config: &RuntimeConfig,
    ) -> Result<ort::environment::EnvironmentBuilder, VisionError> {
        Ok(ort::init())
    }

    /// The provider in use.
    pub const fn ep(&self) -> OrtEp {
        self.ep
    }

    /// Builds a session on the configured provider, registered with `error_on_failure` so a
    /// broken driver is a start-up error instead of a silent fall back to the CPU.
    pub fn session(&self, model: &Path, settings: SessionSettings) -> Result<Session, VisionError> {
        let providers = self.providers(settings)?;
        Ok(commit(&self.env, model, providers)?)
    }

    /// The provider list for one session. Each arm exists only with its feature; the CPU needs
    /// none (ONNX Runtime's own CPU provider is always last).
    fn providers(
        &self,
        settings: SessionSettings,
    ) -> Result<Vec<ExecutionProviderDispatch>, VisionError> {
        let _ = settings;
        match self.ep {
            OrtEp::Cpu => Ok(Vec::new()),
            #[cfg(feature = "cuda")]
            OrtEp::Cuda => {
                use ort::ep::{
                    ArenaExtendStrategy,
                    cuda::{CUDA, ConvAlgorithmSearch},
                };
                let mut cuda = CUDA::default()
                    .with_device_id(self.device_id)
                    .with_arena_extend_strategy(ArenaExtendStrategy::SameAsRequested)
                    .with_conv_algorithm_search(ConvAlgorithmSearch::Heuristic)
                    .with_conv_max_workspace(false);
                if let Some(mb) = settings.gpu_mem_limit_mb {
                    cuda = cuda.with_memory_limit(mb * 1024 * 1024);
                }
                Ok(vec![cuda.build().error_on_failure()])
            }
            #[cfg(feature = "migraphx")]
            OrtEp::MiGraphX => Ok(vec![
                ort::ep::MIGraphX::default()
                    .with_device_id(self.device_id)
                    .with_fp16(true)
                    .build()
                    .error_on_failure(),
            ]),
            #[cfg(feature = "webgpu")]
            OrtEp::WebGpu => Ok(vec![
                ort::ep::WebGPU::default()
                    .with_device_id(self.device_id)
                    .with_dawn_backend_type(ort::ep::webgpu::DawnBackendType::Vulkan)
                    .build()
                    .error_on_failure(),
            ]),
            #[allow(
                unreachable_patterns,
                reason = "reachable only in builds without some GPU feature"
            )]
            other => Err(VisionError::EpNotCompiled(other)),
        }
    }
}
```

The builder chain goes through `ort::Result` before `VisionError`: each builder step returns `ort::Error<SessionBuilder>`, which carries the builder back and isn't `Send + Sync`; `?` into `ort::Result` converts it to the plain `ort::Error` ✓ (compile error E0277, then fixed).

```rust
/// The builder chain. `ort::Error<SessionBuilder>` (what each builder step returns) carries the
/// builder back and isn't `Send + Sync`, so the chain `?`s into `ort::Result` first, which
/// converts it to the plain `ort::Error`, and only then into [`VisionError`].
fn commit(
    env: &Environment,
    model: &Path,
    providers: Vec<ExecutionProviderDispatch>,
) -> ort::Result<Session> {
    Session::builder(env)?
        .with_execution_providers(providers)?
        .with_optimization_level(GraphOptimizationLevel::All)?
        .commit_from_file(model)
}
```

**The session pool.** `Session::run` takes `&mut self` in rc.14 ✓, so a session serves one caller at a time; `Session` is `Send + Sync` ✓. `api`'s CPU models (the face detector and the text embedder) each get a `SessionPool` of N sessions: N blocking threads run the model at once, and the next waits on a `Condvar`. Waiting on a `std` lock is fine on a blocking thread, which is the only place the pool is used. The pool is generic over its value, so its checkout logic is tested without ONNX Runtime.

```rust
/// N interchangeable values handed out one caller at a time. Generic so the checkout logic is
/// tested without ONNX Runtime; production uses [`SessionPool`].
#[derive(Debug)]
pub struct Pool<T> {
    name: &'static str,
    idle: Mutex<Vec<T>>,
    returned: Condvar,
    size: usize,
}

/// A pool of ONNX Runtime sessions of one model.
pub type SessionPool = Pool<Session>;

/// A checked-out value. It borrows the pool for `'p`, so it can't outlive the pool, and gives the
/// value back in `Drop`, however the caller's scope ends (an early return, `?`, a panic).
///
/// `'p` is the borrow of the pool: a caller in `spawn_blocking` owns an `Arc` of whatever holds
/// the pool (moved into the closure, so the closure is `'static`) and checks out inside the
/// closure; the guard then lives no longer than that closure's borrow of its own `Arc`.
#[derive(Debug)]
pub struct Pooled<'p, T> {
    pool: &'p Pool<T>,
    value: Option<T>,
}
```
```rust
impl<T> Pool<T> {
    /// A pool of the given values. `name` appears in logs.
    pub fn new(name: &'static str, values: Vec<T>) -> Self {
        let size = values.len();
        Self {
            name,
            idle: Mutex::new(values),
            returned: Condvar::new(),
            size,
        }
    }

    /// How many values the pool holds in all.
    pub const fn size(&self) -> usize {
        self.size
    }

    /// The pool's name.
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Waits for a free value. Blocks the thread: call only from a blocking thread.
    pub fn checkout(&self) -> Pooled<'_, T> {
        // The lock guards a Vec push or pop, so a panic elsewhere can't leave it inconsistent.
        let mut idle = self.idle.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(value) = idle.pop() {
                return Pooled {
                    pool: self,
                    value: Some(value),
                };
            }
            idle = self
                .returned
                .wait(idle)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn give_back(&self, value: T) {
        self.idle
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(value);
        self.returned.notify_one();
    }
}
```
```rust
impl<T> Drop for Pooled<'_, T> {
    fn drop(&mut self) {
        if let Some(value) = self.value.take() {
            self.pool.give_back(value);
        }
    }
}
```

`Pooled<'p, T>` borrows the pool for `'p`: it can't outlive the pool, and it returns its value in `Drop` however the caller's scope ends (an early `?`, a panic). How it meets `spawn_blocking`'s `'static` bound is in section 12.

---

## 5. The model manifest

`ml/models/manifest.toml` lists every file a worker or `api` loads, with its SHA-256, input recipe and `[model.config]` thresholds ([vision 6](../vision/README.md#6-model-files-versions-and-pinning)). Each loader calls `Manifest::verify` on the entries it loads and refuses to start on a mismatch. `perception_config` is the first 16 hex digits of the SHA-256 over every entry's configuration table, reported as the `perception_config` key of `model_versions` ([contracts 5](../architecture/contracts.md#post-internaljobsidcomplete)).

```rust
/// One `[[model]]` entry.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ModelEntry {
    /// `leaf_detector`, `sam_encoder`, `bioclip`, `yunet`, `text_embedder` …
    pub name: String,
    /// Reported in `model_versions` and each observation's `model_version`.
    pub version: String,
    /// Relative to the manifest's directory.
    pub file: PathBuf,
    /// Lowercase hex SHA-256 of the file.
    pub sha256: String,
    /// The licence, for the notices.
    pub licence: String,
    /// Where it came from.
    #[serde(default)]
    pub source: String,
    /// Where it runs.
    pub runs_on: RunsOn,
    /// The input recipe, for image models.
    #[serde(default)]
    pub input: Option<ModelInput>,
    /// Logical output → tensor name.
    #[serde(default)]
    pub outputs: BTreeMap<String, String>,
    /// Thresholds, caps and temperatures, set from the evaluation set.
    #[serde(default)]
    pub config: toml::Table,
    /// For label matrices: the `label_set_version` they belong to.
    #[serde(default)]
    pub label_set_version: Option<String>,
    /// For the disease head: its `classifier_labels_version`.
    #[serde(default)]
    pub classifier_labels_version: Option<String>,
}

/// `[model.input]`: the image recipe, applied in Rust ([vision models 1]).
///
/// [vision models 1]: ../../../docs/vision/models.md#1-conventions-for-every-model
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ModelInput {
    /// The input tensor's name.
    pub name: String,
    /// Its shape, NCHW.
    pub shape: Vec<Dim>,
    /// Channel order.
    pub colour: Colour,
    /// The resize rule.
    pub resize: ResizeRule,
    /// Per-channel mean, on 0–1 values.
    #[serde(default)]
    pub mean: Option<[f32; 3]>,
    /// Per-channel standard deviation.
    #[serde(default)]
    pub std: Option<[f32; 3]>,
}

/// How an image becomes the model's input square.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResizeRule {
    /// Squash to the input size, ignoring the aspect ratio (RF-DETR, SAM 2).
    Squash,
    /// Shortest side to the input size, then a centre crop (`BioCLIP` 2).
    ShortestSideCrop,
    /// Longest side to the input size, then pad (`MobileSAM`).
    LongestSidePad,
}

/// Channel order of the input tensor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Colour {
    /// RGB.
    Rgb,
    /// BGR (`YuNet`).
    Bgr,
}

/// Where a model runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunsOn {
    /// The perception worker's GPU.
    Perception,
    /// `api`'s CPU (faces, text embeddings).
    Api,
}

/// The parsed manifest, with the directory its relative paths start from.
#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    root: PathBuf,
    models: Vec<ModelEntry>,
}
```
```rust
impl Manifest {
    /// Reads and parses the manifest. Hashes are checked separately ([`Manifest::verify`]),
    /// for only the files this process loads.
    pub fn load(path: &Path) -> Result<Self, ManifestError> {
        let text = std::fs::read_to_string(path).map_err(|source| ManifestError::Io {
            path: path.to_owned(),
            source,
        })?;
        let mut manifest = Self::parse(&text)?;
        manifest.root = path.parent().map(Path::to_owned).unwrap_or_default();
        Ok(manifest)
    }

    /// Parses manifest text; paths are relative to the current directory.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let file: ManifestFile = toml::from_str(text)?;
        Ok(Self {
            root: PathBuf::new(),
            models: file.models,
        })
    }

    /// Every entry.
    pub fn models(&self) -> &[ModelEntry] {
        &self.models
    }

    /// The entry named `name`.
    pub fn entry(&self, name: &str) -> Result<&ModelEntry, ManifestError> {
        self.models
            .iter()
            .find(|m| m.name == name)
            .ok_or_else(|| ManifestError::Missing(name.to_owned()))
    }

    /// Every entry whose name starts with `prefix` (the label matrices, one per version).
    pub fn entries_with_prefix<'m>(
        &'m self,
        prefix: &'m str,
    ) -> impl Iterator<Item = &'m ModelEntry> + 'm {
        self.models
            .iter()
            .filter(move |m| m.name.starts_with(prefix))
    }

    /// The absolute path of an entry's file.
    pub fn path(&self, entry: &ModelEntry) -> PathBuf {
        self.root.join(&entry.file)
    }

    /// Checks the SHA-256 of each named entry's file against the manifest.
    pub fn verify(&self, names: &[&str]) -> Result<(), ManifestError> {
        for name in names {
            let entry = self.entry(name)?;
            let actual = sha256_file(&self.path(entry))?;
            if !actual.eq_ignore_ascii_case(&entry.sha256) {
                return Err(ManifestError::HashMismatch {
                    name: entry.name.clone(),
                    expected: entry.sha256.clone(),
                    actual,
                });
            }
        }
        Ok(())
    }

    /// `perception_config` ([contracts 5], `model_versions` keys): the first 16 hex digits of
    /// the SHA-256 over every entry's name and `[model.config]` table, in manifest order. TOML
    /// tables serialise with sorted keys, so the hash depends only on the values.
    pub fn perception_config(&self) -> String {
        let mut hasher = Sha256::new();
        for entry in &self.models {
            hasher.update(entry.name.as_bytes());
            hasher.update(b"\n");
            hasher.update(entry.config.to_string().as_bytes());
            hasher.update(b"\n");
        }
        let digest = HEXLOWER.encode(&hasher.finalize());
        digest[..16].to_owned()
    }
}
```

Entry names the code reads: `leaf_detector`, `sam_encoder`, `sam_decoder`, `bioclip`, every `bioclip_labels*` (one label matrix per `label_set_version`), every `disease_head*` (one per `classifier_labels_version`), `plant_probe`, `yunet`, `text_embedder` and `text_embedder_tokenizer`. The heads and label matrices are JSON files with their matrices as base64 little-endian `f32` (section 6); the format is ours, since [vision 6](../vision/README.md#6-model-files-versions-and-pinning) fixes the manifest but not these files.

---

## 6. Perception

`PerceptionPipeline` is the perception worker's `VisionModel` ([contracts 5, results](../architecture/contracts.md#results); [vision models](../vision/models.md)). It answers `perceive` and returns `ModelError::Unsupported` for the other kinds.

### The inference thread

One OS thread owns every GPU session for the life of the process; jobs send it owned tensors over a bounded channel (`mpsc::channel(1)`) and await a `oneshot`. The worker holds one job (`slots = 1`) and runs the models one after another, so one owner fits, `Session::run(&mut self)` needs no lock, and ONNX Runtime's threads stay off tokio's workers, so the heartbeat keeps ticking during a long batch (the `rust-async` skill, `blocking.md` section 4). Sessions load on the thread, with an optional warm-up at the largest batch, before the worker claims anything.

```rust
/// The GPU models of the perception worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModelSlot {
    /// RF-DETR-S leaf detector.
    Detector,
    /// SAM 2.1 image encoder.
    SamEncoder,
    /// SAM 2.1 box decoder.
    SamDecoder,
    /// `BioCLIP` 2 vision tower.
    Bioclip,
}

/// A model to load on the thread.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionSpec {
    /// Which model.
    pub slot: ModelSlot,
    /// Its file (hash already checked).
    pub path: PathBuf,
    /// Its arena limit.
    pub settings: SessionSettings,
    /// A warm-up input run once at its largest batch, so the arena is grown before the first job
    /// ([vision 4]). `None` skips it.
    ///
    /// [vision 4]: ../../../docs/vision/README.md#4-placement-and-vram-budgets
    pub warm_up: Option<Vec<(String, ArrayD<f32>)>>,
}

/// Named input tensors, owned: they cross a channel to another thread, so they can't borrow
/// anything from the job (the channel's item type must be `'static`).
pub type Inputs = Vec<(String, ArrayD<f32>)>;

/// The handle jobs use. Cloning shares the one thread.
#[derive(Debug, Clone)]
pub struct Inference {
    requests: mpsc::Sender<Request>,
}

/// Terminates a run when dropped before [`TerminateOnDrop::disarm`].
pub struct TerminateOnDrop(Option<Arc<RunOptions>>);
```
```rust
impl Inference {
    /// Starts the thread, loads every session on it and waits until they are ready, so a missing
    /// driver or file fails the worker's start-up before it claims anything. The runtime moves
    /// onto the thread (sessions keep its environment alive).
    pub async fn start(runtime: OrtRuntime, specs: Vec<SessionSpec>) -> Result<Self, VisionError> {
        let (requests, mut inbox) = mpsc::channel::<Request>(1);
        let (ready, loaded) = oneshot::channel::<Result<(), VisionError>>();
        thread::Builder::new()
            .name("inference".to_owned())
            .spawn(move || {
                let mut sessions = match load(&runtime, specs) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = ready.send(Err(e));
                        return;
                    }
                };
                if ready.send(Ok(())).is_err() {
                    return;
                }
                while let Some(req) = inbox.blocking_recv() {
                    let result = match sessions.get_mut(&req.slot) {
                        Some(session) => run(session, &req.inputs, &req.outputs, &req.options),
                        None => Err(VisionError::Shape {
                            model: "inference",
                            detail: format!("{:?} is not loaded", req.slot),
                        }),
                    };
                    let _ = req.reply.send(result);
                }
            })
            .map_err(|e| VisionError::Resize(format!("spawning the inference thread: {e}")))?;
        loaded.await.map_err(|_| VisionError::InferenceGone)??;
        Ok(Self { requests })
    }

    /// Runs one model and returns the named outputs, copied out of ONNX Runtime.
    ///
    /// Cancellation is by drop ([`domain::ports::VisionModel`]): if the job's future is dropped
    /// while the model runs, the [`TerminateOnDrop`] guard calls `RunOptions::terminate`, so the
    /// inference thread stops the run instead of finishing a batch nobody will read.
    pub async fn run(
        &self,
        slot: ModelSlot,
        inputs: Inputs,
        outputs: &[&str],
    ) -> Result<Vec<ArrayD<f32>>, VisionError> {
        let options = Arc::new(RunOptions::new()?);
        let (reply, response) = oneshot::channel();
        let req = Request {
            slot,
            inputs,
            outputs: outputs.iter().map(|&s| s.to_owned()).collect(),
            options: Arc::clone(&options),
            reply,
        };
        let guard = TerminateOnDrop(Some(options));
        self.requests
            .send(req)
            .await
            .map_err(|_| VisionError::InferenceGone)?;
        let result = response.await.map_err(|_| VisionError::InferenceGone)?;
        guard.disarm();
        result
    }
}
```

**Cancellation by drop.** The `VisionModel` port cancels by dropping the job's future ([domain 9](domain.md#models)). A dropped future would leave a batch running on the thread, so `Inference::run` arms a `TerminateOnDrop` guard holding the run's `Arc<RunOptions>`: dropped before the reply arrives, it calls `RunOptions::terminate`, and ONNX Runtime ends the run with an error. How quickly a CUDA run notices the flag is *unverified* (the ort example is `no_run` for that reason).

### Batching and caps

Each model runs once per job over all its inputs, split at its own cap, never above the job-wide `PERCEPTION_MAX_BATCH` ([contracts 5, batching](../architecture/contracts.md#results); [vision 4](../vision/README.md#4-placement-and-vram-budgets)).

```rust
/// Batch caps ([contracts 5], batching; [vision 4]): the job-wide `PERCEPTION_MAX_BATCH` and
/// each model's own cap below it.
///
/// [vision 4]: ../../../docs/vision/README.md#4-placement-and-vram-budgets
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchCaps {
    /// `PERCEPTION_MAX_BATCH` (16).
    pub job: usize,
    /// Leaf detector images (8).
    pub detector: usize,
    /// SAM encoder images (2).
    pub sam_encoder: usize,
    /// SAM decoder boxes per call (16).
    pub sam_boxes: usize,
    /// `BioCLIP` images or crops (16).
    pub bioclip: usize,
}

/// The perception settings, from the manifest's `[model.config]` tables and the environment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PerceptionConfig {
    /// Batch caps.
    pub caps: BatchCaps,
    /// Detector thresholds.
    pub detector: DetectorConfig,
    /// SAM thresholds.
    pub sam: SamConfig,
    /// `τ_leaf` of the plant check.
    pub tau_leaf: f32,
    /// `τ_plant` of the plant check.
    pub tau_plant: f32,
    /// `T_species`.
    pub species_temperature: f32,
    /// Leaf boxes grow by this share on each side before the disease crop.
    pub crop_grow: f64,
}
```
```rust
/// One image as a `[3, side, side]` tensor, normalised by the recipe.
pub fn to_tensor(image: ImageView<'_>, input: &ModelInput) -> Result<Array3<f32>, VisionError> {
    let side = input.side().ok_or_else(|| VisionError::Shape {
        model: "preprocess",
        detail: format!("{} has no fixed side", input.name),
    })?;
    let resized = match input.resize {
        ResizeRule::Squash => imaging::resize(image, side, side)?,
        ResizeRule::ShortestSideCrop => {
            let (w, h) = (image.width(), image.height());
            let scale = f64::from(side) / f64::from(w.min(h).max(1));
            let (rw, rh) = (
                ((f64::from(w) * scale).round() as u32).max(side),
                ((f64::from(h) * scale).round() as u32).max(side),
            );
            let r = imaging::resize(image, rw, rh)?;
            imaging::crop(
                ImageView::from(&r),
                PixelRect {
                    x: (rw - side) / 2,
                    y: (rh - side) / 2,
                    w: side,
                    h: side,
                },
            )
        }
        ResizeRule::LongestSidePad => {
            let fitted = imaging::resize_long_side(image, side)?;
            let mut padded = image::RgbImage::new(side, side);
            image::imageops::replace(&mut padded, &fitted, 0, 0);
            padded
        }
    };
    let mean = input.mean.unwrap_or([0.0; 3]);
    let std = input.std.unwrap_or([1.0; 3]);
    let s = side as usize;
    let mut t = Array3::<f32>::zeros((3, s, s));
    for (x, y, p) in resized.enumerate_pixels() {
        for c in 0..3 {
            let channel = match input.colour {
                Colour::Rgb => c,
                Colour::Bgr => 2 - c,
            };
            let v = f32::from(p.0[channel]) / 255.0;
            t[[c, y as usize, x as usize]] = (v - mean[c]) / std[c];
        }
    }
    Ok(t)
}

/// Stacks `[3, h, w]` tensors into contiguous `[n, 3, h, w]` batches of at most `cap`, the
/// layout `TensorRef::from_array_view` needs.
pub fn batches(tensors: &[Array3<f32>], cap: usize) -> Result<Vec<Array4<f32>>, VisionError> {
    tensors
        .chunks(cap.max(1))
        .map(|chunk| {
            let views: Vec<ArrayView3<'_, f32>> = chunk.iter().map(|t| t.view()).collect();
            ndarray::stack(Axis(0), &views).map_err(|e| VisionError::Shape {
                model: "batch",
                detail: e.to_string(),
            })
        })
        .collect()
}

/// The effective cap of a model: its own cap, never above the job-wide `PERCEPTION_MAX_BATCH`.
pub const fn effective_cap(model_cap: usize, job_cap: usize) -> usize {
    let cap = if model_cap < job_cap {
        model_cap
    } else {
        job_cap
    };
    if cap == 0 { 1 } else { cap }
}
```

### Stages

| Stage | Models | Output | Rule |
|---|---|---|---|
| `segmentation` | RF-DETR over the plant shots (≤ 8 a batch); SAM 2.1 encoder (≤ 2) and box decoder (≤ 16 boxes a call, one prompt per box) | `leaf` regions with masks as COCO RLE; a `plant` region from the leaves on a whole-plant shot with ≥ 3 leaf boxes | Region ids `<image_id>.<n>` from 1 per image; a mask below `τ_mask` (0.6) is dropped and the box kept |
| `plant_check` | The detector's top leaf score and the plant probe on the `BioCLIP` embedding | `plant_present` | Present when any plant shot has a leaf score ≥ `τ_leaf` or a plant probability ≥ `τ_plant`; soil shots never count |
| `species` | `BioCLIP` 2 pass 1 over every image, the label matrix of the payload's `label_set_version` | top 3 | Zero-shot softmax of `(100/T)·e·tᵀ` per plant shot, averaged with weights 2 (whole plant, flower or fruit) and 1 (leaves) |
| `disease` | `BioCLIP` 2 pass 2 over the leaf crops (boxes grown 10%), the disease head of the payload's `classifier_labels_version` | per crop top 3 | Restricted to the working species' classes before the softmax; whole close-ups when no leaf region; `species_not_supported` and `no_leaves_found` skips |
| `image_embedding` | pass 1's vectors | 768-value vectors, `model = bioclip-2@<revision>` | No extra GPU time |

The order of work differs from the stage list: segmentation runs first because the plant check uses its scores, and `BioCLIP`'s first pass serves the probe, species and embeddings at once. A version the worker hasn't loaded fails the job before any GPU time with `VisionError::UnknownLabelVersion`, which the worker reports as `unknown_label_version`, not retryable ([worker 6](worker.md#6-leases-heartbeats-and-completion)).

```rust
/// The perception adapter: the inference thread, the recipes and the CPU heads.
#[derive(Debug)]
pub struct PerceptionPipeline {
    inference: Inference,
    recipes: Arc<Recipes>,
    labels: BTreeMap<String, Arc<LabelMatrix>>,
    disease: BTreeMap<String, Arc<DiseaseHead>>,
    probe: Option<PlantProbe>,
    config: PerceptionConfig,
    versions: BTreeMap<String, String>,
    bioclip_version: String,
    cpu: Arc<Semaphore>,
}
```
```rust
/// The detector's thresholds, from its manifest `[model.config]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectorConfig {
    /// `τ_det`.
    pub score_threshold: f32,
    /// Boxes under this many pixels on their shorter side (stored image) are dropped.
    pub min_side_px: f32,
    /// At most this many leaves per image.
    pub max_per_image: usize,
    /// The final `NMS`.
    pub nms_iou: f32,
}

/// One leaf box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detection {
    /// Normalised to the stored image.
    pub bbox: BBox,
    /// The sigmoid score.
    pub score: f32,
}

/// Decodes one image's queries. `dets` is `[queries, 4]` (normalised centre x, centre y, width,
/// height; the squashed resize makes them the stored image's), `logits` is `[queries, classes]`
/// with `leaf` at index 0. `image_size` is the stored image's, for the size rule.
pub fn decode(
    dets: &[f32],
    logits: &[f32],
    classes: usize,
    image_size: (u32, u32),
    cfg: &DetectorConfig,
) -> Vec<Detection> {
    let queries = dets.len() / 4;
    let (iw, ih) = (image_size.0 as f32, image_size.1 as f32);
    let mut found: Vec<Detection> = (0..queries)
        .filter_map(|q| {
            let score = sigmoid(logits[q * classes]);
            if score < cfg.score_threshold {
                return None;
            }
            let [cx, cy, w, h] = [
                dets[q * 4],
                dets[q * 4 + 1],
                dets[q * 4 + 2],
                dets[q * 4 + 3],
            ];
            if (w * iw).min(h * ih) < cfg.min_side_px {
                return None;
            }
            let x0 = (cx - w / 2.0).clamp(0.0, 1.0);
            let y0 = (cy - h / 2.0).clamp(0.0, 1.0);
            let x1 = (cx + w / 2.0).clamp(0.0, 1.0);
            let y1 = (cy + h / 2.0).clamp(0.0, 1.0);
            BBox::new(
                f64::from(x0),
                f64::from(y0),
                f64::from(x1 - x0),
                f64::from(y1 - y0),
            )
            .ok()
            .map(|bbox| Detection { bbox, score })
        })
        .collect();
    found.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<Detection> = Vec::new();
    for d in found {
        if kept.len() == cfg.max_per_image {
            break;
        }
        if kept
            .iter()
            .all(|k| iou(&k.bbox, &d.bbox) <= f64::from(cfg.nms_iou))
        {
            kept.push(d);
        }
    }
    kept
}
```
```rust
/// SAM's settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SamConfig {
    /// `τ_mask`: a mask whose predicted `IoU` is lower is dropped and the box kept.
    pub min_iou: f32,
    /// A `plant` region from the leaves is tried on a whole-plant shot with at least this many
    /// leaf boxes ([vision models 5]).
    ///
    /// [vision models 5]: ../../../docs/vision/models.md#5-plant-soil-and-surroundings-regions
    pub plant_min_leaves: usize,
    /// The enclosing box grows by this share.
    pub plant_grow: f64,
    /// The plant mask is kept only when it covers at least this share of the leaf boxes' area.
    pub plant_min_cover: f64,
}

/// A normalised box as corners in the 1024 frame: the squash scales x and y independently.
pub fn prompt(bbox: BBox) -> [f32; 4] {
    let [x, y, w, h] = bbox.to_array();
    [
        (x * f64::from(FRAME)) as f32,
        (y * f64::from(FRAME)) as f32,
        ((x + w) * f64::from(FRAME)) as f32,
        ((y + h) * f64::from(FRAME)) as f32,
    ]
}

/// Resizes one 256 × 256 logit map bilinearly to the stored image and thresholds it at 0. The
/// squash makes the map cover the whole image, so the resize is direct.
pub fn upsample(logits: &ArrayView2<'_, f32>, size: (u32, u32)) -> MaskBuf {
    let (lh, lw) = logits.dim();
    let mut mask = MaskBuf::empty(size.0, size.1);
    let (sx, sy) = (lw as f32 / size.0 as f32, lh as f32 / size.1 as f32);
    for y in 0..size.1 {
        let fy = ((y as f32 + 0.5) * sy - 0.5).clamp(0.0, (lh - 1) as f32);
        let (y0, ty) = (fy.floor() as usize, fy - fy.floor());
        let y1 = (y0 + 1).min(lh - 1);
        for x in 0..size.0 {
            let fx = ((x as f32 + 0.5) * sx - 0.5).clamp(0.0, (lw - 1) as f32);
            let (x0, tx) = (fx.floor() as usize, fx - fx.floor());
            let x1 = (x0 + 1).min(lw - 1);
            let top = logits[[y0, x0]] * (1.0 - tx) + logits[[y0, x1]] * tx;
            let bottom = logits[[y1, x0]] * (1.0 - tx) + logits[[y1, x1]] * tx;
            if top * (1.0 - ty) + bottom * ty > 0.0 {
                mask.set(x, y, true);
            }
        }
    }
    mask
}

/// The box around every leaf box, grown, for the `plant` prompt.
pub fn enclosing(leaves: &[BBox], grow: f64) -> Option<BBox> {
    let (mut x0, mut y0, mut x1, mut y1) = (1.0f64, 1.0f64, 0.0f64, 0.0f64);
    for b in leaves {
        let [x, y, w, h] = b.to_array();
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x + w);
        y1 = y1.max(y + h);
    }
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (dx, dy) = ((x1 - x0) * grow, (y1 - y0) * grow);
    let (x0, y0) = ((x0 - dx).max(0.0), (y0 - dy).max(0.0));
    let (x1, y1) = ((x1 + dx).min(1.0), (y1 + dy).min(1.0));
    BBox::new(x0, y0, x1 - x0, y1 - y0).ok()
}
```
```rust
/// The species rows of one `label_set_version`: `N × 768`, L2-normalised text embeddings of
/// every `bioclip_label`, averaged over the 80 templates offline.
#[derive(Debug, Clone, PartialEq)]
pub struct LabelMatrix {
    /// `label_set_version`.
    pub version: String,
    /// Row `i` is this species.
    pub species: Vec<(SpeciesId, String)>,
    rows: Vec<f32>,
}

/// The weighted average of per-shot probabilities, then the top `k` as `species.candidates`.
pub fn combine_species(
    matrix: &LabelMatrix,
    per_shot: &[(ImageKind, Vec<f32>)],
    k: usize,
) -> Vec<SpeciesLabel> {
    let mut sum = vec![0.0f32; matrix.species.len()];
    let mut weights = 0.0f32;
    for (kind, probs) in per_shot {
        let w = shot_weight(*kind);
        if w == 0.0 {
            continue;
        }
        weights += w;
        for (s, p) in sum.iter_mut().zip(probs) {
            *s += w * p;
        }
    }
    if weights == 0.0 {
        return Vec::new();
    }
    let mut ranked: Vec<(usize, f32)> = sum.iter().map(|s| s / weights).enumerate().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    ranked
        .into_iter()
        .take(k)
        .map(|(i, p)| SpeciesLabel {
            species_id: Some(matrix.species[i].0.clone()),
            label: matrix.species[i].1.clone(),
            score: Confidence::saturating(f64::from(p)),
        })
        .collect()
}

/// Per-shot weights for combining species probabilities: several views beat one, and a single
/// leaf close-up is the weakest ([vision models 6], step 4). Soil shots aren't scored.
pub const fn shot_weight(kind: ImageKind) -> f32 {
    match kind {
        ImageKind::WholePlant | ImageKind::FlowerFruit => 2.0,
        ImageKind::LeafCloseup | ImageKind::LeafUnderside => 1.0,
        _ => 0.0,
    }
}

/// The disease head: one class per `classifier_labels.yaml` entry (species, problem or healthy).
#[derive(Debug, Clone, PartialEq)]
pub struct DiseaseHead {
    /// `classifier_labels_version`.
    pub version: String,
    temperature: f32,
    labels: Vec<(SpeciesId, ProblemOrHealthy)>,
    linear: Linear,
}

/// The plant probe: logistic regression on the embedding, `σ(w · e + b)` ([vision models 2]).
#[derive(Debug, Clone, PartialEq)]
pub struct PlantProbe {
    weight: Vec<f32>,
    bias: f32,
}

/// The plant check over the plant shots ([vision models 2]): present when any plant shot has a
/// top leaf score ≥ `τ_leaf` or a plant probability ≥ `τ_plant`. Soil shots never count.
pub fn plant_present(shots: &[(ImageKind, f32, f32)], tau_leaf: f32, tau_plant: f32) -> bool {
    shots.iter().any(|&(kind, leaf, plant)| {
        shot_weight(kind) > 0.0 && (leaf >= tau_leaf || plant >= tau_plant)
    })
}
```

**Partial results.** A `StageClock` sends `stage_started` when a stage begins and the stage's `partial_result` followed by `stage_completed` when it ends (or `stage_completed` with `skipped`). A failed send is logged and ignored: `complete` repeats every stage, and a `409 lease_lost` is handled by the worker's sink, which cancels the job ([worker 7](worker.md#7-cancellation)).

```rust
/// Sends `stage_started` on creation and `stage_completed` (with the partial result first) on
/// finish. A failed send is logged and ignored: `complete` repeats every stage, and a
/// `lease_lost` is handled by the worker's sink, which cancels the job.
struct StageClock<'p> {
    progress: &'p dyn ProgressSink,
    stage: Stage,
    started: Instant,
}
```

**The species head** of [vision models 6](../vision/models.md#6-species-bioclip) (`SPECIES_HEAD_ENABLED`) is not wired: it is turned on only if the day-2 evaluation shows it helps. The `LabelMatrix::probabilities` output is the place it would merge.

---

## 7. The upload pipeline

`api` runs `UploadProcessor::process` inside `spawn_blocking`, behind its own semaphore, for every `POST /scans/{id}/images` ([contracts 3](../architecture/contracts.md#post-scansidimages)), and `process_chat` for chat images. The order is fixed:

1. **Sniff and decode.** The format comes from the first bytes, never from the file name or content type; HEIC is an ISO-BMFF `ftyp` box with a HEIF brand, which `image` doesn't recognise ✓. JPEG, PNG and WebP decode with `image`, which reads the EXIF orientation and bakes it into the pixels; HEIC decodes with libheif (feature `heic`), which applies `irot` itself. Decoding is bounded (12,000 pixels a side, 512 MiB). Re-encoding later drops every metadata chunk ✓.
2. **Quality gate**, every scan kind: blur (variance of the 3×3 Laplacian) and exposure (luma histogram) on a 512-pixel downscale, so the scores don't depend on resolution. A failing shot is `422 image_quality` with the problem's hint. Chat images skip it.
3. **Faces**, fail closed: `YuNet` 2026may in two passes, the whole image at 1280 pixels and overlapping 1280-pixel tiles of the full-resolution image (stride 960), score 0.3, NMS 0.3 within a pass, boxes from both passes **united** when they overlap by more than IoU 0.2 or one contains the other, each grown by 25% on every side, then a Gaussian blur of sigma a quarter of the box's longer side ([vision models 9](../vision/models.md#9-face-detector)). Any error fails the upload: no photo is stored unblurred.
4. **Depth** (section 8), validated against the photo's sensor-frame size before the resize, so the stored size knows whether a depth map is kept.
5. **Resize**: a long side of `default_long_side` (1600), or at least 2048 pixels when the depth survives validation ([contracts 3](../architecture/contracts.md#post-scansidimages)); never upscaled.
6. **Lossy WebP** at quality 82 through libwebp.

```rust
/// The pipeline's settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UploadConfig {
    /// Decoder bounds.
    pub limits: DecodeLimits,
    /// The stored long side of a photo without depth. Contracts 3 fixes only the `LiDAR` rule
    /// (at least 2048); 1600 keeps an ordinary photo at 100–500 KB of `WebP`.
    pub default_long_side: u32,
    /// libwebp quality, 0–100.
    pub webp_quality: f32,
    /// The quality gate.
    pub quality: QualityConfig,
    /// The face detector.
    pub faces: FaceConfig,
    /// How much more each blur box grows when it zeroes the depth map ([contracts 3]: 10%).
    pub depth_face_growth: f32,
}

/// One upload, borrowed from the multipart body for the duration of the call. `'a` is that
/// borrow: `api` moves the owned `Bytes` into the blocking closure and borrows them there, so no
/// image is copied to cross onto the blocking thread.
#[derive(Debug, Clone, Copy)]
pub struct UploadInput<'a> {
    /// The shot's kind.
    pub kind: ImageKind,
    /// The `file` part.
    pub file: &'a [u8],
    /// The depth parts, when sent.
    pub depth: Option<DepthInput<'a>>,
}

/// A stored-ready image.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessedImage {
    /// Lossy `WebP`, upright, faces blurred, no metadata.
    pub webp: Bytes,
    /// Width of the stored image.
    pub width: u32,
    /// Height of the stored image.
    pub height: u32,
}

/// The result of [`UploadProcessor::process`].
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessedUpload {
    /// The image to store.
    pub image: ProcessedImage,
    /// `UploadImageResponse.quality`.
    pub quality: QualityReport,
    /// Present when depth parts were sent: the depth to store, or why it was dropped.
    pub depth: Option<Result<ProcessedDepth, DepthProblem>>,
}

/// Why an upload is refused. Depth problems are never here: they ride in
/// [`ProcessedUpload::depth`].
#[derive(Debug, thiserror::Error)]
pub enum UploadError {
    /// `422 image_quality`, with `hint = problem.hint()`.
    #[error("image quality: {problem:?}")]
    Quality {
        /// The failing check.
        problem: QualityProblem,
        /// The blur score.
        blur_score: f64,
        /// The mean exposure.
        exposure: f64,
    },
    /// `415 unsupported_media_type`.
    #[error("unsupported media")]
    UnsupportedMedia(#[source] DecodeError),
    /// Face detection or blurring failed: the image isn't stored (`503 processing_unavailable`).
    #[error("face blurring failed")]
    FaceBlur(#[source] VisionError),
    /// Resizing or encoding failed (`503 processing_unavailable`).
    #[error("encoding failed")]
    Encode(#[source] VisionError),
}

/// The upload pipeline: the face detector and the settings. `Send + Sync`; build one at start
/// and share it in an `Arc`.
#[derive(Debug)]
pub struct UploadProcessor {
    faces: FaceDetector,
    config: UploadConfig,
}
```
```rust
impl UploadProcessor {
    /// Loads the face detector (hash checked).
    pub fn load(
        runtime: &OrtRuntime,
        manifest: &Manifest,
        config: UploadConfig,
    ) -> Result<Self, VisionError> {
        Ok(Self {
            faces: FaceDetector::load(runtime, manifest, config.faces)?,
            config,
        })
    }

    /// The face detector's model version.
    pub fn face_model_version(&self) -> &str {
        self.faces.version()
    }

    /// A scan upload. Blocking.
    ///
    /// Order: decode → quality gate (every scan kind) → faces on the full-resolution upright
    /// image → blur → resize (2048+ when the depth survives validation) → `WebP` → depth.
    pub fn process(&self, input: UploadInput<'_>) -> Result<ProcessedUpload, UploadError> {
        let decoded = imaging::decode_upload(input.file, self.config.limits)
            .map_err(UploadError::UnsupportedMedia)?;
        let view = ImageView::from(&decoded.image);
        let scores = quality::score(view, &self.config.quality).map_err(UploadError::Encode)?;
        if let Some(problem) = quality::verdict(&scores, &self.config.quality) {
            return Err(UploadError::Quality {
                problem,
                blur_score: scores.blur_score,
                exposure: scores.exposure,
            });
        }
        let faces: Vec<_> = self
            .faces
            .detect(view)
            .map_err(UploadError::FaceBlur)?
            .into_iter()
            .map(|f| f.grown(self.faces.grow()))
            .collect();
        let mut upright = decoded.image;
        blur_faces(&mut upright, &faces);

        let size = upright.dimensions();
        let long = size.0.max(size.1);
        // Validate the depth against the photo first (it needs only sizes), so the stored size
        // knows whether a depth map will be kept.
        let depth_ctx = |stored_size| DepthContext {
            kind: input.kind,
            orientation: decoded.orientation.value(),
            sensor_size: decoded.sensor_size,
            stored_size,
            faces: &faces,
            face_growth: self.config.depth_face_growth,
        };
        let with_depth_size = imaging::fit_long_side(
            size,
            stored_long_side(long, true, self.config.default_long_side),
        );
        let depth = input
            .depth
            .map(|d| process_depth(&d, &depth_ctx(with_depth_size)));
        let keeps_depth = matches!(depth, Some(Ok(_)));
        let target = stored_long_side(long, keeps_depth, self.config.default_long_side);
        let stored = imaging::resize_long_side(ImageView::from(&upright), target)
            .map_err(UploadError::Encode)?;
        let webp = imaging::encode_webp(ImageView::from(&stored), self.config.webp_quality);
        Ok(ProcessedUpload {
            image: ProcessedImage {
                webp: Bytes::from(webp),
                width: stored.width(),
                height: stored.height(),
            },
            quality: QualityReport {
                blur_score: scores.blur_score,
                exposure: scores.exposure,
                passed: BoolLit,
            },
            depth,
        })
    }

    /// A chat image: the same without the quality gate and without depth. Blocking.
    pub fn process_chat(&self, file: &[u8]) -> Result<ProcessedImage, UploadError> {
        let decoded = imaging::decode_upload(file, self.config.limits)
            .map_err(UploadError::UnsupportedMedia)?;
        let faces: Vec<_> = self
            .faces
            .detect(ImageView::from(&decoded.image))
            .map_err(UploadError::FaceBlur)?
            .into_iter()
            .map(|f| f.grown(self.faces.grow()))
            .collect();
        let mut upright = decoded.image;
        blur_faces(&mut upright, &faces);
        let stored =
            imaging::resize_long_side(ImageView::from(&upright), self.config.default_long_side)
                .map_err(UploadError::Encode)?;
        Ok(ProcessedImage {
            webp: Bytes::from(imaging::encode_webp(
                ImageView::from(&stored),
                self.config.webp_quality,
            )),
            width: stored.width(),
            height: stored.height(),
        })
    }
}
```

```rust
/// A borrowed RGB8 image, row-major, 3 bytes per pixel. `'a` is the borrow of the pixel buffer:
/// the view is `Copy`, costs nothing to pass around, and can't outlive the buffer it reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageView<'a> {
    width: u32,
    height: u32,
    pixels: &'a [u8],
}

/// The formats an upload may have ([contracts 1], images).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceFormat {
    /// JPEG.
    Jpeg,
    /// PNG.
    Png,
    /// `WebP`.
    WebP,
    /// HEIC/HEIF (decoded only with the `heic` feature).
    Heic,
}

/// Recognises the format from the first bytes, never from the file name or the part's content
/// type. HEIC is an ISO-BMFF `ftyp` box with a HEIF brand, which `image` doesn't sniff.
pub fn sniff(bytes: &[u8]) -> Option<SourceFormat> {
    if bytes.len() >= 12
        && &bytes[4..8] == b"ftyp"
        && matches!(
            &bytes[8..12],
            b"heic" | b"heix" | b"hevc" | b"hevx" | b"mif1" | b"msf1"
        )
    {
        return Some(SourceFormat::Heic);
    }
    match image::guess_format(bytes).ok()? {
        ImageFormat::Jpeg => Some(SourceFormat::Jpeg),
        ImageFormat::Png => Some(SourceFormat::Png),
        ImageFormat::WebP => Some(SourceFormat::WebP),
        _ => None,
    }
}

/// EXIF orientation 1–8. The rear camera produces only the rotations (1, 3, 6, 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExifOrientation(u8);

/// How the stored image is turned relative to the sensor frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rotation {
    /// EXIF 1.
    None,
    /// EXIF 6: a clockwise quarter turn.
    Cw90,
    /// EXIF 3.
    Half,
    /// EXIF 8: a counter-clockwise quarter turn.
    Ccw90,
}

/// Bounds on what an upload may decode to, checked before pixels are allocated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeLimits {
    /// The widest side accepted, pixels.
    pub max_side: u32,
    /// The most bytes the decoder may allocate.
    pub max_alloc: u64,
}

/// A decoded upload: upright pixels, and what the sensor frame was.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    /// Upright RGB8 pixels (the EXIF orientation is applied; metadata is gone).
    pub image: RgbImage,
    /// The orientation the file carried (HEIC: always upright, libheif applies `irot` itself).
    pub orientation: ExifOrientation,
    /// The size before orientation, in the sensor frame the depth map shares.
    pub sensor_size: (u32, u32),
    /// The format.
    pub format: SourceFormat,
}

/// Decodes an upload, reads its EXIF orientation and bakes it into the pixels. Re-encoding
/// later drops every metadata chunk (EXIF, GPS, ICC), as [contracts 1] requires.
pub fn decode_upload(bytes: &[u8], limits: DecodeLimits) -> Result<Decoded, DecodeError> {
    match sniff(bytes) {
        None => Err(DecodeError::Unsupported),
        Some(SourceFormat::Heic) => decode_heic(bytes, limits),
        Some(format) => {
            let invalid = |e: image::ImageError| DecodeError::Invalid(Box::new(e));
            let mut reader = ImageReader::new(Cursor::new(bytes))
                .with_guessed_format()
                .map_err(|e| DecodeError::Invalid(Box::new(e)))?;
            let mut l = image::Limits::default();
            l.max_image_width = Some(limits.max_side);
            l.max_image_height = Some(limits.max_side);
            l.max_alloc = Some(limits.max_alloc);
            reader.limits(l);
            let mut decoder = reader.into_decoder().map_err(invalid)?;
            let orientation = decoder.orientation().map_err(invalid)?;
            let sensor_size = decoder.dimensions();
            let mut img = DynamicImage::from_decoder(decoder).map_err(invalid)?;
            img.apply_orientation(orientation);
            Ok(Decoded {
                image: img.into_rgb8(),
                orientation: ExifOrientation::from_image(orientation),
                sensor_size,
                format,
            })
        }
    }
}

/// The size with the long side at most `long_side`, keeping the aspect ratio; never upscales.
pub fn fit_long_side(size: (u32, u32), long_side: u32) -> (u32, u32) {
    let (w, h) = size;
    let long = w.max(h);
    if long <= long_side || long == 0 {
        return (w, h);
    }
    let scale = f64::from(long_side) / f64::from(long);
    let scaled = |v: u32| ((f64::from(v) * scale).round() as u32).max(1);
    (scaled(w), scaled(h))
}

/// Resizes to exactly `width × height` with Lanczos3 (`fast_image_resize`, SIMD).
pub fn resize(view: ImageView<'_>, width: u32, height: u32) -> Result<RgbImage, VisionError> {
    if (view.width, view.height) == (width, height) {
        return RgbImage::from_raw(width, height, view.pixels.to_vec())
            .ok_or_else(|| VisionError::Resize("buffer size".to_owned()));
    }
    let err = |e: &dyn std::fmt::Display| VisionError::Resize(e.to_string());
    let src = ImageRef::new(view.width, view.height, view.pixels, PixelType::U8x3)
        .map_err(|e| err(&e))?;
    let mut dst = Image::new(width, height, PixelType::U8x3);
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
    Resizer::new()
        .resize(&src, &mut dst, &options)
        .map_err(|e| err(&e))?;
    RgbImage::from_raw(width, height, dst.into_vec())
        .ok_or_else(|| VisionError::Resize("buffer size".to_owned()))
}

/// Lossy `WebP` through libwebp (`image` 0.25 writes only lossless `WebP`). The output has no
/// metadata chunks.
pub fn encode_webp(view: ImageView<'_>, quality: f32) -> Vec<u8> {
    webp::Encoder::from_rgb(view.pixels, view.width, view.height)
        .encode(quality)
        .to_vec()
}

/// JPEG, for the VLM's base64 data URLs (quality 90, [vision vlm 5]).
///
/// [vision vlm 5]: ../../../docs/vision/vlm.md#5-image-preparation-and-tokens
pub fn encode_jpeg(view: ImageView<'_>, quality: u8) -> Result<Vec<u8>, VisionError> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality).write_image(
        view.pixels,
        view.width,
        view.height,
        image::ExtendedColorType::Rgb8,
    )?;
    Ok(out)
}
```

```rust
/// Thresholds of the quality gate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QualityConfig {
    /// The analysis downscale's long side, pixels.
    pub analysis_long_side: u32,
    /// Below this blur score the shot is `too_blurry`.
    pub min_blur_score: f64,
    /// Below this mean luma (0–1) the shot is `too_dark`.
    pub min_exposure: f64,
    /// Above this mean luma the shot is `too_bright`.
    pub max_exposure: f64,
    /// More than this share of pixels at luma ≤ 15 is `too_dark`.
    pub max_dark_share: f64,
    /// More than this share of pixels at luma ≥ 240 is `too_bright`.
    pub max_bright_share: f64,
}

/// The two scores of a shot ([contracts 3], `UploadImageResponse.quality`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QualityScores {
    /// Variance of the 3×3 Laplacian of the luma downscale.
    pub blur_score: f64,
    /// Mean luma, 0–1.
    pub exposure: f64,
    /// Share of pixels at luma ≤ 15.
    pub dark_share: f64,
    /// Share of pixels at luma ≥ 240.
    pub bright_share: f64,
}

/// The first failing check, in the order blur, dark, bright; `None` when the shot passes.
pub fn verdict(scores: &QualityScores, config: &QualityConfig) -> Option<QualityProblem> {
    if scores.blur_score < config.min_blur_score {
        Some(QualityProblem::TooBlurry)
    } else if scores.exposure < config.min_exposure || scores.dark_share > config.max_dark_share {
        Some(QualityProblem::TooDark)
    } else if scores.exposure > config.max_exposure || scores.bright_share > config.max_bright_share
    {
        Some(QualityProblem::TooBright)
    } else {
        None
    }
}
```

```rust
/// The detector's settings ([vision models 9], passes for a 12-megapixel photo).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceConfig {
    /// Pass A: the whole image scaled to this long side.
    pub pass_a_long_side: u32,
    /// Pass B: square tiles of the full-resolution image, this many pixels a side.
    pub tile: u32,
    /// Pass B: the step between tiles (960 = 25% overlap).
    pub tile_stride: u32,
    /// Recall first: a false blur costs a little evidence, a missed face is a privacy failure.
    pub score_threshold: f32,
    /// `NMS` within a pass.
    pub nms_iou: f32,
    /// Boxes from different passes are united when they overlap by more than this or one
    /// contains the other.
    pub merge_iou: f32,
    /// Each side grows by this share of the box before blurring.
    pub grow: f32,
    /// Sessions in the pool: how many uploads detect faces at once.
    pub sessions: usize,
}

/// A face box, normalised to the upright image (`[0, 1]` on each axis), so the same box applies
/// to the photo at any size and to its depth map.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceBox {
    /// Left.
    pub x: f32,
    /// Top.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
    /// `YuNet`'s score.
    pub score: f32,
}

/// Unites boxes from different passes: any two that overlap by more than `iou`, or where one
/// contains the other, become their union, repeated until nothing changes. Union rather than
/// suppression, so a face half-caught by a tile is covered by the whole-image box and vice versa.
pub fn union_merge(mut boxes: Vec<FaceBox>, iou: f32) -> Vec<FaceBox> {
    loop {
        let mut merged = false;
        'outer: for i in 0..boxes.len() {
            for j in i + 1..boxes.len() {
                if boxes[i].iou(&boxes[j]) > iou || boxes[i].contains_or_inside(&boxes[j]) {
                    let u = boxes[i].union(&boxes[j]);
                    boxes.swap_remove(j);
                    boxes[i] = u;
                    merged = true;
                    break 'outer;
                }
            }
        }
        if !merged {
            return boxes;
        }
    }
}

/// Decodes one stride (`OpenCV`'s `face_detect.cpp`): anchor `i` sits at row `i / cols`, column
/// `i % cols`; score = √(clamp(cls) · clamp(obj)); centre = ((col + b0)·s, (row + b1)·s); size =
/// (e^b2·s, e^b3·s). Boxes come out in pixels of the padded input.
pub fn decode_stride(out: StrideOutputs<'_>, padded_width: u32, threshold: f32) -> Vec<[f32; 5]> {
    let cols = (padded_width / out.stride) as usize;
    let s = out.stride as f32;
    let mut boxes = Vec::new();
    for (i, (&cls, &obj)) in out.cls.iter().zip(out.obj).enumerate() {
        let score = (cls.clamp(0.0, 1.0) * obj.clamp(0.0, 1.0)).sqrt();
        if score < threshold || cols == 0 {
            continue;
        }
        let (row, col) = ((i / cols) as f32, (i % cols) as f32);
        let b = &out.bbox[i * 4..i * 4 + 4];
        let (cx, cy) = ((col + b[0]) * s, (row + b[1]) * s);
        let (w, h) = (b[2].exp() * s, b[3].exp() * s);
        boxes.push([cx - w / 2.0, cy - h / 2.0, w, h, score]);
    }
    boxes
}

/// Tile origins covering `size` with square tiles of `tile` pixels every `stride` pixels; the
/// last row and column are pulled back so they end at the image edge.
pub fn tile_origins(size: (u32, u32), tile: u32, stride: u32) -> Vec<(u32, u32)> {
    let axis = |len: u32| -> Vec<u32> {
        if len <= tile {
            return vec![0];
        }
        let mut v: Vec<u32> = (0..len - tile).step_by(stride.max(1) as usize).collect();
        v.push(len - tile);
        v.dedup();
        v
    };
    let (xs, ys) = (axis(size.0), axis(size.1));
    ys.iter()
        .flat_map(|&y| xs.iter().map(move |&x| (x, y)))
        .collect()
}

/// Blurs each (already grown) box in place with a Gaussian of sigma = a quarter of the box's
/// longer side, strong enough not to be reversible. Pixels outside the boxes are untouched.
pub fn blur_faces(image: &mut RgbImage, faces: &[FaceBox]) {
    let size = image.dimensions();
    for face in faces {
        let r = face.to_pixels(size);
        if r.w == 0 || r.h == 0 {
            continue;
        }
        let sigma = (r.w.max(r.h) as f32 / 4.0).max(2.0);
        let region = imageops::crop_imm(image, r.x, r.y, r.w, r.h).to_image();
        let blurred = imageops::blur(&region, sigma);
        imageops::replace(image, &blurred, i64::from(r.x), i64::from(r.y));
    }
}
```

The `YuNet` decode follows OpenCV's `face_detect.cpp` (score = √(cls·obj), centre and size from the stride grid); it was reproduced in Python and found 29 of 29 faces on a test photo ([vision models 9](../vision/models.md#9-face-detector)). A parity test against OpenCV's `FaceDetectorYN` on 10 photos needs the model file and a real runtime, so it runs in the rig's integration tests, not here.

---

## 8. Depth

The depth path of an upload ([contracts 3, depth](../architecture/contracts.md#post-scansidimages); [architecture 6.7](../architecture/README.md#67-depth-and-measurement-lidar)). Depth problems never fail the upload: every step returns a `DepthProblem`, and the photo is stored without its depth.

| Step | Code | Problem on failure |
|---|---|---|
| Sent with a guided check | `process_depth` | `not_for_this_kind` |
| Over 4 MB | `process_depth` | `too_large` |
| Not exactly a 16-bit grayscale PNG | `decode_depth_png` | `unsupported_format` |
| Mirrored orientation, aspect off by > 1% | `domain::rules::uploads::check_depth` | `invalid_meta`, `aspect_mismatch` |
| Under 20% coverage | `check_depth_coverage` | `too_sparse` |
| `depth_meta` unparseable; focal lengths, principal point, reference aspect or gravity length wrong | `validate_meta` | `invalid_meta` |
| Confidence map of another size or format | `decode_confidence_png` | `invalid_meta`, `unsupported_format` |
| Storing it | `api` | `storage_error` |

Then the maps turn with the photo by exact pixel moves (no interpolation, so 0 stays "no depth"); the intrinsics, distortion centre and gravity turn with them (a quarter turn swaps `fx` with `fy` and the reference sizes); the intrinsics are scaled to the stored image, so the stored `DepthMeta`'s `ref_width`/`ref_height` **are** the stored image's size; depth and confidence are zeroed inside every blur box grown a further 10% and rounded outwards on the map's grid; and both maps are encoded as PNG.

```rust
/// A depth map's three levels of confidence (`depth_confidence`: 0 low, 1 medium, 2 high).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DepthConfidence {
    /// 0.
    Low = 0,
    /// 1.
    Medium = 1,
    /// 2.
    High = 2,
}

/// An owned depth map in millimetres, row-major; 0 is "no depth".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepthMapBuf {
    width: u32,
    height: u32,
    mm: Vec<u16>,
}

/// A borrowed depth map. `'a` is the borrow of the millimetre buffer, so the measurement stage
/// reads one decoded map from many functions without copying it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DepthMap<'a> {
    width: u32,
    height: u32,
    mm: &'a [u16],
}

/// An owned confidence map (levels 0–2), the same size as its depth map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfidenceMapBuf {
    width: u32,
    height: u32,
    levels: Vec<u8>,
}

/// A borrowed confidence map; `'a` borrows the level buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfidenceMap<'a> {
    width: u32,
    height: u32,
    levels: &'a [u8],
}
```
```rust
/// Rotates a row-major grid by exact pixel moves (no interpolation, so "no depth" stays 0).
/// Returns the new width, height and samples. Generic over the sample type: `u16` depth and
/// `u8` confidence share one implementation.
pub fn rotate_grid<T: Copy>(w: u32, h: u32, data: &[T], rotation: Rotation) -> (u32, u32, Vec<T>) {
    let (w_us, h_us) = (w as usize, h as usize);
    match rotation {
        Rotation::None => (w, h, data.to_vec()),
        Rotation::Half => (w, h, data.iter().rev().copied().collect()),
        Rotation::Cw90 => {
            // new (x', y') = (h - 1 - y, x): new width h, height w
            let mut out = Vec::with_capacity(data.len());
            for ny in 0..w_us {
                for nx in 0..h_us {
                    out.push(data[(h_us - 1 - nx) * w_us + ny]);
                }
            }
            (h, w, out)
        }
        Rotation::Ccw90 => {
            // new (x', y') = (y, w - 1 - x)
            let mut out = Vec::with_capacity(data.len());
            for ny in 0..w_us {
                for nx in 0..h_us {
                    out.push(data[nx * w_us + (w_us - 1 - ny)]);
                }
            }
            (h, w, out)
        }
    }
}

/// Turns a point of a `ref_size` frame with the pixels, in continuous pixel coordinates (pixel
/// `i` covers `[i, i + 1)`): clockwise `(u, v) → (H − v, u)`, half `(W − u, H − v)`,
/// counter-clockwise `(v, W − u)`. Used for the principal point and the distortion centre.
#[allow(
    clippy::many_single_char_names,
    reason = "u, v, w, h: the pixel-coordinate notation of the formulas"
)]
pub fn rotate_point(p: [f64; 2], ref_size: (f64, f64), rotation: Rotation) -> [f64; 2] {
    let (w, h) = ref_size;
    let [u, v] = p;
    match rotation {
        Rotation::None => [u, v],
        Rotation::Cw90 => [h - v, u],
        Rotation::Half => [w - u, h - v],
        Rotation::Ccw90 => [v, w - u],
    }
}

/// Turns gravity (camera coordinates: x right, y down, z forward) with the pixels: clockwise
/// `(x, y) → (−y, x)`, half `(−x, −y)`, counter-clockwise `(y, −x)`; z doesn't change.
pub fn rotate_gravity(g: [f64; 3], rotation: Rotation) -> [f64; 3] {
    let [x, y, z] = g;
    match rotation {
        Rotation::None => [x, y, z],
        Rotation::Cw90 => [-y, x, z],
        Rotation::Half => [-x, -y, z],
        Rotation::Ccw90 => [y, -x, z],
    }
}

/// Turns intrinsics with the pixels: a quarter turn swaps `fx` with `fy` and the reference
/// width with its height, and the principal point turns like any point.
pub fn rotate_intrinsics(k: Intrinsics, rotation: Rotation) -> Intrinsics {
    let size = (f64::from(k.ref_width), f64::from(k.ref_height));
    let [cx, cy] = rotate_point([k.cx, k.cy], size, rotation);
    if rotation.swaps_axes() {
        Intrinsics {
            fx: k.fy,
            fy: k.fx,
            cx,
            cy,
            ref_width: k.ref_height,
            ref_height: k.ref_width,
        }
    } else {
        Intrinsics { cx, cy, ..k }
    }
}

/// Scales intrinsics to another size of the same field of view: `fx` and `cx` by the width
/// ratio, `fy` and `cy` by the height ratio.
pub fn scale_intrinsics(k: Intrinsics, size: (u32, u32)) -> Intrinsics {
    let sx = f64::from(size.0) / f64::from(k.ref_width);
    let sy = f64::from(size.1) / f64::from(k.ref_height);
    Intrinsics {
        fx: k.fx * sx,
        fy: k.fy * sy,
        cx: k.cx * sx,
        cy: k.cy * sy,
        ref_width: size.0,
        ref_height: size.1,
    }
}

/// The stored `DepthMeta`: rotated with the photo, then scaled to the stored image's size. The
/// distortion centre is rotated and scaled the same way; its lookup table is in normalised
/// radius and doesn't change.
pub fn transform_meta(meta: &DepthMeta, rotation: Rotation, stored: (u32, u32)) -> DepthMeta {
    let rotated = rotate_intrinsics(meta.intrinsics, rotation);
    let scaled = scale_intrinsics(rotated, stored);
    let distortion = meta.distortion.as_ref().map(|d| {
        let ref_size = (
            f64::from(meta.intrinsics.ref_width),
            f64::from(meta.intrinsics.ref_height),
        );
        let [x, y] = rotate_point(d.center, ref_size, rotation);
        let sx = f64::from(stored.0) / f64::from(rotated.ref_width);
        let sy = f64::from(stored.1) / f64::from(rotated.ref_height);
        domain::scan::Distortion {
            center: [x * sx, y * sy],
            inverse_lookup: d.inverse_lookup.clone(),
        }
    });
    DepthMeta {
        source: meta.source,
        intrinsics: scaled,
        distortion,
        gravity: meta.gravity.map(|g| rotate_gravity(g, rotation)),
        device_model: meta.device_model.clone(),
        filtered: meta.filtered,
    }
}

/// Checks what the meta must hold before anything is rotated: positive focal lengths and
/// reference size, a reference aspect matching the photo's (within 1%), a principal point inside
/// the frame, and gravity of unit length (±5%) when sent.
pub fn validate_meta(meta: &DepthMeta, sensor_size: (u32, u32)) -> Result<(), DepthProblem> {
    let k = &meta.intrinsics;
    let finite = [k.fx, k.fy, k.cx, k.cy].iter().all(|v| v.is_finite());
    if !finite || k.fx <= 0.0 || k.fy <= 0.0 || k.ref_width == 0 || k.ref_height == 0 {
        return Err(DepthProblem::InvalidMeta);
    }
    if k.cx < 0.0 || k.cy < 0.0 || k.cx > f64::from(k.ref_width) || k.cy > f64::from(k.ref_height) {
        return Err(DepthProblem::InvalidMeta);
    }
    let ref_aspect = f64::from(k.ref_width) / f64::from(k.ref_height);
    let photo_aspect = f64::from(sensor_size.0) / f64::from(sensor_size.1.max(1));
    if (ref_aspect / photo_aspect - 1.0).abs() > 0.01 {
        return Err(DepthProblem::InvalidMeta);
    }
    if let Some(g) = meta.gravity {
        let norm = (g[0] * g[0] + g[1] * g[1] + g[2] * g[2]).sqrt();
        if !norm.is_finite() || (norm - 1.0).abs() > 0.05 {
            return Err(DepthProblem::InvalidMeta);
        }
    }
    Ok(())
}

/// Zeroes depth, and confidence when present, inside each face box grown by `grow` (0.10) and
/// rounded outwards on the map's grid, so no edge of a face survives ([contracts 3]). The boxes
/// are the already-grown blur boxes, normalised to the upright image the map now shares.
pub fn zero_faces(
    depth: &mut DepthMapBuf,
    mut confidence: Option<&mut ConfidenceMapBuf>,
    faces: &[FaceBox],
    grow: f32,
) {
    let size = (depth.width, depth.height);
    for face in faces {
        let r = face.grown(grow).to_pixels(size);
        for y in r.y..(r.y + r.h).min(size.1) {
            for x in r.x..(r.x + r.w).min(size.0) {
                let i = y as usize * size.0 as usize + x as usize;
                depth.mm[i] = 0;
                if let Some(c) = confidence.as_deref_mut() {
                    c.levels[i] = 0;
                }
            }
        }
    }
}
```
```rust
/// The three depth parts of an upload, borrowed from the request body for the duration of the
/// blocking call.
#[derive(Debug, Clone, Copy)]
pub struct DepthInput<'a> {
    /// The `depth` part.
    pub depth: &'a [u8],
    /// The `depth_confidence` part, when sent.
    pub confidence: Option<&'a [u8]>,
    /// The `depth_meta` part, raw JSON.
    pub meta: &'a [u8],
}

/// What the upload knows about the photo when it processes the depth.
#[derive(Debug, Clone, Copy)]
pub struct DepthContext<'f> {
    /// The shot's kind.
    pub kind: ImageKind,
    /// The photo's EXIF orientation value (1–8).
    pub orientation: u8,
    /// The photo's size before orientation, the frame the depth map is in.
    pub sensor_size: (u32, u32),
    /// The stored image's size.
    pub stored_size: (u32, u32),
    /// The grown blur boxes. `'f` borrows the upload's face list.
    pub faces: &'f [FaceBox],
    /// How much more each blur box grows on the depth map (0.10).
    pub face_growth: f32,
}

/// Depth ready to store.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessedDepth {
    /// 16-bit PNG, millimetres, upright.
    pub depth_png: Bytes,
    /// 8-bit PNG, when the capture path sent one.
    pub confidence_png: Option<Bytes>,
    /// Rotated, and scaled to the stored image (`ref_width`/`ref_height` = its size).
    pub meta: DepthMeta,
    /// The depth map's width, upright.
    pub width: u32,
    /// Its height.
    pub height: u32,
}

/// The whole depth path, in the contract's order. Each failure maps to its `problem` code.
pub fn process_depth(
    input: &DepthInput<'_>,
    ctx: &DepthContext<'_>,
) -> Result<ProcessedDepth, DepthProblem> {
    // The checks that need no pixels first, so a guided shot or an oversized part never decodes.
    if !ctx.kind.takes_depth() {
        return Err(DepthProblem::NotForThisKind);
    }
    if input.depth.len() as u64 > domain::rules::uploads::DEPTH_MAX_BYTES {
        return Err(DepthProblem::TooLarge);
    }
    let mut depth = decode_depth_png(input.depth)?;
    check_depth(
        ctx.kind,
        ctx.sensor_size,
        (depth.width, depth.height),
        input.depth.len() as u64,
        ctx.orientation,
    )?;
    let non_zero = depth.mm.iter().filter(|&&v| v != 0).count() as u64;
    check_depth_coverage(non_zero, depth.mm.len() as u64)?;
    let meta: DepthMeta =
        serde_json::from_slice(input.meta).map_err(|_| DepthProblem::InvalidMeta)?;
    validate_meta(&meta, ctx.sensor_size)?;
    let mut confidence = input
        .confidence
        .map(|c| decode_confidence_png(c, (depth.width, depth.height)))
        .transpose()?;
    let rotation = crate::imaging::ExifOrientation::new(ctx.orientation)
        .and_then(crate::imaging::ExifOrientation::rotation)
        .ok_or(DepthProblem::InvalidMeta)?;
    let (w, h, mm) = rotate_grid(depth.width, depth.height, &depth.mm, rotation);
    depth = DepthMapBuf {
        width: w,
        height: h,
        mm,
    };
    if let Some(c) = confidence.as_mut() {
        let (cw, ch, levels) = rotate_grid(c.width, c.height, &c.levels, rotation);
        *c = ConfidenceMapBuf {
            width: cw,
            height: ch,
            levels,
        };
    }
    zero_faces(&mut depth, confidence.as_mut(), ctx.faces, ctx.face_growth);
    let meta = transform_meta(&meta, rotation, ctx.stored_size);
    let depth_png = encode_depth_png(depth.view()).map_err(|_| DepthProblem::UnsupportedFormat)?;
    let confidence_png = confidence
        .as_ref()
        .map(|c| encode_confidence_png(c.view()))
        .transpose()
        .map_err(|_| DepthProblem::UnsupportedFormat)?;
    Ok(ProcessedDepth {
        depth_png: Bytes::from(depth_png),
        confidence_png: confidence_png.map(Bytes::from),
        meta,
        width: depth.width,
        height: depth.height,
    })
}
```

**Coordinate convention.** Points use continuous pixel coordinates (pixel `i` covers `[i, i+1)`), so a clockwise turn maps `(u, v)` to `(H − v, u)`. Whether Apple's principal point is corner- or centre-based is *unverified* (the LiDAR day-1 check); the difference is half a pixel, about 0.1 mm at 30 cm.

---

## 9. Measurement

The measurement stage ([contracts 7, measurement](../architecture/contracts.md#measurement); [architecture 6.7](../architecture/README.md#67-depth-and-measurement-lidar)): deterministic geometry over the stored photo, its stored depth map and its segmentation regions, called by `pipeline` on a blocking thread in `api`. An error never fails the scan: `pipeline` completes the stage as skipped.

```rust
/// A segmentation region of the photo, as the perception result stored it.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasureRegion {
    /// `<image_id>.<n>`.
    pub region_id: RegionId,
    /// Its class.
    pub class: RegionClass,
    /// Its box.
    pub bbox: BBox,
    /// Its mask, when segmentation produced one.
    pub mask: Option<RleMask>,
}

/// One photo to measure, borrowed for the duration of the blocking call. `'a` borrows the
/// caller's buffers: the stored file, the depth parts, the meta and the regions.
#[derive(Debug, Clone, Copy)]
pub struct MeasureInput<'a> {
    /// The photo.
    pub image_id: ImageId,
    /// Its kind: `whole_plant` gives height and width, the leaf kinds the leaf quantities.
    pub kind: ImageKind,
    /// The stored image file (`WebP`).
    pub photo: &'a [u8],
    /// The stored 16-bit depth PNG.
    pub depth_png: &'a [u8],
    /// The stored confidence PNG, when there is one.
    pub confidence_png: Option<&'a [u8]>,
    /// The stored meta (rotated and scaled to the stored image by the upload).
    pub meta: &'a DepthMeta,
    /// The photo's regions.
    pub regions: &'a [MeasureRegion],
}

/// One measured quantity of one image: an observation's `detail.measurement`, its confidence,
/// and its `region` box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasuredQuantity {
    /// The measurement.
    pub measurement: Measurement,
    /// The lowest band of the criteria.
    pub certainty: Certainty,
    /// `certainty.confidence()`: 0.4, 0.65 or 0.85.
    pub confidence: Confidence,
    /// The plant's box, or the box around the measured leaves.
    pub bbox: BBox,
}

/// Why a photo couldn't be measured. Any error skips the stage; the scan goes on.
#[derive(Debug, thiserror::Error)]
pub enum MeasureError {
    /// The stored photo didn't decode.
    #[error("photo: {0}")]
    Photo(#[source] VisionError),
    /// The stored depth or confidence map didn't decode.
    #[error("depth: {0:?}")]
    Depth(DepthProblem),
    /// A region's mask didn't decode, or has another size than the photo.
    #[error("mask: {0}")]
    Mask(#[from] RleError),
    /// The meta doesn't fit the photo.
    #[error("meta: {0}")]
    Meta(&'static str),
}

/// The measurement stage. `Send + Sync`: build one at start and share it in an `Arc`.
#[derive(Clone)]
pub struct Measurer {
    config: MeasurementConfig,
    spots: Arc<dyn SpotSegmenter>,
}
```
```rust
impl Measurer {
    /// A measurer with the given constants and spot segmenter.
    pub fn new(config: MeasurementConfig, spots: Arc<dyn SpotSegmenter>) -> Self {
        Self { config, spots }
    }

    /// `measure-1+<spot segmenter version>`.
    pub fn version(&self) -> String {
        format!("{ALGORITHM_VERSION}+{}", self.spots.version())
    }

    /// The constants in use.
    pub const fn config(&self) -> &MeasurementConfig {
        &self.config
    }

    /// Measures one photo. Blocking. Returns only the quantities whose needs are met
    /// ([contracts 7], the table of quantities); an empty list is a valid result.
    pub fn measure(&self, input: &MeasureInput<'_>) -> Result<Vec<MeasuredQuantity>, MeasureError> {
        let photo = imaging::decode_stored(input.photo).map_err(MeasureError::Photo)?;
        let size = photo.dimensions();
        let depth = depth::decode_depth_png(input.depth_png).map_err(MeasureError::Depth)?;
        let confidence = input
            .confidence_png
            .map(|c| depth::decode_confidence_png(c, (depth.view().width(), depth.view().height())))
            .transpose()
            .map_err(MeasureError::Depth)?;
        let aspect = |w: u32, h: u32| f64::from(w) / f64::from(h.max(1));
        if (aspect(size.0, size.1) / aspect(depth.view().width(), depth.view().height()) - 1.0)
            .abs()
            > 0.01
        {
            return Err(MeasureError::Meta("depth map and photo differ in aspect"));
        }
        let scene = DepthScene::new(
            depth.view(),
            confidence.as_ref().map(depth::ConfidenceMapBuf::view),
            Camera::for_depth(input.meta, depth.view()),
        );
        let ctx = Context {
            input,
            size,
            scene: &scene,
            image: ImageView::from(&photo),
            image_camera: Camera::for_image(input.meta, size),
            filtered_without_confidence: input.meta.filtered && confidence.is_none(),
        };
        match input.kind {
            ImageKind::WholePlant => self.whole_plant(&ctx),
            ImageKind::LeafCloseup | ImageKind::LeafUnderside => self.leaves(&ctx),
            _ => Ok(Vec::new()),
        }
    }

    /// [contracts 7] organism length: the longer side of the box at the median confident
    /// depth inside it, in millimetres. Not tilt-corrected; shown, never scored.
    pub fn organism_length_mm(
        &self,
        depth: &DepthMap<'_>,
        meta: &DepthMeta,
        bbox: BBox,
    ) -> Option<f64> {
        let rect = PixelRect::from_normalised(bbox.to_array(), (depth.width(), depth.height()));
        let mut z: Vec<f64> = Vec::new();
        for y in rect.y..(rect.y + rect.h).min(depth.height()) {
            for x in rect.x..(rect.x + rect.w).min(depth.width()) {
                z.extend(depth.metres(x, y));
            }
        }
        let z = plane::median(&z)?;
        let k = meta.intrinsics;
        let [_, _, w, h] = bbox.to_array();
        let width_m = w * f64::from(k.ref_width) * z / k.fx;
        let height_m = h * f64::from(k.ref_height) * z / k.fy;
        Some(width_m.max(height_m) * 1000.0)
    }

    fn seed(input: &MeasureInput<'_>, n: u32) -> u64 {
        plane::seed_for(&[input.image_id.to_string().as_bytes(), &n.to_le_bytes()])
    }

    fn whole_plant(&self, ctx: &Context<'_, '_>) -> Result<Vec<MeasuredQuantity>, MeasureError> {
        let cfg = &self.config;
        let Some(g) = ctx.input.meta.gravity else {
            return Ok(Vec::new()); // both quantities need gravity
        };
        let gravity = Vector3::new(g[0], g[1], g[2]).normalize();
        let (dw, dh) = ctx.scene.size();
        let plant = ctx
            .input
            .regions
            .iter()
            .find(|r| r.class == RegionClass::Plant);
        let hint = match plant {
            Some(MeasureRegion { mask: Some(m), .. }) => {
                let m = rle::decode(m)?;
                PlantHint::Mask(mask::resample(m.view(), dw, dh))
            }
            Some(r) => PlantHint::Box(MaskBuf::from_rect(
                dw,
                dh,
                PixelRect::from_normalised(r.bbox.to_array(), (dw, dh)),
            )),
            None => PlantHint::None,
        };
        let Some(geo) = measure_plant(ctx.scene, gravity, &hint, cfg, Self::seed(ctx.input, 0))
        else {
            return Ok(Vec::new());
        };
        let criteria = Criteria {
            coverage: geo.coverage,
            residual_m: None,
            distance_m: geo.distance_m,
            range_m: cfg.whole_plant_range_m,
            plant_region: Some(geo.region),
            filtered_without_confidence: ctx.filtered_without_confidence,
        };
        let certainty = band(&criteria, cfg);
        let bbox = plant
            .map(|r| r.bbox)
            .or_else(|| BBox::new(geo.bbox[0], geo.bbox[1], geo.bbox[2], geo.bbox[3]).ok())
            .unwrap_or(BBox::FULL);
        let one = |quantity, metres: f64| MeasuredQuantity {
            measurement: single(quantity, metres * 100.0, geo.distance_m),
            certainty,
            confidence: certainty.confidence(),
            bbox,
        };
        let mut out = Vec::new();
        out.extend(geo.height_m.map(|h| one(MeasureQuantity::PlantHeightCm, h)));
        out.extend(geo.width_m.map(|w| one(MeasureQuantity::CanopyWidthCm, w)));
        Ok(out)
    }

    /// Measures every leaf region that has a mask, with the region's box.
    fn measure_leaves(
        &self,
        ctx: &Context<'_, '_>,
    ) -> Result<Vec<(LeafGeometry, BBox)>, MeasureError> {
        let cfg = &self.config;
        let mut measured: Vec<(LeafGeometry, BBox)> = Vec::new();
        for region in ctx
            .input
            .regions
            .iter()
            .filter(|r| r.class == RegionClass::Leaf)
        {
            let Some(rle_mask) = &region.mask else {
                continue;
            };
            let leaf = rle::decode(rle_mask)?;
            if (leaf.width(), leaf.height()) != ctx.size {
                return Err(MeasureError::Meta("mask size differs from the photo"));
            }
            if let Some(geo) = measure_leaf(
                ctx.scene,
                &ctx.image_camera,
                ctx.image,
                &leaf,
                self.spots.as_ref(),
                cfg,
                Self::seed(ctx.input, region.region_id.n),
            ) {
                measured.push((geo, region.bbox));
            }
        }
        Ok(measured)
    }

    fn leaves(&self, ctx: &Context<'_, '_>) -> Result<Vec<MeasuredQuantity>, MeasureError> {
        let cfg = &self.config;
        let measured = self.measure_leaves(ctx)?;
        if measured.is_empty() {
            return Ok(Vec::new());
        }
        let bbox = union_bbox(measured.iter().map(|(_, b)| *b));
        let leaves: Vec<&LeafGeometry> = measured.iter().map(|(g, _)| g).collect();
        let rate = |used: &[&LeafGeometry], distance_m: f64| {
            band(
                &Criteria {
                    coverage: pooled_coverage(used),
                    residual_m: plane::median(&used.iter().map(|l| l.rms_m).collect::<Vec<_>>()),
                    distance_m,
                    range_m: cfg.leaf_range_m,
                    plant_region: None,
                    filtered_without_confidence: ctx.filtered_without_confidence,
                },
                cfg,
            )
        };
        let mut out = Vec::new();
        let mut push = |quantity, items: &[f64], used: &[&LeafGeometry]| {
            let distances: Vec<f64> = used.iter().map(|l| l.distance_m).collect();
            if let Some(m) = aggregate(quantity, items, &distances) {
                let certainty = rate(used, m.distance_m);
                out.push(MeasuredQuantity {
                    measurement: m,
                    certainty,
                    confidence: certainty.confidence(),
                    bbox,
                });
            }
        };
        let per_leaf =
            |f: fn(&LeafGeometry) -> f64| leaves.iter().map(|l| f(l)).collect::<Vec<_>>();
        push(
            MeasureQuantity::LeafLengthCm,
            &per_leaf(|l| l.length_m * 100.0),
            &leaves,
        );
        push(
            MeasureQuantity::LeafAreaCm2,
            &per_leaf(|l| l.area_m2 * 1e4),
            &leaves,
        );
        push(
            MeasureQuantity::LesionCount,
            &per_leaf(|l| l.spots.len() as f64),
            &leaves,
        );
        push(
            MeasureQuantity::AffectedAreaPct,
            &per_leaf(|l| l.affected_pct),
            &leaves,
        );
        push(
            MeasureQuantity::HoleCount,
            &per_leaf(|l| l.holes.len() as f64),
            &leaves,
        );
        let sized = |items: fn(&LeafGeometry) -> &[leaf::SizedItem]| {
            let used: Vec<&LeafGeometry> = leaves
                .iter()
                .copied()
                .filter(|l| items(l).iter().any(|s| s.diameter_px >= cfg.min_sized_px))
                .collect();
            let values: Vec<f64> = used
                .iter()
                .flat_map(|l| items(l).iter())
                .filter(|s| s.diameter_px >= cfg.min_sized_px)
                .map(|s| s.diameter_m() * 1000.0)
                .collect();
            (values, used)
        };
        let (spots, with_spots) = sized(|l| &l.spots);
        push(MeasureQuantity::LesionDiameterMm, &spots, &with_spots);
        let (holes, with_holes) = sized(|l| &l.holes);
        push(MeasureQuantity::HoleDiameterMm, &holes, &with_holes);
        Ok(out)
    }
}
```

### Constants

Every constant is in `MeasurementConfig`, with the contract's value as the default; the ones the contract doesn't give are marked "ours" and are starting values for the evaluation set's LiDAR cases ([contracts 12](../architecture/contracts.md#12-open-questions), question 10).

```rust
/// The measurement stage's constants.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasurementConfig {
    /// Masks shrink by this many depth pixels before depth is read inside them. Contract: 2.
    pub shrink_depth_px: u32,
    /// Spots and holes under this many pixels across (equivalent diameter, stored-image pixels)
    /// are counted but not sized. Contract: 4.
    pub min_sized_px: f64,
    /// Upper percentile for height and canopy width edges. Contract: 98.
    pub high_percentile: f64,
    /// Lower percentile for canopy width. Contract: 2.
    pub low_percentile: f64,

    /// Ground candidates: surface normal within this many degrees of gravity. Contract: 15.
    pub ground_normal_max_deg: f64,
    /// A RANSAC plane is a ground candidate with at least this share of the candidate points.
    /// Contract: 5%.
    pub ground_min_share: f64,
    /// The base is the lowest share of the plant's points along gravity. Contract: 5%.
    pub base_share: f64,
    /// The ground may sit this far above the base where leaves touch the soil. Contract: 5 cm.
    pub ground_above_base_m: f64,
    /// Points this close to the ground plane are ground. Contract: 3 cm.
    pub ground_band_m: f64,
    /// Neighbouring depth pixels within this distance join the connected layer. Contract: 5 cm.
    pub layer_step_m: f64,
    /// A `plant` mask whose lowest points are this far above the ground (a pot): height starts
    /// at those points. Contract: 5 cm.
    pub pot_gap_m: f64,
    /// At most this many ground planes are tried (ours).
    pub ground_max_planes: usize,
    /// RANSAC inlier distance for the ground (ours: `LiDAR` noise at 1–3 m).
    pub ground_inlier_m: f64,

    /// RANSAC iterations per plane (ours).
    pub ransac_iterations: usize,
    /// RANSAC inlier distance for a leaf plane (ours: about `LiDAR`'s noise at 30 cm).
    pub leaf_inlier_m: f64,
    /// A leaf needs at least this many confident depth points for a plane (ours).
    pub min_plane_points: usize,
    /// The leaf length search samples at most this many outline points (ours; keeps it O(n²)
    /// bounded).
    pub max_outline_points: usize,

    /// Coverage for high / medium confidence. Contract: 60% / 30%.
    pub coverage_high: f64,
    /// See `coverage_high`.
    pub coverage_medium: f64,
    /// Plane residual (RMS) for high / medium. Contract: 5 mm / 10 mm.
    pub residual_high_m: f64,
    /// See `residual_high_m`.
    pub residual_medium_m: f64,
    /// Valid distance for `whole_plant`. Contract: 0.4–4 m.
    pub whole_plant_range_m: (f64, f64),
    /// Valid distance for the leaf kinds. Contract: 0.25–0.6 m (the lower bound moves with the
    /// day-1 check).
    pub leaf_range_m: (f64, f64),
    /// Up to this share beyond the upper end is medium. Contract: 20%.
    pub beyond_upper_medium: f64,
}
```

### Pixels to a plane

A stored-image pixel `(u, v)` is the ray `K⁻¹ · (u + ½, v + ½, 1)` after undistortion, with `K` scaled to the stored image; a depth pixel is that ray scaled by its z-depth on the depth map's own grid. The area a pixel covers on a plane `n · p + d = 0` is `t² / (fx · fy · |n · dir|)`, where the ray meets the plane at `t · dir`: a fronto-parallel pixel at 30 cm with `fx = 1000` is 0.3 mm square, and a 45° tilt multiplies it by √2 (both tested).

```rust
/// Apple's lens-distortion lookup ([`domain::scan::Distortion`]): the table gives, for radii
/// from 0 to the largest distance between the centre and a corner, the relative magnification
/// to apply to a distorted point's offset from the centre. Linear interpolation between entries,
/// as Apple's sample code does (*unverified* against a real capture until the day-1 check).
#[derive(Debug, Clone, PartialEq)]
pub struct Undistort {
    center: [f64; 2],
    max_radius: f64,
    table: Vec<f64>,
}

/// A camera: intrinsics for one image grid, with optional undistortion.
#[derive(Debug, Clone, PartialEq)]
pub struct Camera {
    k: Intrinsics,
    undistort: Option<Undistort>,
}

/// `fx` in stored-image pixels, for the indicator margin (2 × `distance_m` / `fx`). The upload
/// stores intrinsics already scaled to the stored image, so this only rescales when the stored
/// width differs from `ref_width`.
pub fn stored_fx(meta: &DepthMeta, stored_width: u32) -> f64 {
    meta.intrinsics.fx * f64::from(stored_width) / f64::from(meta.intrinsics.ref_width)
}
```
```rust
/// The area on `plane` of the stored-image pixel `(u, v)`: with the ray `dir = K⁻¹(u, v, 1)`
/// meeting the plane at `t·dir`, the footprint is `t² / (fx · fy · |n · dir|)`.
pub fn pixel_area(camera: &Camera, plane: &Plane, u: u32, v: u32) -> Option<(f64, Vector3<f64>)> {
    let dir = camera.ray(f64::from(u), f64::from(v));
    let t = plane.intersect(&dir)?;
    let k = camera.intrinsics();
    let area = t * t / (k.fx * k.fy * plane.n.dot(&dir).abs());
    Some((area, dir * t))
}
```

### Deterministic RANSAC

`rand`'s `StdRng` is documented as non-portable (a future version may change the algorithm) ✓, so measurements use `ChaCha8Rng` from `chacha20`, seeded with FNV-1a 64 of the image id and the region number, and draw indices by a fixed multiply-shift rule on `next_u64`. A test pins the stream (`seed 7 → 157, 167, 704, 726, 601`), so a dependency bump that would change every measurement fails loudly. Percentiles are nearest-rank, and the contract's medians are the nearest-rank 50th percentile.

```rust
/// A plane `n · p + d = 0` with a unit normal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    /// Unit normal.
    pub n: Vector3<f64>,
    /// Offset.
    pub d: f64,
}

/// A fitted plane with its inliers and their RMS distance (the contract's residual).
#[derive(Debug, Clone, PartialEq)]
pub struct PlaneFit {
    /// The plane, normal facing the camera.
    pub plane: Plane,
    /// Indices of the inliers into the input.
    pub inliers: Vec<usize>,
    /// RMS distance of the inliers to the plane, metres.
    pub rms: f64,
}

/// The least-squares plane through ≥ 3 points: the right singular vector of the centred 3×3
/// covariance with the smallest singular value (looked up, not assumed last).
pub fn fit_plane(points: &[Vector3<f64>]) -> Option<(Plane, f64)> {
    if points.len() < 3 {
        return None;
    }
    let n = points.len() as f64;
    let c = points.iter().sum::<Vector3<f64>>() / n;
    let cov = points
        .iter()
        .map(|p| (p - c) * (p - c).transpose())
        .sum::<Matrix3<f64>>();
    let svd = SVD::new(cov, false, true);
    let v_t = svd.v_t?;
    let (imin, _) = svd
        .singular_values
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(b.1))?;
    let mut normal = v_t.row(imin).transpose().normalize();
    if normal.dot(&c) > 0.0 {
        normal = -normal; // face the camera
    }
    let plane = Plane {
        n: normal,
        d: -normal.dot(&c),
    };
    let rms = (points
        .iter()
        .map(|p| plane.distance(p).powi(2))
        .sum::<f64>()
        / n)
        .sqrt();
    Some((plane, rms))
}

/// The measurement RNG: `ChaCha8` seeded from a `u64`.
pub fn seeded(seed: u64) -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(seed)
}

/// An index below `n` from the RNG's next `u64`: Lemire's multiply-shift on the high 32 bits,
/// a fixed rule that doesn't depend on `rand`'s `random_range` staying the same.
pub fn below<R: Rng>(rng: &mut R, n: usize) -> usize {
    (((rng.next_u64() >> 32) * n as u64) >> 32) as usize
}

/// FNV-1a 64 over bytes: the seed of a measurement, from the image id and the region number,
/// so a rerun of the same photo draws the same samples.
pub fn seed_for(parts: &[&[u8]]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for &b in *part {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h ^= 0xff;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// RANSAC for one plane: `iterations` minimal samples, keep the plane with the most inliers
/// within `threshold`, refit on its inliers by SVD.
pub fn ransac(
    points: &[Vector3<f64>],
    threshold: f64,
    iterations: usize,
    seed: u64,
) -> Option<PlaneFit> {
    if points.len() < 3 {
        return None;
    }
    let mut rng = seeded(seed);
    let mut best: Option<(Plane, usize)> = None;
    for _ in 0..iterations {
        let n = points.len();
        let (i, j, k) = (below(&mut rng, n), below(&mut rng, n), below(&mut rng, n));
        if i == j || j == k || i == k {
            continue;
        }
        let Some(candidate) = plane_from_3(&points[i], &points[j], &points[k]) else {
            continue;
        };
        let count = points
            .iter()
            .filter(|p| candidate.distance(p) <= threshold)
            .count();
        if best.as_ref().is_none_or(|b| count > b.1) {
            best = Some((candidate, count));
        }
    }
    let (candidate, _) = best?;
    let inliers: Vec<usize> = (0..points.len())
        .filter(|&i| candidate.distance(&points[i]) <= threshold)
        .collect();
    let chosen: Vec<_> = inliers.iter().map(|&i| points[i]).collect();
    let (plane, rms) = fit_plane(&chosen)?;
    Some(PlaneFit {
        plane,
        inliers,
        rms,
    })
}

/// Nearest-rank percentile (`p` in 0–100) on a copy: the value at rank ⌈p/100 · n⌉.
pub fn percentile(values: &[f64], p: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let rank = ((p / 100.0) * v.len() as f64).ceil().max(1.0) as usize;
    Some(v[rank.min(v.len()) - 1])
}

/// The median as the contract's `value` uses it: the nearest-rank 50th percentile.
pub fn median(values: &[f64]) -> Option<f64> {
    percentile(values, 50.0)
}
```

### Leaves, spots and holes

For each `leaf` region with a mask on a leaf close-up or underside: the mask is resampled to the depth grid and eroded by two depth pixels; a RANSAC plane (4 mm inliers) is fitted to the confident depth inside it; its residual is the inliers' RMS distance. Leaf length is the longest distance between outline points projected onto the plane (sampled to 1,500 points); area is the sum of the mask pixels' footprints. Spots are 4-connected components of the pixels the `SpotSegmenter` marks necrotic, sized by the circle of their projected area and sized only from 4 pixels across; halo pixels count toward `affected_area_pct` only. Holes are the outside components inside the leaf's bounding box that don't reach its edge.

```rust
/// One sized item (a spot or a hole): its projected area and its size in stored-image pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizedItem {
    /// Projected area, m².
    pub area_m2: f64,
    /// Equivalent diameter in stored-image pixels (2·√(pixels/π)).
    pub diameter_px: f64,
}

/// Everything measured on one leaf.
#[derive(Debug, Clone, PartialEq)]
pub struct LeafGeometry {
    /// The fitted plane.
    pub plane: Plane,
    /// Its RMS residual, metres.
    pub rms_m: f64,
    /// Confident pixels over shrunken-mask pixels on the depth grid.
    pub coverage: f64,
    /// Median camera distance of the plane's points, metres.
    pub distance_m: f64,
    /// Longest distance across the projected outline, metres.
    pub length_m: f64,
    /// Projected area, m².
    pub area_m2: f64,
    /// Every spot (necrotic component).
    pub spots: Vec<SizedItem>,
    /// Necrotic and halo area over leaf area, percent.
    pub affected_pct: f64,
    /// Every hole.
    pub holes: Vec<SizedItem>,
}

/// Measures one leaf. `mask` is on the stored image's grid; `None` when the leaf has too few
/// confident depth points for a plane.
pub fn measure_leaf(
    scene: &DepthScene<'_>,
    image_camera: &Camera,
    image: ImageView<'_>,
    leaf: &MaskBuf,
    spots: &dyn SpotSegmenter,
    cfg: &MeasurementConfig,
    seed: u64,
) -> Option<LeafGeometry> {
    let (dw, dh) = scene.size();
    let on_depth = mask::resample(leaf.view(), dw, dh);
    let shrunk = mask::erode(on_depth.view(), cfg.shrink_depth_px);
    let region = scene.points_in(&shrunk);
    if region.points.len() < cfg.min_plane_points {
        return None;
    }
    let fit = ransac(
        &region.points,
        cfg.leaf_inlier_m,
        cfg.ransac_iterations,
        seed,
    )?;
    let distance_m = region.median_distance()?;
    let plane = fit.plane;

    let leaf_pixels: Vec<(u32, u32)> = (0..leaf.height())
        .flat_map(|y| (0..leaf.width()).map(move |x| (x, y)))
        .filter(|&(x, y)| leaf.get(x, y))
        .collect();
    let area_m2 = projected_area(image_camera, &plane, &leaf_pixels);
    let outline: Vec<Vector3<f64>> = mask::outline(leaf.view())
        .into_iter()
        .filter_map(|(u, v)| pixel_area(image_camera, &plane, u, v).map(|(_, p)| p))
        .collect();
    let length_m = longest_extent(&outline, cfg.max_outline_points);

    let classes = spots.segment(image.as_domain(), leaf.view());
    let class = |x: u32, y: u32| classes.classes[y as usize * classes.width as usize + x as usize];
    let necrotic = mask::components(leaf.width(), leaf.height(), |x, y| {
        class(x, y) == SpotClass::Necrotic
    });
    let spot_items: Vec<SizedItem> = necrotic
        .iter()
        .map(|c| sized(image_camera, &plane, c))
        .collect();
    let affected: Vec<(u32, u32)> = leaf_pixels
        .iter()
        .copied()
        .filter(|&(x, y)| matches!(class(x, y), SpotClass::Necrotic | SpotClass::Halo))
        .collect();
    let affected_pct = if area_m2 > 0.0 {
        100.0 * projected_area(image_camera, &plane, &affected) / area_m2
    } else {
        0.0
    };
    let holes = mask::holes(leaf.view())
        .iter()
        .map(|c| sized(image_camera, &plane, c))
        .collect();
    Some(LeafGeometry {
        plane,
        rms_m: fit.rms,
        coverage: region.coverage(),
        distance_m,
        length_m,
        area_m2,
        spots: spot_items,
        affected_pct,
        holes,
    })
}
```
```rust
/// Thresholds in HSV, relative to the leaf's median value. Starting values, set from the
/// evaluation set's `LiDAR` leaves ([architecture 15], open decision 17).
///
/// [architecture 15]: ../../../docs/architecture/README.md#15-open-decisions
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpotThresholds {
    /// Darker than this share of the leaf's median value: black or dark brown.
    pub dark_value: f64,
    /// Brown: hue from this many degrees …
    pub brown_hue_min: f64,
    /// … to this many …
    pub brown_hue_max: f64,
    /// … with at least this saturation, and darker than `brown_value` × the leaf's median.
    pub brown_saturation: f64,
    /// See `brown_saturation`.
    pub brown_value: f64,
    /// Pale: saturation under this, and brighter than `pale_value` × the leaf's median.
    pub pale_saturation: f64,
    /// See `pale_saturation`.
    pub pale_value: f64,
    /// Yellow halo: hue from this many degrees …
    pub halo_hue_min: f64,
    /// … to this many, with at least `halo_saturation`.
    pub halo_hue_max: f64,
    /// See `halo_hue_max`.
    pub halo_saturation: f64,
}

/// The colour-threshold segmenter.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ColourSpotSegmenter {
    /// Its thresholds.
    pub thresholds: SpotThresholds,
}
```

### Height and canopy width

```rust
/// The plant's region on the depth grid, from segmentation, when there is one.
#[derive(Debug, Clone, PartialEq)]
pub enum PlantHint {
    /// A `plant` mask (resampled to the depth grid): the plant points are its points.
    Mask(MaskBuf),
    /// Only a `plant` box: the base comes from it and the layer stays inside it.
    Box(MaskBuf),
    /// No `plant` region: the central third of the frame.
    None,
}

/// What the whole-plant shot gave.
#[derive(Debug, Clone, PartialEq)]
pub struct PlantGeometry {
    /// Height, metres; `None` without ground beneath the base, or when the plant touches the
    /// top or bottom edge.
    pub height_m: Option<f64>,
    /// Canopy width, metres; `None` when the plant touches a side edge.
    pub width_m: Option<f64>,
    /// Median camera distance of the plant points, metres.
    pub distance_m: f64,
    /// The coverage criterion: confident pixels over region pixels (the shrunken mask or box, or
    /// the central third).
    pub coverage: f64,
    /// Mask or layer.
    pub region: PlantRegion,
    /// The plant points' bounding box on the depth grid, normalised `[x, y, w, h]`.
    pub bbox: [f64; 4],
}

/// A ground candidate plane and its height along "up" (−gravity).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundCandidate {
    /// The plane.
    pub plane: Plane,
    /// The height of its inliers' centroid along −gravity, metres.
    pub height_m: f64,
}

/// Steps 1–2: every ground candidate, highest first.
pub fn ground_candidates(
    scene: &DepthScene<'_>,
    gravity: &Vector3<f64>,
    cfg: &MeasurementConfig,
    seed: u64,
) -> Vec<GroundCandidate> {
    let cos_max = cfg.ground_normal_max_deg.to_radians().cos();
    let (w, h) = scene.size();
    let mut pool: Vec<Vector3<f64>> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if let (Some(n), Some(p)) = (normal_at(scene, x, y), scene.point(x, y))
                && n.dot(gravity).abs() >= cos_max
            {
                pool.push(p);
            }
        }
    }
    let min_inliers = ((pool.len() as f64) * cfg.ground_min_share).ceil() as usize;
    let mut out = Vec::new();
    for round in 0..cfg.ground_max_planes {
        if pool.len() < min_inliers.max(3) {
            break;
        }
        let Some(fit) = ransac(
            &pool,
            cfg.ground_inlier_m,
            cfg.ransac_iterations,
            seed.wrapping_add(round as u64),
        ) else {
            break;
        };
        if fit.inliers.len() < min_inliers.max(3) {
            break;
        }
        let centroid =
            fit.inliers.iter().map(|&i| pool[i]).sum::<Vector3<f64>>() / fit.inliers.len() as f64;
        out.push(GroundCandidate {
            plane: fit.plane,
            height_m: up_height(&centroid, gravity),
        });
        let inliers: std::collections::BTreeSet<usize> = fit.inliers.into_iter().collect();
        pool = pool
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !inliers.contains(i))
            .map(|(_, p)| p)
            .collect();
    }
    out.sort_by(|a, b| b.height_m.total_cmp(&a.height_m));
    out
}

/// Step 4: the highest candidate at or below the base, with the allowance.
pub fn choose_ground(
    candidates: &[GroundCandidate],
    base_height_m: f64,
    cfg: &MeasurementConfig,
) -> Option<GroundCandidate> {
    candidates
        .iter()
        .find(|c| c.height_m <= base_height_m + cfg.ground_above_base_m)
        .copied()
}

/// Step 5 without a mask: the connected layer from the confident non-ground pixel nearest the
/// frame's centre (inside the box when given), grown through 4-neighbours whose points lie
/// within `layer_step_m` of each other.
pub fn connected_layer(
    scene: &DepthScene<'_>,
    is_ground: &dyn Fn(&Vector3<f64>) -> bool,
    inside: Option<&MaskBuf>,
    cfg: &MeasurementConfig,
) -> Vec<(u32, u32)> {
    let (w, h) = scene.size();
    let allowed = |x: u32, y: u32| -> Option<Vector3<f64>> {
        if inside.is_some_and(|m| !m.get(x, y)) {
            return None;
        }
        scene.point(x, y).filter(|p| !is_ground(p))
    };
    let (cx, cy) = (f64::from(w) / 2.0, f64::from(h) / 2.0);
    let seed = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| allowed(x, y).is_some())
        .min_by(|a, b| {
            let d = |p: &(u32, u32)| (f64::from(p.0) - cx).powi(2) + (f64::from(p.1) - cy).powi(2);
            d(a).total_cmp(&d(b))
        });
    let Some(seed) = seed else {
        return Vec::new();
    };
    let mut visited = vec![false; w as usize * h as usize];
    let idx = |x: u32, y: u32| y as usize * w as usize + x as usize;
    visited[idx(seed.0, seed.1)] = true;
    let mut queue = VecDeque::from([seed]);
    let mut out = Vec::new();
    while let Some((x, y)) = queue.pop_front() {
        out.push((x, y));
        let Some(p) = allowed(x, y) else { continue };
        for (nx, ny) in [
            (x.wrapping_sub(1), y),
            (x + 1, y),
            (x, y.wrapping_sub(1)),
            (x, y + 1),
        ] {
            if nx >= w || ny >= h || visited[idx(nx, ny)] {
                continue;
            }
            if let Some(q) = allowed(nx, ny)
                && (q - p).norm() <= cfg.layer_step_m
            {
                visited[idx(nx, ny)] = true;
                queue.push_back((nx, ny));
            }
        }
    }
    out
}

/// Steps 1–6. `gravity` is the stored meta's unit vector towards the ground.
pub fn measure_plant(
    scene: &DepthScene<'_>,
    gravity: Vector3<f64>,
    hint: &PlantHint,
    cfg: &MeasurementConfig,
    seed: u64,
) -> Option<PlantGeometry> {
    let size = scene.size();
    let base_region = match hint {
        PlantHint::Mask(m) | PlantHint::Box(m) => m.clone(),
        PlantHint::None => central_third(size),
    };
    let shrunk = crate::mask::erode(base_region.view(), cfg.shrink_depth_px);
    let region = scene.points_in(&shrunk);
    let heights: Vec<f64> = region
        .points
        .iter()
        .map(|p| up_height(p, &gravity))
        .collect();
    let base = percentile(&heights, cfg.base_share * 100.0)?;
    let ground = choose_ground(&ground_candidates(scene, &gravity, cfg, seed), base, cfg);
    let is_ground =
        |p: &Vector3<f64>| ground.is_some_and(|g| g.plane.distance(p) <= cfg.ground_band_m);

    let (pixels, region_kind) = match hint {
        PlantHint::Mask(m) => {
            let px: Vec<(u32, u32)> = region
                .pixels
                .iter()
                .copied()
                .filter(|&(x, y)| m.get(x, y) && scene.point(x, y).is_some_and(|p| !is_ground(&p)))
                .collect();
            (px, PlantRegion::Mask)
        }
        PlantHint::Box(b) => (
            connected_layer(scene, &is_ground, Some(b), cfg),
            PlantRegion::Layer,
        ),
        PlantHint::None => (
            connected_layer(scene, &is_ground, None, cfg),
            PlantRegion::Layer,
        ),
    };
    let points: Vec<Vector3<f64>> = pixels
        .iter()
        .filter_map(|&(x, y)| scene.point(x, y))
        .collect();
    if points.is_empty() {
        return None;
    }
    let (w, h) = size;
    let touches_vertical = pixels.iter().any(|&(_, y)| y == 0 || y + 1 == h);
    let touches_side = pixels.iter().any(|&(x, _)| x == 0 || x + 1 == w);

    let up: Vec<f64> = points.iter().map(|p| up_height(p, &gravity)).collect();
    let top = percentile(&up, cfg.high_percentile)?;
    let height_m = ground.filter(|_| !touches_vertical).map(|g| {
        let mut floor = g.height_m;
        if region_kind == PlantRegion::Mask
            && let Some(lowest) = percentile(&up, cfg.low_percentile)
            && lowest - g.height_m > cfg.pot_gap_m
        {
            floor = lowest; // a pot or planter: start at the plant's own lowest points
        }
        top - floor
    });

    let centroid = points.iter().sum::<Vector3<f64>>() / points.len() as f64;
    let across = gravity.cross(&centroid.normalize());
    let width_m = (!touches_side && across.norm() > 1e-9).then(|| {
        let axis = across.normalize();
        let along: Vec<f64> = points.iter().map(|p| axis.dot(p)).collect();
        match (
            percentile(&along, cfg.high_percentile),
            percentile(&along, cfg.low_percentile),
        ) {
            (Some(hi), Some(lo)) => hi - lo,
            _ => 0.0,
        }
    });
    let distances: Vec<f64> = points.iter().map(Vector3::norm).collect();
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for &(x, y) in &pixels {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x + 1);
        y1 = y1.max(y + 1);
    }
    Some(PlantGeometry {
        height_m,
        width_m,
        distance_m: super::plane::median(&distances)?,
        coverage: region.coverage(),
        region: region_kind,
        bbox: [
            f64::from(x0) / f64::from(w),
            f64::from(y0) / f64::from(h),
            f64::from(x1 - x0) / f64::from(w),
            f64::from(y1 - y0) / f64::from(h),
        ],
    })
}
```

Surface normals for the ground candidates come from each depth pixel's right and lower neighbours. Width runs along `gravity × line of sight` (horizontal, across the view). The edge rules use the plant points' depth pixels after the ground is removed: a point on the top or bottom row cancels the height, on a side column the width.

### Confidence and aggregation

```rust
/// Where a whole-plant measurement's points came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlantRegion {
    /// A `plant` mask: high.
    Mask,
    /// The connected depth layer: medium (a pot counts as plant).
    Layer,
}

/// The inputs of the criteria table. `None` marks a criterion that doesn't apply.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Criteria {
    /// Share of the shrunken region's pixels with medium or high depth confidence.
    pub coverage: f64,
    /// Plane residual (RMS), metres: leaf, spot and hole quantities.
    pub residual_m: Option<f64>,
    /// Median distance, metres.
    pub distance_m: f64,
    /// The kind's valid range.
    pub range_m: (f64, f64),
    /// Height and canopy width only.
    pub plant_region: Option<PlantRegion>,
    /// `filtered: true` without a confidence map.
    pub filtered_without_confidence: bool,
}

/// The band: the lowest any applicable criterion reaches.
pub fn band(c: &Criteria, cfg: &MeasurementConfig) -> Certainty {
    let tier = |high: bool, medium: bool| {
        if high {
            Certainty::High
        } else if medium {
            Certainty::Medium
        } else {
            Certainty::Low
        }
    };
    let mut bands = vec![tier(
        c.coverage >= cfg.coverage_high,
        c.coverage >= cfg.coverage_medium,
    )];
    if let Some(r) = c.residual_m {
        bands.push(tier(r <= cfg.residual_high_m, r <= cfg.residual_medium_m));
    }
    let (lo, hi) = c.range_m;
    bands.push(if c.distance_m < lo {
        Certainty::Low
    } else {
        tier(
            c.distance_m <= hi,
            c.distance_m <= hi * (1.0 + cfg.beyond_upper_medium),
        )
    });
    if let Some(region) = c.plant_region {
        bands.push(match region {
            PlantRegion::Mask => Certainty::High,
            PlantRegion::Layer => Certainty::Medium,
        });
    }
    bands.push(if c.filtered_without_confidence {
        Certainty::Medium
    } else {
        Certainty::High
    });
    bands.into_iter().min().unwrap_or(Certainty::Low)
}
```
```rust
/// `value` = nearest-rank median of the items, `min`, `max`, `n`; `distance_m` = median of the
/// contributing leaves' distances. `None` without items.
pub fn aggregate(
    quantity: MeasureQuantity,
    items: &[f64],
    distances: &[f64],
) -> Option<Measurement> {
    let value = plane::median(items)?;
    Some(Measurement {
        quantity,
        value,
        min: items.iter().copied().fold(f64::INFINITY, f64::min),
        max: items.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        n: u32::try_from(items.len()).unwrap_or(u32::MAX),
        distance_m: plane::median(distances)?,
    })
}
```

For a quantity over several leaves, contracts 7 doesn't say how per-leaf criteria combine; here coverage is the leaves' mean, the residual their median RMS, and the distance the quantity's `distance_m` (section 15). `MeasuredQuantity.certainty` is the band; `confidence` is `certainty.confidence()` (0.4, 0.65, 0.85).

**What `pipeline` needs** to answer measurable indicators: the measurements with their bands (it picks the best per quantity: highest confidence, then largest `n`), `distance_m` in each, and `fx` in stored-image pixels, which is `meta.intrinsics.fx` of the stored meta (`stored_fx` rescales if a caller ever passes another width).

---

## 10. The text embedder

snowflake-arctic-embed-xs in fp32 at revision `d8c86521`, with the recipe of [vision models 10](../vision/models.md#10-text-embedder): `tokenizer.json` with truncation at 512 and padding to the longest with id 0, `token_type_ids` all zeros, CLS pooling (`last_hidden_state[:, 0, :]`), L2 to unit length dividing by `max(‖v‖, 1e-12)`, 384 dimensions. The query prefix goes before chat questions and search queries, nothing before knowledge chunks and memories.

```rust
/// Put before every chat question and search query ([vision models 10]).
pub const QUERY_PREFIX: &str = "Represent this sentence for searching relevant passages: ";

/// The embedder's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbedderConfig {
    /// Sessions in the pool: how many embeddings run at once.
    pub sessions: usize,
    /// Truncation, tokens.
    pub max_tokens: usize,
}

/// The embedder. Cloning shares the sessions. One instance serves both sides of a retrieval:
/// the caller passes [`EmbedMode`] on every `TextEmbedder::embed` call, and only
/// [`EmbedMode::Query`] gets [`QUERY_PREFIX`].
#[derive(Clone)]
pub struct ArcticEmbedder {
    inner: Arc<Inner>,
}

/// The texts as the model reads them: [`QUERY_PREFIX`] before each query, documents unchanged.
pub fn model_inputs(mode: EmbedMode, texts: &[String]) -> Vec<String> {
    texts
        .iter()
        .map(|t| match mode {
            EmbedMode::Query => format!("{QUERY_PREFIX}{t}"),
            EmbedMode::Document => t.clone(),
        })
        .collect()
}

/// CLS pooling (each row's first token) and L2 normalisation, dividing by max(‖v‖, 1e-12).
pub fn cls_pool_normalise(hidden: &[f32], batch: usize, seq: usize, dim: usize) -> Vec<Vec<f32>> {
    (0..batch)
        .map(|b| {
            let start = b * seq * dim;
            let cls = &hidden[start..start + dim];
            let norm = cls.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-12);
            cls.iter().map(|v| v / norm).collect()
        })
        .collect()
}
```
```rust
#[async_trait]
impl TextEmbedder for ArcticEmbedder {
    fn model(&self) -> EmbeddingModel {
        self.inner.model.clone()
    }

    /// Moves a clone of `self` (an `Arc`), the `Copy` mode and an owned copy of the texts into
    /// `spawn_blocking`, whose closure must be `'static`: it can't borrow `&self` or `texts`,
    /// which only live as long as this call.
    async fn embed(&self, mode: EmbedMode, texts: &[String]) -> Result<Vec<Embedding>, ModelError> {
        let this = self.clone();
        let texts = texts.to_vec();
        tokio::task::spawn_blocking(move || this.embed_blocking(mode, &texts))
            .await
            .map_err(VisionError::from)
            .and_then(|r| r)
            .map_err(|e| e.into_model_error("arctic-embed-xs"))
    }
}
```

**Two modes, one session pool.** The prefix depends on whether the texts are queries or documents, so `TextEmbedder::embed(mode, texts)` takes a `domain::ports::EmbedMode` (re-exported here) and `model_inputs` puts `QUERY_PREFIX` before queries only. One `ArcticEmbedder` (one `Arc` of sessions and tokenizer) serves chat, search and memory writes; each caller names the mode. Vectors are stored with `embedding_model = stored_name()` (`snowflake-arctic-embed-xs@d8c86521`).

The golden test of [vision models 10](../vision/models.md#10-text-embedder) (5 fixed strings, the first 8 values within 1e-4, norms within 1e-5) needs the model file and a runtime; it runs with the Python ingestion's fixtures in the integration tests.

---

## 11. Masks and COCO RLE

Masks cross the worker API as COCO RLE ([contracts 5, `RleMask`](../architecture/contracts.md#results)). No maintained Rust crate does this, and the algorithm is short, so `rle` ports `rleToString`, `rleFrString` and the encode and decode loops of the COCO API's `maskApi.c`. Masks are flattened column-major, `size` is `[height, width]`, and from the third count each is stored as the difference to the count two places earlier. Decoding checks every character, truncated varints, negative runs, the 64-megapixel cap and that the runs cover exactly `height × width`. Six fixtures written by pycocotools decode to the same pixels and re-encode byte for byte ✓.

```rust
/// Why a mask string is invalid.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RleError {
    /// A character outside `0`…`o` (48–111).
    #[error("invalid character {0:#04x}")]
    BadChar(u8),
    /// A count's last character is missing, or a count is longer than any valid one.
    #[error("truncated count")]
    Truncated,
    /// A count decodes to a negative run.
    #[error("negative run")]
    Negative,
    /// The runs don't add up to `height × width`.
    #[error("runs sum to {sum}, expected {expected}")]
    SizeMismatch {
        /// Their sum.
        sum: u64,
        /// `height × width`.
        expected: u64,
    },
    /// Over [`MAX_PIXELS`].
    #[error("mask too large")]
    TooLarge,
}

/// `rleFrString`: the counts of a compressed string.
#[allow(
    clippy::many_single_char_names,
    reason = "the names of maskApi.c, so the port reads against it"
)]
pub fn counts_from_string(s: &str) -> Result<Vec<u32>, RleError> {
    let b = s.as_bytes();
    let mut counts: Vec<i64> = Vec::with_capacity(b.len());
    let mut p = 0;
    while p < b.len() {
        let (mut x, mut k, mut more) = (0i64, 0u32, true);
        while more {
            let ch = *b.get(p).ok_or(RleError::Truncated)?;
            if !(48..48 + 64).contains(&ch) {
                return Err(RleError::BadChar(ch));
            }
            let c = i64::from(ch - 48);
            x |= (c & 0x1f) << (5 * k);
            more = c & 0x20 != 0;
            p += 1;
            k += 1;
            if !more && (c & 0x10) != 0 {
                x |= -1i64 << (5 * k);
            }
            if k > 12 {
                return Err(RleError::Truncated);
            }
        }
        if counts.len() > 2 {
            x += counts[counts.len() - 2];
        }
        counts.push(x);
    }
    counts
        .into_iter()
        .map(|c| u32::try_from(c).map_err(|_| RleError::Negative))
        .collect()
}

/// `rleToString`: the compressed string of counts.
pub fn string_from_counts(counts: &[u32]) -> String {
    let mut s = String::new();
    for (i, &c) in counts.iter().enumerate() {
        let mut x = i64::from(c);
        if i > 2 {
            x -= i64::from(counts[i - 2]);
        }
        loop {
            let mut ch = (x & 0x1f) as u8;
            x >>= 5;
            let more = if ch & 0x10 != 0 { x != -1 } else { x != 0 };
            if more {
                ch |= 0x20;
            }
            s.push(char::from(ch + 48));
            if !more {
                break;
            }
        }
    }
    s
}

/// Encodes a mask (non-zero = inside) the way `maskApi.c` does, byte for byte.
pub fn encode(mask: MaskView<'_>) -> RleMask {
    let (w, h) = (mask.width(), mask.height());
    let mut counts = Vec::new();
    let (mut run, mut current) = (0u32, false);
    for col in 0..w {
        for row in 0..h {
            let v = mask.data()[row as usize * w as usize + col as usize] != 0;
            if v != current {
                counts.push(run);
                run = 0;
                current = v;
            }
            run += 1;
        }
    }
    counts.push(run);
    RleMask {
        size: [h, w],
        counts: string_from_counts(&counts),
    }
}

/// Decodes a mask, checking every count and that the runs cover exactly `height × width`.
pub fn decode(rle: &RleMask) -> Result<MaskBuf, RleError> {
    let [h, w] = rle.size;
    let total = u64::from(h) * u64::from(w);
    if total > MAX_PIXELS {
        return Err(RleError::TooLarge);
    }
    let counts = counts_from_string(&rle.counts)?;
    let sum: u64 = counts.iter().map(|&c| u64::from(c)).sum();
    if sum != total {
        return Err(RleError::SizeMismatch {
            sum,
            expected: total,
        });
    }
    let mut mask = MaskBuf::empty(w, h);
    let (mut i, mut value) = (0u64, false);
    for c in counts {
        if value {
            for j in i..i + u64::from(c) {
                mask.set((j / u64::from(h)) as u32, (j % u64::from(h)) as u32, true);
            }
        }
        i += u64::from(c);
        value = !value;
    }
    Ok(mask)
}
```
```rust
/// An owned 8-bit mask, row-major: non-zero inside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskBuf {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

/// Nearest-neighbour resampling of a mask to another size of the same field of view (the
/// stored image's mask onto the depth map's grid).
pub fn resample(mask: MaskView<'_>, width: u32, height: u32) -> MaskBuf {
    let mut out = MaskBuf::empty(width, height);
    let (sx, sy) = (
        f64::from(mask.width()) / f64::from(width),
        f64::from(mask.height()) / f64::from(height),
    );
    for y in 0..height {
        let my = (((f64::from(y) + 0.5) * sy) as u32).min(mask.height() - 1);
        for x in 0..width {
            let mx = (((f64::from(x) + 0.5) * sx) as u32).min(mask.width() - 1);
            if mask.data()[my as usize * mask.width() as usize + mx as usize] != 0 {
                out.set(x, y, true);
            }
        }
    }
    out
}

/// Erodes by `radius` pixels with a square structuring element: a pixel stays inside only when
/// every pixel within `radius` (Chebyshev distance) is inside. Contracts 7 shrinks masks by two
/// depth pixels before reading depth.
pub fn erode(mask: MaskView<'_>, radius: u32) -> MaskBuf {
    let (w, h) = (mask.width(), mask.height());
    let inside = |x: u32, y: u32| mask.data()[y as usize * w as usize + x as usize] != 0;
    // Separable: rows, then columns. A pixel survives a pass when its run of inside pixels
    // reaches `radius` on both sides.
    let mut rows = MaskBuf::empty(w, h);
    for y in 0..h {
        for x in 0..w {
            let lo = x.checked_sub(radius);
            let hi = (x + radius < w).then_some(x + radius);
            if let (Some(lo), Some(hi)) = (lo, hi)
                && (lo..=hi).all(|xx| inside(xx, y))
            {
                rows.set(x, y, true);
            }
        }
    }
    let mut out = MaskBuf::empty(w, h);
    for y in 0..h {
        for x in 0..w {
            let lo = y.checked_sub(radius);
            let hi = (y + radius < h).then_some(y + radius);
            if let (Some(lo), Some(hi)) = (lo, hi)
                && (lo..=hi).all(|yy| rows.get(x, yy))
            {
                out.set(x, y, true);
            }
        }
    }
    out
}

/// One 4-connected component: its pixels, row-major order of discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    /// `(x, y)` of each pixel.
    pub pixels: Vec<(u32, u32)>,
    /// Whether any pixel lies on the image border.
    pub touches_border: bool,
}

/// The 4-connected components of the pixels for which `select(x, y)` holds, breadth first.
pub fn components(width: u32, height: u32, select: impl Fn(u32, u32) -> bool) -> Vec<Component> {
    let mut seen = vec![false; width as usize * height as usize];
    let idx = |x: u32, y: u32| y as usize * width as usize + x as usize;
    let mut out = Vec::new();
    let mut queue = VecDeque::new();
    for y in 0..height {
        for x in 0..width {
            if seen[idx(x, y)] || !select(x, y) {
                continue;
            }
            seen[idx(x, y)] = true;
            queue.push_back((x, y));
            let mut c = Component {
                pixels: Vec::new(),
                touches_border: false,
            };
            while let Some((cx, cy)) = queue.pop_front() {
                c.pixels.push((cx, cy));
                c.touches_border |= cx == 0 || cy == 0 || cx + 1 == width || cy + 1 == height;
                let neighbours = [
                    (cx.wrapping_sub(1), cy),
                    (cx + 1, cy),
                    (cx, cy.wrapping_sub(1)),
                    (cx, cy + 1),
                ];
                for (nx, ny) in neighbours {
                    if nx < width && ny < height && !seen[idx(nx, ny)] && select(nx, ny) {
                        seen[idx(nx, ny)] = true;
                        queue.push_back((nx, ny));
                    }
                }
            }
            out.push(c);
        }
    }
    out
}

/// Holes: gaps enclosed by the mask's outline ([contracts 7]), the components of the outside
/// pixels within the mask's bounding box that don't reach the box's edge.
pub fn holes(mask: MaskView<'_>) -> Vec<Component> {
    let (w, h) = (mask.width(), mask.height());
    let inside = |x: u32, y: u32| mask.data()[y as usize * w as usize + x as usize] != 0;
    let leaf = components(w, h, inside);
    let Some(bounds) = leaf.iter().max_by_key(|c| c.area()).map(Component::bounds) else {
        return Vec::new();
    };
    let in_box = |x: u32, y: u32| {
        x >= bounds.x && y >= bounds.y && x < bounds.x + bounds.w && y < bounds.y + bounds.h
    };
    components(w, h, |x, y| in_box(x, y) && !inside(x, y))
        .into_iter()
        .filter(|c| {
            !c.pixels.iter().any(|&(x, y)| {
                x == bounds.x
                    || y == bounds.y
                    || x + 1 == bounds.x + bounds.w
                    || y + 1 == bounds.y + bounds.h
            })
        })
        .collect()
}

/// The outline: inside pixels with at least one 4-neighbour outside (or on the border).
pub fn outline(mask: MaskView<'_>) -> Vec<(u32, u32)> {
    let (w, h) = (mask.width(), mask.height());
    let inside = |x: u32, y: u32| mask.data()[y as usize * w as usize + x as usize] != 0;
    let mut out = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if !inside(x, y) {
                continue;
            }
            let edge = x == 0
                || y == 0
                || x + 1 == w
                || y + 1 == h
                || !inside(x - 1, y)
                || !inside(x + 1, y)
                || !inside(x, y - 1)
                || !inside(x, y + 1);
            if edge {
                out.push((x, y));
            }
        }
    }
    out
}
```

`api` decodes a stored mask to an 8-bit PNG for `GET /scans/{id}/masks/{mask_id}` with `rle::decode` and the `image` PNG encoder.

---

## 12. Concurrency and lifetimes

| What | Runs on | Why |
|---|---|---|
| Upload pipeline, measurement, RLE | `api`'s blocking pool, called inside `spawn_blocking` behind a semaphore | CPU-bound for 0.1–1 s; tokio's blocking pool grows without bound otherwise |
| Face detector, text embedder | the caller's blocking thread, one pooled session each | `Session::run(&mut self)`; the pool lets N calls run at once |
| Perception models | the inference thread, for the life of the process | one owner, no locks, ONNX Runtime's threads off tokio's workers |
| Perception decoding and pre-processing | the blocking pool, behind `PerceptionPipeline.cpu` | owned images in, owned tensors out |

**Named lifetimes.**

| Lifetime | Where | What it borrows |
|---|---|---|
| `'a` | `ImageView<'a>`, `DepthMap<'a>`, `ConfidenceMap<'a>`, `domain::ports::MaskView<'a>` | the pixel or sample buffer; views are `Copy` and can't outlive their buffer, so one decoded map is read by many functions without copying |
| `'a` | `UploadInput<'a>`, `DepthInput<'a>`, `MeasureInput<'a>` | the request's bytes and records for one blocking call: `api` moves the owned `Bytes` into the closure and borrows them inside it, so nothing is copied to cross onto the thread |
| `'f` | `DepthContext<'f>` | the upload's face list, read by the depth step |
| `'p` | `Pooled<'p, T>` | the pool, so a checked-out session can't outlive it |
| `'a` | `DepthScene<'a>` | the decoded depth and confidence buffers, owned by `Measurer::measure`'s stack frame for the whole measurement |
| `'i`, `'s` | the private `Context<'i, 's>` of `measure` | the input (`'i`) and the scene and image (`'s`), kept apart because the scene is built inside the call while the input comes from the caller |
| `'m` | `Manifest::entries_with_prefix<'m>` | the manifest and the prefix; the returned iterator captures both (the 2024 edition captures every in-scope lifetime in `impl Trait`, which is what this iterator needs) |

**`'static` and `Send` across threads.** `spawn_blocking` takes a `FnOnce + Send + 'static` closure, because the blocking task may outlive the caller's future (dropping the `JoinHandle` doesn't stop it) ✓ (tokio docs). So no closure here borrows the caller: `ArcticEmbedder::embed` moves a clone of itself (an `Arc` and a mode) and an owned copy of the texts in; `PerceptionPipeline::decode` and `tensors` move owned `Bytes` clones, `RgbImage`s and the recipe in, plus the semaphore's `OwnedSemaphorePermit`, so the permit is held until the work ends even if the job is dropped. Inside such a closure a `Pooled<'p, _>` guard borrows the pool through the `Arc` the closure owns, so `'p` is bounded by the closure's own lifetime and the `'static` rule holds. Tensors sent to the inference thread are owned (`Inputs = Vec<(String, ArrayD<f32>)>`): a channel's item type must be `'static` and `Send` to cross threads.

**Shared state.** `UploadProcessor`, `Measurer`, `ArcticEmbedder` and `PerceptionPipeline` are `Send + Sync` and shared in `Arc`s; `Measurer` holds its segmenter as `Arc<dyn SpotSegmenter>` (dyn compatible: no generic methods, the lifetime is on the method). Pure geometry (planes, RANSAC, rotations) uses generics and static dispatch: `rotate_grid<T: Copy>` serves `u16` depth and `u8` confidence with one implementation; `below<R: Rng>` takes any RNG.

---

## 13. Errors

```rust
/// Everything that can go wrong in this crate outside the upload pipeline's own outcomes
/// (`upload::UploadError`) and the measurement stage's (`measure::MeasureError`).
#[derive(Debug, thiserror::Error)]
pub enum VisionError {
    /// ONNX Runtime refused to load, build or run a session.
    #[error("ONNX Runtime: {0}")]
    Ort(#[from] ort::Error),
    /// The model manifest is unreadable, or a file's hash doesn't match it.
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    /// The configured execution provider isn't compiled into this build.
    #[error("execution provider {0:?} is not compiled into this build")]
    EpNotCompiled(OrtEp),
    /// A model returned a tensor of an unexpected shape or name.
    #[error("{model}: unexpected output: {detail}")]
    Shape {
        /// The model.
        model: &'static str,
        /// What was wrong.
        detail: String,
    },
    /// An image couldn't be decoded or encoded.
    #[error("image: {0}")]
    Image(#[from] image::ImageError),
    /// A resize failed (a zero-sized source or target).
    #[error("resize: {0}")]
    Resize(String),
    /// The tokenizer failed.
    #[error("tokenizer: {0}")]
    Tokenizer(#[source] BoxError),
    /// A payload names a label list or classifier mapping this worker hasn't loaded.
    #[error("{what} {version} is not loaded")]
    UnknownLabelVersion {
        /// `label_set_version` or `classifier_labels_version`.
        what: &'static str,
        /// The version asked for.
        version: String,
    },
    /// A mask failed to encode or decode.
    #[error(transparent)]
    Rle(#[from] RleError),
    /// The job was cancelled while a model ran (`RunOptions::terminate`).
    #[error("cancelled")]
    Cancelled,
    /// The inference thread has stopped.
    #[error("the inference thread has stopped")]
    InferenceGone,
    /// A blocking task panicked.
    #[error("blocking task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}
```
```rust
impl VisionError {
    /// The port error for `model`. A failed model is retryable ([contracts 5], `model_error`);
    /// an unknown label version isn't (`unknown_label_version`), which the worker reads from
    /// [`VisionError::is_unknown_label_version`] before it maps the error.
    pub fn into_model_error(self, model: &'static str) -> ModelError {
        ModelError::Failed {
            model,
            source: Box::new(self),
        }
    }

    /// Whether this is `unknown_label_version` ([contracts 5], `JobErrorCode`).
    pub const fn is_unknown_label_version(&self) -> bool {
        matches!(self, Self::UnknownLabelVersion { .. })
    }
}
```

| Error | Becomes |
|---|---|
| `VisionError` from a perception job | `ModelError::Failed` → `complete` with `model_error`, retryable; `UnknownLabelVersion` → `unknown_label_version`, not retryable (the worker downcasts the source) |
| `VisionError` from the embedder | `ModelError::Failed` → `api`'s `503 upstream_unavailable` path for chat |
| `UploadError::Quality` | `422 image_quality` with `problem.hint()` |
| `UploadError::UnsupportedMedia` | `415 unsupported_media_type` |
| `UploadError::FaceBlur`, `Encode` | `503 processing_unavailable`; the image isn't stored |
| `DepthProblem` | `UploadImageResponse.depth = { stored: false, problem }`; the photo is stored |
| `MeasureError` | the measurement stage is skipped (`stage_completed` with `skipped: error`) |
| `ManifestError` | start-up fails |

---

## 14. Testing

| Area | Tests |
|---|---|
| Pool | never more checkouts than values (8 threads, 2 values); a panicking caller still returns its value |
| Manifest | parsing an entry and its config; `perception_config` follows the values |
| Imaging | EXIF orientation 6 turns 40×20 into 20×40 and is gone after WebP; HEIC is sniffed by its brand; `fit_long_side` never upscales; boxes round outwards |
| Quality | a sharp checkerboard passes, its blur fails; a black frame is too dark |
| Faces | union merge of overlaps and containment; growth clamped; 12 tiles cover 12 megapixels; YuNet decode places an anchor on its cell; blur changes only the box |
| Depth | quarter-turn intrinsics and inverse; gravity turns with the pixels; grid rotation matches image rotation; PNG round trip and format checks; face zeroing with the extra 10%; `process_depth` maps each failure to its problem |
| Measurement | pixel footprints (fronto-parallel and 45°); the longest extent; RANSAC finds a 15° plane among outliers and is deterministic; the pinned stream; percentiles; the criteria table; ground choice (raised bed over path, bench above the base); the colour classes; **a synthetic 18 cm leaf at 30 cm with a 30 mm spot measures within 2 mm, 3 cm² and 0.5 cm, twice identically**; organism length |
| Perception | tensors are CHW and normalised for every resize rule; batches split at the effective cap; detector decode keeps confident large boxes and drops duplicates; SAM prompts and upsampling; species scores follow the closest row and shot weights; the disease head is restricted to the working species; soil shots never count for the plant check |
| Embedder | CLS pooling and normalisation |
| RLE and masks | pycocotools fixtures byte for byte; garbage rejected; holes, erosion, resampling, outlines |

Nothing here runs ONNX Runtime: `ort` with `cuda` can't link on macOS, and the default `load-dynamic` links nothing but would need a runtime at the first session (the `rust-workers` skill, `ort.md`). Model-level checks (YuNet parity with OpenCV, the embedder's golden vectors, warm-up at the largest batch, VRAM budgets) run on the rig on day 1–2 ([vision evaluation](../vision/evaluation.md)).

---

## 15. Open questions

Real ones, for [contracts 12](../architecture/contracts.md#12-open-questions) and [vision 11](../vision/README.md#11-open-decisions):

1. **Pooling criteria across leaves.** Contracts 7's confidence table is per measurement, but leaf quantities pool several leaves. Here: mean coverage, median residual. Alternatives: the lowest leaf's band, or one measurement per leaf.
2. **The stored size of a photo without depth.** Contracts 3 fixes only "at least 2048" with depth; this crate uses 1600 (100–500 KB of WebP, as storage 9 estimates). And "uploaded with depth" is read as "whose depth survived validation" (agreed with `api`): a dropped depth map stores the photo at the ordinary size.
3. **Resolved: `TextEmbedder` takes an `EmbedMode`** ([domain 9](domain.md#9-boundary-traits)). The two-mode wrapper is gone; every call names its mode.
4. **Nullable schema shapes** (`anyOf` with `null`) are used for soil readings and `other_name` in the worker's VLM schemas; xgrammar's handling of `anyOf` is *unverified* (the day-2 schema check).
5. **The head and label-matrix files don't load yet.** `ml/models/FORMATS.md` (PR #6) is the format of record, and `perception/heads.rs` reads an older shape: a label matrix's `labels` (the code wants `species`), a head label's `problem_id` (the code reads `problem`, so every class would load as `healthy`), and the probe's base64 `bias` (the code wants a number). `store`'s `label_set_version` is an FNV-1a hash where `FORMATS.md` fixes a SHA-256 recipe. See [vision 6](../vision/README.md#delivering-model-files-to-the-backend) and roadmap gap 6.
6. **Apple's principal-point convention** and the inverse-distortion table's exact use (section 8, [camera](#pixels-to-a-plane)) are *unverified* until the LiDAR day-1 check ([contracts 12](../architecture/contracts.md#12-open-questions), question 11).

---

## 16. Sources

**Rust language**
- The Book: [16.4 `Send` and `Sync`](https://doc.rust-lang.org/book/ch16-04-extensible-concurrency-sync-and-send.html) · [10.3 lifetimes](https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html) · [15.3 `Drop`](https://doc.rust-lang.org/book/ch15-03-drop.html) · [18.2 trait objects](https://doc.rust-lang.org/book/ch18-02-trait-objects.html)
- The Reference: [dyn compatibility](https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility) · [lifetime elision](https://doc.rust-lang.org/reference/lifetime-elision.html)
- [Edition Guide, RPIT lifetime capture](https://doc.rust-lang.org/edition-guide/rust-2024/rpit-lifetime-capture.html) · std [`Condvar`](https://doc.rust-lang.org/std/sync/struct.Condvar.html) · [`PoisonError::into_inner`](https://doc.rust-lang.org/std/sync/struct.PoisonError.html#method.into_inner)
- tokio 1.53.3: [`spawn_blocking`](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html) · [`Semaphore::acquire_owned`](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Semaphore.html#method.acquire_owned) · [`mpsc::Receiver::blocking_recv`](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/struct.Receiver.html#method.blocking_recv)

**Crates at the pinned versions**
- [ort 2.0.0-rc.14](https://docs.rs/ort/2.0.0-rc.14/ort/) ([`Session::run`](https://docs.rs/ort/2.0.0-rc.14/ort/session/struct.Session.html#method.run), [`RunOptions::terminate`](https://docs.rs/ort/2.0.0-rc.14/ort/session/struct.RunOptions.html#method.terminate), [execution providers](https://ort.pyke.io/perf/execution-providers)) and its source (`ep/`, `environment.rs`, `ort-sys` `dist.tsv`)
- [image 0.25.10](https://docs.rs/image/0.25.10/image/) · [imageproc 0.27.0](https://docs.rs/imageproc/0.27.0/imageproc/) · [fast_image_resize 6.1.0](https://docs.rs/fast_image_resize/6.1.0/fast_image_resize/) · [webp 0.3.1](https://docs.rs/webp/0.3.1/webp/) · [png 0.18.1](https://docs.rs/png/0.18.1/png/) · [libheif-rs 3.0.0](https://docs.rs/libheif-rs/3.0.0/libheif_rs/)
- [nalgebra 0.35.0 `SVD`](https://docs.rs/nalgebra/0.35.0/nalgebra/linalg/struct.SVD.html) · [rand 0.10.3](https://docs.rs/rand/0.10.3/rand/) · [chacha20 0.10.2](https://docs.rs/chacha20/0.10.2/chacha20/) · [tokenizers 0.23.2](https://docs.rs/tokenizers/0.23.2/tokenizers/) · [ndarray 0.17.2](https://docs.rs/ndarray/0.17.2/ndarray/)

**Models and algorithms**
- [COCO API `maskApi.c`](https://github.com/cocodataset/cocoapi/blob/master/common/maskApi.c) · [OpenCV Zoo YuNet](https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet) · [ONNX Runtime CUDA execution provider](https://onnxruntime.ai/docs/execution-providers/CUDA-ExecutionProvider.html) · [MIGraphX execution provider](https://onnxruntime.ai/docs/execution-providers/MIGraphX-ExecutionProvider.html) · [WebGPU execution provider](https://onnxruntime.ai/docs/execution-providers/WebGPU-ExecutionProvider.html)

**Project documents, research and skills**
- [Data contracts](../architecture/contracts.md) · [architecture](../architecture/README.md) · [storage](../storage/README.md) · [domain](domain.md) · [pipeline](pipeline.md) · [api](api.md) · [worker](worker.md)
- [Computer-vision design](../vision/README.md): [models](../vision/models.md), [VLM](../vision/vlm.md), [evaluation](../vision/evaluation.md)
- The backend research notes (vision, AMD RX 9070 XT, model decisions), with the vision and AMD probe crates; ✓ marks a fact compiled or run there or here
- Skills in the main checkout's `.claude/skills/`: `rust` (`versions.md`), `rust-workers` (`ort.md`), `rust-async` (`blocking.md`), `rust-errors`, `rust-testing`

External links were taken from the research notes, which opened them on 2026-10-15; the crate facts marked ✓ were compiled against the pinned versions in the scratch workspace.
