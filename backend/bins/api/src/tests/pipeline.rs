//! The real `pipeline::Pipeline` over the fake store: the stage events a job's `complete`
//! sends for the ones the stream lost (backend roadmap gap 9, `Pipeline::job_completed`), and
//! `POST /plants/{id}/species` and `POST /scans/{id}/submit` through the router with the
//! pipeline wired in.

use std::sync::{Arc, Mutex};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use tower::ServiceExt;

use domain::{
    events::{ResolvedBy, RunReason, ScanEventBody, Stage},
    id::{ImageId, JobId, ScanId, SpeciesId},
    jobs::{CompleteJob, JobKind, JobOutcome, JobSpec},
    knowledge::ClassifierLabels,
    ports::{
        ClaimAttempt, CompletionOutcome, GardenContext, GuidedFinish, Lease, LeasedJob,
        MockBeliefStore, MockBlobStore, MockKnowledgeStore, MockMemoryStore, MockTextEmbedder,
        MockWeatherProvider, ObservationDiff, RecentScan, ScanSnapshot, SpeciesConfirmation,
        StartedRun, StoreError, VlmOptions,
    },
    records::{
        observation::{KnowledgeRef, Observation, ObservationSource, ObservationStage},
        weather::{Coordinates, LocationPrecision, WeatherCell},
    },
    scan::{
        GuidedCheck, GuidedCheckStatus, GuidedKind, ImageKind, Plant, PlantStatus, Question,
        QuestionKind, QuestionStatus, Scan, ScanFailure, ScanFailureCode, ScanImage, ScanStatus,
        SpeciesChoice, SpeciesRef, SpeciesStatus,
    },
    testing::{FixedClock, SequentialIds},
    url::{NoUrl, Stored},
    wire::{BBox, Confidence, EventSeq, Region, Role, Run, Timestamp, V1},
};
use pipeline::{
    ImageMeasurer, MeasureFailed, MeasureRequest, MeasuredQuantity, Pipeline, PipelineConfig,
    PipelineDeps, PlanAuthor, PlanGuard, ScanWriter,
};

use super::{FakeStore, GARDEN, NOW, SCAN, config, json, no_images, signed_in};

const PLANT: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5P";
const JOB: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5J";
const CHECK_IMAGE: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5K";

/// The stub worker's full result for a five-photo scan: plant check, four regions, species and
/// disease, no image embeddings.
const STUB_RESULT: &str =
    include_str!("../../../../crates/pipeline/src/fixtures/stub_perception_result.json");

/// Every `emit` call, in order.
#[derive(Default)]
struct Recorder(Mutex<Vec<Vec<ScanEventBody<Stored>>>>);

