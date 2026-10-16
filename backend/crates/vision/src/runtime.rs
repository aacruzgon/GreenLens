//! The ONNX Runtime environment, the execution provider chosen at start (`ORT_EP`), and session
//! building with the settings of [vision 5] (CUDA arena, cuDNN search, memory limits).
//!
//! One environment per process: `ort` rc.14 refuses a second `build()`, and `Session::builder`
//! takes the environment by reference. Every session keeps a clone of it alive.
//!
//! [vision 5]: ../../../docs/vision/README.md#5-runtimes-and-versions

use std::{fmt, path::Path, str::FromStr};

use ort::{
    environment::{Environment, GlobalThreadPoolOptions},
    ep::ExecutionProviderDispatch,
    session::{Session, builder::GraphOptimizationLevel},
};

use crate::VisionError;

pub mod pool;

pub use pool::{Pool, Pooled, SessionPool};

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

impl OrtEp {
    /// The `ort_ep` value of `model_versions` ([contracts 5]).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cuda => "cuda",
            Self::MiGraphX => "migraphx",
            Self::WebGpu => "webgpu",
            Self::Cpu => "cpu",
        }
    }

    /// Whether this build can register it.
    pub const fn is_compiled(self) -> bool {
        match self {
            Self::Cuda => cfg!(feature = "cuda"),
            Self::MiGraphX => cfg!(feature = "migraphx"),
            Self::WebGpu => cfg!(feature = "webgpu"),
            Self::Cpu => true,
        }
    }
}

impl fmt::Display for OrtEp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An `ORT_EP` value that names no provider.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("ORT_EP must be cuda, migraphx, webgpu or cpu, not {0:?}")]
pub struct UnknownEp(pub String);

impl FromStr for OrtEp {
    type Err = UnknownEp;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "cuda" => Ok(Self::Cuda),
            "migraphx" => Ok(Self::MiGraphX),
            "webgpu" => Ok(Self::WebGpu),
            "cpu" => Ok(Self::Cpu),
            _ => Err(UnknownEp(s.to_owned())),
        }
    }
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

impl RuntimeConfig {
    /// `api`: the CPU, two shared intra-op threads.
    pub fn cpu(name: &'static str) -> Self {
        Self {
            ep: OrtEp::Cpu,
            device_id: 0,
            dylib_path: None,
            intra_threads: 2,
            name,
        }
    }
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

impl fmt::Debug for OrtRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OrtRuntime")
            .field("ep", &self.ep)
            .field("device_id", &self.device_id)
            .finish_non_exhaustive()
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ort_ep_parses_and_names_itself() {
        assert_eq!("CUDA".parse::<OrtEp>().unwrap(), OrtEp::Cuda);
        assert_eq!(OrtEp::MiGraphX.as_str(), "migraphx");
        assert!("rocm".parse::<OrtEp>().is_err());
        assert!(OrtEp::Cpu.is_compiled());
    }
}
