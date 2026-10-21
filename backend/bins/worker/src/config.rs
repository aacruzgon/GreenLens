//! Command line and environment. One process has one capability (`--caps`); everything else is
//! deployment configuration read from the environment, so one image serves every card.

use std::{path::PathBuf, time::Duration};

use clap::{ArgAction, Args, Parser, ValueEnum, builder::BoolishValueParser};
use domain::{jobs::Capability, vlm::BoxFormat};
use integrations::openai_compat::ServerFlavour;
use secrecy::SecretString;
use vision::runtime::OrtEp;

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

impl Caps {
    /// The heartbeat's and claim's `caps`.
    pub const fn capability(self) -> Capability {
        match self {
            Self::Perception => Capability::Perception,
            Self::Vlm => Capability::Vlm,
            Self::Fallback => Capability::Fallback,
        }
    }
}

/// `VLM_ADAPTER`: the server behind `VLM_BASE_URL`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum VlmServerKind {
    /// `vLLM` (CUDA or `ROCm` image).
    Vllm,
    /// llama.cpp `llama-server` (CUDA or Vulkan).
    Llamacpp,
}

impl VlmServerKind {
    /// The integrations client's flavour.
    pub const fn flavour(self) -> ServerFlavour {
        match self {
            Self::Vllm => ServerFlavour::Vllm,
            Self::Llamacpp => ServerFlavour::LlamaCpp,
        }
    }
}

/// `VLM_BOX_FORMAT` ([contracts 6], box formats per model adapter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum BoxFormatArg {
    /// 0–1000 (Qwen3-VL, Qwen3.5).
    Norm1000,
    /// Pixels of the prepared image (Qwen2.5-VL; the hosted LLM).
    AbsPxPrepared,
    /// 0–1.
    Norm1,
}

impl BoxFormatArg {
    /// The domain's box format.
    pub const fn format(self) -> BoxFormat {
        match self {
            Self::Norm1000 => BoxFormat::Norm1000,
            Self::AbsPxPrepared => BoxFormat::AbsPxPrepared,
            Self::Norm1 => BoxFormat::Norm1,
        }
    }
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
    /// Stub vision (canned perception results for the walking skeleton).
    #[command(flatten)]
    pub stub: StubArgs,
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
    #[arg(
        id = "vlm_api_key",
        long = "vlm-api-key",
        env = "VLM_API_KEY",
        hide_env_values = true
    )]
    pub api_key: Option<SecretString>,
    /// `vllm` or `llamacpp`.
    #[arg(long, env = "VLM_ADAPTER", value_enum, default_value = "vllm")]
    pub adapter: VlmServerKind,
    /// The served model name (`--served-model-name vlm`).
    #[arg(long, env = "VLM_SERVED_NAME", default_value = "vlm")]
    pub served_name: String,
    /// `vlm_model`: repository@revision of the checkpoint the sidecar serves.
    #[arg(
        id = "vlm_model",
        long = "vlm-model",
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
    #[arg(
        id = "vlm_max_pixels",
        long = "vlm-max-pixels",
        env = "VLM_MAX_PIXELS",
        default_value_t = 1_003_520
    )]
    pub max_pixels: u32,
    /// Jobs held at once; the sidecar's `--max-num-seqs` (or `-np`) has the same value.
    #[arg(
        id = "vlm_slots",
        long = "vlm-slots",
        env = "VLM_SLOTS",
        default_value_t = 1
    )]
    pub slots: u32,
}

/// `--caps fallback` settings ([vision vlm 12]).
///
/// [vision vlm 12]: ../../../docs/vision/vlm.md#12-the-fallback-worker
#[derive(Debug, Args)]
pub struct FallbackArgs {
    /// The LLM API key.
    #[arg(
        id = "llm_api_key",
        long = "llm-api-key",
        env = "LLM_API_KEY",
        hide_env_values = true
    )]
    pub api_key: Option<SecretString>,
    /// The hosted model.
    #[arg(
        id = "fallback_model",
        long = "fallback-model",
        env = "FALLBACK_VISION_MODEL",
        default_value = "claude-opus-5-5"
    )]
    pub model: String,
    /// Jobs held at once, bounded by the LLM API's rate limit.
    #[arg(
        id = "fallback_slots",
        long = "fallback-slots",
        env = "FALLBACK_SLOTS",
        default_value_t = 4
    )]
    pub slots: u32,
    /// Long side of a prepared image: the size the API doesn't resize.
    #[arg(long, env = "FALLBACK_MAX_LONG_SIDE", default_value_t = 1568)]
    pub max_long_side: u32,
    /// Pixels of a prepared image (about 1.15 megapixels, the standard tier).
    #[arg(
        id = "fallback_max_pixels",
        long = "fallback-max-pixels",
        env = "FALLBACK_MAX_PIXELS",
        default_value_t = 1_150_000
    )]
    pub max_pixels: u32,
}

