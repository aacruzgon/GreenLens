//! The adapter against a scripted backend: validation retries and the `all_measured` skip.
#![allow(
    clippy::unused_async_trait_impl,
    clippy::struct_excessive_bools,
    clippy::single_match_else,
    reason = "fakes implement async traits without awaiting, and set flags per test"
)]

use std::{collections::VecDeque, sync::Mutex};

use bytes::Bytes;
use domain::{
    jobs::{JobImage, WorkerImagePath},
    ports::ProgressError,
    scan::ImageKind,
    vlm::BoxFormat,
};

use super::*;
use crate::vlm::{
    backend::VlmReply,
    profile::{CallBudget, CertaintyMap, VlmIdentity},
};

struct Scripted {
    replies: Mutex<VecDeque<VlmReply>>,
    calls: Mutex<u32>,
}

impl VlmBackend for Scripted {
    fn name(&self) -> &'static str {
        "scripted"
    }
    async fn call<'a>(&'a self, _call: &'a VlmCall<'a>) -> Result<VlmReply, ModelError> {
        *self.calls.lock().unwrap() += 1;
        self.replies
            .lock()
            .unwrap()
            .pop_front()
            .ok_or(ModelError::Unsupported {
                model: "scripted",
                what: "more calls",
            })
    }
}

struct Events(Mutex<Vec<WorkerEvent>>);

#[async_trait]
impl ProgressSink for Events {
    async fn send(&self, events: Vec<WorkerEvent>) -> Result<(), ProgressError> {
        self.0.lock().unwrap().extend(events);
        Ok(())
    }
}

fn reply(text: &str, stop: Stop) -> VlmReply {
    VlmReply {
        text: text.to_owned(),
        stop,
    }
}

fn adapter(replies: Vec<VlmReply>) -> VlmAdapter<Scripted> {
    let profile = AdapterProfile {
        box_format: BoxFormat::Norm1000,
        cell_px: 32,
        max_pixels: 1_003_520,
        max_long_side: None,
        jpeg_quality: 90,
        budget: CallBudget::LOCAL,
        certainty: CertaintyMap::default(),
        identity: VlmIdentity {
            model: "m@1".to_owned(),
            quant: None,
            server: "vllm@v0.31.0".to_owned(),
        },
    };
    VlmAdapter::new(
        Scripted {
            replies: Mutex::new(replies.into()),
            calls: Mutex::new(0),
        },
        profile,
        Arc::new(Semaphore::new(1)),
    )
}

mod record;

const IMG: &str = "01JAX6BBBBBBBBBBBBBBBBBBBB";
const JOB: &str = "01JAX5AAAAAAAAAAAAAAAAAAAA";

fn png() -> LoadedImage {
    let mut bytes = Vec::new();
    image::RgbImage::from_pixel(64, 48, image::Rgb([60, 140, 50]))
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    LoadedImage {
        image: JobImage {
            image_id: IMG.parse().unwrap(),
            kind: ImageKind::WholePlant,
            width: 64,
            height: 48,
            url: WorkerImagePath::new(JOB.parse().unwrap(), IMG.parse().unwrap()),
        },
        bytes: Bytes::from(bytes),
    }
}

const FINDINGS: &str = r#"{"species_guess":{"species":"other","other_name":"basil","certainty":"medium"},
  "soil":{"moisture":null,"surface":null,"structure":null,"colour":null,"cover":null,"deposits":null,"erosion":null,"weeds":null},
  "organisms":[],"damage":[],"notes":""}"#;

fn payload() -> VlmPayload {
    VlmPayload {
        findings_images: vec![png().image],
        indicator_images: vec![],
        working_species: None,
        species_options: vec![],
        organism_options: vec![],
        candidates: vec![],
    }
}

fn ctx() -> JobContext {
    JobContext {
        job_id: JOB.parse().unwrap(),
        scan_id: JOB.parse().unwrap(),
        run: None,
        attempt: 1,
    }
}

#[tokio::test]
async fn a_cut_off_answer_is_retried_once_then_succeeds() {
    let a = adapter(vec![
        reply("{", Stop::Length),
        reply(FINDINGS, Stop::Complete),
    ]);
    let events = Arc::new(Events(Mutex::new(Vec::new())));
    let result = a
        .analyse(ctx(), &payload(), &[png()], events.clone())
        .await
        .unwrap();
    assert_eq!(
        result.findings.species_guess.other_name.as_deref(),
        Some("basil")
    );
    assert_eq!(*a.backend.calls.lock().unwrap(), 2);
    // every indicator was measured: the indicators call is skipped and says so
    let skipped = events.0.lock().unwrap().iter().any(|e| matches!(e,
        WorkerEvent::StageCompleted { stage: Stage::VlmIndicators, skipped: Some(s), .. } if s.reason == "all_measured"));
    assert!(skipped);
    assert_eq!(result.indicators.answers.len(), 0);
}

#[tokio::test]
async fn two_invalid_answers_fail_with_a_retryable_model_error() {
    let a = adapter(vec![
        reply("not json", Stop::Complete),
        reply("{}", Stop::Refusal),
    ]);
    let err = a
        .analyse(
            ctx(),
            &payload(),
            &[png()],
            Arc::new(Events(Mutex::new(Vec::new()))),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ModelError::InvalidOutput { .. }));
    assert!(err.retryable());
}

#[tokio::test]
async fn images_are_prepared_on_the_cell_grid() {
    let a = adapter(vec![]);
    let prepared = a.prepare(&[png()]).await.unwrap();
    assert_eq!((prepared[0].width, prepared[0].height), (64, 32));
}