impl Recorder {
    fn calls(&self) -> Vec<Vec<ScanEventBody<Stored>>> {
        self.0.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl ScanWriter for Recorder {
    async fn emit(
        &self,
        _: ScanId,
        _: Run,
        events: Vec<ScanEventBody<Stored>>,
    ) -> Result<Vec<domain::events::ScanEvent<Stored>>, StoreError> {
        self.0.lock().unwrap().push(events);
        Ok(Vec::new())
    }
    async fn plan_lock(&self, _: ScanId) -> PlanGuard {
        PlanGuard::new(Arc::new(tokio::sync::Mutex::new(())).lock_owned().await)
    }
}

/// Measurement is never reached by these tests.
struct NoMeasurer;

impl ImageMeasurer for NoMeasurer {
    fn version(&self) -> String {
        "measure-test".into()
    }
    fn measure(&self, _: &MeasureRequest) -> Result<Vec<MeasuredQuantity>, MeasureFailed> {
        unreachable!("not used by these tests")
    }
}

/// The plan writer is never reached by these tests.
struct NoPlans;

#[async_trait::async_trait]
impl PlanAuthor for NoPlans {
    async fn write_plan(
        &self,
        _: &agent::plan_writer::PlanInput<'_>,
    ) -> Result<agent::plan_writer::WrittenPlan, agent::PlanWriterError> {
        unreachable!("not used by these tests")
    }
}

fn now() -> Timestamp {
    NOW.parse().unwrap()
}

/// The pipeline over `store`, with an empty knowledge base and `writer` recording its events.
fn pipeline(store: Arc<FakeStore>, writer: Arc<Recorder>) -> Pipeline {
    pipeline_with_blobs(store, writer, Arc::new(MockBlobStore::new()))
}

/// [`pipeline`] over the object store `blobs`.
fn pipeline_with_blobs(
    store: Arc<FakeStore>,
    writer: Arc<Recorder>,
    blobs: Arc<dyn domain::ports::BlobStore>,
) -> Pipeline {
    let mut knowledge = MockKnowledgeStore::new();
    knowledge.expect_problems().returning(|_| Ok(Vec::new()));
    knowledge.expect_vlm_options().returning(|| {
        Ok(VlmOptions {
            label_set_version: "labels-1".into(),
            species: Vec::new(),
            organisms: Vec::new(),
        })
    });
    knowledge.expect_classifier_labels().returning(|| {
        Ok(ClassifierLabels {
            model_version: "classifier-1".into(),
            labels: Vec::new(),
        })
    });
    Pipeline::new(PipelineDeps {
        store,
        writer,
        knowledge: Arc::new(knowledge),
        weather: Arc::new(MockWeatherProvider::new()),
        memory: Arc::new(MockMemoryStore::new()),
        beliefs: Arc::new(MockBeliefStore::new()),
        embedder: Arc::new(MockTextEmbedder::new()),
        blobs,
        measurer: Arc::new(NoMeasurer),
        plan_author: Arc::new(NoPlans),
        clock: Arc::new(FixedClock::at(now())),
        ids: Arc::new(SequentialIds::default()),
        config: PipelineConfig::default(),
    })
}

fn plant(species: Option<SpeciesRef>) -> Plant<Stored> {
    Plant {
        id: PLANT.parse().unwrap(),
        garden_id: GARDEN.parse().unwrap(),
        name: "Tomato by the fence".into(),
        species_status: if species.is_some() {
            SpeciesStatus::Confirmed
        } else {
            SpeciesStatus::Unknown
        },
        species,
        custom_species_name: None,
        species_candidates: Vec::new(),
        status: PlantStatus::Healthy,
        position: None,
        latest_scan: None,
        created_at: now(),
    }
}

/// An `analysing` scan in run 1 (`planned` with a guided check in progress when `guided`).
fn snapshot(guided: bool) -> ScanSnapshot {
    let coordinates = Coordinates {
        lat: 43.65,
        lon: -79.38,
    };
    ScanSnapshot {
        scan: Scan {
            id: SCAN.parse().unwrap(),
            garden_id: GARDEN.parse().unwrap(),
            plant_id: PLANT.parse().unwrap(),
            status: if guided {
                ScanStatus::Planned
            } else {
                ScanStatus::Analysing
            },
            run: Run(1),
            images: Vec::new(),
            guided_checks: if guided {
                vec![GuidedCheck {
                    image_id: CHECK_IMAGE.parse().unwrap(),
                    kind: GuidedKind::JarTest,
                    status: GuidedCheckStatus::Analysing,
                    result: None,
                }]
            } else {
                Vec::new()
            },
            created_at: now(),
            created_by: None,
            submitted_at: Some(now()),
            open_question: None,
            diagnosis_id: None,
            plan_id: None,
            failure: None,
            last_event_seq: EventSeq(4),
        },
        plant: plant(None),
        garden: GardenContext {
            id: GARDEN.parse().unwrap(),
            name: "Back yard".into(),
            coordinates,
            precision: LocationPrecision::About1Km,
            weather_cell: WeatherCell::of(coordinates),
            time_zone: domain::garden::gardens::TimeZone("America/Toronto".into()),
            soil_profile: None,
            map_version: 0,
        },
        working_species: None,
        custom_species: false,
        observations: Vec::new(),
        evidence: Vec::new(),
        candidates: Vec::new(),
        weather: None,
        open_question_claim: None,
        open_question_indicator: None,
        masks: Vec::new(),
        depth: Vec::new(),
        questions_asked: 0,
        questions: Vec::new(),
        diagnosis: None,
        plan: None,
        plan_diagnosis: None,
        rescore_pending: false,
    }
}

fn lease() -> Lease {
    Lease {
        job: JOB.parse().unwrap(),
        token: "01JAX3K9T6R2B8V0QZ4M7D1E5T".parse().unwrap(),
        worker: "rig-3070-perception-1".parse().unwrap(),
    }
}

fn job(kind: JobKind) -> LeasedJob {
    LeasedJob {
        job: JOB.parse::<JobId>().unwrap(),
        kind,
        scan: SCAN.parse().unwrap(),
        run: Some(Run(1)),
        question: None,
        segmentation: kind == JobKind::Perception,
    }
}

fn done(result: serde_json::Value) -> CompleteJob {
    CompleteJob {
        v: V1,
        outcome: JobOutcome::Done { result },
        model_versions: std::collections::BTreeMap::new(),
        timings_ms: std::collections::BTreeMap::new(),
    }
}

/// One finding of the stub's result, as the store's diff returns it.
fn finding() -> Observation<Stored> {
    Observation {
        id: "01JAX3K9T6R2B8V0QZ4M7D1E52".parse().unwrap(),
        scan_id: SCAN.parse().unwrap(),
        run: Run(1),
        stage: ObservationStage::PlantCheck,
        pipeline_stage: Stage::PlantCheck,
        label: "plant".into(),
        knowledge_ref: None,
        role: None,
        detail: None,
        confidence: Confidence::new(0.9).unwrap().into(),
        region: Some(Region::unmasked(
            "01JAX6BBBBBBBBBBBBBBBBBBB1".parse::<ImageId>().unwrap(),
            BBox::FULL,
            None,
        )),
        source: ObservationSource::Segmentation,
        model_version: "stub".into(),
        created_at: now(),
    }
}

/// The run's stored events, up to `plant_check` started: the rest of the job's stage events
/// were lost while `api` was down.
fn lost_after_plant_check() -> Vec<ScanEventBody<Stored>> {
    vec![
        ScanEventBody::RunStarted {
            reason: RunReason::Submit,
            rerun_stages: domain::rules::stages::run_stages(None, false, false),
        },
        ScanEventBody::StageStarted {
            stage: Stage::PlantCheck,
        },
    ]
}

fn stored(diff: ObservationDiff) -> CompletionOutcome {
    CompletionOutcome::Stored {
        claim: ClaimAttempt::NotNeeded,
        diff,
    }
}

fn completed(stage: Stage) -> ScanEventBody<Stored> {
    ScanEventBody::StageCompleted {
        stage,
        duration_ms: 0,
        skipped: None,
    }
}

fn started(stage: Stage) -> ScanEventBody<Stored> {
    ScanEventBody::StageStarted { stage }
}

/// A store whose snapshot is `snapshot(guided)` and whose `complete_job` answers `outcome`.
fn store_completing(guided: bool, outcome: CompletionOutcome) -> FakeStore {
    let mut store = FakeStore::default();
    store
        .scan_repo
        .expect_snapshot()
        .returning(move |_| Ok(snapshot(guided)));
    store
        .job_repo
        .expect_complete_job()
        .times(1)
        .returning(move |_, _, _| Ok(outcome.clone()));
    store
}

/// A perception job whose stage events after `plant_check` started were lost: one `emit`,
/// with the missing stage events first and then the findings.
#[tokio::test]
async fn a_perception_job_sends_its_lost_stage_events_then_its_findings() {
    let mut store = store_completing(
        false,
        stored(ObservationDiff {
            added: vec![finding()],
            retracted: Vec::new(),
        }),
    );
    store
        .event_repo
        .expect_run_stage_events()
        .times(1)
        .returning(|_, _| Ok(lost_after_plant_check()));
    let writer = Arc::new(Recorder::default());
    let pipeline = pipeline(Arc::new(store), Arc::clone(&writer));

    let handled = pipeline
        .job_completed(
            &lease(),
            job(JobKind::Perception),
            done(serde_json::from_str(STUB_RESULT).unwrap()),
        )
        .await
        .expect("completed");

    assert!(matches!(handled.value, CompletionOutcome::Stored { .. }));
    assert_eq!(
        writer.calls(),
        [vec![
            completed(Stage::PlantCheck),
            started(Stage::Segmentation),
            completed(Stage::Segmentation),
            started(Stage::Species),
            completed(Stage::Species),
            started(Stage::Disease),
            completed(Stage::Disease),
            ScanEventBody::Finding {
                observation: finding()
            },
        ]]
    );
}

/// A repeated `complete` (the store answers `AlreadyCompleted`) sends nothing and reads no
/// stage events.
#[tokio::test]
async fn a_repeated_complete_sends_nothing() {
    let mut store = store_completing(false, CompletionOutcome::AlreadyCompleted);
    store.event_repo.expect_run_stage_events().never();
    let writer = Arc::new(Recorder::default());
    let pipeline = pipeline(Arc::new(store), Arc::clone(&writer));

    let handled = pipeline
        .job_completed(
            &lease(),
            job(JobKind::Perception),
            done(serde_json::from_str(STUB_RESULT).unwrap()),
        )
        .await
        .expect("completed");

    assert_eq!(handled.value, CompletionOutcome::AlreadyCompleted);
    assert_eq!(handled.follow_up, None);
    assert_eq!(writer.calls().len(), 0, "no emit");
}

/// When the run's stage events can't be read, the findings still go out, with no stage
/// events, and `complete` succeeds.
#[tokio::test]
async fn a_failed_stage_event_read_still_sends_the_findings() {
    let mut store = store_completing(
        false,
        stored(ObservationDiff {
            added: vec![finding()],
            retracted: Vec::new(),
        }),
    );
    store
        .event_repo
        .expect_run_stage_events()
        .times(1)
        .returning(|_, _| Err(StoreError::Unavailable("connection reset".into())));
    let writer = Arc::new(Recorder::default());
    let pipeline = pipeline(Arc::new(store), Arc::clone(&writer));

    pipeline
        .job_completed(
            &lease(),
            job(JobKind::Perception),
            done(serde_json::from_str(STUB_RESULT).unwrap()),
        )
        .await
        .expect("completed");

    assert_eq!(
        writer.calls(),
        [vec![ScanEventBody::Finding {
            observation: finding()
        }]]
    );
}

/// A `verify` job whose `stage_completed` was lost: its `verify` stage is closed.
#[tokio::test]
async fn a_verify_job_closes_its_verify_stage() {
    let mut store = store_completing(false, stored(ObservationDiff::default()));
    store
        .event_repo
        .expect_run_stage_events()
        .times(1)
        .returning(|_, _| {
            let mut events = lost_after_plant_check();
            events.push(started(Stage::Verify));
            Ok(events)
        });
    let writer = Arc::new(Recorder::default());
    let pipeline = pipeline(Arc::new(store), Arc::clone(&writer));

    pipeline
        .job_completed(
            &lease(),
            job(JobKind::Verify),
            done(serde_json::json!({ "indicators": { "answers": [] } })),
        )
        .await
        .expect("completed");

    assert_eq!(writer.calls(), [vec![completed(Stage::Verify)]]);
}

/// A guided check's completion sends `guided_check_done` and no stage events: the `guided`
/// stage is shared by the run's checks, so a catch-up couldn't tell them apart. It doesn't
/// read the run's stage events.
#[tokio::test]
async fn a_guided_completion_sends_no_stage_events() {
    let mut store = FakeStore::default();
    store
        .scan_repo
        .expect_snapshot()
        .returning(|_| Ok(snapshot(true)));
    store.job_repo.expect_complete_job().never();
    store.event_repo.expect_run_stage_events().never();
    store
        .analysis_repo
        .expect_finish_guided_check()
        .times(1)
        .returning(|_, _, _| {
            Ok(GuidedFinish {
                scan_status: ScanStatus::Planned,
                claim: ClaimAttempt::NotNeeded,
            })
        });
    let writer = Arc::new(Recorder::default());
    let pipeline = pipeline(Arc::new(store), Arc::clone(&writer));

    pipeline
        .job_completed(
            &lease(),
            job(JobKind::Guided),
            done(serde_json::json!({ "texture": {
                "kind": "jar_test", "settled": true, "texture": "loamy", "confidence": 0.65
            }})),
        )
        .await
        .expect("completed");

    let calls = writer.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(
        matches!(&calls[0][..], [ScanEventBody::GuidedCheckDone { check }]
            if check.status == GuidedCheckStatus::Done),
        "{calls:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// Perception masks ([contracts 5], `PerceptionRegion.mask`): each is written to object storage
// as a PNG before `complete_job` records it with the segmentation observations, and its region
// gets the `mask_id` that `GET /scans/{id}` signs as `mask_url`.

const LEAF_IMAGE: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5A";
const OTHER_IMAGE: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5B";
const MASK: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5M";
const QUESTION: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5Q";

fn scan_image(id: &str, width: u32, height: u32) -> domain::scan::ScanImage<Stored> {
    domain::scan::ScanImage {
        id: id.parse().unwrap(),
        kind: domain::scan::ImageKind::LeafCloseup,
        answers_question_id: None,
        width,
        height,
        url: domain::url::NoUrl,
        uploaded_at: now(),
        depth: None,
    }
}

/// A leaf mask on a 4 × 3 photo: the middle two columns of the lower two rows.
fn leaf_rle() -> domain::jobs::RleMask {
    let mut mask = vision::mask::MaskBuf::empty(4, 3);
    for (x, y) in [(1, 1), (2, 1), (1, 2), (2, 2)] {
        mask.set(x, y, true);
    }
    vision::rle::encode(mask.view())
}

/// Three leaf regions on two 4 × 3 photos: one with a mask, one without, and one whose mask
/// has another size than its photo.
fn masked_result() -> serde_json::Value {
    let region = |id: String, image: &str, mask: Option<domain::jobs::RleMask>| {
        let mut r = serde_json::json!({
            "region_id": id, "image_id": image, "class": "leaf",
            "bbox": [0.1, 0.1, 0.5, 0.5], "confidence": 0.9
        });
        if let Some(mask) = mask {
            r["mask"] = serde_json::to_value(mask).unwrap();
        }
        r
    };
    let other_size = domain::jobs::RleMask {
        size: [6, 8],
        counts: "P1".into(),
    };
    serde_json::json!({ "regions": [
        region(format!("{LEAF_IMAGE}.1"), LEAF_IMAGE, Some(leaf_rle())),
        region(format!("{LEAF_IMAGE}.2"), LEAF_IMAGE, None),
        region(format!("{OTHER_IMAGE}.1"), OTHER_IMAGE, Some(other_size)),
    ]})
}

fn snapshot_with_photos() -> ScanSnapshot {
    let mut snap = snapshot(false);
    snap.scan.images = vec![scan_image(LEAF_IMAGE, 4, 3), scan_image(OTHER_IMAGE, 4, 3)];
    snap
}

/// Every completion `complete_job` was given.
type Completions = Arc<Mutex<Vec<domain::ports::JobCompletion>>>;

/// A store whose lease is `live` (or already completed) and whose `complete_job` records the
/// completion and answers `outcome`.
fn store_recording(live: bool, outcome: CompletionOutcome, completions: &Completions) -> FakeStore {
    store_recording_on(snapshot_with_photos, live, outcome, completions)
}

/// [`store_recording`] with the snapshot `snap`.
fn store_recording_on(
    snap: fn() -> ScanSnapshot,
    live: bool,
    outcome: CompletionOutcome,
    completions: &Completions,
) -> FakeStore {
    let mut store = FakeStore::default();
    store
        .scan_repo
        .expect_snapshot()
        .returning(move |_| Ok(snap()));
    // Checked before the masks are encoded and, when live, again after their puts.
    store
        .job_repo
        .expect_live_lease()
        .times(if live { 2 } else { 1 })
        .returning(move |_, _| Ok(live.then(|| job(JobKind::Perception))));
    let completions = Arc::clone(completions);
    store
        .job_repo
        .expect_complete_job()
        .times(1)
        .returning(move |_, completion, _| {
            completions.lock().unwrap().push(completion);
            Ok(outcome.clone())
        });
    store
        .event_repo
        .expect_run_stage_events()
        .returning(|_, _| Ok(lost_after_plant_check()));
    store
}

/// The observation writes of the one completion recorded.
fn recorded_writes(completions: &Completions) -> Vec<domain::ports::ObservationWrite> {
    let completions = completions.lock().unwrap().clone();
    match &completions[..] {
        [domain::ports::JobCompletion::Done { observations, .. }] => observations.clone(),
        other => panic!("one `done` completion, got {other:?}"),
    }
}

#[tokio::test]
async fn a_perception_result_stores_its_masks_and_links_their_regions() {
    use domain::ports::BlobStore as _;
    let completions = Completions::default();
    let store = store_recording(true, stored(ObservationDiff::default()), &completions);
    let blobs = Arc::new(blob::MemoryBlobStore::new(Arc::new(FixedClock::at(now()))));
    let pipeline = pipeline_with_blobs(
        Arc::new(store),
        Arc::new(Recorder::default()),
        Arc::clone(&blobs) as Arc<dyn domain::ports::BlobStore>,
    );

    pipeline
        .job_completed(&lease(), job(JobKind::Perception), done(masked_result()))
        .await
        .expect("completed");

    let writes = recorded_writes(&completions);
    let [segmentation] = &writes[..] else {
        panic!("the segmentation write alone: {writes:?}")
    };
    assert_eq!(segmentation.stage, Stage::Segmentation);
    let [mask] = &segmentation.masks[..] else {
        panic!("one mask: {:?}", segmentation.masks)
    };
    assert_eq!(mask.region_id.to_string(), format!("{LEAF_IMAGE}.1"));
    assert_eq!(mask.image.to_string(), LEAF_IMAGE);
    assert_eq!(mask.rle, leaf_rle());
    assert_eq!(
        mask.storage_key,
        domain::ports::ObjectKey::mask(GARDEN.parse().unwrap(), SCAN.parse().unwrap(), mask.id)
    );
    let linked: Vec<_> = segmentation
        .observations
        .iter()
        .map(|o| {
            let region = o.region.as_ref().expect("a segmentation region");
            (
                region.region_id.expect("a region id").to_string(),
                region.mask.mask_id,
            )
        })
        .collect();
    assert_eq!(
        linked,
        [
            (format!("{LEAF_IMAGE}.1"), Some(mask.id)),
            (format!("{LEAF_IMAGE}.2"), None),
            (format!("{OTHER_IMAGE}.1"), None),
        ],
        "a region without a fitting mask keeps none"
    );
    let png = blobs
        .get_bytes(&mask.storage_key)
        .await
        .expect("the object is written");
    assert_eq!(png.as_ref(), vision::rle::to_png(&leaf_rle()).unwrap());
    let object = blobs.get(&mask.storage_key).await.unwrap();
    assert_eq!(object.content_type, "image/png");
    let all = blobs
        .list(&domain::ports::ObjectPrefix::scan(
            GARDEN.parse().unwrap(),
            SCAN.parse().unwrap(),
        ))
        .await
        .unwrap();
    assert_eq!(all.len(), 1, "one object, for the one stored mask");
}

/// A `complete` repeated after the job is done (the lease is no longer live) writes no object
/// and records no mask; the store answers `AlreadyCompleted`.
#[tokio::test]
async fn a_repeated_complete_writes_no_mask() {
    let completions = Completions::default();
    let store = store_recording(false, CompletionOutcome::AlreadyCompleted, &completions);
    let mut blobs = MockBlobStore::new();
    blobs.expect_put().never();
    let pipeline = pipeline_with_blobs(
        Arc::new(store),
        Arc::new(Recorder::default()),
        Arc::new(blobs),
    );

    let handled = pipeline
        .job_completed(&lease(), job(JobKind::Perception), done(masked_result()))
        .await
        .expect("completed");

    assert_eq!(handled.value, CompletionOutcome::AlreadyCompleted);
    let writes = recorded_writes(&completions);
    assert!(writes.iter().all(|w| w.masks.is_empty()));
    assert!(
        writes
            .iter()
            .flat_map(|w| &w.observations)
            .all(|o| o.region.as_ref().is_none_or(|r| r.mask.mask_id.is_none()))
    );
}

/// A put that fails fails the completion before anything is recorded, so the worker's retry
/// of `complete` writes the masks again.
#[tokio::test]
async fn a_failed_mask_put_fails_the_completion() {
    let mut store = FakeStore::default();
    store
        .scan_repo
        .expect_snapshot()
        .returning(|_| Ok(snapshot_with_photos()));
    store
        .job_repo
        .expect_live_lease()
        .returning(|_, _| Ok(Some(job(JobKind::Perception))));
    store
        .event_repo
        .expect_run_stage_events()
        .returning(|_, _| Ok(lost_after_plant_check()));
    store.job_repo.expect_complete_job().never();
    let mut blobs = MockBlobStore::new();
    blobs.expect_put().times(1).returning(|_, _, _| {
        Err(domain::ports::BlobError::Unavailable(
            "connection reset".into(),
        ))
    });
    let pipeline = pipeline_with_blobs(
        Arc::new(store),
        Arc::new(Recorder::default()),
        Arc::new(blobs),
    );

    let result = pipeline
        .job_completed(&lease(), job(JobKind::Perception), done(masked_result()))
        .await;

    assert!(
        matches!(result, Err(pipeline::PipelineError::Blob(_))),
        "{result:?}"
    );
}

/// A mask sized `[width, height]` instead of `[height, width]` on a photo that isn't square
/// doesn't fit it, though its pixel count does: it is left out, its region kept without it,
/// and nothing is written to object storage.
#[tokio::test]
async fn a_transposed_mask_is_left_out() {
    let completions = Completions::default();
    let mut store = FakeStore::default();
    store
        .scan_repo
        .expect_snapshot()
        .returning(|_| Ok(snapshot_with_photos()));
    store.job_repo.expect_live_lease().never();
    let recorded = Arc::clone(&completions);
    store
        .job_repo
        .expect_complete_job()
        .times(1)
        .returning(move |_, completion, _| {
            recorded.lock().unwrap().push(completion);
            Ok(stored(ObservationDiff::default()))
        });
    store
        .event_repo
        .expect_run_stage_events()
        .returning(|_, _| Ok(lost_after_plant_check()));
    let mut blobs = MockBlobStore::new();
    blobs.expect_put().never();
    let pipeline = pipeline_with_blobs(
        Arc::new(store),
        Arc::new(Recorder::default()),
        Arc::new(blobs),
    );
    // The leaf photo is 4 wide and 3 high; this mask is 3 wide and 4 high.
    let transposed = vision::rle::encode(vision::mask::MaskBuf::empty(3, 4).view());
    assert_eq!(transposed.size, [4, 3]);
    let result = serde_json::json!({ "regions": [{
        "region_id": format!("{LEAF_IMAGE}.1"), "image_id": LEAF_IMAGE, "class": "leaf",
        "bbox": [0.1, 0.1, 0.5, 0.5], "confidence": 0.9, "mask": transposed
    }]});

    pipeline
        .job_completed(&lease(), job(JobKind::Perception), done(result))
        .await
        .expect("completed");

    let writes = recorded_writes(&completions);
    let [segmentation] = &writes[..] else {
        panic!("the segmentation write alone: {writes:?}")
    };
    assert_eq!(segmentation.masks, []);
    assert!(
        segmentation
            .observations
            .iter()
            .all(|o| o.region.as_ref().is_some_and(|r| r.mask.mask_id.is_none()))
    );
}

/// A lease lost while the masks were put (a garden deletion cancelled the job, and its listing
/// may have missed them): the objects are deleted, nothing is recorded, and the worker gets
/// `lease_lost`.
#[tokio::test]
async fn masks_put_after_the_lease_is_lost_are_deleted() {
    let mut store = FakeStore::default();
    store
        .scan_repo
        .expect_snapshot()
        .returning(|_| Ok(snapshot_with_photos()));
    let checks = std::sync::atomic::AtomicUsize::new(0);
    store
        .job_repo
        .expect_live_lease()
        .times(2)
        .returning(move |_, _| {
            if checks.fetch_add(1, std::sync::atomic::Ordering::Relaxed) == 0 {
                Ok(Some(job(JobKind::Perception)))
            } else {
                Err(StoreError::Conflict(domain::ports::Conflict::LeaseLost {
                    job_id: JOB.parse().unwrap(),
                }))
            }
        });
    store
        .event_repo
        .expect_run_stage_events()
        .returning(|_, _| Ok(lost_after_plant_check()));
    store.job_repo.expect_complete_job().never();
    let blobs = Arc::new(blob::MemoryBlobStore::new(Arc::new(FixedClock::at(now()))));
    let pipeline = pipeline_with_blobs(
        Arc::new(store),
        Arc::new(Recorder::default()),
        Arc::clone(&blobs) as Arc<dyn domain::ports::BlobStore>,
    );

    let result = pipeline
        .job_completed(&lease(), job(JobKind::Perception), done(masked_result()))
        .await;

    assert!(
        matches!(
            result,
            Err(pipeline::PipelineError::Store(StoreError::Conflict(
                domain::ports::Conflict::LeaseLost { .. }
            )))
        ),
        "{result:?}"
    );
    let left = domain::ports::BlobStore::list(
        blobs.as_ref(),
        &domain::ports::ObjectPrefix::garden(GARDEN.parse().unwrap()),
    )
    .await
    .unwrap();
    assert_eq!(left, Vec::new(), "the mask put is deleted");
}

/// A `complete` repeated concurrently by the same lease: the other call finished the job
/// while this one put its masks, so the second lease check finds it done. The objects are
/// deleted and none is recorded, and the store answers `AlreadyCompleted`.
#[tokio::test]
async fn masks_put_while_a_duplicate_complete_finished_the_job_are_deleted() {
    let completions = Completions::default();
    let mut store = FakeStore::default();
    store
        .scan_repo
        .expect_snapshot()
        .returning(|_| Ok(snapshot_with_photos()));
    let checks = std::sync::atomic::AtomicUsize::new(0);
    store
        .job_repo
        .expect_live_lease()
        .times(2)
        .returning(move |_, _| {
            Ok(
                (checks.fetch_add(1, std::sync::atomic::Ordering::Relaxed) == 0)
                    .then(|| job(JobKind::Perception)),
            )
        });
    let recorded = Arc::clone(&completions);
    store
        .job_repo
        .expect_complete_job()
        .times(1)
        .returning(move |_, completion, _| {
            recorded.lock().unwrap().push(completion);
            Ok(CompletionOutcome::AlreadyCompleted)
        });
    store
        .event_repo
        .expect_run_stage_events()
        .returning(|_, _| Ok(lost_after_plant_check()));
    let blobs = Arc::new(blob::MemoryBlobStore::new(Arc::new(FixedClock::at(now()))));
    let pipeline = pipeline_with_blobs(
        Arc::new(store),
        Arc::new(Recorder::default()),
        Arc::clone(&blobs) as Arc<dyn domain::ports::BlobStore>,
    );

    let handled = pipeline
        .job_completed(&lease(), job(JobKind::Perception), done(masked_result()))
        .await
        .expect("completed");

    assert_eq!(handled.value, CompletionOutcome::AlreadyCompleted);
    let writes = recorded_writes(&completions);
    assert!(writes.iter().all(|w| w.masks.is_empty()));
    let left = domain::ports::BlobStore::list(
        blobs.as_ref(),
        &domain::ports::ObjectPrefix::garden(GARDEN.parse().unwrap()),
    )
    .await
    .unwrap();
    assert_eq!(left, Vec::new(), "the mask put is deleted");
}

/// A put that fails after another succeeded: the mask already put is deleted before the
/// completion fails, so the worker's retry, which writes under new ids, leaves no orphan.
#[tokio::test]
async fn a_failed_mask_put_deletes_the_masks_already_put() {
    let mut store = FakeStore::default();
    store
        .scan_repo
        .expect_snapshot()
        .returning(|_| Ok(snapshot_with_photos()));
    store
        .job_repo
        .expect_live_lease()
        .times(1)
        .returning(|_, _| Ok(Some(job(JobKind::Perception))));
    store
        .event_repo
        .expect_run_stage_events()
        .returning(|_, _| Ok(lost_after_plant_check()));
    store.job_repo.expect_complete_job().never();
    let put = Arc::new(Mutex::new(Vec::new()));
    let mut blobs = MockBlobStore::new();
    let puts = Arc::clone(&put);
    blobs.expect_put().times(2).returning(move |key, _, _| {
        let mut puts = puts.lock().unwrap();
        if puts.is_empty() {
            puts.push(key.clone());
            Ok(())
        } else {
            Err(domain::ports::BlobError::Unavailable(
                "connection reset".into(),
            ))
        }
    });
    let deleted = Arc::new(Mutex::new(Vec::new()));
    let on_delete = Arc::clone(&deleted);
    blobs.expect_delete_keys().times(1).returning(move |keys| {
        on_delete.lock().unwrap().extend_from_slice(keys);
        Ok(domain::ports::DeleteReport {
            deleted: keys.len() as u64,
            failed: Vec::new(),
        })
    });
    let pipeline = pipeline_with_blobs(
        Arc::new(store),
        Arc::new(Recorder::default()),
        Arc::new(blobs),
    );
    // Two masks that fit: one on each 4 × 3 photo.
    let region = |image: &str| {
        serde_json::json!({
            "region_id": format!("{image}.1"), "image_id": image, "class": "leaf",
            "bbox": [0.1, 0.1, 0.5, 0.5], "confidence": 0.9, "mask": leaf_rle()
        })
    };
    let regions = serde_json::json!({ "regions": [region(LEAF_IMAGE), region(OTHER_IMAGE)] });

    let result = pipeline
        .job_completed(&lease(), job(JobKind::Perception), done(regions))
        .await;

    assert!(
        matches!(result, Err(pipeline::PipelineError::Blob(_))),
        "{result:?}"
    );
    let put = put.lock().unwrap().clone();
    assert_eq!(put.len(), 1, "the first put succeeded");
    assert_eq!(*deleted.lock().unwrap(), put, "and its object is deleted");
}

/// A species-change run's job, whose stages leave segmentation out ([contracts 3],
/// observations across runs): the worker passes the earlier run's regions through without
/// masks, at confidence 1. The completion writes no segmentation, so the store keeps the
/// earlier observations with their `mask_id`s and their masks; no object is written and the
/// lease isn't checked for masks. The job's stages decide, so this holds with no
/// `run_started` stored for the run (its emit, after `start_run`, may have failed).
#[tokio::test]
async fn a_species_change_run_keeps_the_earlier_regions_and_masks() {
    let completions = Completions::default();
    let store = store_without_run_started(&completions, 0, 1);
    let mut blobs = MockBlobStore::new();
    blobs.expect_put().never();
    let pipeline = pipeline_with_blobs(
        Arc::new(store),
        Arc::new(Recorder::default()),
        Arc::new(blobs),
    );
    // The earlier run's regions as the worker passes them through, with the stub's species
    // and disease.
    let mut result: serde_json::Value = serde_json::from_str(STUB_RESULT).unwrap();
    result
        .as_object_mut()
        .unwrap()
        .remove("plant_present")
        .expect("the stub has a plant check");
    for region in result["regions"].as_array_mut().unwrap() {
        region.as_object_mut().unwrap().remove("mask");
        region["confidence"] = serde_json::json!(1.0);
    }
    assert_eq!(
        result["regions"].as_array().unwrap().len(),
        4,
        "the stub's four regions"
    );

    pipeline
        .job_completed(&lease(), species_change_job(), done(result))
        .await
        .expect("completed");

    let writes = recorded_writes(&completions);
    let stages: Vec<_> = writes.iter().map(|w| w.stage).collect();
    assert_eq!(
        stages,
        [Stage::Species, Stage::Disease],
        "no segmentation write, so no mask is deleted"
    );
    assert!(writes.iter().all(|w| w.masks.is_empty()));
}

/// A species-change run's perception job, as the store reads it: its stages are the run's
/// (`run_stages` after a done perception base and a measurement), without segmentation.
fn species_change_job() -> LeasedJob {
    let earlier = domain::rules::stages::EarlierRun {
        perception_base_done: true,
        measured: true,
    };
    let stages = domain::rules::stages::run_stages(Some(earlier), false, false);
    let perception = domain::rules::stages::perception_stages(&stages);
    assert!(!perception.contains(&domain::jobs::PerceptionStage::Segmentation));
    LeasedJob {
        segmentation: perception.contains(&domain::jobs::PerceptionStage::Segmentation),
        ..job(JobKind::Perception)
    }
}

/// A store whose run has no `run_started` stored, with the leaf photo answering the question
/// and the other photo the run's; it expects `leases` lease checks and records `completions`
/// completions.
fn store_without_run_started(completions: &Completions, leases: usize, times: usize) -> FakeStore {
    let mut store = FakeStore::default();
    store.scan_repo.expect_snapshot().returning(|_| {
        let mut snap = snapshot_with_photos();
        snap.scan.images[0].answers_question_id = Some(QUESTION.parse().unwrap());
        Ok(snap)
    });
    store
        .job_repo
        .expect_live_lease()
        .times(leases)
        .returning(|_, _| Ok(Some(job(JobKind::Perception))));
    let recorded = Arc::clone(completions);
    store
        .job_repo
        .expect_complete_job()
        .times(times)
        .returning(move |_, completion, _| {
            recorded.lock().unwrap().push(completion);
            Ok(stored(ObservationDiff::default()))
        });
    store
        .event_repo
        .expect_run_stage_events()
        .returning(|_, _| Ok(Vec::new()));
    store
}

/// A question's photo is segmented in a species-change run whose own job left segmentation
/// out: the run's job writes no segmentation, and the question's job (whose stages are
/// segmentation alone) stores its regions and masks, on its photo alone.
#[tokio::test]
async fn a_question_photo_is_segmented_in_a_species_change_run() {
    let completions = Completions::default();
    let store = store_without_run_started(&completions, 2, 2);
    let blobs = Arc::new(blob::MemoryBlobStore::new(Arc::new(FixedClock::at(now()))));
    let pipeline = pipeline_with_blobs(
        Arc::new(store),
        Arc::new(Recorder::default()),
        Arc::clone(&blobs) as Arc<dyn domain::ports::BlobStore>,
    );
    // The run's job: the earlier run's region on the run's photo, passed through.
    let passed_through = serde_json::json!({ "regions": [{
        "region_id": format!("{OTHER_IMAGE}.1"), "image_id": OTHER_IMAGE, "class": "leaf",
        "bbox": [0.1, 0.1, 0.5, 0.5], "confidence": 1.0
    }]});
    pipeline
        .job_completed(&lease(), species_change_job(), done(passed_through))
        .await
        .expect("the run's job completed");
    // The question's job, segmentation alone on the question's photo.
    let question_job = LeasedJob {
        question: Some(QUESTION.parse().unwrap()),
        segmentation: true,
        ..job(JobKind::Perception)
    };
    let segmented = serde_json::json!({ "regions": [{
        "region_id": format!("{LEAF_IMAGE}.1"), "image_id": LEAF_IMAGE, "class": "leaf",
        "bbox": [0.1, 0.1, 0.5, 0.5], "confidence": 0.9, "mask": leaf_rle()
    }]});
    pipeline
        .job_completed(&lease(), question_job, done(segmented))
        .await
        .expect("the question's job completed");

    let completions = completions.lock().unwrap().clone();
    let [
        domain::ports::JobCompletion::Done {
            observations: run, ..
        },
        domain::ports::JobCompletion::Done {
            observations: question,
            ..
        },
    ] = &completions[..]
    else {
        panic!("two `done` completions, got {completions:?}")
    };
    assert!(
        run.iter().all(|w| w.stage != Stage::Segmentation),
        "the run's job keeps the earlier segmentation: {run:?}"
    );
    let [segmentation] = &question[..] else {
        panic!("the question's segmentation write alone: {question:?}")
    };
    assert_eq!(segmentation.stage, Stage::Segmentation);
    assert_eq!(
        segmentation.images,
        [LEAF_IMAGE.parse::<ImageId>().unwrap()]
    );
    assert!(!segmentation.replace_imageless);
    let [mask] = &segmentation.masks[..] else {
        panic!("one mask: {:?}", segmentation.masks)
    };
    assert_eq!(mask.image.to_string(), LEAF_IMAGE);
    let all = domain::ports::BlobStore::list(
        blobs.as_ref(),
        &domain::ports::ObjectPrefix::garden(GARDEN.parse().unwrap()),
    )
    .await
    .unwrap();
    assert_eq!(all.len(), 1, "one object, for the question photo's mask");
}

/// A question photo's `partial_result` covers that photo alone and replaces nothing tied to
/// no image, as the job's `complete` does ([contracts 3]): the run's segmentation and masks
/// on the other photos stay.
#[tokio::test]
async fn a_question_photos_partial_result_covers_that_photo_alone() {
    let mut store = FakeStore::default();
    store.scan_repo.expect_snapshot().returning(|_| {
        let mut snap = snapshot_with_photos();
        snap.scan.images[1].answers_question_id = Some(QUESTION.parse().unwrap());
        Ok(snap)
    });
    let writes = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&writes);
    store
        .analysis_repo
        .expect_store_partial()
        .times(2)
        .returning(move |_, write| {
            recorded.lock().unwrap().push(write);
            Ok(ObservationDiff::default())
        });
    let pipeline = pipeline(Arc::new(store), Arc::new(Recorder::default()));
    let regions: Vec<domain::jobs::PerceptionRegion> =
        serde_json::from_value(masked_result()["regions"].clone()).unwrap();
    let partial = |question| {
        let mut job = job(JobKind::Perception);
        job.question = question;
        let events = vec![domain::jobs::WorkerEvent::PartialResult {
            stage: Stage::Segmentation,
            result: domain::jobs::PartialResult::Segmentation {
                regions: regions.clone(),
            },
        }];
        (job, events)
    };

    let (question_job, events) = partial(Some(QUESTION.parse().unwrap()));
    pipeline
        .worker_events(&lease(), question_job, events)
        .await
        .expect("stored");
    let (run_job, events) = partial(None);
    pipeline
        .worker_events(&lease(), run_job, events)
        .await
        .expect("stored");

    let writes = writes.lock().unwrap().clone();
    let covered: Vec<_> = writes
        .iter()
        .map(|w: &domain::ports::ObservationWrite| {
            (
                w.images.iter().map(ToString::to_string).collect::<Vec<_>>(),
                w.replace_imageless,
            )
        })
        .collect();
    assert_eq!(
        covered,
        [
            (vec![OTHER_IMAGE.to_owned()], false),
            (vec![LEAF_IMAGE.to_owned()], true),
        ],
        "the question's photo alone, then the run's photos without it"
    );
}

/// `GET /scans/{id}` signs a stored mask as `mask_url`, and that URL serves the PNG.
#[tokio::test]
async fn a_stored_mask_is_signed_on_the_scan_and_served_as_png() {
    use futures::StreamExt as _;
    use http_body_util::BodyExt as _;
    let mask_id: domain::id::MaskId = MASK.parse().unwrap();
    let key =
        domain::ports::ObjectKey::mask(GARDEN.parse().unwrap(), SCAN.parse().unwrap(), mask_id);
    let png = vision::rle::to_png(&leaf_rle()).unwrap();
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Viewer));
    store
        .scan_repo
        .expect_scan_detail()
        .times(1)
        .returning(move |_, _| {
            let snap = snapshot_with_photos();
            let mut leaf = finding();
            leaf.region = Some(Region {
                region_id: Some(format!("{LEAF_IMAGE}.1").parse().unwrap()),
                image_id: LEAF_IMAGE.parse().unwrap(),
                bbox: BBox::FULL,
                mask: domain::url::StoredMask {
                    mask_id: Some(mask_id),
                },
            });
            Ok(domain::scan::ScanDetail {
                scan: snap.scan,
                plant: snap.plant,
                observations: vec![leaf],
                weather: None,
                diagnosis: None,
                plan: None,
            })
        });
    let stored_key = key.clone();
    store
        .scan_repo
        .expect_mask_object()
        .times(1)
        .returning(move |scan, mask| {
            assert_eq!((scan.to_string(), mask), (SCAN.to_owned(), mask_id));
            Ok(Some(domain::ports::ObjectRef {
                key: stored_key.clone(),
                content_type: domain::ports::ContentType::Png,
            }))
        });
    let mut blob = MockBlobStore::new();
    let bytes = bytes::Bytes::from(png.clone());
    blob.expect_get()
        .withf(move |k| *k == key)
        .times(1)
        .returning(move |_| {
            let body = bytes.clone();
            Ok(domain::ports::BlobObject {
                content_type: "image/png".to_owned(),
                length: Some(body.len() as u64),
                etag: None,
                body: futures::stream::once(async move { Ok(body) }).boxed(),
            })
        });
    let app = crate::router::app(super::state(store, auth, blob, no_images()));

    let response = app
        .clone()
        .oneshot(super::get(&format!("/scans/{SCAN}")))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    let region = &body["observations"][0]["region"];
    assert!(region.get("mask_id").is_none(), "{region}");
    let url = region["mask_url"].as_str().expect("a signed mask URL");
    let path = url
        .strip_prefix("https://api.test")
        .expect("on the public base");
    assert!(
        path.starts_with(&format!("/scans/{SCAN}/masks/{MASK}?exp=")),
        "{path}"
    );

    let response = app
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
    let served = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(served.as_ref(), png.as_slice());
}

// ---------------------------------------------------------------------------------------------
// Species ids on user writes ([contracts 9], plants): the store refuses one the knowledge base
// doesn't hold with `invalid_request` at the request's field.

fn unknown_species(field: &'static str) -> StoreError {
    StoreError::Rejected(domain::error::DomainError::invalid(
        field,
        "not a species of the knowledge base",
    ))
}

fn tomato() -> SpeciesRef {
    SpeciesRef {
        id: "tomato".parse().unwrap(),
        scientific_name: "Solanum lycopersicum".into(),
        common_name: "Tomato".into(),
    }
}

fn post(uri: &str, body: &serde_json::Value) -> Request<Body> {
    Request::post(uri)
        .header(header::AUTHORIZATION, "Bearer session-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// The router with the real pipeline over the same store.
fn app_with_pipeline(store: FakeStore, auth: domain::ports::MockAuthVerifier) -> axum::Router {
    let state = super::state_with_pipeline(store, auth, config(), |store| {
        Arc::new(pipeline(store, Arc::new(Recorder::default())))
    });
    crate::router::app(state)
}

#[tokio::test]
async fn creating_a_plant_with_an_unknown_species_is_400() {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Editor));
    store
        .plant_repo
        .expect_create_plant()
        .times(1)
        .returning(|_, new| {
            assert_eq!(
                new.species.map(|s| s.to_string()).as_deref(),
                Some("nosuch")
            );
            Err(unknown_species("species_id"))
        });
    let app = crate::router::app(super::state(store, auth, MockBlobStore::new(), no_images()));

    let response = app
        .oneshot(post(
            "/plants",
            &serde_json::json!({ "garden_id": GARDEN, "name": "Basil", "species_id": "nosuch" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json(response).await;
    assert_eq!(body["error"]["code"], "invalid_request");
    assert_eq!(body["error"]["retryable"], false);
    assert_eq!(body["error"]["details"]["field"], "species_id");
}

#[tokio::test]
async fn creating_a_plant_with_a_known_species_is_201() {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Editor));
    store
        .plant_repo
        .expect_create_plant()
        .times(1)
        .returning(|_, _| Ok(plant(Some(tomato()))));
    let app = crate::router::app(super::state(store, auth, MockBlobStore::new(), no_images()));

    let response = app
        .oneshot(post(
            "/plants",
            &serde_json::json!({ "garden_id": GARDEN, "name": "Basil", "species_id": "tomato" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json(response).await;
    assert_eq!(body["species"]["id"], "tomato");
    assert_eq!(body["species_status"], "confirmed");
}

#[tokio::test]
async fn confirming_an_unknown_species_is_400() {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Editor));
    store
        .plant_repo
        .expect_confirm_species()
        .times(1)
        .returning(|_, _, choice| {
            assert_eq!(choice, SpeciesChoice::Known("nosuch".parse().unwrap()));
            Err(unknown_species("species_id"))
        });
    let app = app_with_pipeline(store, auth);

    let response = app
        .oneshot(post(
            &format!("/plants/{PLANT}/species"),
            &serde_json::json!({ "species_id": "nosuch" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json(response).await;
    assert_eq!(body["error"]["code"], "invalid_request");
    assert_eq!(body["error"]["details"]["field"], "species_id");
}

#[tokio::test]
async fn confirming_a_known_species_is_200() {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Editor));
    store
        .plant_repo
        .expect_confirm_species()
        .times(1)
        .returning(|_, _, _| {
            Ok(SpeciesConfirmation {
                plant: plant(Some(tomato())),
                recent_scan: None,
            })
        });
    let app = app_with_pipeline(store, auth);

    let response = app
        .oneshot(post(
            &format!("/plants/{PLANT}/species"),
            &serde_json::json!({ "species_id": "tomato" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["species"]["id"], "tomato");
}

// ---------------------------------------------------------------------------------------------
// The run's working species ([architecture 6.2], step 4; [contracts 3], species confirmation):
// the suggestion after the VLM's findings makes its best candidate the working species of the
// run it came from, so confirming that species keeps the run and its question, and confirming
// another one starts a new run. The deployed control plane's bug of 2026-10-23: the suggestion
// never reached the scan, so confirming the suggested tomato re-ran the scan.

const QUESTION: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5Q";

fn species(id: &str) -> SpeciesId {
    id.parse().unwrap()
}

fn pepper() -> SpeciesRef {
    SpeciesRef {
        id: species("pepper"),
        scientific_name: "Capsicum annuum".into(),
        common_name: "Pepper".into(),
    }
}

/// `BioCLIP`'s tomato pick, 0.967, in run `run`.
fn bioclip_tomato(run: u32) -> Observation<Stored> {
    Observation {
        run: Run(run),
        stage: ObservationStage::Species,
        pipeline_stage: Stage::Species,
        label: "Tomato".into(),
        knowledge_ref: Some(KnowledgeRef::Species(species("tomato"))),
        source: ObservationSource::Bioclip,
        confidence: Confidence::new(0.967).unwrap().into(),
        ..finding()
    }
}

/// A `vlm` result whose findings name tomato too, with no indicator answers.
fn vlm_result() -> serde_json::Value {
    serde_json::json!({
        "findings": {
            "species_guess": { "species_id": "tomato", "confidence": 0.85 },
            "soil": [], "organisms": [], "damage": [], "notes": ""
        },
        "indicators": { "answers": [] }
    })
}

/// A store completing run 1's `vlm` job while the scan, `analysing`, is in run `current`
/// with `BioCLIP`'s tomato pick.
fn store_completing_vlm(current: u32) -> FakeStore {
    let mut store = FakeStore::default();
    store.scan_repo.expect_snapshot().returning(move |_| {
        let mut snap = snapshot(false);
        snap.scan.run = Run(current);
        snap.observations = vec![bioclip_tomato(current)];
        Ok(snap)
    });
    store
        .job_repo
        .expect_complete_job()
        .times(1)
        .returning(|_, _, _| Ok(stored(ObservationDiff::default())));
    store
        .event_repo
        .expect_run_stage_events()
        .returning(|_, _| Ok(Vec::new()));
    store
}

/// The working species of every `species_suggested` sent.
fn suggested(calls: &[Vec<ScanEventBody<Stored>>]) -> Vec<Option<SpeciesId>> {
    calls
        .iter()
        .flatten()
        .filter_map(|e| match e {
            ScanEventBody::SpeciesSuggested {
                working_species_id, ..
            } => Some(working_species_id.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_suggestion_sets_the_working_species_of_its_run() {
    let mut store = store_completing_vlm(1);
    store
        .plant_repo
        .expect_suggest_species()
        .withf(|scan, run, candidates, working, status| {
            *scan == SCAN.parse::<ScanId>().unwrap()
                && *run == Run(1)
                && candidates.len() == 1
                && working.as_ref() == Some(&species("tomato"))
                && *status == SpeciesStatus::Suggested
        })
        .times(1)
        .returning(|_, _, _, _, _| Ok(true));
    let writer = Arc::new(Recorder::default());
    let pipeline = pipeline(Arc::new(store), Arc::clone(&writer));

    pipeline
        .job_completed(&lease(), job(JobKind::Vlm), done(vlm_result()))
        .await
        .expect("completed");

    assert_eq!(suggested(&writer.calls()), [Some(species("tomato"))]);
}

/// Run 1's `vlm` job completing after the scan moved to run 2 suggests nothing: no write,
/// no event. A suggestion the store finds stale (a run started in between) sends no event.
#[tokio::test]
async fn a_stale_suggestion_is_ignored() {
    let mut store = store_completing_vlm(2);
    store.plant_repo.expect_suggest_species().never();
    let writer = Arc::new(Recorder::default());
    pipeline(Arc::new(store), Arc::clone(&writer))
        .job_completed(&lease(), job(JobKind::Vlm), done(vlm_result()))
        .await
        .expect("completed");
    assert_eq!(suggested(&writer.calls()), []);

    let mut store = store_completing_vlm(1);
    store
        .plant_repo
        .expect_suggest_species()
        .times(1)
        .returning(|_, _, _, _, _| Ok(false));
    let writer = Arc::new(Recorder::default());
    pipeline(Arc::new(store), Arc::clone(&writer))
        .job_completed(&lease(), job(JobKind::Vlm), done(vlm_result()))
        .await
        .expect("completed");
    assert_eq!(suggested(&writer.calls()), []);
}

/// The router with the real pipeline, its events recorded by `writer`.
fn app_recording(
    store: FakeStore,
    auth: domain::ports::MockAuthVerifier,
    writer: Arc<Recorder>,
) -> axum::Router {
    let state = super::state_with_pipeline(store, auth, config(), move |store| {
        Arc::new(pipeline(store, writer))
    });
    crate::router::app(state)
}

/// The plant after the store confirmed `choice`.
fn confirmed_plant(choice: &SpeciesChoice) -> Plant<Stored> {
    match choice {
        SpeciesChoice::Known(id) => plant(Some(if *id == species("pepper") {
            pepper()
        } else {
            tomato()
        })),
        SpeciesChoice::Custom(name) => Plant {
            species_status: SpeciesStatus::Confirmed,
            custom_species_name: Some(name.clone()),
            ..plant(None)
        },
    }
}

/// The scan after run 1 suggested a species and asked an indicator question, with `working`
/// as the run's working species, and the store confirming `chosen`.
fn store_confirming(
    chosen: SpeciesChoice,
    working: Option<SpeciesId>,
) -> (FakeStore, domain::ports::MockAuthVerifier) {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Editor));
    let recent_working = working.clone();
    store
        .plant_repo
        .expect_confirm_species()
        .times(1)
        .returning(move |_, _, choice| {
            assert_eq!(choice, chosen);
            Ok(SpeciesConfirmation {
                plant: confirmed_plant(&choice),
                recent_scan: Some(RecentScan {
                    scan: SCAN.parse().unwrap(),
                    status: ScanStatus::AwaitingAnswer,
                    created_at: now(),
                    working_species: recent_working.clone(),
                }),
            })
        });
    store.scan_repo.expect_snapshot().returning(move |_| {
        let mut snap = snapshot(false);
        snap.scan.status = ScanStatus::AwaitingAnswer;
        snap.scan.open_question = Some(Question {
            id: QUESTION.parse().unwrap(),
            run: Run(1),
            kind: QuestionKind::Indicator,
            text: "Are there concentric rings in the spots?".into(),
            options: None,
            indicator_id: None,
            photo_kind: None,
            photo_hint: None,
            candidates: Vec::new(),
            asked_at: now(),
            continue_at: now(),
            status: QuestionStatus::Open,
        });
        snap.working_species.clone_from(&working);
        snap.observations = vec![bioclip_tomato(1)];
        Ok(snap)
    });
    (store, auth)
}

/// Completes run 1's `vlm` job and returns the working species the pipeline gave the store's
/// `suggest_species` (here the fake's record; the write itself is the store test
/// `a_suggestion_sets_its_runs_working_species`): tomato, the best candidate.
async fn suggested_working_species() -> Option<SpeciesId> {
    let recorded: Arc<Mutex<Option<SpeciesId>>> = Arc::default();
    let mut store = store_completing_vlm(1);
    let write = Arc::clone(&recorded);
    store
        .plant_repo
        .expect_suggest_species()
        .times(1)
        .returning(move |_, _, _, working, _| {
            *write.lock().unwrap() = working;
            Ok(true)
        });
    pipeline(Arc::new(store), Arc::new(Recorder::default()))
        .job_completed(&lease(), job(JobKind::Vlm), done(vlm_result()))
        .await
        .expect("completed");
    let working = recorded.lock().unwrap().clone();
    assert_eq!(
        working,
        Some(species("tomato")),
        "the suggestion was stored"
    );
    working
}

/// Run 1's suggestion, then confirming the suggested species: the working species the
/// confirmation compares with is the one the suggestion's store write recorded (here the
/// fake's record of `suggest_species`; the write itself is the store test
/// `a_suggestion_sets_its_runs_working_species`), so the run and its question are kept.
#[tokio::test]
async fn confirming_the_suggested_species_keeps_the_run_and_its_question() {
    let working = suggested_working_species().await;

    let (mut store, auth) = store_confirming(SpeciesChoice::Known(species("tomato")), working);
    store.scan_repo.expect_start_run().never();
    let writer = Arc::new(Recorder::default());

    let response = app_recording(store, auth, Arc::clone(&writer))
        .oneshot(post(
            &format!("/plants/{PLANT}/species"),
            &serde_json::json!({ "species_id": "tomato" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        writer.calls(),
        [vec![ScanEventBody::SpeciesConfirmed {
            species_id: Some(species("tomato")),
            custom_name: None,
            rerun: false,
        }]]
    );
}

#[tokio::test]
async fn confirming_another_species_starts_a_run() {
    let (mut store, auth) = store_confirming(
        SpeciesChoice::Known(species("pepper")),
        Some(species("tomato")),
    );
    store
        .job_repo
        .expect_live_workers()
        .returning(|_| Ok(Vec::new()));
    store
        .scan_repo
        .expect_start_run()
        .withf(|_, _, start| {
            start.reason == RunReason::SpeciesChanged
                && start.expected_status == ScanStatus::AwaitingAnswer
                && start.expected_run == Run(1)
                && start.working_species == Some(species("pepper"))
        })
        .times(1)
        .returning(|_, _, _| {
            Ok(StartedRun {
                run: Run(2),
                from: ScanStatus::AwaitingAnswer,
                to: ScanStatus::Analysing,
                superseded_question: Some(QUESTION.parse().unwrap()),
                cancelled_jobs: Vec::new(),
            })
        });
    let writer = Arc::new(Recorder::default());

    let response = app_recording(store, auth, Arc::clone(&writer))
        .oneshot(post(
            &format!("/plants/{PLANT}/species"),
            &serde_json::json!({ "species_id": "pepper" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let calls = writer.calls();
    assert!(
        matches!(&calls[..], [events] if matches!(&events[..], [
            ScanEventBody::SpeciesConfirmed { rerun: true, .. },
            ScanEventBody::RunStarted { reason: RunReason::SpeciesChanged, .. },
            ScanEventBody::QuestionResolved { by: ResolvedBy::Superseded, .. },
            ScanEventBody::StatusChanged { to: ScanStatus::Analysing, .. },
        ])),
        "{calls:?}"
    );
}

/// "Something else" after a suggestion ([contracts 3]): the custom name differs from the run's
/// working species, so a run starts without a species, skipping species identification, and
/// supersedes the open question. The working species is the one run 1's suggestion recorded,
/// as in the test above: before the suggestion set it, the run had none, a custom name
/// compared equal to it, and no run started.
#[tokio::test]
async fn confirming_a_custom_species_after_a_suggestion_starts_a_run() {
    let working = suggested_working_species().await;
    let custom = SpeciesChoice::Custom("Heirloom tomato".into());
    let (mut store, auth) = store_confirming(custom, working);
    store
        .job_repo
        .expect_live_workers()
        .returning(|_| Ok(Vec::new()));
    store
        .scan_repo
        .expect_start_run()
        .withf(|_, _, start| {
            start.reason == RunReason::SpeciesChanged
                && start.expected_run == Run(1)
                && start.working_species.is_none()
                && matches!(&start.first_job.spec, JobSpec::Perception(p)
                    if p.species_id.is_none() && p.skip_species_identification)
        })
        .times(1)
        .returning(|_, _, _| {
            Ok(StartedRun {
                run: Run(2),
                from: ScanStatus::AwaitingAnswer,
                to: ScanStatus::Analysing,
                superseded_question: Some(QUESTION.parse().unwrap()),
                cancelled_jobs: Vec::new(),
            })
        });
    let writer = Arc::new(Recorder::default());

    let response = app_recording(store, auth, Arc::clone(&writer))
        .oneshot(post(
            &format!("/plants/{PLANT}/species"),
            &serde_json::json!({ "custom_name": "Heirloom tomato" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let calls = writer.calls();
    assert!(
        matches!(&calls[..], [events] if matches!(&events[..], [
            ScanEventBody::SpeciesConfirmed { species_id: None, custom_name: Some(_), rerun: true },
            ScanEventBody::RunStarted { reason: RunReason::SpeciesChanged, .. },
            ScanEventBody::QuestionResolved { by: ResolvedBy::Superseded, .. },
            ScanEventBody::StatusChanged { to: ScanStatus::Analysing, .. },
        ])),
        "{calls:?}"
    );
}

/// A species confirmed while the scan is `queued` ([contracts 3]: the run uses the new
/// species): run 1 started without it, so the pipeline has the store give that run the
/// plant's confirmed species (`adopt_confirmed_species`; the writes are the store tests
/// `a_queued_confirmation_becomes_its_runs_working_species` and
/// `a_queued_confirmation_rewrites_its_queued_perception_job`), and no run starts.
#[tokio::test]
async fn confirming_while_queued_sets_the_runs_working_species() {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Editor));
    store
        .plant_repo
        .expect_confirm_species()
        .times(1)
        .returning(|_, _, _| {
            Ok(SpeciesConfirmation {
                plant: plant(Some(pepper())),
                recent_scan: Some(RecentScan {
                    scan: SCAN.parse().unwrap(),
                    status: ScanStatus::Queued,
                    created_at: now(),
                    working_species: None,
                }),
            })
        });
    store.scan_repo.expect_snapshot().returning(|_| {
        let mut snap = snapshot(false);
        snap.scan.status = ScanStatus::Queued;
        snap.plant = plant(Some(pepper()));
        Ok(snap)
    });
    store
        .plant_repo
        .expect_adopt_confirmed_species()
        .withf(|scan, run| *scan == SCAN.parse::<ScanId>().unwrap() && *run == Run(1))
        .times(1)
        .returning(|_, _| Ok(true));
    store.scan_repo.expect_start_run().never();
    let writer = Arc::new(Recorder::default());

    let response = app_recording(store, auth, Arc::clone(&writer))
        .oneshot(post(
            &format!("/plants/{PLANT}/species"),
            &serde_json::json!({ "species_id": "pepper" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        writer.calls(),
        [vec![ScanEventBody::SpeciesConfirmed {
            species_id: Some(species("pepper")),
            custom_name: None,
            rerun: false,
        }]]
    );
}

/// The store for a confirmation of pepper while the scan is `queued`: `confirm_species` sees
/// the scan `queued`, and the snapshots answer `snaps` in order (the last one again after
/// that), each a scan in run 1 with no working species.
fn store_confirming_while_queued(
    snaps: Vec<ScanStatus>,
) -> (FakeStore, domain::ports::MockAuthVerifier) {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Editor));
    store
        .plant_repo
        .expect_confirm_species()
        .times(1)
        .returning(|_, _, _| {
            Ok(SpeciesConfirmation {
                plant: plant(Some(pepper())),
                recent_scan: Some(RecentScan {
                    scan: SCAN.parse().unwrap(),
                    status: ScanStatus::Queued,
                    created_at: now(),
                    working_species: None,
                }),
            })
        });
    let read = Arc::new(Mutex::new(0_usize));
    store.scan_repo.expect_snapshot().returning(move |_| {
        let mut n = read.lock().unwrap();
        let mut snap = snapshot(false);
        snap.scan.status = snaps[(*n).min(snaps.len() - 1)];
        snap.plant = plant(Some(pepper()));
        *n += 1;
        Ok(snap)
    });
    (store, auth)
}

/// A worker claims the `queued` scan's `perception` job between the confirmation's snapshot
/// and `adopt_confirmed_species`, so the store refuses the adopt (the scan is `analysing`) and
/// the job runs without the species. The pipeline decides again on the scan as it now is:
/// pepper differs from the run's working species, so a species-change run starts with it,
/// instead of a `rerun: false` that would drop the confirmation.
#[tokio::test]
async fn a_claim_before_the_queued_adopt_starts_a_run_with_the_species() {
    let (mut store, auth) =
        store_confirming_while_queued(vec![ScanStatus::Queued, ScanStatus::Analysing]);
    store
        .plant_repo
        .expect_adopt_confirmed_species()
        .withf(|scan, run| *scan == SCAN.parse::<ScanId>().unwrap() && *run == Run(1))
        .times(1)
        .returning(|_, _| Ok(false));
    store
        .job_repo
        .expect_live_workers()
        .returning(|_| Ok(Vec::new()));
    store
        .scan_repo
        .expect_start_run()
        .withf(|_, _, start| {
            start.reason == RunReason::SpeciesChanged
                && start.expected_status == ScanStatus::Analysing
                && start.expected_run == Run(1)
                && start.working_species == Some(species("pepper"))
                && matches!(&start.first_job.spec, JobSpec::Perception(p)
                    if p.species_id == Some(species("pepper")) && !p.skip_species_identification)
        })
        .times(1)
        .returning(|_, _, _| {
            Ok(StartedRun {
                run: Run(2),
                from: ScanStatus::Analysing,
                to: ScanStatus::Analysing,
                superseded_question: None,
                cancelled_jobs: Vec::new(),
            })
        });
    let writer = Arc::new(Recorder::default());

    let response = app_recording(store, auth, Arc::clone(&writer))
        .oneshot(post(
            &format!("/plants/{PLANT}/species"),
            &serde_json::json!({ "species_id": "pepper" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let calls = writer.calls();
    assert!(
        matches!(&calls[..], [events] if matches!(&events[..], [
            ScanEventBody::SpeciesConfirmed { rerun: true, .. },
            ScanEventBody::RunStarted { reason: RunReason::SpeciesChanged, .. },
        ])),
        "{calls:?}"
    );
}

/// An adopt refused while every snapshot still reads `queued` (the run changing under each
/// try) is tried 3 times (`MAX_ADOPT_ATTEMPTS`), then answered as a retryable
/// `503 upstream_unavailable`, with no event sent as if the species had been adopted.
#[tokio::test]
async fn an_adopt_refused_every_time_is_a_retryable_error() {
    let (mut store, auth) = store_confirming_while_queued(vec![ScanStatus::Queued]);
    store
        .plant_repo
        .expect_adopt_confirmed_species()
        .times(3)
        .returning(|_, _| Ok(false));
    store.scan_repo.expect_start_run().never();
    let writer = Arc::new(Recorder::default());

    let response = app_recording(store, auth, Arc::clone(&writer))
        .oneshot(post(
            &format!("/plants/{PLANT}/species"),
            &serde_json::json!({ "species_id": "pepper" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(response.headers().contains_key("retry-after"));
    assert!(writer.calls().is_empty(), "{:?}", writer.calls());
}

// ---------------------------------------------------------------------------------------------
// Submit's working species ([architecture 6.2], step 4): a run starts with the plant's
// confirmed species, the one its `perception` job names, and otherwise with none, never the
// scan's working species from an earlier run.

/// The scan to submit: `status` (with `failure`), a whole-plant photo, `plant`, and `working`
/// as the scan's working species.
fn submittable(
    status: ScanStatus,
    failure: Option<&ScanFailure>,
    plant: &Plant<Stored>,
    working: Option<&SpeciesId>,
) -> ScanSnapshot {
    let mut snap = snapshot(false);
    snap.scan.status = status;
    snap.scan.run = Run(u32::from(status != ScanStatus::Pending));
    snap.scan.failure = failure.cloned();
    snap.scan.images = vec![ScanImage {
        id: "01JAX6BBBBBBBBBBBBBBBBBBB1".parse().unwrap(),
        kind: ImageKind::WholePlant,
        answers_question_id: None,
        width: 1024,
        height: 768,
        url: NoUrl,
        uploaded_at: now(),
        depth: None,
    }];
    snap.plant = plant.clone();
    snap.working_species = working.cloned();
    snap
}

/// A store submitting `snap`'s scan, expecting a run that starts with `expected` as its
/// working species and its `perception` job's species.
fn store_submitting(
    snap: ScanSnapshot,
    expected: Option<SpeciesId>,
) -> (FakeStore, domain::ports::MockAuthVerifier) {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Editor));
    let scan = snap.scan.clone();
    store
        .scan_repo
        .expect_scan()
        .returning(move |_, _| Ok(scan.clone()));
    store
        .scan_repo
        .expect_snapshot()
        .returning(move |_| Ok(snap.clone()));
    store
        .job_repo
        .expect_live_workers()
        .returning(|_| Ok(Vec::new()));
    store
        .scan_repo
        .expect_start_run()
        .withf(move |_, _, start| {
            let named = match &start.first_job.spec {
                JobSpec::Perception(p) => p.species_id.as_ref(),
                _ => None,
            };
            matches!(start.first_job.spec, JobSpec::Perception(_))
                && start.working_species == expected
                && named == expected.as_ref()
        })
        .times(1)
        .returning(|_, _, start| {
            Ok(StartedRun {
                run: start.expected_run.next(),
                from: start.expected_status,
                to: ScanStatus::Queued,
                superseded_question: None,
                cancelled_jobs: Vec::new(),
            })
        });
    (store, auth)
}

async fn submit(store: FakeStore, auth: domain::ports::MockAuthVerifier) -> StatusCode {
    app_with_pipeline(store, auth)
        .oneshot(
            Request::post(format!("/scans/{SCAN}/submit"))
                .header(header::AUTHORIZATION, "Bearer session-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn submitting_a_confirmed_plants_scan_starts_with_its_species() {
    let snap = submittable(ScanStatus::Pending, None, &plant(Some(tomato())), None);
    let (store, auth) = store_submitting(snap, Some(species("tomato")));
    assert_eq!(submit(store, auth).await, StatusCode::ACCEPTED);
}

/// A resubmitted run of an unconfirmed plant identifies the species again: it doesn't keep the
/// failed run's suggestion, which the snapshot still holds.
#[tokio::test]
async fn a_resubmitted_run_does_not_keep_the_earlier_suggestion() {
    let suggested = Plant {
        species_status: SpeciesStatus::Suggested,
        species: Some(tomato()),
        ..plant(None)
    };
    let failure = ScanFailure::new(ScanFailureCode::ProcessingError, "Something went wrong.");
    let snap = submittable(
        ScanStatus::Failed,
        Some(&failure),
        &suggested,
        Some(&species("tomato")),
    );
    let (store, auth) = store_submitting(snap, None);
    assert_eq!(submit(store, auth).await, StatusCode::ACCEPTED);
}
