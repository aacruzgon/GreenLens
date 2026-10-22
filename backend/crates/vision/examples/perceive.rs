//! Runs the perception worker's own models on image files, without the API or a job queue:
//! `ml/training/eval/spot_fit.py` takes the leaf masks the worker would cut (the leaf detector,
//! then SAM) from photos with labelled lesions. It is a first cut of the perception harness
//! (docs/vision/dev-roadmap.md, P2).
//!
//! ```text
//! cd backend
//! echo '{"manifest": "/path/manifest.cpu.toml", "images": [{"name": "a", "path": "/x/a.jpg"}]}' \
//!   | ORT_DYLIB_PATH=/path/libonnxruntime.so cargo run -q --release -p vision --example perceive
//! ```
//!
//! Reads `{"manifest", "ep"?, "threads"?, "stages"?, "images": [{"name", "path"}]}` on stdin.
//! `ep` is `cpu` (the default, with `manifest.cpu.toml`'s fp32 files) or `cuda`; `stages`
//! defaults to `["segmentation"]`. Each image runs as its own job, as a question photo's
//! segmentation-only job does. Writes one JSON line per image: `{"name", "width", "height",
//! "regions"}` with the job's `PerceptionRegion`s (COCO RLE masks), or `{"name", "error"}`.
//! Exit code 2 when the input or the models can't be read.

use std::{io::Read, io::Write, path::PathBuf, sync::Arc};

use async_trait::async_trait;
use domain::{
    jobs::{JobImage, PerceptionPayload, PerceptionStage, WorkerEvent, WorkerImagePath},
    ports::{JobContext, LoadedImage, ProgressError, ProgressSink, VisionModel},
    scan::ImageKind,
};
use serde::Deserialize;
use vision::{
    manifest::Manifest,
    perception::{BatchCaps, PerceptionPipeline},
    runtime::{OrtEp, OrtRuntime, RuntimeConfig},
};

#[derive(Deserialize)]
struct Input {
    manifest: PathBuf,
    #[serde(default = "cpu")]
    ep: String,
    #[serde(default = "threads")]
    threads: usize,
    #[serde(default = "segmentation")]
    stages: Vec<PerceptionStage>,
    images: Vec<Image>,
}

fn cpu() -> String {
    "cpu".to_owned()
}

fn threads() -> usize {
    8
}

fn segmentation() -> Vec<PerceptionStage> {
    vec![PerceptionStage::Segmentation]
}

#[derive(Deserialize)]
struct Image {
    name: String,
    path: PathBuf,
}

/// Progress goes nowhere: only the final result is printed.
struct Discard;

#[async_trait]
impl ProgressSink for Discard {
    async fn send(&self, _events: Vec<WorkerEvent>) -> Result<(), ProgressError> {
        Ok(())
    }
}

fn fail(what: &str, e: &dyn std::fmt::Display) -> ! {
    #[allow(clippy::print_stderr, reason = "a command-line tool's error")]
    {
        eprintln!("perceive: {what}: {e}");
    }
    std::process::exit(2);
}

/// A fixed ULID per job and per image: the result only has to name its own image.
fn ulid(n: usize) -> String {
    format!("01JAX5AAAAAAAAAAAAAAAA{n:04}")
}

async fn one(
    pipeline: &PerceptionPipeline,
    stages: &[PerceptionStage],
    n: usize,
    image: &Image,
) -> Result<serde_json::Value, String> {
    let bytes = std::fs::read(&image.path).map_err(|e| e.to_string())?;
    let (width, height) = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .into_dimensions()
        .map_err(|e| e.to_string())?;
    let id = ulid(n);
    let image_id = id.parse().map_err(|e| format!("{e:?}"))?;
    let job_id = id.parse().map_err(|e| format!("{e:?}"))?;
    let job = JobImage {
        image_id,
        kind: ImageKind::LeafCloseup,
        width,
        height,
        url: WorkerImagePath::new(job_id, image_id),
    };
    let payload = PerceptionPayload {
        images: vec![job.clone()],
        stages: stages.to_vec(),
        species_id: None,
        skip_species_identification: true,
        regions: None,
        label_set_version: String::new(),
        classifier_labels_version: String::new(),
        question_id: None,
    };
    let ctx = JobContext {
        job_id,
        scan_id: id.parse().map_err(|e| format!("{e:?}"))?,
        run: None,
        attempt: 1,
    };
    let loaded = [LoadedImage {
        image: job,
        bytes: bytes.into(),
    }];
    let result = pipeline
        .perceive(ctx, &payload, &loaded, Arc::new(Discard))
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "name": image.name,
        "width": width,
        "height": height,
        "regions": result.regions,
        "plant_present": result.plant_present,
    }))
}

#[tokio::main]
async fn main() {
    let mut raw = String::new();
    let input: Input = std::io::stdin()
        .read_to_string(&mut raw)
        .map_err(|e| e.to_string())
        .and_then(|_| serde_json::from_str(&raw).map_err(|e| e.to_string()))
        .unwrap_or_else(|e| fail("can't read the input", &e));
    let ep: OrtEp = input.ep.parse().unwrap_or_else(|e| fail("unknown ep", &e));
    let manifest = Manifest::load(&input.manifest).unwrap_or_else(|e| fail("manifest", &e));
    let runtime = OrtRuntime::init(&RuntimeConfig {
        ep,
        device_id: 0,
        dylib_path: std::env::var_os("ORT_DYLIB_PATH").map(PathBuf::from),
        intra_threads: input.threads,
        name: "perceive-example",
    })
    .unwrap_or_else(|e| fail("ONNX Runtime", &e));
    let pipeline = PerceptionPipeline::load(runtime, &manifest, BatchCaps::default(), 2)
        .await
        .unwrap_or_else(|e| fail("models", &e));
    let mut out = std::io::stdout().lock();
    for (n, image) in input.images.iter().enumerate() {
        let line = one(&pipeline, &input.stages, n, image)
            .await
            .unwrap_or_else(|e| serde_json::json!({"name": image.name, "error": e}));
        // reason: a closed stdout means the caller stopped reading; nothing to report to.
        let _ = writeln!(out, "{line}");
    }
}
