//! The day-2 VLM benchmark's driver (task J2 of [docs/vision/dev-roadmap.md]; evaluation 8):
//! the worker's own `analyse` on evaluation cases against a live server, with each job's wall
//! time and every backend call it made. `ml/training/eval/vlm_bench.py` writes the cases, starts
//! this test and scores what it writes.
//!
//! [docs/vision/dev-roadmap.md]: ../../../../../../docs/vision/dev-roadmap.md

use std::{
    collections::BTreeMap,
    io::Write as _,
    sync::{Arc, Mutex},
    time::Instant,
};

use domain::{
    jobs::{Indicator, JobImage, VlmPayload},
    ports::{LoadedImage, VisionModel},
    rules::routing::{RoutableImage, findings_images, indicator_images},
};
use serde::Deserialize;
use serde_json::json;
use tokio::{sync::Semaphore, task::JoinSet};

use super::record::{Discard, Recording, adapter_with, ctx, hosted_adapter, stored};
use crate::vlm::{VlmAdapter, backend::VlmBackend};

/// One line of `VLM_BENCH_CASES`: the payload a `vlm` job would carry, and the photo file of
/// each of its image ids.
#[derive(Deserialize)]
struct Case {
    #[serde(rename = "case_id")]
    id: String,
    payload: VlmPayload,
    photos: BTreeMap<String, String>,
}

/// The images numbered within their kind, in the order given, for the routing rules.
fn routable(images: &[JobImage]) -> Vec<RoutableImage<JobImage>> {
    let mut out: Vec<RoutableImage<JobImage>> = Vec::new();
    for image in images {
        let nth = out.iter().filter(|r| r.kind == image.kind).count();
        out.push(RoutableImage {
            kind: image.kind,
            nth_of_kind: u32::try_from(nth).unwrap_or(u32::MAX),
            item: image.clone(),
        });
    }
    out
}

/// Picks and orders both calls' images with the pipeline's own rules, as
/// `pipeline::payloads::vlm_payload` does, so the bench sends what a scan would. The case
/// offers its shots as `findings_images`, and in `indicator_images` any extra image for the
/// indicators call (a one-photo case's photo again, as a `leaf_closeup`).
fn route(payload: &mut VlmPayload) {
    let shots = std::mem::take(&mut payload.findings_images);
    let mut offered = shots.clone();
    for image in &payload.indicator_images {
        if !offered.iter().any(|o| o.image_id == image.image_id) {
            offered.push(image.clone());
        }
    }
    payload.findings_images = findings_images(&routable(&shots));
    let indicators: Vec<&Indicator> = payload
        .candidates
        .iter()
        .flat_map(|c| &c.indicators)
        .collect();
    payload.indicator_images = if indicators.is_empty() {
        Vec::new()
    } else {
        indicator_images(&routable(&offered), &indicators, None)
    };
}

/// Every image of the payload once, stored as the bucket would hold it, with the payload's
/// sizes set to the stored ones.
fn load(photos: &BTreeMap<String, String>, payload: &mut VlmPayload) -> Vec<LoadedImage> {
    let mut loaded: Vec<LoadedImage> = Vec::new();
    for image in payload
        .findings_images
        .iter()
        .chain(&payload.indicator_images)
    {
        if loaded.iter().any(|l| l.image.image_id == image.image_id) {
            continue;
        }
        let id = image.image_id.to_string();
        let path = photos
            .get(&id)
            .unwrap_or_else(|| panic!("no photo for image {id}"));
        loaded.push(stored(path, image));
    }
    let sizes: Vec<&JobImage> = loaded.iter().map(|l| &l.image).collect();
    for image in payload
        .findings_images
        .iter_mut()
        .chain(&mut payload.indicator_images)
    {
        if let Some(s) = sizes.iter().find(|s| s.image_id == image.image_id) {
            image.clone_from(s);
        }
    }
    loaded
}

