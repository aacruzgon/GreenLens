# ONNX Runtime on the perception worker: ort 2.0.0-rc.14

Part of [rust-workers](SKILL.md). Versions: ort 2.0.0-rc.14 with default features off and `std`, `ndarray`, `tracing`, `load-dynamic`, `api-29`, plus the execution providers `cuda`, `migraphx` and `webgpu` as features; ndarray 0.17.2 ([versions.md](../rust/versions.md)). One binary opens Microsoft's ONNX Runtime 1.31 on NVIDIA and the CPU, or AMD's 1.29 MIGraphX build, from `ORT_DYLIB_PATH` at start. What runs where is fixed by [architecture 10](../../../docs/architecture/README.md#10-models-and-gpu-placement) and [backend worker 11](../../../docs/backend/worker.md#11-multi-vendor-gpus); batching by [contracts 5, results](../../../docs/architecture/contracts.md#results). The `vision` crate's runtime is the design this file follows ([backend vision 4](../../../docs/backend/vision.md#4-onnx-runtime-environment-execution-providers-session-pool), [6](../../../docs/backend/vision.md#6-perception)); where they differ, vision wins.

**ort is a release candidate, and its API changes between candidates.** In rc.14, `Session::builder` takes an `&Environment` returned by `ort::init()...build()`; code written for earlier candidates may not compile. Read the [rc.14 API reference](https://docs.rs/ort/2.0.0-rc.14/ort/) and the source in `~/.cargo/registry/src/*/ort-2.0.0-rc.14/`, not older examples. With `load-dynamic` nothing is linked at build time, so every block here builds, links and runs its tests on any machine, macOS included; none of them builds a session or calls into ONNX Runtime, which needs the library at run time ([versions.md, notes](../rust/versions.md#notes-and-things-not-confirmed)).

## Contents

1. [Environment and execution providers](#1-environment-and-execution-providers)
2. [CUDA on the rig](#2-cuda-on-the-rig)
3. [Sessions and threads](#3-sessions-and-threads)
4. [The inference thread](#4-the-inference-thread)
5. [Tensors and batches](#5-tensors-and-batches)

## 1. Environment and execution providers

- **One environment per process.** `EnvironmentBuilder::build` returns the error "only one environment is allowed per process" once one has been built (`ort-2.0.0-rc.14/src/environment.rs`, `CURRENT_ENV`). Build it once at start; every session keeps it alive (`SharedSessionInner` holds it, `src/session/mod.rs`). [ort::init](https://docs.rs/ort/2.0.0-rc.14/ort/environment/fn.init.html), [EnvironmentBuilder::build](https://docs.rs/ort/2.0.0-rc.14/ort/environment/struct.EnvironmentBuilder.html#method.build)
- **Open the library with `ort::init_from(path)`, at start.** With `load-dynamic`, `init_from` loads the given `libonnxruntime` and returns an error if it can't. Plain `ort::init()` defers the load to the first ONNX Runtime call, which reads `ORT_DYLIB_PATH` (or looks for `libonnxruntime.so` on the loader path) and **panics** if the library doesn't load (`src/lib.rs`, `setup_api`: "Failed to load ONNX Runtime dylib"). So the worker passes its `ORT_DYLIB_PATH` setting to `init_from` and falls back to `init()` only when none is set, as `OrtRuntime::init` does ([backend vision 4](../../../docs/backend/vision.md#4-onnx-runtime-environment-execution-providers-session-pool); [backend worker 4](../../../docs/backend/worker.md#4-configuration)). [ort::init_from](https://docs.rs/ort/2.0.0-rc.14/ort/environment/fn.init_from.html)
- **`api-29` is what lets AMD's 1.29 load.** ort asks the library for the C API version `ORT_API_VERSION`, which is 17 plus one for each `api-N` feature enabled, so 29 here (`ort-sys-2.0.0-rc.14/src/version.rs`), and an ONNX Runtime older than that version returns no API, which ort turns into the panic "Failed to initialize ORT API" (`src/lib.rs`, `setup_api`). ort's default features include `api-31`, which AMD's 1.29 build can't serve; with default features off and `api-29`, Microsoft's 1.31 and AMD's 1.29 both load. Read in the source, not run against either library ([versions.md, notes](../rust/versions.md#notes-and-things-not-confirmed); [backend vision 2](../../../docs/backend/vision.md#2-dependencies)).
- **The execution provider is a cargo feature, chosen at run time by `ORT_EP`** (`cuda`, `migraphx`, `webgpu` or `cpu`). Each GPU provider's builder exists only with its feature: `ep::CUDA` with `cuda`, `ep::MIGraphX` with `migraphx`, `ep::WebGPU` with `webgpu` (`src/ep/mod.rs`, the `#[cfg(feature = …)]` on each module). The CPU needs none: ONNX Runtime's own CPU provider is always last. Asking for a provider the build doesn't have fails start-up ([backend vision 4](../../../docs/backend/vision.md#4-onnx-runtime-environment-execution-providers-session-pool); [backend worker 11](../../../docs/backend/worker.md#11-multi-vendor-gpus)). The rig uses CUDA on the RTX 3070; MIGraphX (AMD's ROCm provider) and WebGPU (Dawn on Vulkan, any vendor) are there should perception move to the RX 9070 XT. [ep::CUDA](https://docs.rs/ort/2.0.0-rc.14/ort/ep/cuda/struct.CUDA.html), [ep::MIGraphX](https://docs.rs/ort/2.0.0-rc.14/ort/ep/migraphx/struct.MIGraphX.html), [ep::WebGPU](https://docs.rs/ort/2.0.0-rc.14/ort/ep/webgpu/struct.WebGPU.html)
- **Execution providers can be registered on the session or on the environment.** `vision` gives each session its list (`SessionBuilder::with_execution_providers`); ones registered on the environment apply to every session, after the session's own. [SessionBuilder::with_execution_providers](https://docs.rs/ort/2.0.0-rc.14/ort/session/builder/struct.SessionBuilder.html#method.with_execution_providers), [EnvironmentBuilder::with_execution_providers](https://docs.rs/ort/2.0.0-rc.14/ort/environment/struct.EnvironmentBuilder.html#method.with_execution_providers), [ort: global EPs](https://ort.pyke.io/perf/execution-providers#global-eps)
- **A provider that fails to register is skipped**: ort logs the error, warns that it "may fall back to CPU" if none registered, and the session runs on the CPU (`src/ep/mod.rs`, `apply_execution_providers`). On the perception worker that would be a silent slowdown, so add `.error_on_failure()`: registration then returns the error and `commit_from_file` fails at start. [ExecutionProviderDispatch::error_on_failure](https://docs.rs/ort/2.0.0-rc.14/ort/ep/struct.ExecutionProviderDispatch.html#method.error_on_failure), [ort: fallback behavior](https://ort.pyke.io/perf/execution-providers#fallback-behavior)
- `ExecutionProvider::is_available()` only says ONNX Runtime was compiled with the provider, not that it will work. [ExecutionProvider::is_available](https://docs.rs/ort/2.0.0-rc.14/ort/ep/trait.ExecutionProvider.html#method.is_available)
- Even with CUDA registered, an operator CUDA doesn't support runs on the CPU. To check that a model runs wholly on the GPU, commit it once with `with_disable_cpu_fallback()`, which fails instead. [SessionBuilder::with_disable_cpu_fallback](https://docs.rs/ort/2.0.0-rc.14/ort/session/builder/struct.SessionBuilder.html#method.with_disable_cpu_fallback), [ort: registering EPs](https://ort.pyke.io/perf/execution-providers#registering-execution-providers)

`ORT_EP` and the provider list, tested without ONNX Runtime: building an `ExecutionProviderDispatch` is plain Rust, and nothing calls the library until a session registers it (`src/ep/mod.rs`, `ExecutionProviderDispatch::new`). The checker builds `ort` with all three provider features, so every arm compiles here; in the `vision` crate each GPU arm carries its feature's `#[cfg]`, with a catch-all arm that fails for a provider the build lacks ([backend vision 4](../../../docs/backend/vision.md#4-onnx-runtime-environment-execution-providers-session-pool), `OrtRuntime::providers`).

```rust
use ort::ep::{self, ExecutionProviderDispatch};

/// `ORT_EP=cuda|migraphx|webgpu|cpu` (`vision::runtime::OrtEp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrtEp {
    /// NVIDIA, CUDA 13 (the RTX 3070 perception worker).
    Cuda,
    /// AMD, `MIGraphX` on `ROCm`: needs AMD's ONNX Runtime 1.29 and the `api-29` build.
    MiGraphX,
    /// `WebGPU` through Dawn's Vulkan backend: any vendor, no `ROCm`.
    WebGpu,
    /// ONNX Runtime's own CPU provider.
    Cpu,
}

impl std::str::FromStr for OrtEp {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "cuda" => Ok(Self::Cuda),
            "migraphx" => Ok(Self::MiGraphX),
            "webgpu" => Ok(Self::WebGpu),
            "cpu" => Ok(Self::Cpu),
            other => Err(format!(
                "ORT_EP {other:?} is not cuda, migraphx, webgpu or cpu"
            )),
        }
    }
}

/// The providers every session registers, each with `error_on_failure` so a broken driver
/// fails start-up instead of running on the CPU. The CPU needs none.
pub fn providers(which: OrtEp, device_id: i32) -> Vec<ExecutionProviderDispatch> {
    match which {
        OrtEp::Cpu => Vec::new(),
        OrtEp::Cuda => vec![
            ep::CUDA::default()
                .with_device_id(device_id)
                .build()
                .error_on_failure(),
        ],
        OrtEp::MiGraphX => vec![
            ep::MIGraphX::default()
                .with_device_id(device_id)
                .with_fp16(true)
                .build()
                .error_on_failure(),
        ],
        OrtEp::WebGpu => vec![
            ep::WebGPU::default()
                .with_device_id(device_id)
                .with_dawn_backend_type(ep::webgpu::DawnBackendType::Vulkan)
                .build()
                .error_on_failure(),
        ],
    }
}

#[test]
fn ort_ep_picks_one_provider_and_the_cpu_needs_none() {
    let names = |s: &str| -> Vec<&'static str> {
        let which: OrtEp = s.parse().unwrap();
        providers(which, 0)
            .iter()
            .map(ExecutionProviderDispatch::name)
            .collect()
    };
    assert_eq!(names("cuda"), ["CUDAExecutionProvider"]);
    assert_eq!(names("migraphx"), ["MIGraphXExecutionProvider"]);
    assert_eq!(names("webgpu"), ["WebGpuExecutionProvider"]);
    assert_eq!(names("cpu"), [] as [&str; 0]);
    assert!("rocm".parse::<OrtEp>().is_err());
}
```

## 2. CUDA on the rig

- **The image brings its vendor's ONNX Runtime.** The perception worker's image is built on `nvidia/cuda:13.0.3-cudnn-runtime-ubuntu24.04` (the CUDA 13 runtime with cuDNN 9) with Microsoft's ONNX Runtime 1.31 libraries next to `ORT_DYLIB_PATH=/opt/onnxruntime/lib/libonnxruntime.so.1.31.0`, and the host has NVIDIA driver 580.65.06 or later ([backend worker 11](../../../docs/backend/worker.md#11-multi-vendor-gpus), [12](../../../docs/backend/worker.md#12-deployment)). The compose service sets `ORT_EP=cuda`. Architecture 10 asks for "CUDA 12.8+ builds" for the 5070 Ti; CUDA 13 meets that.
- **The CUDA provider is a separate shared library** that ONNX Runtime opens when the provider registers, and it needs the CUDA libraries ort lists in `CUDA_DYLIBS` (`libcudart.so.13`, `libcublasLt.so.13`, `libcublas.so.13`, `libnvrtc.so.13`, `libcurand.so.10`, `libcufft.so.12`; `src/ep/cuda.rs`) and cuDNN 9 on the loader path. `ep::cuda::preload_dylibs` (on with `load-dynamic`) can load them from given directories first. [ort: dynamically-linked EP requirements](https://ort.pyke.io/perf/execution-providers#dynamically-linked-ep-requirements), [ep::cuda::preload_dylibs](https://docs.rs/ort/2.0.0-rc.14/ort/ep/cuda/fn.preload_dylibs.html)
- **One NVIDIA driver serves both NVIDIA cards** (the 3070 for perception, the 5070 Ti for the vLLM sidecar), so it must suit both CUDA 13 images. [NVIDIA: CUDA Toolkit release notes, tables 2 and 3](https://docs.nvidia.com/cuda/cuda-toolkit-release-notes/index.html#cuda-driver)
- **AMD.** Should perception move to the RX 9070 XT, the same worker builds with `--features migraphx` and opens AMD's ONNX Runtime 1.29 from `rocm/onnxruntime` (which `api-29` accepts), or with `--features webgpu` for Dawn on Vulkan without ROCm; `ORT_EP` picks it at start. MIGraphX compiles a program per input shape, so batches would be padded to fixed sizes ([backend worker 11](../../../docs/backend/worker.md#11-multi-vendor-gpus)). Not used now: perception stays on the 3070.
- **The alternative, `download-binaries`, links pyke's prebuilt 1.31 instead** (the `vision` crate keeps it as a feature, [backend vision 2](../../../docs/backend/vision.md#2-dependencies)). It holds one provider family per build: the build script picks the dist whose features match exactly, and pyke publishes no `x86_64-unknown-linux-gnu` dist with both `cuda13` and `migraphx` (`ort-sys-2.0.0-rc.14/build/download/dist.tsv`), so a build with both features gets the `link_error_bad_dist_features` cfg and fails to link (`build/main.rs`). Its CUDA builds need CUDA 13.2 or newer and cuDNN 9.23 or newer ([ort: CUDA](https://ort.pyke.io/perf/execution-providers#cuda)); `ORT_CUDA_VERSION=12` finds no binaries, since only `cuda13` is published (`build/download/resolve.rs`). Its `copy-dylibs` feature leaves **symlinks** in `target/<profile>/` into ort's download cache, not copies (`build/dynamic_link.rs`), so a Docker `COPY` of `target/release/` carries dangling links: copy with `cp -L`. None of this applies to the `load-dynamic` design.
- pyke's prebuilt binaries need AVX2; `ort::init()` prints a warning on an x86-64 CPU without it (`src/environment.rs`).
- Which CUDA 13 minor Microsoft's 1.31 build needs, and whether all of this runs on the rig's cards, is the day-1 check in [architecture 10](../../../docs/architecture/README.md#10-models-and-gpu-placement). *Not confirmed here.*

## 3. Sessions and threads

- **Load every model once, at start**, on the inference thread, and only claim jobs once loading succeeded. Never per job: a load inside a job would count against its `max_runtime` of 2 minutes ([contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)), and `commit_from_file` reads and optimises the whole graph each time ([SessionBuilder::commit_from_file](https://docs.rs/ort/2.0.0-rc.14/ort/session/builder/struct.SessionBuilder.html#method.commit_from_file)). The worker's start-up check also fails on a model file whose hash differs from the manifest ([backend worker 4](../../../docs/backend/worker.md#4-configuration)).
- **`run` and `run_with_options` take `&mut self` in rc.14**: some ONNX Runtime internals aren't thread-safe, and ort recommends one session per thread or batching. [Session: why does run take &mut self](https://docs.rs/ort/2.0.0-rc.14/ort/session/struct.Session.html#why-does-run-take-mut-self), [Session::run](https://docs.rs/ort/2.0.0-rc.14/ort/session/struct.Session.html#method.run)
- **Two owners, by process** ([backend vision 4](../../../docs/backend/vision.md#4-onnx-runtime-environment-execution-providers-session-pool), [6](../../../docs/backend/vision.md#6-perception)):

  | Process | Models | Owner |
  |---|---|---|
  | The perception worker | Its GPU models (detector, SAM encoder and decoder, BioCLIP) | **One inference thread** owns every session (section 4): the worker holds one job (`slots = 1`) and runs the models one after another, so one owner fits and no lock is needed |
  | `api` | The CPU models (face detector, text embedder) | **A `SessionPool`** of N sessions per model, checked out from blocking threads, because `api` serves several uploads and chat queries at once. `Session` is `Send + Sync`, and the pool's guard gives the session back in `Drop` |

- **`run` blocks its calling thread, which also takes part in the computation.** By default each session has its own intra-op pool with one thread per physical core, and idle pool threads spin. Never call it on a tokio worker thread: use the inference thread of [rust-async, blocking 4](../rust-async/blocking.md#4-the-perception-worker-and-onnx-runtimes-threads), or a blocking thread for `api`'s pool. [ONNX Runtime: intra-op threads](https://onnxruntime.ai/docs/performance/tune-performance/threading.html#set-number-of-intra-op-threads), [spinning](https://onnxruntime.ai/docs/performance/tune-performance/threading.html#thread-spinning-behavior), [Alice Ryhl: what is blocking](https://ryhl.io/blog/async-what-is-blocking/)
- **One global intra-op pool for all the sessions**, so four or five models don't each start a core-sized pool. With a global pool on the environment, every session uses it unless built with `with_independent_thread_pool()` (`src/session/builder/impl_commit.rs`, `DisablePerSessionThreads`). Turn spinning off: ort's own advice when inference isn't continuous. `ORT_INTRA_THREADS` (default 2) is a *starting value* for a GPU-bound worker; measure on day 2 ([backend worker 4](../../../docs/backend/worker.md#4-configuration)). [EnvironmentBuilder::with_global_thread_pool](https://docs.rs/ort/2.0.0-rc.14/ort/environment/struct.EnvironmentBuilder.html#method.with_global_thread_pool), [GlobalThreadPoolOptions::with_spin_control](https://docs.rs/ort/2.0.0-rc.14/ort/environment/struct.GlobalThreadPoolOptions.html#method.with_spin_control), [ONNX Runtime: thread management](https://onnxruntime.ai/docs/performance/tune-performance/threading.html)
- **Don't set inter-op threads**: that pool exists only with parallel execution, which is off by default. [SessionBuilder::with_inter_threads](https://docs.rs/ort/2.0.0-rc.14/ort/session/builder/struct.SessionBuilder.html#method.with_inter_threads), [ONNX Runtime: inter-op threads](https://onnxruntime.ai/docs/performance/tune-performance/threading.html#set-number-of-inter-op-threads)
- **Graph optimization: `GraphOptimizationLevel::All`**, which is ONNX Runtime's default; ort's `SessionBuilder::new` doesn't change it. Stating it keeps it from being lowered by accident. [GraphOptimizationLevel](https://docs.rs/ort/2.0.0-rc.14/ort/session/builder/enum.GraphOptimizationLevel.html)
- **`?` the builder chain into `ort::Result` first.** Each builder step returns `ort::Error<SessionBuilder>`, which carries the builder back and isn't `Send + Sync`; `?` into `ort::Result` converts it to the plain `ort::Error`, which then converts into the crate's own error or `anyhow` ([backend vision 4](../../../docs/backend/vision.md#4-onnx-runtime-environment-execution-providers-session-pool), `commit`; the block in section 4 does the same).

## 4. The inference thread

The thread owns the sessions; jobs send it owned tensors over a bounded channel and wait on a `oneshot`. This is the design of [rust-async, blocking 4](../rust-async/blocking.md#4-the-perception-worker-and-onnx-runtimes-threads) and of `vision::perception::Inference` ([backend vision 6, the inference thread](../../../docs/backend/vision.md#the-inference-thread)), with the ort calls filled in.

- **Cancellation is by drop.** The `VisionModel` port takes no token: when the job is cancelled (`cancel_job_ids`, `lease_lost`, the drain running out) or hits its soft deadline, the worker drops the model's future ([backend domain 9, models](../../../docs/backend/domain.md#models); [backend worker 7](../../../docs/backend/worker.md#7-cancellation)). A dropped future alone would not stop anything: the thread runs on and its answer goes to a closed `oneshot` ([oneshot::Sender::send](https://docs.rs/tokio/1.53.3/tokio/sync/oneshot/struct.Sender.html#method.send)).
- **So the wait holds a `TerminateOnDrop` guard.** `RunOptions::terminate` sets a flag that a run under those options checks, from any thread; the run then ends with an error. Each request carries an `Arc<RunOptions>` and the async side keeps a clone in the guard: dropped before the answer arrives, the guard calls `terminate()`; once the answer arrived, it is disarmed. [RunOptions::terminate](https://docs.rs/ort/2.0.0-rc.14/ort/session/struct.RunOptions.html#method.terminate), [Session::run_with_options](https://docs.rs/ort/2.0.0-rc.14/ort/session/struct.Session.html#method.run_with_options). How quickly a CUDA run notices the flag is *unverified*: ort's own example is `no_run` for that reason.
- The job task returns, and its slot is freed, as soon as the future is dropped; the thread may still be finishing the terminated run, so the next job's request waits in the bounded channel (`mpsc::channel(1)`) until the thread takes it ([backend vision 6](../../../docs/backend/vision.md#the-inference-thread)).
- The tested block models `RunOptions` behind a `Terminate` trait and the inference thread with a task, because a real run needs ONNX Runtime. The ort block after it implements `Terminate` for `RunOptions` and carries an exact copy of the `Terminate` region (`Terminate`, `TerminateOnDrop`, `InferenceGone`, `await_run`), which `check_examples.py` compares with the tested one.
- `RunOptions<NoSelectedOutputs>` (the default) is `Send` and `Sync`, so an `Arc` of it can go to the inference thread (`ort-2.0.0-rc.14/src/session/run_options.rs`). [RunOptions](https://docs.rs/ort/2.0.0-rc.14/ort/session/struct.RunOptions.html)
- Outputs borrow the session (`SessionOutputs<'_>`), so copy what you need before the next run. [SessionOutputs](https://docs.rs/ort/2.0.0-rc.14/ort/session/struct.SessionOutputs.html)

```rust
// canonical: rust-workers/ort.md Terminate
/// Stops a run in progress from another thread: `ort::session::RunOptions` in production.
pub trait Terminate: Send + Sync + 'static {
    fn terminate(&self);
}

/// Terminates the run when dropped, unless the run already answered.
struct TerminateOnDrop<T: Terminate>(Option<std::sync::Arc<T>>);

impl<T: Terminate> TerminateOnDrop<T> {
    fn disarm(&mut self) {
        self.0 = None;
    }
}

impl<T: Terminate> Drop for TerminateOnDrop<T> {
    fn drop(&mut self) {
        if let Some(run) = self.0.take() {
            run.terminate();
        }
    }
}

/// The inference thread stopped before it answered (`vision::VisionError::InferenceGone`).
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("the inference thread has stopped")]
pub struct InferenceGone;

/// Waits for the inference thread's answer to one request. Cancellation is by drop: dropping
/// this future before the answer arrives terminates the run inside ONNX Runtime.
pub async fn await_run<T: Terminate, R>(
    run: std::sync::Arc<T>,
    reply: tokio::sync::oneshot::Receiver<R>,
) -> Result<R, InferenceGone> {
    let mut guard = TerminateOnDrop(Some(run));
    let answer = reply.await;
    guard.disarm();
    answer.map_err(|_| InferenceGone)
}
// end: Terminate

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use tokio::sync::oneshot;
    use tokio_util::sync::CancellationToken;

    use super::*;

    /// A run that never finishes by itself: it answers once terminated, like a
    /// `run_with_options` that sees its terminate flag.
    #[derive(Default)]
    struct FakeRun {
        terminated: AtomicU32,
        flag: CancellationToken,
    }

    impl Terminate for FakeRun {
        fn terminate(&self) {
            self.terminated.fetch_add(1, Ordering::SeqCst);
            self.flag.cancel();
        }
    }

    /// Stands in for the inference thread running one request.
    fn start(run: &FakeRun) -> oneshot::Receiver<Result<Vec<f32>, &'static str>> {
        let (answer, reply) = oneshot::channel();
        let flag = run.flag.clone();
        tokio::spawn(async move {
            flag.cancelled().await;
            // Err only means nobody waits for the answer any more.
            let _ = answer.send(Err("terminated"));
        });
        reply
    }

    #[tokio::test(start_paused = true)]
    async fn a_cancelled_job_drops_the_wait_and_terminates_the_run() {
        let run = Arc::new(FakeRun::default());
        let reply = start(&run);
        let cancel = CancellationToken::new();
        let canceller = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            canceller.cancel(); // `cancel_job_ids` named the job
        });
        // What the worker's job task does: select on the token, drop the model's future.
        tokio::select! {
            () = cancel.cancelled() => {}
            _ = await_run(run.clone(), reply) => panic!("the run never finishes by itself"),
        }
        assert_eq!(run.terminated.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn the_soft_deadline_drops_the_wait_and_terminates_the_run() {
        let run = Arc::new(FakeRun::default());
        let reply = start(&run);
        let waited = tokio::time::timeout(
            Duration::from_secs(1), // the job's soft deadline
            await_run(run.clone(), reply),
        )
        .await;
        assert!(waited.is_err(), "the deadline dropped the wait");
        assert_eq!(run.terminated.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_finished_run_is_not_terminated() {
        let run = Arc::new(FakeRun::default());
        let (answer, reply) = oneshot::channel::<Result<Vec<f32>, &'static str>>();
        answer.send(Ok(vec![0.5])).expect("the receiver is alive");
        let out = await_run(run.clone(), reply).await;
        assert_eq!(out, Ok(Ok(vec![0.5])));
        assert_eq!(run.terminated.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_thread_that_stopped_is_inference_gone() {
        let run = Arc::new(FakeRun::default());
        let (answer, reply) = oneshot::channel::<Vec<f32>>();
        drop(answer); // the thread ended without answering
        assert_eq!(await_run(run.clone(), reply).await, Err(InferenceGone));
        assert_eq!(run.terminated.load(Ordering::SeqCst), 0);
    }
}
```

The same wait with ort. It builds and links; `main` is never run, because it would open ONNX Runtime and build a session:

```rust
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use ndarray::Array4;
use ort::environment::{EnvironmentBuilder, GlobalThreadPoolOptions};
use ort::ep;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::{RunOptions, Session};
use ort::value::TensorRef;
use tokio::sync::{mpsc, oneshot};

/// Every session, loaded once at start. Each `Session` keeps the `Environment` alive.
struct Models {
    disease: Session,
}

/// `ORT_DYLIB_PATH` when set: open that library now, so a missing one is an error here
/// instead of a panic at the first ONNX Runtime call.
fn open(dylib: Option<&Path>) -> anyhow::Result<EnvironmentBuilder> {
    Ok(match dylib {
        Some(path) => ort::init_from(path)?,
        None => ort::init(),
    })
}

fn load_models(dir: &Path, dylib: Option<&Path>, cpu_threads: usize) -> anyhow::Result<Models> {
    // One intra-op pool shared by every session, instead of one pool per session.
    let pool = GlobalThreadPoolOptions::default()
        .with_intra_threads(cpu_threads)?
        .with_spin_control(false)?;
    let env = open(dylib)? // a second `build()` in the same process is an error
        .with_name("perception-worker")
        .with_global_thread_pool(pool)
        .build()?;
    // `ORT_EP=cuda`: error_on_failure, so no silent fallback to the CPU (section 1).
    let cuda = ep::CUDA::default()
        .with_device_id(0)
        .build()
        .error_on_failure();
    let disease = commit(&env, &dir.join("disease_classifier.onnx"), cuda)?;
    Ok(Models { disease })
}

/// `ort::Error<SessionBuilder>` isn't `Send + Sync`: `?` into `ort::Result` first (section 3).
fn commit(
    env: &ort::environment::Environment,
    model: &Path,
    provider: ep::ExecutionProviderDispatch,
) -> ort::Result<Session> {
    Session::builder(env)?
        .with_execution_providers([provider])?
        .with_optimization_level(GraphOptimizationLevel::All)? // ONNX Runtime's default, stated
        .commit_from_file(model)
}

impl Models {
    /// `[n, 3, h, w]` in, `n * classes` logits out. `run_with_options` takes `&mut self`.
    fn classify(&mut self, batch: &Array4<f32>, options: &RunOptions) -> ort::Result<Vec<f32>> {
        let input = TensorRef::from_array_view(batch)?; // needs a contiguous array
        let outputs = self
            .disease
            .run_with_options(ort::inputs![input], options)?;
        let (_shape, logits) = outputs[0].try_extract_tensor::<f32>()?;
        Ok(logits.to_vec()) // copy out: `outputs` borrows the session
    }
}

impl Terminate for RunOptions {
    fn terminate(&self) {
        // The inherent `RunOptions::terminate`, which sets ONNX Runtime's flag.
        if let Err(e) = RunOptions::terminate(self) {
            tracing::warn!(error = %e, "could not terminate the run");
        }
    }
}

struct Request {
    batch: Array4<f32>,
    options: Arc<RunOptions>, // the async side keeps a clone to terminate the run
    reply: oneshot::Sender<ort::Result<Vec<f32>>>,
}

/// The inference thread (rust-async blocking.md §4) owns the sessions; jobs send it batches.
#[derive(Clone)]
pub struct Inference {
    // Fields drop in order: the sender first, so the channel is closed when `Joined` waits.
    requests: mpsc::Sender<Request>,
    _joined: Arc<Joined>,
}

/// Joins the thread when the last clone drops. `exit` unloads the CUDA libraries under a
/// detached thread that is still releasing sessions (exit 139 on the 3070, 2026-10-20).
struct Joined(Option<thread::JoinHandle<()>>);

impl Drop for Joined {
    fn drop(&mut self) {
        if let Some(thread) = self.0.take()
            && thread.join().is_err()
        {
            tracing::error!("the inference thread panicked");
        }
    }
}

impl Inference {
    /// Loads the models on the new thread and waits until they are ready, so start-up fails
    /// loudly (no library, no CUDA, a missing file) before the worker claims anything.
    pub async fn start(
        dir: PathBuf,
        dylib: Option<PathBuf>,
        cpu_threads: usize,
    ) -> anyhow::Result<Self> {
        let (requests, mut inbox) = mpsc::channel::<Request>(1);
        let (ready, loaded) = oneshot::channel();
        let handle = thread::Builder::new()
            .name("inference".to_owned())
            .spawn(move || {
                let mut models = match load_models(&dir, dylib.as_deref(), cpu_threads) {
                    Ok(models) => models,
                    // Err only means `start` stopped waiting: nobody is left to tell.
                    Err(e) => return drop(ready.send(Err(e))),
                };
                if ready.send(Ok(())).is_err() {
                    return;
                }
                while let Some(req) = inbox.blocking_recv() {
                    let logits = models.classify(&req.batch, &req.options);
                    // Err only means the job stopped waiting (its future was dropped).
                    let _ = req.reply.send(logits);
                }
            })?;
        // Built before the wait, so a failed load still joins the thread.
        let inference = Self {
            requests,
            _joined: Arc::new(Joined(Some(handle))),
        };
        loaded.await??;
        Ok(inference)
    }

    /// Runs one batch. Dropping this future terminates the run inside ONNX Runtime instead of
    /// waiting it out (cancellation by drop).
    pub async fn classify(&self, batch: Array4<f32>) -> anyhow::Result<Vec<f32>> {
        let options = Arc::new(RunOptions::new()?);
        let (reply, response) = oneshot::channel();
        let req = Request {
            batch,
            options: Arc::clone(&options),
            reply,
        };
        self.requests.send(req).await?;
        let logits = await_run(options, response).await?;
        Ok(logits?)
    }
}

// copy of: rust-workers/ort.md Terminate
/// Stops a run in progress from another thread: `ort::session::RunOptions` in production.
pub trait Terminate: Send + Sync + 'static {
    fn terminate(&self);
}

/// Terminates the run when dropped, unless the run already answered.
struct TerminateOnDrop<T: Terminate>(Option<std::sync::Arc<T>>);

impl<T: Terminate> TerminateOnDrop<T> {
    fn disarm(&mut self) {
        self.0 = None;
    }
}

impl<T: Terminate> Drop for TerminateOnDrop<T> {
    fn drop(&mut self) {
        if let Some(run) = self.0.take() {
            run.terminate();
        }
    }
}

/// The inference thread stopped before it answered (`vision::VisionError::InferenceGone`).
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("the inference thread has stopped")]
pub struct InferenceGone;

/// Waits for the inference thread's answer to one request. Cancellation is by drop: dropping
/// this future before the answer arrives terminates the run inside ONNX Runtime.
pub async fn await_run<T: Terminate, R>(
    run: std::sync::Arc<T>,
    reply: tokio::sync::oneshot::Receiver<R>,
) -> Result<R, InferenceGone> {
    let mut guard = TerminateOnDrop(Some(run));
    let answer = reply.await;
    guard.disarm();
    answer.map_err(|_| InferenceGone)
}
// end: Terminate

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let dylib = std::env::var_os("ORT_DYLIB_PATH").map(PathBuf::from);
    let inference = Inference::start(PathBuf::from("/models"), dylib, 2).await?;
    let batch = Array4::<f32>::zeros((16, 3, 224, 224));
    let logits = inference.classify(batch).await?;
    tracing::info!(values = logits.len(), "classified");
    Ok(())
}
```

## 5. Tensors and batches

- **Inputs**: `TensorRef::from_array_view(&array)` borrows an ndarray array and fails if it isn't contiguous; `Tensor::from_array(array)` takes ownership and copies to a contiguous layout if needed; both also take `(shape, data)`, with `Vec<T>` or `Box<[T]>` for an owned tensor and `&[T]` for a borrowed one. `ort::inputs![...]` builds the input list. [TensorRef](https://docs.rs/ort/2.0.0-rc.14/ort/value/type.TensorRef.html), [Tensor](https://docs.rs/ort/2.0.0-rc.14/ort/value/type.Tensor.html), [inputs!](https://docs.rs/ort/2.0.0-rc.14/ort/macro.inputs.html)
- **Outputs**: `outputs[0]` or `outputs["name"]`, then `try_extract_tensor::<f32>()` for `(shape, &[f32])` or `try_extract_array::<f32>()` for an ndarray view. [Value::try_extract_tensor](https://docs.rs/ort/2.0.0-rc.14/ort/value/struct.Value.html#method.try_extract_tensor), [Value::try_extract_array](https://docs.rs/ort/2.0.0-rc.14/ort/value/struct.Value.html#method.try_extract_array)
- **ndarray 0.17 to match ort.** ort depends on `ndarray = "0.17"`; a different minor is a different crate and its arrays won't convert ([Cargo: SemVer compatibility](https://doc.rust-lang.org/cargo/reference/resolver.html#semver-compatibility)). In 0.17 methods such as `view` and `is_standard_layout` live on `ArrayRef`, so `Array3::view` as a path no longer resolves; call them as methods. [versions.md](../rust/versions.md#dependencies), [ndarray 0.17 ArrayRef](https://docs.rs/ndarray/0.17.2/ndarray/struct.ArrayRef.html)
- **Batches**: each model runs once per job over all its inputs; the ONNX models have a dynamic batch dimension. A batch larger than `PERCEPTION_MAX_BATCH` (default 16, the job-wide cap) is split, and each model has its own batch cap, never above it: `DETECTOR_MAX_BATCH` 8 and `SAM_ENCODER_MAX_BATCH` 2 are below it, `BIOCLIP_MAX_BATCH` 16 equals it. `SAM_MAX_BOXES` (16) is not a batch cap but the number of box prompts in one SAM decoder call ([backend worker 4](../../../docs/backend/worker.md#4-configuration); [contracts 5, results](../../../docs/architecture/contracts.md#results)). Send a stage's `partial_result` after its last batch. [contracts 5, results](../../../docs/architecture/contracts.md#results)
- **Decode and pre-process off the inference thread.** The perception worker decodes and pre-processes on the blocking pool behind the CPU semaphore and sends only tensors to the inference thread, so the GPU's thread does nothing but run models ([backend vision 6](../../../docs/backend/vision.md#6-perception); [rust-async, blocking 4](../rust-async/blocking.md#4-the-perception-worker-and-onnx-runtimes-threads)).

```rust
use ndarray::{Array3, Array4, ArrayView3, Axis, ShapeError};

/// Leaf crops `[3, h, w]` into batches `[n, 3, h, w]` of at most `max` (the model's cap, never
/// above `PERCEPTION_MAX_BATCH`).
pub fn batches(crops: &[Array3<f32>], max: usize) -> Result<Vec<Array4<f32>>, ShapeError> {
    crops
        .chunks(max.max(1))
        .map(|chunk| {
            let views: Vec<ArrayView3<'_, f32>> = chunk.iter().map(|c| c.view()).collect();
            ndarray::stack(Axis(0), &views)
        })
        .collect()
}

#[test]
fn splits_at_the_max_batch_and_stays_contiguous() {
    let crops = vec![Array3::<f32>::zeros((3, 224, 224)); 33];
    let out = batches(&crops, 16).unwrap();
    let sizes: Vec<usize> = out.iter().map(|b| b.len_of(Axis(0))).collect();
    assert_eq!(sizes, [16, 16, 1]);
    assert!(out.iter().all(ndarray::ArrayBase::is_standard_layout)); // what TensorRef::from_array_view needs
}
```
