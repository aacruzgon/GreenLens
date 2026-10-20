//! The real `pipeline::Pipeline` over the fake store: the stage events a job's `complete`
//! sends for the ones the stream lost (backend roadmap gap 9, `Pipeline::job_completed`), and
//! `POST /plants/{id}/species` through the router with the pipeline wired in.

use std::sync::{Arc, Mutex};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use tower::ServiceExt;

use domain::{
    events::{RunReason, ScanEventBody, Stage},
    id::{ImageId, JobId, ScanId},
    jobs::{CompleteJob, JobKind, JobOutcome},
    ports::{
        ClaimAttempt, CompletionOutcome, GardenContext, GuidedFinish, Lease, LeasedJob,
        MockBeliefStore, MockBlobStore, MockKnowledgeStore, MockMemoryStore, MockTextEmbedder,
        MockWeatherProvider, ObservationDiff, ScanSnapshot, SpeciesConfirmation, StoreError,
    },
    records::{
        observation::{Observation, ObservationSource, ObservationStage},
        weather::{Coordinates, LocationPrecision, WeatherCell},
    },
    scan::{
        GuidedCheck, GuidedCheckStatus, GuidedKind, Plant, PlantStatus, Scan, ScanStatus,
        SpeciesChoice, SpeciesRef, SpeciesStatus,
    },
    testing::{FixedClock, SequentialIds},
    url::Stored,
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
    let mut knowledge = MockKnowledgeStore::new();
    knowledge.expect_problems().returning(|_| Ok(Vec::new()));
    Pipeline::new(PipelineDeps {
        store,
        writer,
        knowledge: Arc::new(knowledge),
        weather: Arc::new(MockWeatherProvider::new()),
        memory: Arc::new(MockMemoryStore::new()),
        beliefs: Arc::new(MockBeliefStore::new()),
        embedder: Arc::new(MockTextEmbedder::new()),
        blobs: Arc::new(MockBlobStore::new()),
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
