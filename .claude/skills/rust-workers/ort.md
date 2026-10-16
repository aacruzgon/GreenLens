# ONNX Runtime on the perception worker: ort 2.0.0-rc.14

Part of [rust-workers](SKILL.md). Versions: ort 2.0.0-rc.14 with the `cuda` feature (a wrapper for ONNX Runtime 1.31), ndarray 0.17.2 ([versions.md](../rust/versions.md)). What runs where is fixed by [architecture 10](../../../docs/architecture/README.md#10-models-and-gpu-placement); batching by [contracts 5, results](../../../docs/architecture/contracts.md#results).

**ort is a release candidate, and its API changes between candidates.** In rc.14, `Session::builder` takes an `&Environment` returned by `ort::init()...build()`; code written for earlier candidates may not compile. Read the [rc.14 API reference](https://docs.rs/ort/2.0.0-rc.14/ort/) and the source in `~/.cargo/registry/src/*/ort-2.0.0-rc.14/`, not older examples. ort can't link on macOS with the `cuda` feature, so the example below is checked and linted but never run, and no test uses ort ([versions.md, notes](../rust/versions.md#notes-and-things-not-confirmed)).

## Contents

1. [Environment and execution providers](#1-environment-and-execution-providers)
2. [CUDA on the rig](#2-cuda-on-the-rig)
3. [Sessions and threads](#3-sessions-and-threads)
4. [The inference thread](#4-the-inference-thread)
5. [Tensors and batches](#5-tensors-and-batches)

## 1. Environment and execution providers

- **One environment per process.** `EnvironmentBuilder::build` returns the error "only one environment is allowed per process" once one has been built (`ort-2.0.0-rc.14/src/environment.rs`, `CURRENT_ENV`). Build it once at start; every session keeps it alive (`SharedSessionInner` holds it, `src/session/mod.rs`). [ort::init](https://docs.rs/ort/2.0.0-rc.14/ort/environment/fn.init.html), [EnvironmentBuilder::build](https://docs.rs/ort/2.0.0-rc.14/ort/environment/struct.EnvironmentBuilder.html#method.build)
- **Execution providers registered on the environment apply to every session**; ones given to a session are registered first. [EnvironmentBuilder::with_execution_providers](https://docs.rs/ort/2.0.0-rc.14/ort/environment/struct.EnvironmentBuilder.html#method.with_execution_providers), [ort: global EPs](https://ort.pyke.io/perf/execution-providers#global-eps)
- **A provider that fails to register is skipped**: ort logs the error, warns that it "may fall back to CPU" if none registered, and the session runs on the CPU (`src/ep/mod.rs`, `apply_execution_providers`). On the perception worker that would be a silent slowdown, so add `.error_on_failure()`: registration then returns the error and `commit_from_file` fails at start. [ExecutionProviderDispatch::error_on_failure](https://docs.rs/ort/2.0.0-rc.14/ort/ep/struct.ExecutionProviderDispatch.html#method.error_on_failure), [ort: fallback behavior](https://ort.pyke.io/perf/execution-providers#fallback-behavior)
- `ExecutionProvider::is_available()` only says ONNX Runtime was compiled with the provider, not that it will work. [ExecutionProvider::is_available](https://docs.rs/ort/2.0.0-rc.14/ort/ep/trait.ExecutionProvider.html#method.is_available)
- Even with CUDA registered, an operator CUDA doesn't support runs on the CPU. To check that a model runs wholly on the GPU, commit it once with `with_disable_cpu_fallback()`, which fails instead. [SessionBuilder::with_disable_cpu_fallback](https://docs.rs/ort/2.0.0-rc.14/ort/session/builder/struct.SessionBuilder.html#method.with_disable_cpu_fallback), [ort: registering EPs](https://ort.pyke.io/perf/execution-providers#registering-execution-providers)

## 2. CUDA on the rig

- **ort's prebuilt CUDA binaries need CUDA 13.2 or newer and cuDNN 9.23 or newer**, and support compute capabilities 7.5, 8.x (the RTX 3070 is 8.6) and 12.x (the RTX 5070 Ti). [ort: CUDA](https://ort.pyke.io/perf/execution-providers#cuda)
- The build script chooses `cuda12` or `cuda13` from `ORT_CUDA_VERSION`, `CUDA_HOME`, `NV_CUDA_CUDART_VERSION` or `nvcc`, and guesses 13 otherwise. Only a `cuda13` build is published for `x86_64-unknown-linux-gnu`, so `ORT_CUDA_VERSION=12` finds no binaries (`ort-sys-2.0.0-rc.14/build/download/resolve.rs` and `dist.tsv`). The rig image therefore needs the CUDA 13 runtime libraries (`libcudart.so.13`, `libcublas.so.13`, …) and cuDNN 9 (`ort-2.0.0-rc.14/src/ep/cuda.rs`, `CUDA_DYLIBS`). Architecture 10 asks for "CUDA 12.8+ builds" for the 5070 Ti; CUDA 13.2 meets that, but CUDA 12.x does not meet ort's requirement.
- **One NVIDIA driver on the rig serves both cards** (the 3070 for perception, the 5070 Ti for the vLLM sidecar), so it must support CUDA 13.2: the R595 driver branch, or R580 or later under CUDA minor-version compatibility. [NVIDIA: CUDA Toolkit release notes, tables 2 and 3](https://docs.nvidia.com/cuda/cuda-toolkit-release-notes/index.html#cuda-driver)
- **The CUDA provider is a separate shared library**, loaded when it registers. The default `copy-dylibs` feature doesn't copy anything on Linux: it creates **symlinks** in `target/<profile>/` (and `deps/`, `examples/`) pointing into ort's download cache outside `target/` (`ort-sys-2.0.0-rc.14/build/dynamic_link.rs`, `copy_dylibs`, `std::os::unix::fs::symlink`; the cache is `dfbin/<target>/…` under `internal::dirs::cache_dir()`, `build/main.rs`). A Docker `COPY` of `target/release/` from the build stage therefore carries dangling links. Copy the libraries with the links followed (`cp -L target/release/libonnxruntime*.so* /out/` in the build stage), keep them next to the binary and set the rpath to `$ORIGIN` (`[profile.release] rpath = true` and `-Wl,-rpath,$ORIGIN`), or set `LD_LIBRARY_PATH`. [ort: dynamically-linked EP requirements](https://ort.pyke.io/perf/execution-providers#dynamically-linked-ep-requirements), [ort: runtime dylib loading](https://ort.pyke.io/setup/linking#runtime-dylib-loading)
- pyke's prebuilt binaries need AVX2; `ort::init()` prints a warning on an x86-64 CPU without it (`src/environment.rs`).
- Whether all of this runs on the rig's cards is the day-1 check in [architecture 10](../../../docs/architecture/README.md#10-models-and-gpu-placement). *Not confirmed here.*

## 3. Sessions and threads

- **Load every model once, at start**, on the inference thread, and only claim jobs once loading succeeded. Never per job: a load inside a job would count against its `max_runtime` of 2 minutes ([contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)), and `commit_from_file` reads and optimises the whole graph each time ([SessionBuilder::commit_from_file](https://docs.rs/ort/2.0.0-rc.14/ort/session/builder/struct.SessionBuilder.html#method.commit_from_file)). *This skill's choice.*
- **`run` and `run_with_options` take `&mut self` in rc.14**: some ONNX Runtime internals aren't thread-safe, and ort recommends one session per thread or batching. The perception worker holds one job (`slots = 1`) and batches inside it, so one thread owning the sessions fits. [Session: why does run take &mut self](https://docs.rs/ort/2.0.0-rc.14/ort/session/struct.Session.html#why-does-run-take-mut-self), [Session::run](https://docs.rs/ort/2.0.0-rc.14/ort/session/struct.Session.html#method.run)
- **`run` blocks its calling thread, which also takes part in the computation.** By default each session has its own intra-op pool with one thread per physical core, and idle pool threads spin. Never call it on a tokio worker thread: use the inference thread of [rust-async, blocking 4](../rust-async/blocking.md#4-the-perception-worker-and-onnx-runtimes-threads). [ONNX Runtime: intra-op threads](https://onnxruntime.ai/docs/performance/tune-performance/threading.html#set-number-of-intra-op-threads), [spinning](https://onnxruntime.ai/docs/performance/tune-performance/threading.html#thread-spinning-behavior), [Alice Ryhl: what is blocking](https://ryhl.io/blog/async-what-is-blocking/)
- **One global intra-op pool for all the sessions**, so four or five models don't each start a core-sized pool. With a global pool on the environment, every session uses it unless built with `with_independent_thread_pool()` (`src/session/builder/impl_commit.rs`, `DisablePerSessionThreads`). Turn spinning off: ort's own advice when inference isn't continuous. Two threads is a *starting value* for a GPU-bound worker; measure on day 2. [EnvironmentBuilder::with_global_thread_pool](https://docs.rs/ort/2.0.0-rc.14/ort/environment/struct.EnvironmentBuilder.html#method.with_global_thread_pool), [GlobalThreadPoolOptions::with_spin_control](https://docs.rs/ort/2.0.0-rc.14/ort/environment/struct.GlobalThreadPoolOptions.html#method.with_spin_control), [ONNX Runtime: thread management](https://onnxruntime.ai/docs/performance/tune-performance/threading.html)
- **Don't set inter-op threads**: that pool exists only with parallel execution, which is off by default. [SessionBuilder::with_inter_threads](https://docs.rs/ort/2.0.0-rc.14/ort/session/builder/struct.SessionBuilder.html#method.with_inter_threads), [ONNX Runtime: inter-op threads](https://onnxruntime.ai/docs/performance/tune-performance/threading.html#set-number-of-inter-op-threads)
- **Graph optimization: `GraphOptimizationLevel::All`**, which is ONNX Runtime's default; ort's `SessionBuilder::new` doesn't change it. Stating it keeps it from being lowered by accident. [GraphOptimizationLevel](https://docs.rs/ort/2.0.0-rc.14/ort/session/builder/enum.GraphOptimizationLevel.html), [ONNX Runtime: thread management](https://onnxruntime.ai/docs/performance/tune-performance/threading.html)

## 4. The inference thread

The thread owns the sessions; jobs send it batches over a bounded channel and wait on a `oneshot`. This is the design of [rust-async, blocking 4](../rust-async/blocking.md#4-the-perception-worker-and-onnx-runtimes-threads), with the ort calls filled in.

- **Terminate a run when the job is cancelled, and when its future is dropped.** `RunOptions::terminate` sets a flag that a run under those options checks, from any thread; the run then ends with an error. Pass an `Arc<RunOptions>` with each request and keep a clone on the async side. [RunOptions::terminate](https://docs.rs/ort/2.0.0-rc.14/ort/session/struct.RunOptions.html#method.terminate), [Session::run_with_options](https://docs.rs/ort/2.0.0-rc.14/ort/session/struct.Session.html#method.run_with_options). How quickly a CUDA run notices the flag is *unverified*: ort's own example is `no_run` for that reason.
- **Two paths reach `terminate()`**, both in `await_run` below. `Runner::run` gets the job's `CancellationToken` ([loop.md](loop.md)): when it fires, `await_run` terminates the run and then waits for the thread's answer, so the job's slot is freed only once the thread is free. And `await_run` holds a `TerminateOnDrop` guard, so a future dropped mid-run (the job's soft deadline, or `run_job` giving up after its cancel grace) terminates the run too. A dropped future alone would not stop anything: the thread runs on and its answer goes to a closed `oneshot` ([oneshot::Sender::send](https://docs.rs/tokio/1.53.3/tokio/sync/oneshot/struct.Sender.html#method.send)).
- Tests can't link ort ([versions.md, notes](../rust/versions.md#notes-and-things-not-confirmed)), so the tested block models `RunOptions` behind a `Terminate` trait and the inference thread with a task. The ort block after it implements `Terminate` for `RunOptions` and carries an exact copy of the `Terminate` region (`Terminate`, `TerminateOnDrop`, `RunStopped`, `await_run`), which `check_examples.py` compares with the tested one.
- `RunOptions<NoSelectedOutputs>` (the default) is `Send` and `Sync`, so an `Arc` of it can go to the inference thread (`ort-2.0.0-rc.14/src/session/run_options.rs`). [RunOptions](https://docs.rs/ort/2.0.0-rc.14/ort/session/struct.RunOptions.html)
- Outputs borrow the session (`SessionOutputs<'_>`), so copy what you need before the next run. [SessionOutputs](https://docs.rs/ort/2.0.0-rc.14/ort/session/struct.SessionOutputs.html)

```rust
use std::sync::Arc;

use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

// canonical: rust-workers/ort.md Terminate
/// Stops a run in progress from another thread: `ort::session::RunOptions` in production.
pub trait Terminate: Send + Sync + 'static {
    fn terminate(&self);
}

/// Terminates the run when dropped, unless the run already answered.
struct TerminateOnDrop<T: Terminate>(Option<std::sync::Arc<T>>);

impl<T: Terminate> TerminateOnDrop<T> {
    fn terminate(&mut self) {
        if let Some(run) = self.0.take() {
            run.terminate();
        }
    }

    fn disarm(&mut self) {
        self.0 = None;
    }
}

impl<T: Terminate> Drop for TerminateOnDrop<T> {
    fn drop(&mut self) {
        self.terminate();
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum RunStopped {
    #[error("the job was cancelled")]
    Cancelled,
    #[error("the inference thread is gone")]
    ThreadGone,
}

/// Waits for the inference thread's answer to one request. A cancel of `cancel` terminates
/// the run and waits until the thread answers; dropping this future terminates it too.
pub async fn await_run<T: Terminate, R>(
    run: std::sync::Arc<T>,
    mut reply: tokio::sync::oneshot::Receiver<R>,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<R, RunStopped> {
    let mut guard = TerminateOnDrop(Some(run));
    tokio::select! {
        biased;
        answer = &mut reply => {
            guard.disarm();
            answer.map_err(|_| RunStopped::ThreadGone)
        }
        () = cancel.cancelled() => {
            guard.terminate();
            // Err only means the thread is gone, which frees it just as well.
            let _ = reply.await;
            Err(RunStopped::Cancelled)
        }
    }
}
// end: Terminate

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

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
    async fn a_cancelled_token_terminates_the_run() {
        let run = Arc::new(FakeRun::default());
        let reply = start(&run);
        let cancel = CancellationToken::new();
        let canceller = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            canceller.cancel(); // `cancel_job_ids` named the job
        });
        let out = await_run(run.clone(), reply, &cancel).await;
        assert_eq!(out, Err(RunStopped::Cancelled));
        assert_eq!(run.terminated.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_dropped_future_terminates_the_run() {
        let run = Arc::new(FakeRun::default());
        let reply = start(&run);
        let never = CancellationToken::new();
        let waited = tokio::time::timeout(
            Duration::from_secs(1), // the job's soft deadline
            await_run(run.clone(), reply, &never),
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
        let out = await_run(run.clone(), reply, &CancellationToken::new()).await;
        assert_eq!(out, Ok(Ok(vec![0.5])));
        assert_eq!(run.terminated.load(Ordering::SeqCst), 0);
    }
}
```

The same wait with ort (checked and linted, never run):

```rust
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use ndarray::Array4;
use ort::environment::GlobalThreadPoolOptions;
use ort::ep;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::{RunOptions, Session};
use ort::value::TensorRef;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

/// Every session, loaded once at start. Each `Session` keeps the `Environment` alive.
struct Models {
    disease: Session,
}

fn load_models(dir: &Path, cpu_threads: usize) -> ort::Result<Models> {
    // One intra-op pool shared by every session, instead of one pool per session.
    let pool = GlobalThreadPoolOptions::default()
        .with_intra_threads(cpu_threads)?
        .with_spin_control(false)?;
    let env = ort::init() // a second `build()` in the same process is an error
        .with_name("perception-worker")
        .with_global_thread_pool(pool)
        // error_on_failure: no silent fallback to the CPU when CUDA can't register
        .with_execution_providers([ep::CUDA::default()
            .with_device_id(0)
            .build()
            .error_on_failure()])
        .build()?;
    let disease = Session::builder(&env)?
        .with_optimization_level(GraphOptimizationLevel::All)? // ONNX Runtime's default, stated
        .commit_from_file(dir.join("disease_classifier.onnx"))?;
    Ok(Models { disease })
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
    requests: mpsc::Sender<Request>,
}

impl Inference {
    /// Loads the models on the new thread and waits until they are ready, so start-up fails
    /// loudly (no CUDA, a missing file) before the worker claims anything.
    pub async fn start(dir: PathBuf, cpu_threads: usize) -> anyhow::Result<Self> {
        let (requests, mut inbox) = mpsc::channel::<Request>(1);
        let (ready, loaded) = oneshot::channel();
        thread::Builder::new()
            .name("inference".to_owned())
            .spawn(move || {
                let mut models = match load_models(&dir, cpu_threads) {
                    Ok(models) => models,
                    // Err only means `start` stopped waiting: nobody is left to tell.
                    Err(e) => return drop(ready.send(Err(e))),
                };
                if ready.send(Ok(())).is_err() {
                    return;
                }
                while let Some(req) = inbox.blocking_recv() {
                    let logits = models.classify(&req.batch, &req.options);
                    // Err only means the job stopped waiting (cancelled or dropped).
                    let _ = req.reply.send(logits);
                }
            })?;
        loaded.await??;
        Ok(Self { requests })
    }

    /// `cancel` is the job's token (`Runner::run` gets it). A cancel, or dropping this
    /// future, terminates the run inside ONNX Runtime instead of waiting it out.
    pub async fn classify(
        &self,
        batch: Array4<f32>,
        cancel: &CancellationToken,
    ) -> anyhow::Result<Vec<f32>> {
        let options = Arc::new(RunOptions::new()?);
        let (reply, response) = oneshot::channel();
        let req = Request {
            batch,
            options: Arc::clone(&options),
            reply,
        };
        self.requests.send(req).await?;
        let logits = await_run(options, response, cancel).await?;
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
    fn terminate(&mut self) {
        if let Some(run) = self.0.take() {
            run.terminate();
        }
    }

    fn disarm(&mut self) {
        self.0 = None;
    }
}

impl<T: Terminate> Drop for TerminateOnDrop<T> {
    fn drop(&mut self) {
        self.terminate();
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum RunStopped {
    #[error("the job was cancelled")]
    Cancelled,
    #[error("the inference thread is gone")]
    ThreadGone,
}

/// Waits for the inference thread's answer to one request. A cancel of `cancel` terminates
/// the run and waits until the thread answers; dropping this future terminates it too.
pub async fn await_run<T: Terminate, R>(
    run: std::sync::Arc<T>,
    mut reply: tokio::sync::oneshot::Receiver<R>,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<R, RunStopped> {
    let mut guard = TerminateOnDrop(Some(run));
    tokio::select! {
        biased;
        answer = &mut reply => {
            guard.disarm();
            answer.map_err(|_| RunStopped::ThreadGone)
        }
        () = cancel.cancelled() => {
            guard.terminate();
            // Err only means the thread is gone, which frees it just as well.
            let _ = reply.await;
            Err(RunStopped::Cancelled)
        }
    }
}
// end: Terminate

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let inference = Inference::start(PathBuf::from("/models"), 2).await?;
    let batch = Array4::<f32>::zeros((16, 3, 224, 224));
    let logits = inference.classify(batch, &CancellationToken::new()).await?;
    tracing::info!(values = logits.len(), "classified");
    Ok(())
}
```

## 5. Tensors and batches

- **Inputs**: `TensorRef::from_array_view(&array)` borrows an ndarray array and fails if it isn't contiguous; `Tensor::from_array(array)` takes ownership and copies to a contiguous layout if needed; both also take `(shape, data)`, with `Vec<T>` or `Box<[T]>` for an owned tensor and `&[T]` for a borrowed one. `ort::inputs![...]` builds the input list. [TensorRef](https://docs.rs/ort/2.0.0-rc.14/ort/value/type.TensorRef.html), [Tensor](https://docs.rs/ort/2.0.0-rc.14/ort/value/type.Tensor.html), [inputs!](https://docs.rs/ort/2.0.0-rc.14/ort/macro.inputs.html)
- **Outputs**: `outputs[0]` or `outputs["name"]`, then `try_extract_tensor::<f32>()` for `(shape, &[f32])` or `try_extract_array::<f32>()` for an ndarray view. [Value::try_extract_tensor](https://docs.rs/ort/2.0.0-rc.14/ort/value/struct.Value.html#method.try_extract_tensor), [Value::try_extract_array](https://docs.rs/ort/2.0.0-rc.14/ort/value/struct.Value.html#method.try_extract_array)
- **ndarray 0.17 to match ort.** ort depends on `ndarray = "0.17"`; a different minor is a different crate and its arrays won't convert ([Cargo: SemVer compatibility](https://doc.rust-lang.org/cargo/reference/resolver.html#semver-compatibility)). In 0.17 methods such as `view` and `is_standard_layout` live on `ArrayRef`, so `Array3::view` as a path no longer resolves; call them as methods. [versions.md](../rust/versions.md#dependencies), [ndarray 0.17 ArrayRef](https://docs.rs/ndarray/0.17.2/ndarray/struct.ArrayRef.html)
- **Batches**: each model runs once per job over all its inputs; the ONNX models have a dynamic batch dimension, and a batch larger than `PERCEPTION_MAX_BATCH` (default 16) is split. Send a stage's `partial_result` after its last batch. [contracts 5, results](../../../docs/architecture/contracts.md#results)

```rust
use ndarray::{Array3, Array4, ArrayView3, Axis, ShapeError};

/// Leaf crops `[3, h, w]` into batches `[n, 3, h, w]` of at most `max` (`PERCEPTION_MAX_BATCH`).
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
