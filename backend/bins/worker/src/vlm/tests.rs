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
    max_questions: Option<usize>,
    /// The question keys each call's prompt named, in call order.
    prompts: Mutex<Vec<Vec<String>>>,
}

impl VlmBackend for Scripted {
    fn name(&self) -> &'static str {
        "scripted"
    }
    fn max_questions(&self) -> Option<usize> {
        self.max_questions
    }
    async fn call<'a>(&'a self, call: &'a VlmCall<'a>) -> Result<VlmReply, ModelError> {
        *self.calls.lock().unwrap() += 1;
        if let Some(props) = call.schema.get("properties").and_then(Value::as_object) {
            self.prompts
                .lock()
                .unwrap()
                .push(props.keys().cloned().collect());
        }
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
    adapter_with(replies, None)
}

fn adapter_with(replies: Vec<VlmReply>, max_questions: Option<usize>) -> VlmAdapter<Scripted> {
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
            max_questions,
            prompts: Mutex::new(Vec::new()),
        },
        profile,
        Arc::new(Semaphore::new(1)),
    )
}

mod bench;
mod live;
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

fn answers(keys: &[&str]) -> String {
    let map: serde_json::Map<String, Value> = keys
        .iter()
        .map(|k| {
            (
                (*k).to_owned(),
                serde_json::json!({"reason": "r", "answer": "no", "image": null}),
            )
        })
        .collect();
    Value::Object(map).to_string()
}

#[tokio::test]
async fn questions_beyond_the_backend_limit_go_in_parts_each_retried_alone() {
    let a = adapter_with(
        vec![
            reply(&answers(&["q1_a", "q2_b"]), Stop::Complete),
            reply("{", Stop::Length),
            reply(&answers(&["q3_c", "q4_d"]), Stop::Complete),
            reply(&answers(&["q5_e"]), Stop::Complete),
        ],
        Some(2),
    );
    let candidates = [CandidatePrompt {
        problem_id: "tomato_early_blight".parse().unwrap(),
        name: "Early blight".to_owned(),
        indicators: ["a", "b", "c", "d", "e"]
            .iter()
            .map(|slug| Indicator {
                indicator_id: format!("tomato_early_blight.{slug}").parse().unwrap(),
                question: "?".to_owned(),
                r#where: domain::jobs::IndicatorWhere::LowerLeaves,
                image_kinds: vec![ImageKind::WholePlant],
            })
            .collect(),
    }];
    let prepared = a.prepare(&[png()]).await.unwrap();
    let result = a
        .indicators(&candidates, &[&prepared[0]], false)
        .await
        .unwrap();
    let slugs: Vec<&str> = result
        .answers
        .iter()
        .map(|x| x.indicator_id.slug())
        .collect();
    assert_eq!(slugs, ["a", "b", "c", "d", "e"]);
    // three parts, and only the cut-off one was asked again
    assert_eq!(*a.backend.calls.lock().unwrap(), 4);
    let prompts = a.backend.prompts.lock().unwrap().clone();
    assert_eq!(
        prompts,
        [
            vec!["q1_a", "q2_b"],
            vec!["q3_c", "q4_d"],
            vec!["q3_c", "q4_d"],
            vec!["q5_e"]
        ]
    );
}