/// Runs one case through `analyse` and returns whether it finished and its output line.
async fn run_case<B: VlmBackend>(
    a: VlmAdapter<Recording<B>>,
    case: Case,
    max_pixels: u32,
    concurrency: usize,
) -> (bool, serde_json::Value) {
    let mut payload = case.payload;
    route(&mut payload);
    let images = load(&case.photos, &mut payload);
    let job = Instant::now();
    let result = a.analyse(ctx(), &payload, &images, Arc::new(Discard)).await;
    let job_ms = job.elapsed().as_millis();
    let calls: Vec<serde_json::Value> = a
        .backend
        .log
        .lock()
        .unwrap()
        .drain(..)
        .map(|c| {
            let (finish, chars, error) = match c.outcome {
                Ok((stop, chars)) => (Some(format!("{stop:?}")), Some(chars), None),
                Err(e) => (None, None, Some(e)),
            };
            json!({
                "schema": c.schema, "images": c.images, "budget": c.budget, "ms": c.ms,
                "finish": finish, "chars": chars, "error": error,
                "input_tokens": c.tokens.map(|t| t.input),
                "output_tokens": c.tokens.map(|t| t.output),
                "sizes": c.sizes, "text": c.text,
            })
        })
        .collect();
    let ok = result.is_ok();
    let line = match result {
        Ok(r) => json!({
            "case_id": case.id, "ok": true, "job_ms": job_ms,
            "max_pixels": max_pixels, "concurrency": concurrency,
            "result": r, "calls": calls,
        }),
        Err(e) => json!({
            "case_id": case.id, "ok": false, "job_ms": job_ms,
            "max_pixels": max_pixels, "concurrency": concurrency,
            "error": e.to_string(), "calls": calls,
        }),
    };
    (ok, line)
}

fn env_or<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().map_or(default, |v| {
        v.parse()
            .unwrap_or_else(|_| panic!("{name}={v} doesn't parse"))
    })
}

