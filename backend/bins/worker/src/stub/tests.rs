//! The stub's rules and results ([worker 15.1.5]): pure functions and a recording sink, on
//! paused time.
//!
//! [worker 15.1.5]: ../../../../docs/backend/worker.md#1515-tests

use std::sync::Mutex;

use clap::Parser as _;
use domain::{
    id::{ImageId, SpeciesId},
    jobs::{JobImage, JobKind, JobResult, PerceptionStage, ProblemScore, WorkerImagePath},
    ports::ProgressError,
};

use super::*;
use crate::config::{Caps, Cli, check_stub_allowed};

const JOB: &str = "01JAX5AAAAAAAAAAAAAAAAAAAA";

/// A `Cli` from arguments only, so the shell's environment can't change a rule's input.
fn cli(caps: &str, extra: &[&str]) -> Cli {
    let mut args = vec![
        "worker",
        "--caps",
        caps,
        "--worker-id",
        "dev-stub-perception-1",
        "--api-base-url",
        "http://api",
        "--worker-token",
        "t",
    ];
    args.extend_from_slice(extra);
    Cli::try_parse_from(args).expect("arguments parse")
}

const VERSIONS: [&str; 4] = [
    "--label-set-version",
    "labels-1",
    "--classifier-labels-version",
    "classes-1",
];

fn stub_cli(extra: &[&str]) -> Cli {
    let mut args = vec!["--stub-vision"];
    args.extend_from_slice(&VERSIONS);
    args.extend_from_slice(extra);
    cli("perception", &args)
}

#[test]
fn stub_is_refused_in_release_without_the_allow_variable() {
    let plain = stub_cli(&[]);
    assert_eq!(
        check_stub_allowed(&plain, true),
        Err(StubRefused::ReleaseWithoutAllow)
    );
    assert_eq!(check_stub_allowed(&plain, false), Ok(()));
    let allowed = stub_cli(&["--allow-stub-vision"]);
    assert_eq!(check_stub_allowed(&allowed, true), Ok(()));
}

#[test]
fn stub_is_refused_with_vlm_and_fallback_caps() {
    for (flag, caps) in [("vlm", Caps::Vlm), ("fallback", Caps::Fallback)] {
        let mut args = vec!["--stub-vision", "--allow-stub-vision"];
        args.extend_from_slice(&VERSIONS);
        assert_eq!(
            check_stub_allowed(&cli(flag, &args), false),
            Err(StubRefused::WrongCaps(caps))
        );
    }
}

#[test]
fn stub_needs_both_label_versions() {
    let no_set = cli(
        "perception",
        &["--stub-vision", "--classifier-labels-version", "classes-1"],
    );
    assert_eq!(
        check_stub_allowed(&no_set, false),
        Err(StubRefused::MissingVersion("STUB_LABEL_SET_VERSION"))
    );
    let no_classes = cli(
        "perception",
        &["--stub-vision", "--label-set-version", "labels-1"],
    );
    let refused = check_stub_allowed(&no_classes, false).expect_err("refused");
    assert_eq!(
        refused,
        StubRefused::MissingVersion("STUB_CLASSIFIER_LABELS_VERSION")
    );
    assert!(
        refused
            .to_string()
            .contains("STUB_CLASSIFIER_LABELS_VERSION")
    );
    // The flag off is never refused, whatever else is missing.
    assert_eq!(check_stub_allowed(&cli("vlm", &[]), true), Ok(()));
}

#[test]
fn an_empty_version_counts_as_given() {
    // An empty knowledge base has the classifier version "" (store `load_knowledge`).
    let c = cli(
        "perception",
        &[
            "--stub-vision",
            "--label-set-version",
            "labels-1",
            "--classifier-labels-version",
            "",
        ],
    );
    assert_eq!(check_stub_allowed(&c, false), Ok(()));
    let model = StubModel::new(StubConfig::new(&c.stub).expect("config"));
    assert_eq!(model.model_versions()["classifier_labels_version"], "");
}

// ---------------------------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------------------------

/// Every event the stub sends, in order.
#[derive(Default)]
struct Recorder(Mutex<Vec<WorkerEvent>>);

#[async_trait]
impl ProgressSink for Recorder {
    async fn send(&self, events: Vec<WorkerEvent>) -> Result<(), ProgressError> {
        self.0.lock().expect("lock").extend(events);
        Ok(())
    }
}

impl Recorder {
    fn events(&self) -> Vec<WorkerEvent> {
        self.0.lock().expect("lock").clone()
    }
}

