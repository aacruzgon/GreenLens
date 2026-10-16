//! The inference thread of the perception worker: one OS thread owns every GPU session for the
//! life of the process; jobs send it tensors over a bounded channel and wait on a `oneshot`.
//!
//! Why a thread and not a pool: the worker holds one job (`slots = 1`) and runs the models one
//! after another, so one owner fits; `Session::run` takes `&mut self` and the owner never shares
//! it; and ONNX Runtime's own intra-op threads stay off tokio's workers, so the heartbeat keeps
//! ticking during a 30-second batch. A dropped job terminates its running model through
//! `RunOptions::terminate`.

use std::{collections::BTreeMap, path::PathBuf, sync::Arc, thread};

use ndarray::ArrayD;
use ort::{
    session::{RunOptions, Session},
    value::TensorRef,
};
use tokio::sync::{mpsc, oneshot};

use crate::{
    VisionError,
    runtime::{OrtRuntime, SessionSettings},
};

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

struct Request {
    slot: ModelSlot,
    inputs: Inputs,
    outputs: Vec<String>,
    options: Arc<RunOptions>,
    reply: oneshot::Sender<Result<Vec<ArrayD<f32>>, VisionError>>,
}

/// The handle jobs use. Cloning shares the one thread.
#[derive(Debug, Clone)]
pub struct Inference {
    requests: mpsc::Sender<Request>,
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Request")
            .field("slot", &self.slot)
            .finish_non_exhaustive()
    }
}

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

/// Terminates a run when dropped before [`TerminateOnDrop::disarm`].
pub struct TerminateOnDrop(Option<Arc<RunOptions>>);

impl std::fmt::Debug for TerminateOnDrop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("TerminateOnDrop")
            .field(&self.0.is_some())
            .finish()
    }
}

impl TerminateOnDrop {
    /// The run finished: nothing to terminate.
    pub fn disarm(mut self) {
        self.0 = None;
    }
}

impl Drop for TerminateOnDrop {
    fn drop(&mut self) {
        if let Some(options) = self.0.take()
            && let Err(e) = options.terminate()
        {
            tracing::warn!(error = %e, "could not terminate a cancelled run");
        }
    }
}

fn load(
    runtime: &OrtRuntime,
    specs: Vec<SessionSpec>,
) -> Result<BTreeMap<ModelSlot, Session>, VisionError> {
    let mut sessions = BTreeMap::new();
    for spec in specs {
        let mut session = runtime.session(&spec.path, spec.settings)?;
        if let Some(inputs) = &spec.warm_up {
            let options = RunOptions::new()?;
            run(&mut session, inputs, &[], &options)?;
        }
        sessions.insert(spec.slot, session);
    }
    Ok(sessions)
}

/// One run on the inference thread. `SessionOutputs` borrows the session, so the outputs are
/// copied into owned arrays before the next run.
fn run(
    session: &mut Session,
    inputs: &Inputs,
    outputs: &[String],
    options: &RunOptions,
) -> Result<Vec<ArrayD<f32>>, VisionError> {
    let values = inputs
        .iter()
        .map(|(name, array)| Ok((name.as_str(), TensorRef::from_array_view(array)?)))
        .collect::<Result<Vec<_>, VisionError>>()?;
    let result = session.run_with_options(values, options)?;
    outputs
        .iter()
        .map(|name| {
            let value = result
                .get(name.as_str())
                .ok_or_else(|| VisionError::Shape {
                    model: "inference",
                    detail: format!("missing output {name}"),
                })?;
            Ok(value.try_extract_array::<f32>()?.to_owned())
        })
        .collect()
}
