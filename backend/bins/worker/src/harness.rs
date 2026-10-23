//! One perception job without the API (task P2 of [docs/vision/dev-roadmap.md]): a job payload
//! and its photos in, the `complete` body out. The model is built by `main`'s own
//! `build_model` from the worker's environment, and the job runs through [`JobRunner`] with the
//! images read from disk instead of the API, so the result is what the worker would send.
//!
//! [docs/vision/dev-roadmap.md]: ../../../../docs/vision/dev-roadmap.md
#![allow(
    clippy::unused_async_trait_impl,
    reason = "the file-backed API implements an async trait without awaiting"
)]

use std::{
    collections::BTreeMap,
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use bytes::Bytes;
use clap::Parser as _;
use domain::{
    id::{ImageId, WorkerId},
    jobs::{
        ClaimRequest, ClaimedJob, CompleteJob, Heartbeat, HeartbeatResponse, Job, JobImage,
        JobKind, JobOutcome, JobSpec, PerceptionPayload, PerceptionResult, PerceptionStage,
        WorkerEvent, WorkerEvents, WorkerImagePath,
    },
    ports::{LoadedImage, ProgressError, ProgressSink},
    scan::ImageKind,
    wire::{Run, V1},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Semaphore;
use vision::manifest::Manifest;

use crate::{
    api::{CallError, JobLease, WorkerApi},
    config::Cli,
    job::JobRunner,
    vlm::tests::record::stored,
};

const JOB: &str = "01JAX7HARNESS0000000000000";
const WORKER: &str = "perception-harness";

/// `PERCEPTION_HARNESS_JOB`: the payload a `perception` job would carry, and the photo file of
/// each of its image ids. The images' sizes are replaced by the stored ones.
#[derive(Deserialize)]
struct JobFile {
    payload: PerceptionPayload,
    photos: BTreeMap<String, String>,
}

/// The worker API with the images held in memory. Only `image` is called: [`JobRunner`] takes
/// the progress sink as an argument, and the harness reads the `complete` body itself.
struct Files(BTreeMap<ImageId, Bytes>);

impl WorkerApi for Files {
    async fn heartbeat(&self, _hb: &Heartbeat) -> Result<HeartbeatResponse, CallError> {
        Err(CallError::Rejected("the harness has no broker".into()))
    }

    async fn claim(&self, _req: &ClaimRequest) -> Result<Option<ClaimedJob>, CallError> {
        Err(CallError::Rejected("the harness has no broker".into()))
    }

    async fn events(&self, _lease: &JobLease, _events: &WorkerEvents) -> Result<(), CallError> {
        Err(CallError::Rejected("the harness has no broker".into()))
    }

    async fn complete(&self, _lease: &JobLease, _body: &CompleteJob) -> Result<(), CallError> {
        Err(CallError::Rejected("the harness has no broker".into()))
    }

    async fn image(&self, _lease: &JobLease, image: &JobImage) -> Result<Bytes, CallError> {
        self.0
            .get(&image.image_id)
            .cloned()
            .ok_or_else(|| CallError::Rejected(format!("no photo for image {}", image.image_id)))
    }
}

/// Keeps every event the model sends, as the API would receive them.
#[derive(Default)]
struct Events(Mutex<Vec<WorkerEvent>>);

#[async_trait]
impl ProgressSink for Events {
    async fn send(&self, events: Vec<WorkerEvent>) -> Result<(), ProgressError> {
        self.0.lock().unwrap().extend(events);
        Ok(())
    }
}

/// `kind=path,kind=path`: one image per photo, in the order given, with every stage.
fn photos_job(spec: &str, label_set_version: String, classifier_labels_version: String) -> JobFile {
    let mut images = Vec::new();
    let mut photos = BTreeMap::new();
    for (n, item) in spec.split(',').filter(|s| !s.trim().is_empty()).enumerate() {
        let (kind, path) = item
            .split_once('=')
            .unwrap_or_else(|| panic!("{item}: expected kind=path"));
        let kind: ImageKind = serde_json::from_value(json!(kind.trim()))
            .unwrap_or_else(|_| panic!("{kind}: not an image kind"));
        let image_id: ImageId = format!("01JAX7PH0T0S{n:014}").parse().unwrap();
        images.push(JobImage {
            image_id,
            kind,
            width: 0,
            height: 0,
            url: WorkerImagePath::new(JOB.parse().unwrap(), image_id),
        });
        photos.insert(image_id.to_string(), path.trim().to_owned());
    }
    JobFile {
        payload: PerceptionPayload {
            images,
            stages: PerceptionStage::ALL.to_vec(),
            species_id: None,
            skip_species_identification: false,
            regions: None,
            label_set_version,
            classifier_labels_version,
            question_id: None,
        },
        photos,
    }
}

/// `var` if set, otherwise the manifest's one version of `prefix`'s entries.
fn version(
    var: &str,
    manifest: &Manifest,
    prefix: &str,
    of: fn(&vision::manifest::ModelEntry) -> Option<&String>,
) -> String {
    if let Ok(v) = std::env::var(var) {
        return v;
    }
    let all: Vec<&String> = manifest
        .entries_with_prefix(prefix)
        .filter_map(of)
        .collect();
    match all.as_slice() {
        [one] => (*one).clone(),
        _ => panic!("the manifest has {all:?} for {prefix}: set {var} to one of them"),
    }
}

/// Every image of the payload stored as the bucket would hold it, with the payload's sizes set
/// to the stored ones.
fn load(file: &mut JobFile) -> BTreeMap<ImageId, Bytes> {
    let mut out = BTreeMap::new();
    for image in &mut file.payload.images {
        let id = image.image_id.to_string();
        let path = file
            .photos
            .get(&id)
            .unwrap_or_else(|| panic!("no photo for image {id}"));
        let LoadedImage {
            image: sized,
            bytes,
        } = stored(path, image);
        *image = sized;
        out.insert(image.image_id, bytes);
    }
    out
}

fn job(payload: PerceptionPayload) -> Job {
    Job {
        v: V1,
        job_id: JOB.parse().unwrap(),
        scan_id: JOB.parse().unwrap(),
        run: Some(Run(1)),
        attempt: 1,
        lease_token: JOB.parse().unwrap(),
        lease_until: "2026-10-23T12:00:00Z".parse().unwrap(),
        max_runtime_s: u32::try_from(JobKind::Perception.max_runtime(false).as_secs()).unwrap(),
        spec: JobSpec::Perception(payload),
    }
}

/// `nvidia-smi`'s view of the card while the harness runs: the process's own memory, which
/// WSL doesn't report, and the whole card's, sampled every 50 ms.
struct Vram {
    stop: Arc<AtomicBool>,
    thread: thread::JoinHandle<Value>,
}

impl Vram {
    fn start(device: i32) -> Option<Self> {
        let card = card_mib(device)?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            let pid = std::process::id();
            let (mut process_peak, mut card_peak) = (0, card.used);
            while !flag.load(Ordering::Relaxed) {
                process_peak = process_peak.max(process_mib(pid));
                if let Some(c) = card_mib(device) {
                    card_peak = card_peak.max(c.used);
                }
                thread::sleep(Duration::from_millis(50));
            }
            json!({
                "gpu": card.name, "total_mib": card.total, "card_before_mib": card.used,
                "card_peak_mib": card_peak, "process_peak_mib": process_peak,
                "free_at_peak_mib": card.total.saturating_sub(card_peak),
            })
        });
        Some(Self { stop, thread })
    }

    fn finish(self) -> Value {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.join().unwrap()
    }
}