fn model(extra: &[&str]) -> StubModel {
    StubModel::new(StubConfig::new(&stub_cli(extra).stub).expect("config"))
}

fn image_id(n: char) -> ImageId {
    format!("01JAX6BBBBBBBBBBBBBBBBBBB{n}").parse().expect("id")
}

fn image(n: char, kind: ImageKind) -> JobImage {
    JobImage {
        image_id: image_id(n),
        kind,
        width: 2048,
        height: 1536,
        url: WorkerImagePath::new(JOB.parse().expect("job id"), image_id(n)),
    }
}

/// The five photo kinds of a full scan, plus a second close-up.
fn photos() -> Vec<JobImage> {
    vec![
        image('1', ImageKind::WholePlant),
        image('2', ImageKind::LeafCloseup),
        image('3', ImageKind::LeafUnderside),
        image('4', ImageKind::SoilSurface),
        image('5', ImageKind::FlowerFruit),
    ]
}

fn payload(images: Vec<JobImage>, stages: &[PerceptionStage]) -> PerceptionPayload {
    PerceptionPayload {
        images,
        stages: stages.to_vec(),
        species_id: None,
        skip_species_identification: false,
        regions: None,
        label_set_version: "labels-1".to_owned(),
        classifier_labels_version: "classes-1".to_owned(),
        question_id: None,
    }
}

fn context() -> JobContext {
    JobContext {
        job_id: JOB.parse().expect("job id"),
        scan_id: "01JAX3K9T6R2B8V0QZ4M7D1E5F".parse().expect("scan id"),
        run: None,
        attempt: 1,
    }
}

async fn perceive(
    model: &StubModel,
    payload: &PerceptionPayload,
) -> (PerceptionResult, Vec<WorkerEvent>) {
    let sink = Arc::new(Recorder::default());
    let result = model
        .perceive(
            context(),
            payload,
            &[],
            Arc::clone(&sink) as Arc<dyn ProgressSink>,
        )
        .await
        .expect("the stub answers");
    (result, sink.events())
}

#[tokio::test(start_paused = true)]
async fn stub_regions_follow_the_image_kinds() {
    let p = payload(photos(), &[PerceptionStage::Segmentation]);
    let (result, _) = perceive(&model(&[]), &p).await;
    let got: Vec<_> = result
        .regions
        .iter()
        .map(|r| (r.region_id.to_string(), r.class, r.bbox.to_array()))
        .collect();
    assert_eq!(
        got,
        vec![
            (
                format!("{}.1", image_id('1')),
                RegionClass::Plant,
                [0.10, 0.10, 0.80, 0.80]
            ),
            (
                format!("{}.1", image_id('2')),
                RegionClass::Leaf,
                [0.15, 0.15, 0.70, 0.70]
            ),
            (
                format!("{}.1", image_id('3')),
                RegionClass::Leaf,
                [0.15, 0.15, 0.70, 0.70]
            ),
            (
                format!("{}.1", image_id('4')),
                RegionClass::Soil,
                [0.00, 0.50, 1.00, 0.50]
            ),
        ]
    );
    assert!(result.regions.iter().all(|r| r.mask.is_none()));
}