/// Runs every case of `VLM_BENCH_CASES` through the worker's `analyse` against a live server
/// or the hosted fallback model and writes one JSON line per case to `VLM_BENCH_OUT`: `ok`,
/// `job_ms`, the result (or the error) and each backend call with its schema, image count,
/// budget, time, finish, tokens (hosted only), the size of each image sent and the raw answer.
///
/// ```text
/// cd backend
/// VLM_BENCH_BASE_URL=http://127.0.0.1:8000 VLM_BENCH_CASES=cases.jsonl VLM_BENCH_OUT=out.jsonl \
///   VLM_BENCH_MAX_PIXELS=1003520 VLM_BENCH_CONCURRENCY=2 \
///   cargo test -p worker bench_cases -- --ignored --nocapture
/// # the fallback (task P9), with the key in infra/dev/.env (gitignored):
/// set -a; source ../infra/dev/.env; set +a
/// VLM_BENCH_HOSTED_MODEL=claude-opus-5-5 VLM_BENCH_CASES=cases.jsonl VLM_BENCH_OUT=out.jsonl \
///   VLM_BENCH_CONCURRENCY=4 cargo test -p worker bench_cases -- --ignored --nocapture
/// ```
///
/// - `VLM_BENCH_BASE_URL`: the server, without `/v1`; it must serve the model as `vlm`.
/// - `VLM_BENCH_HOSTED_MODEL`: the hosted model instead of a server, with the fallback worker's
///   adapter (`abs_px_prepared`, 1,150,000 pixels and 1568 on the long side, the hosted
///   budgets) and `LLM_API_KEY`. With neither set, the test returns at once.
/// - `VLM_API_KEY`: the server's `--api-key`, if it has one.
/// - `VLM_BENCH_MAX_PIXELS`: the adapter's `max_pixels` (default 1,003,520); the server's own
///   limit must be at least this, or it shrinks the images again.
/// - `VLM_BENCH_CONCURRENCY`: jobs in flight at once, as a worker with that many `VLM_SLOTS`
///   would run them (default 1).
/// - `CPU_PERMITS`: image preparations at once, as the worker's setting (default 2).
///
/// Both calls' images are chosen by the pipeline's routing rules (`route`).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs a VLM server"]
#[allow(
    clippy::print_stderr,
    reason = "the run's summary is the output of this manual test"
)]
async fn bench_cases() {
    let hosted = std::env::var("VLM_BENCH_HOSTED_MODEL").ok();
    let base_url = std::env::var("VLM_BENCH_BASE_URL").unwrap_or_default();
    if hosted.is_none() && base_url.is_empty() {
        eprintln!("VLM_BENCH_BASE_URL and VLM_BENCH_HOSTED_MODEL are unset: nothing to run");
        return;
    }
    let cases_path = std::env::var("VLM_BENCH_CASES").expect("VLM_BENCH_CASES names the cases");
    let out_path = std::env::var("VLM_BENCH_OUT").expect("VLM_BENCH_OUT names the output");
    let max_pixels: u32 = if hosted.is_some() {
        1_150_000
    } else {
        env_or("VLM_BENCH_MAX_PIXELS", 1_003_520)
    };
    let concurrency: usize = env_or("VLM_BENCH_CONCURRENCY", 1);
    let cases: Vec<Case> = std::fs::read_to_string(&cases_path)
        .unwrap_or_else(|e| panic!("reading {cases_path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let total = cases.len();
    let out = Arc::new(Mutex::new(std::fs::File::create(&out_path).unwrap()));
    let cpu = Arc::new(Semaphore::new(env_or("CPU_PERMITS", 2_usize).max(1)));
    let slots = Arc::new(Semaphore::new(concurrency.max(1)));
    let started = Instant::now();

    let mut tasks = JoinSet::new();
    let mut case_of = BTreeMap::new();
    for case in cases {
        let (base_url, cpu, slots, out) =
            (base_url.clone(), cpu.clone(), slots.clone(), out.clone());
        let id = case.id.clone();
        let hosted = hosted.clone();
        let task = tasks.spawn(async move {
            let _slot = slots.acquire_owned().await.unwrap();
            let (ok, line) = match hosted {
                Some(model) => {
                    run_case(hosted_adapter(&model, cpu), case, 1_150_000, concurrency).await
                }
                None => {
                    let a = adapter_with(&base_url, max_pixels, cpu).await;
                    run_case(a, case, max_pixels, concurrency).await
                }
            };
            writeln!(out.lock().unwrap(), "{line}").unwrap();
            ok
        });
        case_of.insert(task.id(), id);
    }
    // A case that panics (an unreadable photo, the server gone) is written as a failed case, and
    // the others go on.
    let mut failed = 0;
    while let Some(done) = tasks.join_next_with_id().await {
        match done {
            Ok((_, true)) => {}
            Ok((_, false)) => failed += 1,
            Err(e) => {
                failed += 1;
                let line = json!({
                    "case_id": case_of.get(&e.id()), "ok": false, "job_ms": 0,
                    "max_pixels": max_pixels, "concurrency": concurrency,
                    "error": format!("panicked: {e}"), "calls": [],
                });
                writeln!(out.lock().unwrap(), "{line}").unwrap();
            }
        }
    }
    eprintln!(
        "{total} cases, {failed} failed, {} s, max_pixels {max_pixels}, concurrency {concurrency}: {out_path}",
        started.elapsed().as_secs()
    );
}

#[test]
fn route_picks_both_calls_images_as_the_pipeline_does() {
    use domain::scan::ImageKind as K;
    let fixture: VlmPayload =
        serde_json::from_str(include_str!("fixtures/tomato_leaf.json")).unwrap();
    let template = fixture.findings_images[0].clone();
    let shot = |n: u8, kind: K| JobImage {
        image_id: format!("01JAX5AAAAAAAAAAAAAAAAAAA{n}").parse().unwrap(),
        kind,
        ..template.clone()
    };
    let ids = |images: &[JobImage]| images.iter().map(|i| i.image_id).collect::<Vec<_>>();

    // Six shots in upload order: findings take the first 4 in the findings order, and the
    // indicators call skips the flower and soil shots that no tomato question names.
    let shots = [
        shot(1, K::FlowerFruit),
        shot(2, K::LeafUnderside),
        shot(3, K::WholePlant),
        shot(4, K::LeafCloseup),
        shot(5, K::LeafCloseup),
        shot(6, K::SoilSurface),
    ];
    let mut payload = VlmPayload {
        findings_images: shots.to_vec(),
        indicator_images: Vec::new(),
        ..fixture.clone()
    };
    route(&mut payload);
    let s = |n: usize| shots[n - 1].image_id;
    assert_eq!(ids(&payload.findings_images), [s(3), s(4), s(2), s(6)]);
    assert_eq!(ids(&payload.indicator_images), [s(4), s(5), s(3), s(2)]);

    // A one-photo case offers its photo again as a close-up, for the indicators call only.
    let (whole, closeup) = (shot(7, K::WholePlant), shot(8, K::LeafCloseup));
    let mut payload = VlmPayload {
        findings_images: vec![whole.clone()],
        indicator_images: vec![closeup.clone(), whole.clone()],
        ..fixture
    };
    route(&mut payload);
    assert_eq!(ids(&payload.findings_images), [whole.image_id]);
    assert_eq!(
        ids(&payload.indicator_images),
        [closeup.image_id, whole.image_id]
    );
}