struct Card {
    name: String,
    used: u64,
    total: u64,
}

fn nvidia_smi(args: &[&str]) -> Option<String> {
    let out = Command::new("nvidia-smi").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn card_mib(device: i32) -> Option<Card> {
    let out = nvidia_smi(&[
        &format!("--id={device}"),
        "--query-gpu=name,memory.used,memory.total",
        "--format=csv,noheader,nounits",
    ])?;
    let mut f = out.lines().next()?.split(',').map(str::trim);
    Some(Card {
        name: f.next()?.to_owned(),
        used: f.next()?.parse().ok()?,
        total: f.next()?.parse().ok()?,
    })
}

fn process_mib(pid: u32) -> u64 {
    nvidia_smi(&[
        "--query-compute-apps=pid,used_memory",
        "--format=csv,noheader,nounits",
    ])
    .unwrap_or_default()
    .lines()
    .filter_map(|l| l.split_once(','))
    .filter(|(p, _)| p.trim().parse() == Ok(pid))
    .filter_map(|(_, m)| m.trim().parse::<u64>().ok())
    .sum()
}

fn ms(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Each stage's `duration_ms`, from its `stage_completed` event.
fn stage_ms(events: &[WorkerEvent]) -> BTreeMap<String, Value> {
    events
        .iter()
        .filter_map(|e| match e {
            WorkerEvent::StageCompleted {
                stage,
                duration_ms,
                skipped,
            } => Some((
                json!(stage).as_str().unwrap_or("?").to_owned(),
                skipped
                    .as_ref()
                    .map_or_else(|| json!(duration_ms), |s| json!({"skipped": s.reason})),
            )),
            _ => None,
        })
        .collect()
}

/// What P2's done condition asks for, per image: leaf regions and their masks, the species top
/// 3, the disease top 3 of each crop and the embedding's size.
fn summary(images: &[JobImage], result: &PerceptionResult) -> Value {
    let per_image: Vec<Value> = images
        .iter()
        .map(|image| {
            let id = image.image_id;
            let regions: Vec<_> = result.regions.iter().filter(|r| r.image_id == id).collect();
            let classes = regions.iter().fold(BTreeMap::new(), |mut m, r| {
                *m.entry(json!(r.class).as_str().unwrap_or("?").to_owned())
                    .or_insert(0) += 1;
                m
            });
            let disease: Vec<Value> = result
                .disease
                .iter()
                .flatten()
                .filter(|d| d.image_id == id)
                .map(|d| {
                    json!({
                        "region_id": d.region_id,
                        "top": d.top.iter().take(3)
                            .map(|p| format!("{} {}", p.problem_id, p.confidence))
                            .collect::<Vec<_>>(),
                    })
                })
                .collect();
            let embedding = result
                .image_embeddings
                .iter()
                .flatten()
                .find(|e| e.image_id == id)
                .map(|e| json!({"model": e.model, "dim": e.vector.len()}));
            json!({
                "image_id": id, "kind": image.kind,
                "regions": classes,
                "masks": regions.iter().filter(|r| r.mask.is_some()).count(),
                "disease": disease, "embedding": embedding,
            })
        })
        .collect();
    json!({
        "plant_present": result.plant_present,
        "species_top3": result.species.as_ref().map(|s| s.candidates.iter().take(3)
            .map(|c| format!("{} ({}) {}", c.label, c.species_id.as_ref()
                .map_or_else(|| "-".to_owned(), ToString::to_string), c.score))
            .collect::<Vec<_>>()),
        "disease_skipped": result.disease_skipped,
        "images": per_image,
    })
}

/// The worker's command line for `--caps perception`, with the rest from its environment.
fn worker_cli() -> Cli {
    let cli = Cli::try_parse_from([
        "worker",
        "--caps",
        "perception",
        "--worker-id",
        WORKER,
        "--api-base-url",
        "http://harness.invalid",
        "--worker-token",
        "harness",
    ])
    .unwrap_or_else(|e| panic!("the worker's environment: {e}"));
    assert!(
        !cli.stub.stub_vision,
        "STUB_VISION is set: unset it to run the real models"
    );
    cli
}

/// The job file, or the job `PERCEPTION_HARNESS_PHOTOS` describes.
fn read_job(job_file: Option<String>, photos: Option<String>, cli: &Cli) -> JobFile {
    match (job_file, photos) {
        (Some(path), _) => serde_json::from_str(
            &std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path}: {e}")),
        )
        .unwrap_or_else(|e| panic!("{path}: {e}")),
        (None, Some(spec)) => {
            let manifest = Manifest::load(&cli.perception.manifest).unwrap();
            photos_job(
                &spec,
                version(
                    "PERCEPTION_HARNESS_LABEL_SET",
                    &manifest,
                    "bioclip_labels",
                    |e| e.label_set_version.as_ref(),
                ),
                version(
                    "PERCEPTION_HARNESS_CLASSIFIER_LABELS",
                    &manifest,
                    "disease_head",
                    |e| e.classifier_labels_version.as_ref(),
                ),
            )
        }
        (None, None) => panic!("set PERCEPTION_HARNESS_JOB or PERCEPTION_HARNESS_PHOTOS"),
    }
}

/// Runs one perception job on the worker's model and writes `PERCEPTION_HARNESS_OUT`: the
/// `complete` body of the last run, its events, the load time, each run's job and stage times,
/// a summary per image and, on an NVIDIA card, peak VRAM.
///
/// ```text
/// cd backend
/// MODEL_MANIFEST=~/.cache/pdm-models/manifest.toml ORT_EP=cpu ORT_DYLIB_PATH=… \
///   PERCEPTION_HARNESS_PHOTOS='whole_plant=/p/plant.jpg,leaf_closeup=/p/leaf.jpg' \
///   PERCEPTION_HARNESS_LABEL_SET=edb29fe878952e7b PERCEPTION_HARNESS_OUT=out.json \
///   cargo test -p worker --release perceive_job -- --ignored --nocapture
/// ```
///
/// - The model comes from the worker's own environment and start-up (`MODEL_MANIFEST`,
///   `ORT_EP`, `ORT_DEVICE_ID`, `ORT_DYLIB_PATH`, `ORT_INTRA_THREADS`, `CPU_PERMITS` and the
///   batch caps, [worker 4]). On a GPU, add `--features cuda` to the cargo command.
/// - `PERCEPTION_HARNESS_JOB`: a JSON file `{"payload": …, "photos": {"<image_id>": "<path>"}}`
///   with the payload a `perception` job carries ([contracts 5]). Or, instead:
/// - `PERCEPTION_HARNESS_PHOTOS`: `kind=path` pairs, comma-separated, in upload order; the job
///   runs every stage. Its label versions are `PERCEPTION_HARNESS_LABEL_SET` and
///   `PERCEPTION_HARNESS_CLASSIFIER_LABELS`, which may be left out when the manifest has only
///   one.
/// - `PERCEPTION_HARNESS_RUNS`: how many times to run the job (default 1), for the time per job.
///
/// Photos are stored as the bucket would hold them (`WebP` at the default long side) before the
/// job sees them. With neither `PERCEPTION_HARNESS_JOB` nor `PERCEPTION_HARNESS_PHOTOS` set, the
/// test returns at once.
///
/// [worker 4]: ../../../../docs/backend/worker.md#4-configuration
/// [contracts 5]: ../../../../docs/architecture/contracts.md#5-worker-api-and-jobs
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the model files and ONNX Runtime"]
#[allow(
    clippy::print_stderr,
    reason = "the run's summary is the output of this manual test"
)]
async fn perceive_job() {
    let (job_file, photos) = (
        std::env::var("PERCEPTION_HARNESS_JOB").ok(),
        std::env::var("PERCEPTION_HARNESS_PHOTOS").ok(),
    );
    if job_file.is_none() && photos.is_none() {
        eprintln!("PERCEPTION_HARNESS_JOB and PERCEPTION_HARNESS_PHOTOS are unset: nothing to run");
        return;
    }
    let out_path =
        std::env::var("PERCEPTION_HARNESS_OUT").expect("PERCEPTION_HARNESS_OUT names the output");
    let runs: usize = std::env::var("PERCEPTION_HARNESS_RUNS").map_or(1, |v| {
        v.parse().expect("PERCEPTION_HARNESS_RUNS is a number")
    });
    let cli = worker_cli();
    let mut file = read_job(job_file, photos, &cli);
    let api = Files(load(&mut file));
    let images = file.payload.images.clone();
    let job = job(file.payload);

    let vram = (cli.perception.ort_ep != vision::runtime::OrtEp::Cpu)
        .then(|| Vram::start(cli.perception.ort_device_id))
        .flatten();
    let started = Instant::now();
    let http = integrations::http::client(&integrations::http::HttpConfig::default()).unwrap();
    let cpu = Arc::new(Semaphore::new(cli.cpu_permits.max(1)));
    let model = crate::build_model(&cli, http, cpu)
        .await
        .unwrap_or_else(|e| panic!("loading the model: {e:#}"));
    let load_ms = ms(started);
    let runner = JobRunner::new(model, WORKER.parse::<WorkerId>().unwrap());

    let mut times = Vec::new();
    let mut last = None;
    for _ in 0..runs.max(1) {
        let events = Arc::new(Events::default());
        let started = Instant::now();
        let body = runner
            .execute(&api, &job, events.clone())
            .await
            .unwrap_or_else(|_| panic!("the harness's lease can't be lost"));
        let events = std::mem::take(&mut *events.0.lock().unwrap());
        times.push(json!({"job_ms": ms(started), "stages_ms": stage_ms(&events)}));
        last = Some((body, events));
    }
    let vram = vram.map(Vram::finish);
    let (body, events) = last.unwrap();

    let summary = match &body.outcome {
        JobOutcome::Done { result } => {
            summary(&images, &serde_json::from_value(result.clone()).unwrap())
        }
        JobOutcome::Failed { error } => json!({"failed": error}),
    };
    let report = json!({
        "job_id": job.job_id, "load_ms": load_ms, "runs": times,
        "vram": vram, "summary": summary, "complete": body, "events": events,
    });
    std::fs::write(&out_path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    eprintln!(
        "{}\nload {load_ms} ms; runs {}; vram {}\n{out_path}",
        serde_json::to_string_pretty(&report["summary"]).unwrap(),
        json!(report["runs"]),
        report["vram"],
    );
    assert!(
        matches!(body.outcome, JobOutcome::Done { .. }),
        "the job failed: see {out_path}"
    );
}

#[test]
fn photos_become_a_payload_in_upload_order_with_every_stage() {
    let file = photos_job(
        "whole_plant=/p/a.jpg, leaf_closeup = /p/b c.jpg,",
        "labels-1".to_owned(),
        "classes-1".to_owned(),
    );
    let kinds: Vec<ImageKind> = file.payload.images.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, [ImageKind::WholePlant, ImageKind::LeafCloseup]);
    assert_eq!(file.payload.stages, PerceptionStage::ALL);
    assert_eq!(file.payload.label_set_version, "labels-1");
    let second = file.payload.images[1].image_id.to_string();
    assert_eq!(file.photos[&second], "/p/b c.jpg");
}

#[test]
fn a_job_file_reads_as_the_payload_and_its_photos() {
    let text = json!({
        "payload": {
            "images": [{
                "image_id": "01JAX7PH0T0S00000000000000", "kind": "whole_plant",
                "width": 0, "height": 0,
                "url": format!("/internal/jobs/{JOB}/images/01JAX7PH0T0S00000000000000"),
            }],
            "stages": ["segmentation", "species"],
            "label_set_version": "labels-1", "classifier_labels_version": "classes-1",
        },
        "photos": {"01JAX7PH0T0S00000000000000": "/p/a.jpg"},
    })
    .to_string();
    let file: JobFile = serde_json::from_str(&text).unwrap();
    assert_eq!(
        file.payload.stages,
        [PerceptionStage::Segmentation, PerceptionStage::Species]
    );
    assert_eq!(file.photos.len(), 1);
}