/// `--stub-vision` settings ([worker 15.1]): canned `perception` results, refused in a release
/// build without `PDM_ALLOW_STUB_VISION=1`.
///
/// [worker 15.1]: ../../../docs/backend/worker.md#151-stub-vision
#[derive(Debug, Args)]
pub struct StubArgs {
    /// Answer `perception` jobs with canned results instead of loading the models.
    #[arg(long, env = "WORKER_STUB_VISION", value_parser = BoolishValueParser::new())]
    pub stub_vision: bool,
    /// Lets `--stub-vision` start in a release build.
    #[arg(long, env = "PDM_ALLOW_STUB_VISION", value_parser = BoolishValueParser::new())]
    pub allow_stub_vision: bool,
    /// The `label_set_version` the heartbeat reports (comma-separated when several).
    #[arg(long, env = "STUB_LABEL_SET_VERSION")]
    pub label_set_version: Option<String>,
    /// The `classifier_labels_version` the heartbeat reports (comma-separated when several).
    #[arg(long, env = "STUB_CLASSIFIER_LABELS_VERSION")]
    pub classifier_labels_version: Option<String>,
    /// Species slugs offered as candidates, best first.
    #[arg(
        long,
        env = "STUB_SPECIES",
        value_delimiter = ',',
        default_value = "tomato"
    )]
    pub species: Vec<String>,
    /// Problem slugs the disease stage may add below `healthy`.
    #[arg(long, env = "STUB_PROBLEMS", value_delimiter = ',')]
    pub problems: Vec<String>,
    /// `false` makes the plant check fail, to exercise `no_plant_found`.
    #[arg(
        long,
        env = "STUB_PLANT_PRESENT",
        action = ArgAction::Set,
        value_parser = BoolishValueParser::new(),
        default_value_t = true
    )]
    pub plant_present: bool,
    /// Pause inside each stage, in milliseconds.
    #[arg(long, env = "STUB_STAGE_DELAY_MS", default_value_t = 150)]
    pub stage_delay_ms: u64,
}

/// Why `--stub-vision` was refused ([worker 15.1.1]); the message names the rule.
///
/// [worker 15.1.1]: ../../../docs/backend/worker.md#1511-turning-it-on
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StubRefused {
    /// Rule 1: a canned `VlmResult` would put invented findings into a diagnosis.
    #[error("--stub-vision is valid only with --caps perception, not {0:?}")]
    WrongCaps(Caps),
    /// Rule 2.
    #[error("--stub-vision in a release build needs PDM_ALLOW_STUB_VISION=1")]
    ReleaseWithoutAllow,
    /// Rule 3: the API queues no `perception` job without them.
    #[error(
        "--stub-vision needs {0}: the API queues a job only for the label versions a worker reports"
    )]
    MissingVersion(&'static str),
    /// A `STUB_SPECIES` or `STUB_PROBLEMS` entry that is not a slug.
    #[error("{variable} holds {value:?}, which is not a slug")]
    BadSlug {
        /// The variable.
        variable: &'static str,
        /// The entry.
        value: String,
    },
}

/// Rules 1 to 3 of `--stub-vision`, a pure function so a test can pass `release` instead of
/// changing its own build. Returns `Ok` when the flag is off.
pub fn check_stub_allowed(cli: &Cli, release: bool) -> Result<(), StubRefused> {
    let stub = &cli.stub;
    if !stub.stub_vision {
        return Ok(());
    }
    if cli.caps != Caps::Perception {
        return Err(StubRefused::WrongCaps(cli.caps));
    }
    if release && !stub.allow_stub_vision {
        return Err(StubRefused::ReleaseWithoutAllow);
    }
    if stub.label_set_version.is_none() {
        return Err(StubRefused::MissingVersion("STUB_LABEL_SET_VERSION"));
    }
    if stub.classifier_labels_version.is_none() {
        return Err(StubRefused::MissingVersion(
            "STUB_CLASSIFIER_LABELS_VERSION",
        ));
    }
    Ok(())
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

impl Timings {
    /// The contract's values with the given drain.
    pub const fn new(drain: Duration) -> Self {
        Self {
            heartbeat_every: domain::rules::jobs::HEARTBEAT_INTERVAL,
            heartbeat_timeout: Duration::from_secs(5),
            claim_wait_s: domain::jobs::ClaimRequest::DEFAULT_WAIT_S,
            stop_before_max_runtime: Duration::from_secs(15),
            drain,
        }
    }
}

impl Cli {
    /// The `slots` for this capability.
    pub const fn slots(&self) -> u32 {
        match self.caps {
            Caps::Perception => 1,
            Caps::Vlm => self.vlm.slots,
            Caps::Fallback => self.fallback.slots,
        }
    }
}
