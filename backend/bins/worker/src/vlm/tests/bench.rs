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
    jobs::{JobImage, VlmPayload},
    ports::{LoadedImage, VisionModel},
};
use serde::Deserialize;
use serde_json::json;
use tokio::{sync::Semaphore, task::JoinSet};

use super::record::{Discard, adapter_with, ctx, stored};

/// One line of `VLM_BENCH_CASES`: the payload a `vlm` job would carry, and the photo file of
/// each of its image ids.
#[derive(Deserialize)]
struct Case {
    #[serde(rename = "case_id")]
    id: String,
    payload: VlmPayload,
    photos: BTreeMap<String, String>,
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

fn env_or<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().map_or(default, |v| {
        v.parse()
            .unwrap_or_else(|_| panic!("{name}={v} doesn't parse"))
    })
}

/// Runs every case of `VLM_BENCH_CASES` through the worker's `analyse` against a live server
/// and writes one JSON line per case to `VLM_BENCH_OUT`: `ok`, `job_ms`, the result (or the
/// error) and each backend call with its schema, image count, budget, time and finish.
///
/// ```text
/// cd backend
/// VLM_BENCH_BASE_URL=http://127.0.0.1:8000 VLM_BENCH_CASES=cases.jsonl VLM_BENCH_OUT=out.jsonl \
///   VLM_BENCH_MAX_PIXELS=1003520 VLM_BENCH_CONCURRENCY=2 \
///   cargo test -p worker bench_cases -- --ignored --nocapture
/// ```
///
/// - `VLM_BENCH_BASE_URL`: the server, without `/v1`; it must serve the model as `vlm`. Unset,
///   the test returns at once.
/// - `VLM_API_KEY`: the server's `--api-key`, if it has one.
/// - `VLM_BENCH_MAX_PIXELS`: the adapter's `max_pixels` (default 1,003,520); the server's own
///   limit must be at least this, or it shrinks the images again.
/// - `VLM_BENCH_CONCURRENCY`: jobs in flight at once, as a worker with that many `VLM_SLOTS`
///   would run them (default 1).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs a VLM server"]
#[allow(
    clippy::print_stderr,
    reason = "the run's summary is the output of this manual test"
)]
async fn bench_cases() {
    let Ok(base_url) = std::env::var("VLM_BENCH_BASE_URL") else {
        eprintln!("VLM_BENCH_BASE_URL is unset: nothing to run");
        return;
    };
    let cases_path = std::env::var("VLM_BENCH_CASES").expect("VLM_BENCH_CASES names the cases");
    let out_path = std::env::var("VLM_BENCH_OUT").expect("VLM_BENCH_OUT names the output");
    let max_pixels: u32 = env_or("VLM_BENCH_MAX_PIXELS", 1_003_520);
    let concurrency: usize = env_or("VLM_BENCH_CONCURRENCY", 1);
    let cases: Vec<Case> = std::fs::read_to_string(&cases_path)
        .unwrap_or_else(|e| panic!("reading {cases_path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let total = cases.len();
    let out = Arc::new(Mutex::new(std::fs::File::create(&out_path).unwrap()));
    let cpu = Arc::new(Semaphore::new(4));
    let slots = Arc::new(Semaphore::new(concurrency.max(1)));
    let started = Instant::now();

    let mut tasks = JoinSet::new();
    let mut case_of = BTreeMap::new();
    for case in cases {
        let (base_url, cpu, slots, out) =
            (base_url.clone(), cpu.clone(), slots.clone(), out.clone());
        let id = case.id.clone();
        let task = tasks.spawn(async move {
            let _slot = slots.acquire_owned().await.unwrap();
            let a = adapter_with(&base_url, max_pixels, cpu).await;
            let mut payload = case.payload;
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