#[tokio::test(start_paused = true)]
async fn stub_result_decodes_as_the_jobs_kind() {
    let m = model(&["--problems", "tomato_early_blight,tomato_late_blight,x_y"]);
    for subset in 0_u32..32 {
        let stages: Vec<PerceptionStage> = PerceptionStage::ALL
            .into_iter()
            .enumerate()
            .filter(|(i, _)| subset & (1 << i) != 0)
            .map(|(_, s)| s)
            .collect();
        let (result, events) = perceive(&m, &payload(photos(), &stages)).await;
        let value = JobResult::Perception(result.clone())
            .to_value()
            .expect("serialises");
        let decoded = JobResult::decode(JobKind::Perception, value).expect("decodes");
        assert_eq!(decoded, JobResult::Perception(result));
        // Three events per stage with a partial result, two without.
        assert!(events.len() <= stages.len() * 3, "{stages:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn stub_follows_the_payloads_species_choice() {
    let m = model(&["--species", "tomato,pepper,basil,mint"]);
    let stages = [PerceptionStage::Species, PerceptionStage::Disease];

    let (open, _) = perceive(&m, &payload(photos(), &stages)).await;
    let candidates = open.species.expect("species").candidates;
    let scores: Vec<f64> = candidates
        .iter()
        .map(|c| {
            serde_json::to_value(c.score)
                .expect("score")
                .as_f64()
                .expect("number")
        })
        .collect();
    assert_eq!(scores, vec![0.62, 0.21, 0.09]);
    let labels: Vec<_> = candidates.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, ["tomato (stub)", "pepper (stub)", "basil (stub)"]);
    assert_eq!(
        candidates[0].species_id,
        Some("tomato".parse::<SpeciesId>().expect("slug"))
    );

    let mut named = payload(photos(), &stages);
    named.species_id = Some("pepper".parse().expect("slug"));
    let (result, events) = perceive(&m, &named).await;
    assert!(result.species.is_none() && result.disease.is_some());
    assert!(!events.iter().any(|e| matches!(
        e,
        WorkerEvent::StageStarted {
            stage: Stage::Species
        }
    )));

    let mut custom = payload(photos(), &stages);
    custom.skip_species_identification = true;
    assert!(perceive(&m, &custom).await.0.species.is_none());
}

#[tokio::test(start_paused = true)]
async fn stub_disease_is_sorted_and_skips_without_leaves() {
    let m = model(&[
        "--problems",
        "tomato_early_blight,tomato_late_blight,tomato_x",
    ]);
    let stages = [PerceptionStage::Segmentation, PerceptionStage::Disease];
    let (result, _) = perceive(&m, &payload(photos(), &stages)).await;
    let disease = result.disease.expect("disease");
    assert_eq!(disease.len(), 2, "one per leaf region");
    for entry in &disease {
        assert!(entry.region_id.is_some());
        let tops: Vec<f64> = entry.top.iter().map(score).collect();
        assert_eq!(tops, vec![0.70, 0.20, 0.10]);
        assert_eq!(entry.top[0].problem_id, ProblemOrHealthy::Healthy);
    }

    // No leaf photo and no leaf region: skipped, with the stage event saying so.
    let only_soil = payload(
        vec![image('4', ImageKind::SoilSurface)],
        &[PerceptionStage::Disease],
    );
    let (skipped, events) = perceive(&m, &only_soil).await;
    assert_eq!(
        skipped.disease_skipped,
        Some(DiseaseSkipped {
            reason: DiseaseSkipReason::NoLeavesFound
        })
    );
    assert!(skipped.disease.is_none());
    assert!(events.iter().any(|e| matches!(
        e,
        WorkerEvent::StageCompleted { stage: Stage::Disease, skipped: Some(s), .. }
            if s.reason == "no_leaves_found"
    )));

    // A close-up with no region (segmentation not re-run, no prior regions): the whole photo.
    let closeup = payload(
        vec![image('2', ImageKind::LeafCloseup)],
        &[PerceptionStage::Disease],
    );
    let whole = perceive(&m, &closeup).await.0.disease.expect("disease");
    assert_eq!(whole.len(), 1);
    assert!(whole[0].region_id.is_none());
    // No problems configured: healthy alone.
    let bare = perceive(&model(&[]), &closeup)
        .await
        .0
        .disease
        .expect("disease");
    assert_eq!(bare[0].top.len(), 1);
}

fn score(p: &ProblemScore) -> f64 {
    serde_json::to_value(p.confidence)
        .expect("confidence")
        .as_f64()
        .expect("number")
}

#[tokio::test(start_paused = true)]
async fn stub_never_sends_embeddings() {
    let p = payload(photos(), &[PerceptionStage::ImageEmbedding]);
    let (result, events) = perceive(&model(&[]), &p).await;
    assert!(result.image_embeddings.is_none());
    assert!(matches!(
        events.last(),
        Some(WorkerEvent::StageCompleted { stage: Stage::ImageEmbedding, skipped: Some(s), .. })
            if s.reason == "stub"
    ));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, WorkerEvent::PartialResult { .. }))
    );
}

#[tokio::test(start_paused = true)]
async fn stub_sends_the_stage_events_in_order() {
    let stages = [PerceptionStage::PlantCheck, PerceptionStage::Segmentation];
    let (_, events) = perceive(&model(&[]), &payload(photos(), &stages)).await;
    let shape: Vec<String> = events
        .iter()
        .map(|e| match e {
            WorkerEvent::StageStarted { stage } => format!("started {stage:?}"),
            WorkerEvent::PartialResult { stage, .. } => format!("partial {stage:?}"),
            WorkerEvent::StageCompleted { stage, .. } => format!("completed {stage:?}"),
            WorkerEvent::Unknown => "unknown".to_owned(),
        })
        .collect();
    assert_eq!(
        shape,
        [
            "started PlantCheck",
            "partial PlantCheck",
            "completed PlantCheck",
            "started Segmentation",
            "partial Segmentation",
            "completed Segmentation",
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn stub_reports_a_missing_plant() {
    let (result, events) = perceive(
        &model(&["--plant-present", "false"]),
        &payload(photos(), &[PerceptionStage::PlantCheck]),
    )
    .await;
    assert_eq!(result.plant_present, Some(false));
    assert!(events.iter().any(|e| matches!(
        e,
        WorkerEvent::PartialResult {
            result: PartialResult::PlantCheck {
                plant_present: false
            },
            ..
        }
    )));
}

#[test]
fn stub_marks_every_version() {
    let v = model(&[]).model_versions();
    assert_eq!(v["stub"], "1");
    for key in [
        "perception_config",
        "leaf_detector",
        "bioclip",
        "disease_head",
    ] {
        assert_eq!(v[key], "stub", "{key}");
    }
    assert_eq!(v["label_set_version"], "labels-1");
    assert_eq!(v["classifier_labels_version"], "classes-1");
    assert!(!v.contains_key("ort_ep"));
}

#[test]
fn stub_refuses_an_unknown_slug_at_start() {
    let c = stub_cli(&["--species", "Not A Slug"]);
    assert!(matches!(
        StubConfig::new(&c.stub),
        Err(StubRefused::BadSlug {
            variable: "STUB_SPECIES",
            ..
        })
    ));
}

#[tokio::test(start_paused = true)]
async fn the_other_kinds_are_unsupported() {
    let guided = GuidedPayload {
        image: image('6', ImageKind::JarTest),
    };
    let loaded = LoadedImage {
        image: guided.image.clone(),
        bytes: bytes::Bytes::new(),
    };
    assert!(matches!(
        model(&[])
            .read_guided_check(context(), &guided, &loaded)
            .await,
        Err(ModelError::Unsupported { model: "stub", .. })
    ));
}

#[test]
fn the_command_definition_is_valid() {
    // clap's debug assertions: no two flattened arguments share a name.
    <Cli as clap::CommandFactory>::command().debug_assert();
}

#[test]
fn stub_module_has_no_io() {
    // Every identifier in the code (comments stripped), so a grouped import such as
    // `use std::{fs, net}` or a path such as `chrono::Utc::now` is caught as well.
    let code: String = include_str!("../stub.rs")
        .lines()
        .map(|line| line.split("//").next().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    let identifiers: std::collections::BTreeSet<&str> = code
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
        .collect();
    for forbidden in [
        "fs",
        "net",
        "env",
        "process",
        "File",
        "TcpStream",
        "UdpSocket",
        "Command",
        "reqwest",
        "integrations",
        "ort",
        "rand",
        "chrono",
        "SystemTime",
        "Instant",
        "Utc",
        "now",
    ] {
        assert!(!identifiers.contains(forbidden), "stub.rs uses {forbidden}");
    }
}

/// `pipeline`'s fixture is the stub's own output for a five-photo scan, so the two cannot drift.
#[tokio::test(start_paused = true)]
async fn stub_result_matches_the_pipeline_fixture() {
    let m = model(&["--problems", "tomato_early_blight"]);
    let p = payload(photos(), &PerceptionStage::ALL);
    let (result, _) = perceive(&m, &p).await;
    let value = JobResult::Perception(result)
        .to_value()
        .expect("serialises");
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/pipeline/src/fixtures/stub_perception_result.json"
    );
    if std::env::var_os("WRITE_STUB_FIXTURE").is_some() {
        std::fs::write(
            path,
            serde_json::to_string_pretty(&value).expect("json") + "\n",
        )
        .expect("write");
    }
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../crates/pipeline/src/fixtures/stub_perception_result.json"
    ))
    .expect("fixture is JSON");
    assert_eq!(value, fixture);
}

#[test]
fn stub_flags_and_plant_present_parse() {
    // `PDM_ALLOW_STUB_VISION=1` also parses (the value parser is boolish; checked by running
    // the release binary): an environment variable can't be set in a test.
    #[derive(clap::Parser)]
    struct Only {
        #[command(flatten)]
        stub: StubArgs,
    }
    let on = Only::try_parse_from(["x", "--allow-stub-vision", "--stub-vision"]).expect("flags");
    assert!(on.stub.allow_stub_vision && on.stub.stub_vision);
    let off = Only::try_parse_from(["x", "--plant-present", "0"]).expect("0 is false");
    assert!(!off.stub.plant_present);
}
