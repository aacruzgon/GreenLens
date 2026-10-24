//! Handler tests: the real router (`router::app`) driven with `tower::ServiceExt::oneshot`, over
//! mockall mocks of the domain ports (a [`fake_store::FakeStore`] of per-area mocks) and small
//! fakes of the seams.

mod beliefs;
mod chat;
mod connect;
mod connector_deletion;
mod connectors;
mod fake_store;
mod pipeline;

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use tokio::sync::Semaphore;
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tower::ServiceExt;

use domain::{
    authz::Membership,
    chat::Preview,
    error::QualityProblem,
    events::{ScanEvent, ScanEventBody, Stage},
    id::{GardenId, PendingActionId, ScanId, UserId},
    jobs::{CompleteJob, WorkerEvent},
    ports::{
        BeliefStore, CompletionOutcome, IdempotencyBegin, Lease, LeasedJob, MockAuthAdmin,
        MockAuthVerifier, MockBlobStore, MockGeocoder, MockKnowledgeStore, MockMemoryStore,
        MockNotifier, MockSoilProvider, MockTextEmbedder, MockWeatherProvider, MockWebhookVerifier,
        NoBeliefs, PhotoAnswerStart, PlanWrite, StoredResponse, VerifiedToken,
    },
    records::plan::ActionPlan,
    rules::stream::StreamState,
    scan::{AnswerRequest, Plant, Scan, ScanStatus, SpeciesChoice},
    testing::{FakeSigner, FixedClock, FixedPhotos, SequentialIds},
    url::Stored,
    wire::{EventSeq, Role, Run, Timestamp, V1},
};

use crate::{
    broker::JobBroker,
    config::Config,
    deletion::DeletionRunner,
    events::{
        hub::{EventHub, Published},
        writer::ScanWriters,
    },
    limits::RateLimits,
    push::Pusher,
    seams::{
        ChatTurns, FollowUp, Handled, ImageProcessing, PhotoInfo, PipelineApi, PipelineError,
        PreviewError, ProcessedImage, ProcessedUpload, RevisedPlan, SubmitOutcome, TurnError,
        TurnParts, TurnRequest, UploadError, UploadInput,
    },
    state::{AppState, Ports},
    workers::WorkerTokens,
};

use self::fake_store::FakeStore;

const USER: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5W";
const GARDEN: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5G";
const SCAN: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5S";
const NOW: &str = "2026-10-18T14:00:00Z";

/// The pipeline is never reached by these tests.
struct NoPipeline;

#[async_trait::async_trait]
impl PipelineApi for NoPipeline {
    async fn follow_up(&self, _: FollowUp) -> Result<(), PipelineError> {
        unreachable!("not used by these tests")
    }
    async fn submit(
        &self,
        _: Membership<domain::authz::Editor>,
        _: ScanId,
    ) -> Result<SubmitOutcome, PipelineError> {
        unreachable!("not used by these tests")
    }
    async fn species_confirmed(
        &self,
        _: Membership<domain::authz::Editor>,
        _: domain::id::PlantId,
        _: SpeciesChoice,
    ) -> Result<Plant<Stored>, PipelineError> {
        unreachable!("not used by these tests")
    }
    async fn answer(
        &self,
        _: Membership<domain::authz::Editor>,
        _: ScanId,
        _: AnswerRequest,
    ) -> Result<Handled<Scan<Stored>>, PipelineError> {
        unreachable!("not used by these tests")
    }
    async fn photo_answer_start(
        &self,
        _: ScanId,
        _: domain::id::QuestionId,
        _: PhotoInfo,
    ) -> Result<PhotoAnswerStart, PipelineError> {
        unreachable!("not used by these tests")
    }
    fn photo_answer_added(&self, _: domain::ports::QuestionStepClaim) -> Option<FollowUp> {
        unreachable!("not used by these tests")
    }
    async fn worker_events(
        &self,
        _: &Lease,
        _: LeasedJob,
        _: Vec<WorkerEvent>,
    ) -> Result<(), PipelineError> {
        unreachable!("not used by these tests")
    }
    async fn job_completed(
        &self,
        _: &Lease,
        _: LeasedJob,
        _: CompleteJob,
    ) -> Result<Handled<CompletionOutcome>, PipelineError> {
        unreachable!("not used by these tests")
    }
    async fn job_failed_for_good(&self, _: LeasedJob) -> Result<Option<FollowUp>, PipelineError> {
        unreachable!("not used by these tests")
    }
    async fn sweep(&self) -> Result<Vec<FollowUp>, PipelineError> {
        unreachable!("not used by these tests")
    }
    async fn store_revision(
        &self,
        _: Membership<domain::authz::Editor>,
        _: ScanId,
        _: PlanWrite,
    ) -> Result<ActionPlan, PipelineError> {
        unreachable!("not used by these tests")
    }
}

/// The agent is never reached by these tests.
struct NoChat;

#[async_trait::async_trait]
impl ChatTurns for NoChat {
    async fn start_turn(&self, _: TurnRequest) -> Result<TurnParts, TurnError> {
        unreachable!("not used by these tests")
    }
    fn apply_preview(
        &self,
        _: &ActionPlan,
        _: &Preview,
        _: PendingActionId,
        _: Timestamp,
    ) -> Result<RevisedPlan, PreviewError> {
        unreachable!("not used by these tests")
    }
}

/// An upload pipeline with a fixed answer.
struct FixedImages(fn() -> Result<ProcessedUpload, UploadError>);

impl ImageProcessing for FixedImages {
    fn process(&self, _: UploadInput<'_>) -> Result<ProcessedUpload, UploadError> {
        (self.0)()
    }
    fn process_chat(&self, _: &[u8]) -> Result<ProcessedImage, UploadError> {
        unreachable!("not used by these tests")
    }
}

fn config() -> Config {
    config_with(&[])
}

/// [`config`] with extra variables of the test's.
fn config_with(extra: &[(&'static str, &str)]) -> Config {
    let long = "x".repeat(40);
    let mut vars = BTreeMap::from([
        ("CLERK_ISSUER", "https://clerk.example.org".to_owned()),
        ("API_URL_SIGNING_KEY", long.clone()),
        ("API_PUBLIC_BASE", "https://api.example.org".to_owned()),
        ("WORKER_TOKENS", format!("rig-3070-perception-1={long}")),
        ("SURREAL_PASS", "p".to_owned()),
        ("SURREAL_ROOT_PASS", "r".to_owned()),
        ("RUSTFS_ACCESS_KEY", "a".to_owned()),
        ("RUSTFS_SECRET_KEY", "s".to_owned()),
        ("CLERK_SECRET_KEY", "sk".to_owned()),
        ("CLERK_WEBHOOK_SECRET", "whsec_x".to_owned()),
        ("LLM_API_KEY", "k".to_owned()),
        ("LLM_CHAT_MODEL", "big".to_owned()),
        ("LLM_CHEAP_MODEL", "small".to_owned()),
        ("API_WEB_ORIGINS", "https://app.example.org".to_owned()),
    ]);
    vars.extend(extra.iter().map(|&(k, v)| (k, v.to_owned())));
    Config::from_lookup(|k| vars.get(k).cloned()).unwrap()
}

/// Signed in as `USER`, a member of `GARDEN` with `role` (`None`: not a member); every scan is
/// in `GARDEN`.
fn signed_in(store: &mut FakeStore, role: Option<Role>) -> MockAuthVerifier {
    let mut auth = MockAuthVerifier::new();
    auth.expect_verify().returning(|_, _| {
        Ok(VerifiedToken {
            auth_id: "user_2abc".parse().unwrap(),
            display_name: None,
            expires_at: "2026-10-18T15:00:00Z".parse().unwrap(),
        })
    });
    store
        .identity_repo
        .expect_upsert_user()
        .returning(|_, _| Ok(USER.parse().unwrap()));
    store
        .access_repo
        .expect_garden_of()
        .returning(|_| Ok(Some(GARDEN.parse().unwrap())));
    store
        .access_repo
        .expect_role_in()
        .returning(move |_, _| Ok(role));
    auth
}

fn state(
    store: FakeStore,
    auth: MockAuthVerifier,
    blob: MockBlobStore,
    images: FixedImages,
) -> Arc<AppState> {
    state_with_beliefs(store, auth, blob, images, Arc::new(NoBeliefs))
}

/// [`state`] with a belief store of the test's.
fn state_with_beliefs(
    store: FakeStore,
    auth: MockAuthVerifier,
    blob: MockBlobStore,
    images: FixedImages,
    beliefs: Arc<dyn BeliefStore>,
) -> Arc<AppState> {
    state_with_config(store, auth, blob, images, beliefs, config())
}

/// [`state`] with a belief store and a configuration of the test's.
fn state_with_config(
    store: FakeStore,
    auth: MockAuthVerifier,
    blob: MockBlobStore,
    images: FixedImages,
    beliefs: Arc<dyn BeliefStore>,
    config: Config,
) -> Arc<AppState> {
    state_with_vendors(
        store,
        auth,
        config,
        crate::connect::MockConnectVendors::new(),
        (blob, images, beliefs),
    )
}

/// [`state_with_config`] with the farm connectors' vendor calls of the test's.
fn state_with_vendors(
    store: FakeStore,
    auth: MockAuthVerifier,
    config: Config,
    vendors: crate::connect::MockConnectVendors,
    parts: (MockBlobStore, FixedImages, Arc<dyn BeliefStore>),
) -> Arc<AppState> {
    build_state(store, auth, config, vendors, parts, |_| {
        Arc::new(NoPipeline)
    })
}

/// [`state`] with the pipeline `make` builds over the state's store.
fn state_with_pipeline(
    store: FakeStore,
    auth: MockAuthVerifier,
    config: Config,
    make: impl FnOnce(Arc<FakeStore>) -> Arc<dyn PipelineApi>,
) -> Arc<AppState> {
    build_state(
        store,
        auth,
        config,
        crate::connect::MockConnectVendors::new(),
        (MockBlobStore::new(), no_images(), Arc::new(NoBeliefs)),
        make,
    )
}

/// The state every builder above shares.
fn build_state(
    store: FakeStore,
    auth: MockAuthVerifier,
    config: Config,
    vendors: crate::connect::MockConnectVendors,
    (blob, images, beliefs): (MockBlobStore, FixedImages, Arc<dyn BeliefStore>),
    make_pipeline: impl FnOnce(Arc<FakeStore>) -> Arc<dyn PipelineApi>,
) -> Arc<AppState> {
    let store: Arc<FakeStore> = Arc::new(store);
    let pipeline = make_pipeline(Arc::clone(&store));
    let clock = Arc::new(FixedClock::at(NOW.parse().unwrap()));
    let tasks = TaskTracker::new();
    let shutdown = CancellationToken::new();
    let config = Arc::new(config);
    let blob: Arc<MockBlobStore> = Arc::new(blob);
    let notifier = Arc::new(MockNotifier::new());
    let hub = Arc::new(EventHub::default());
    let broker = Arc::new(JobBroker::new(
        store.clone(),
        clock.clone(),
        Arc::new(tokio::sync::Notify::new()),
    ));
    let pusher = Pusher::new(store.clone(), notifier.clone());
    let writers = Arc::new(ScanWriters::new(
        store.clone(),
        clock.clone(),
        hub.clone(),
        pusher.clone(),
        tasks.clone(),
        shutdown.clone(),
    ));
    let auth_admin = Arc::new(MockAuthAdmin::new());
    let deletes = blob::DeleteQueue::new(blob.clone());
    let vendors: Arc<dyn crate::connect::ConnectVendors> = Arc::new(vendors);
    // As `wiring.rs` builds it: with connectors on, the runner revokes through the same seam.
    let mut deletions = DeletionRunner::new(
        store.clone(),
        deletes.clone(),
        auth_admin,
        pusher.clone(),
        clock.clone(),
        tasks.clone(),
        shutdown.clone(),
    );
    if let Some(key) = &config.connectors.key {
        deletions = deletions.with_revoke(key.clone(), Arc::clone(&vendors));
    }
    Arc::new(AppState {
        limits: RateLimits::new(1000),
        upload_permits: Arc::new(Semaphore::new(1)),
        workers: WorkerTokens::new(config.worker_tokens.clone()),
        config,
        ports: Ports {
            store: store.clone(),
            memory: Arc::new(MockMemoryStore::new()),
            beliefs,
            knowledge: Arc::new(MockKnowledgeStore::new()),
            blob: blob.clone(),
            auth: Arc::new(auth),
            webhooks: Arc::new(MockWebhookVerifier::new()),
            weather: Arc::new(MockWeatherProvider::new()),
            soil: Arc::new(MockSoilProvider::new()),
            geocoder: Arc::new(MockGeocoder::new()),
            notifier,
            embedder: Arc::new(MockTextEmbedder::new()),
            signer: Arc::new(FakeSigner),
            photos: Arc::new(FixedPhotos(Vec::new())),
            clock: clock.clone(),
            ids: Arc::new(SequentialIds::default()),
        },
        pipeline,
        chat: Arc::new(NoChat),
        images: Arc::new(images),
        hub,
        writers,
        broker,
        deletions,
        deletes,
        connect: vendors,
        farm_sync: None,
        shutdown,
        tasks,
    })
}

fn no_images() -> FixedImages {
    FixedImages(|| unreachable!("not used by this test"))
}

async fn json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn get(uri: &str) -> Request<Body> {
    Request::get(uri)
        .header(header::AUTHORIZATION, "Bearer session-token")
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn an_upload_that_fails_the_quality_check_is_422_and_never_stored() {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Editor));
    let mut blob = MockBlobStore::new();
    blob.expect_put().never();
    store.scan_repo.expect_add_image().never();
    let images = FixedImages(|| {
        Err(UploadError::Quality {
            problem: QualityProblem::TooBlurry,
            blur_score: 41.2,
            exposure: 0.52,
        })
    });
    let app = crate::router::app(state(store, auth, blob, images));

    let boundary = "XyZ";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"kind\"\r\n\r\nleaf_closeup\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.jpg\"\r\n\
         Content-Type: image/jpeg\r\n\r\nnot really a jpeg\r\n--{boundary}--\r\n"
    );
    let request = Request::post(format!("/scans/{SCAN}/images"))
        .header(header::AUTHORIZATION, "Bearer session-token")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json(response).await;
    assert_eq!(body["error"]["code"], "image_quality");
    assert_eq!(body["error"]["retryable"], true);
    assert_eq!(body["error"]["details"]["problem"], "too_blurry");
    assert_eq!(body["error"]["details"]["hint"], "tap_to_focus");
    assert!(
        body["error"]["request_id"]
            .as_str()
            .unwrap()
            .starts_with("req_")
    );
}

#[tokio::test]
async fn a_repeated_key_replays_the_stored_response_without_running_the_handler() {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Editor));
    let stored = serde_json::json!({ "id": SCAN, "status": "pending" });
    let replay = stored.clone();
    store
        .idempotency_repo
        .expect_begin()
        .times(1)
        .returning(move |scope, fingerprint, _| {
            assert_eq!(scope.route, "POST /scans");
            assert_eq!(fingerprint.len(), 64);
            Ok(IdempotencyBegin::Replay(StoredResponse {
                status: 201,
                body: replay.clone(),
            }))
        });
    store.scan_repo.expect_create_scan().never();
    store.idempotency_repo.expect_finish().never();
    let app = crate::router::app(state(store, auth, MockBlobStore::new(), no_images()));

    let request = Request::post("/scans")
        .header(header::AUTHORIZATION, "Bearer session-token")
        .header(header::CONTENT_TYPE, "application/json")
        .header("idempotency-key", "7d444840-9dc0-11d1-b245-5ffdce74fad2")
        .body(Body::from(format!(
            r#"{{"garden_id":"{GARDEN}","new_plant":{{"name":"Tomato"}}}}"#
        )))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(json(response).await, stored);
}

#[tokio::test]
async fn a_key_still_running_is_409_request_in_progress() {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Editor));
    store
        .idempotency_repo
        .expect_begin()
        .returning(|_, _, _| Ok(IdempotencyBegin::InProgress));
    let app = crate::router::app(state(store, auth, MockBlobStore::new(), no_images()));
    let request = Request::post("/scans")
        .header(header::AUTHORIZATION, "Bearer session-token")
        .header(header::CONTENT_TYPE, "application/json")
        .header("idempotency-key", "7d444840-9dc0-11d1-b245-5ffdce74fad2")
        .body(Body::from(format!(r#"{{"garden_id":"{GARDEN}"}}"#)))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(response.headers()[header::RETRY_AFTER], "2");
    let body = json(response).await;
    assert_eq!(body["error"]["code"], "request_in_progress");
    assert_eq!(body["error"]["retry_after_s"], 2);
}

#[tokio::test]
async fn a_non_member_gets_the_same_404_as_an_unknown_scan() {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, None);
    store.scan_repo.expect_scan_detail().never();
    let app = crate::router::app(state(store, auth, MockBlobStore::new(), no_images()));

    let response = app.oneshot(get(&format!("/scans/{SCAN}"))).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let mut body = json(response).await;
    body["error"]["request_id"] = serde_json::Value::Null;
    assert_eq!(
        body,
        serde_json::json!({ "error": {
            "code": "not_found", "message": "Not found.", "retryable": false, "request_id": null
        }})
    );
}

#[tokio::test]
async fn a_viewer_is_403_with_the_required_role() {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Viewer));
    let app = crate::router::app(state(store, auth, MockBlobStore::new(), no_images()));
    let request = Request::delete(format!("/scans/{SCAN}"))
        .header(header::AUTHORIZATION, "Bearer session-token")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = json(response).await;
    assert_eq!(body["error"]["details"]["required_role"], "editor");
}

#[tokio::test]
async fn unknown_paths_get_an_error_body_and_a_request_id() {
    let store = FakeStore::default();
    let app = crate::router::app(state(
        store,
        MockAuthVerifier::new(),
        MockBlobStore::new(),
        no_images(),
    ));
    let response = app
        .oneshot(Request::get("/nope").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let id = response.headers()["x-request-id"]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(id.starts_with("req_"));
    assert_eq!(json(response).await["error"]["request_id"], id);
}

fn event(seq: u64) -> ScanEvent<Stored> {
    ScanEvent {
        v: V1,
        seq: EventSeq(seq),
        run: Run(1),
        scan_id: SCAN.parse().unwrap(),
        ts: NOW.parse().unwrap(),
        body: ScanEventBody::StageStarted {
            stage: Stage::PlantCheck,
        },
    }
}

fn ids(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.strip_prefix("id: "))
        .map(str::to_owned)
        .collect()
}

#[tokio::test]
async fn sse_resume_replays_after_last_event_id_then_goes_live_without_duplicates() {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Viewer));
    store.event_repo.expect_stream_state().returning(|_| {
        Ok(StreamState {
            status: ScanStatus::Analysing,
            question_unresolved: false,
            guided_check_analysing: false,
            step_claimed: false,
        })
    });
    store
        .event_repo
        .expect_events_after()
        .times(1)
        .returning(|_, _, after| {
            assert_eq!(after, EventSeq(1));
            Ok(vec![event(2), event(3)])
        });
    let state = state(store, auth, MockBlobStore::new(), no_images());
    let app = crate::router::app(Arc::clone(&state));

    let request = Request::get(format!("/scans/{SCAN}/events"))
        .header(header::AUTHORIZATION, "Bearer session-token")
        .header("last-event-id", "1")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/event-stream"
    );
    let mut body = response.into_body();
    // The replay: 2 then 3. The subscription was made before it.
    let mut seen = String::new();
    while ids(&seen).len() < 2 {
        let frame = body.frame().await.unwrap().unwrap();
        seen.push_str(std::str::from_utf8(frame.data_ref().unwrap()).unwrap());
    }
    assert_eq!(ids(&seen), ["2", "3"]);
    assert!(seen.contains("event: stage_started\n"));
    // Live: a repeat of 3, then 4, then 5, which brings the scan to rest.
    let scan: ScanId = SCAN.parse().unwrap();
    state.hub.publish(
        scan,
        Published {
            event: event(3),
            rests: false,
        },
    );
    state.hub.publish(
        scan,
        Published {
            event: event(4),
            rests: false,
        },
    );
    state.hub.publish(
        scan,
        Published {
            event: event(5),
            rests: true,
        },
    );
    // `collect` returns only because the stream ends after 5.
    let rest = tokio::time::timeout(Duration::from_secs(5), body.collect())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        ids(std::str::from_utf8(&rest.to_bytes()).unwrap()),
        ["4", "5"]
    );
}

#[tokio::test]
async fn a_resting_event_stored_during_the_replay_still_ends_the_stream() {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Viewer));
    // Read before the replay: not at rest yet.
    store.event_repo.expect_stream_state().returning(|_| {
        Ok(StreamState {
            status: ScanStatus::Analysing,
            question_unresolved: false,
            guided_check_analysing: false,
            step_claimed: false,
        })
    });
    // The resting event 3 was stored between the state read and the replay.
    store
        .event_repo
        .expect_events_after()
        .times(1)
        .returning(|_, _, _| Ok(vec![event(2), event(3)]));
    let state = state(store, auth, MockBlobStore::new(), no_images());
    let app = crate::router::app(Arc::clone(&state));
    let response = app
        .oneshot(get(&format!("/scans/{SCAN}/events")))
        .await
        .unwrap();
    let mut body = response.into_body();
    let mut seen = String::new();
    while ids(&seen).len() < 2 {
        let frame = body.frame().await.unwrap().unwrap();
        seen.push_str(std::str::from_utf8(frame.data_ref().unwrap()).unwrap());
    }
    // Its live copy is a duplicate, but it carries the resting mark: the stream ends.
    state.hub.publish(
        SCAN.parse().unwrap(),
        Published {
            event: event(3),
            rests: true,
        },
    );
    let rest = tokio::time::timeout(Duration::from_secs(5), body.collect())
        .await
        .expect("the stream ends after the resting duplicate")
        .unwrap();
    assert!(rest.to_bytes().is_empty());
    assert_eq!(ids(&seen), ["2", "3"]);
}

/// A live stream whose scan changes without an event: the first state read says `analysing`,
/// later ones answer `later`. The stream must close on its own.
async fn a_stream_that_rechecks(
    later: fn() -> Result<StreamState, domain::ports::StoreError>,
) -> Vec<String> {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Viewer));
    let reads = std::sync::atomic::AtomicUsize::new(0);
    store.event_repo.expect_stream_state().returning(move |_| {
        if reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            Ok(StreamState {
                status: ScanStatus::Analysing,
                question_unresolved: false,
                guided_check_analysing: false,
                step_claimed: true,
            })
        } else {
            later()
        }
    });
    store
        .event_repo
        .expect_events_after()
        .returning(|_, _, after| {
            Ok(if after == EventSeq(0) {
                vec![event(1)]
            } else {
                vec![]
            })
        });
    let app = crate::router::app(state(store, auth, MockBlobStore::new(), no_images()));
    let response = app
        .oneshot(get(&format!("/scans/{SCAN}/events")))
        .await
        .unwrap();
    // Paused time: the 15-second re-check comes at once when nothing else is pending.
    let bytes = tokio::time::timeout(Duration::from_secs(60), response.into_body().collect())
        .await
        .expect("the stream closes after a re-check")
        .unwrap()
        .to_bytes();
    ids(std::str::from_utf8(&bytes).unwrap())
}

#[tokio::test(start_paused = true)]
async fn a_resting_point_reached_without_an_event_closes_the_stream() {
    // The step claim was released without an event (`step_claimed` false, already planned).
    let seen = a_stream_that_rechecks(|| {
        Ok(StreamState {
            status: ScanStatus::Planned,
            question_unresolved: false,
            guided_check_analysing: false,
            step_claimed: false,
        })
    })
    .await;
    assert_eq!(seen, ["1"]);
}

#[tokio::test(start_paused = true)]
async fn deleting_the_scan_closes_its_stream() {
    // `stream_state` reads only scans without `deleting_at`.
    let seen = a_stream_that_rechecks(|| Err(domain::ports::StoreError::NotFound)).await;
    assert_eq!(seen, ["1"]);
}

#[tokio::test]
async fn a_scan_at_rest_replays_and_closes() {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Viewer));
    store.event_repo.expect_stream_state().returning(|_| {
        Ok(StreamState {
            status: ScanStatus::Planned,
            question_unresolved: false,
            guided_check_analysing: false,
            step_claimed: false,
        })
    });
    store
        .event_repo
        .expect_events_after()
        .returning(|_, _, _| Ok(vec![event(1), event(2)]));
    let app = crate::router::app(state(store, auth, MockBlobStore::new(), no_images()));
    let response = app
        .oneshot(get(&format!("/scans/{SCAN}/events")))
        .await
        .unwrap();
    let bytes = tokio::time::timeout(Duration::from_secs(5), response.into_body().collect())
        .await
        .unwrap()
        .unwrap()
        .to_bytes();
    assert_eq!(ids(std::str::from_utf8(&bytes).unwrap()), ["1", "2"]);
}

#[test]
fn ids_used_by_the_tests_parse() {
    let _: UserId = USER.parse().unwrap();
    let _: GardenId = GARDEN.parse().unwrap();
}

const JOB: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5J";
const LEASE: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5K";

#[tokio::test]
async fn late_worker_events_after_complete_are_204_and_dropped() {
    let mut store = FakeStore::default();
    // The job was completed with this token: `live_lease` answers `None`. `NoPipeline`'s
    // `worker_events` panics, so forwarding the events would fail the test.
    store
        .job_repo
        .expect_live_lease()
        .times(1)
        .returning(|lease, _| {
            assert_eq!(lease.job.to_string(), JOB);
            assert_eq!(lease.token.to_string(), LEASE);
            Ok(None)
        });
    store.job_repo.expect_check_lease().never();
    let app = crate::router::app(state(
        store,
        MockAuthVerifier::new(),
        MockBlobStore::new(),
        no_images(),
    ));
    let request = Request::post(format!("/internal/jobs/{JOB}/events"))
        .header(header::AUTHORIZATION, format!("Bearer {}", "x".repeat(40)))
        .header("x-lease-token", LEASE)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"v":1,"events":[{"type":"stage_started","stage":"plant_check"}]}"#,
        ))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn a_claim_body_over_the_json_limit_is_refused_before_the_handler() {
    // Valid JSON padded past 1 MiB: without the group's `DefaultBodyLimit`, axum's 2 MiB
    // default would let it through to the store (whose claim isn't mocked, so it would panic).
    let mut body =
        br#"{"worker_id":"rig-3070-perception-1","caps":["perception"],"slots":1}"#.to_vec();
    body.resize(1024 * 1024 + 4096, b' ');
    let app = crate::router::app(state(
        FakeStore::default(),
        MockAuthVerifier::new(),
        MockBlobStore::new(),
        no_images(),
    ));
    let request = Request::post("/internal/jobs/claim")
        .header(header::AUTHORIZATION, format!("Bearer {}", "x".repeat(40)))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let error = json(response).await;
    assert_eq!(error["error"]["details"]["field"], "body");
}

#[tokio::test]
async fn a_tombstoned_accounts_token_is_401_and_never_creates_a_user() {
    let mut store = FakeStore::default();
    let mut auth = MockAuthVerifier::new();
    auth.expect_verify().returning(|_, _| {
        Ok(VerifiedToken {
            auth_id: "user_2abc".parse().unwrap(),
            display_name: None,
            expires_at: "2026-10-18T15:00:00Z".parse().unwrap(),
        })
    });
    // The store refuses a tombstoned auth id with `NotFound` (`IdentityRepo::upsert_user`).
    store
        .identity_repo
        .expect_upsert_user()
        .times(1)
        .returning(|_, _| Err(domain::ports::StoreError::NotFound));
    store.identity_repo.expect_me().never();
    let app = crate::router::app(state(store, auth, MockBlobStore::new(), no_images()));
    let response = app.oneshot(get("/me")).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(json(response).await["error"]["code"], "unauthenticated");
}

#[tokio::test]
async fn a_second_delete_me_while_a_cascade_runs_starts_nothing() {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Owner));
    store
        .deletion_repo
        .expect_plan_account_deletion()
        .returning(|_| Ok(domain::ports::AccountDeletionPlan::default()));
    // The tombstone already exists: the compare-and-set answers `false`.
    store
        .deletion_repo
        .expect_start_account_deletion()
        .times(1)
        .returning(|user, auth_id, _| {
            assert_eq!(user.to_string(), USER);
            assert_eq!(auth_id.as_str(), "user_2abc");
            Ok(false)
        });
    store.deletion_repo.expect_transfer_ownership().never();
    store.deletion_repo.expect_delete_user().never();
    let app_state = state(store, auth, MockBlobStore::new(), no_images());
    let app = crate::router::app(Arc::clone(&app_state));
    let request = Request::delete("/me")
        .header(header::AUTHORIZATION, "Bearer session-token")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(app_state.tasks.len(), 0, "no second cascade was spawned");
}

/// A state whose `SoilProvider` answers `error` once.
fn soil_state(store: FakeStore, error: fn() -> domain::ports::ProviderError) -> Arc<AppState> {
    let mut soil = MockSoilProvider::new();
    soil.expect_soil_profile()
        .times(1)
        .returning(move |_| Err(error()));
    let shared = state(
        store,
        MockAuthVerifier::new(),
        MockBlobStore::new(),
        no_images(),
    );
    let mut owned = Arc::try_unwrap(shared).ok().expect("one owner");
    owned.ports.soil = Arc::new(soil);
    Arc::new(owned)
}

#[tokio::test]
async fn soil_failures_schedule_the_next_attempt_by_their_kind() {
    let garden: GardenId = GARDEN.parse().unwrap();
    let at = domain::records::weather::Coordinates::new(43.65, -79.38).unwrap();
    let now: Timestamp = NOW.parse().unwrap();
    for (error, wait) in [
        (
            (|| domain::ports::ProviderError::NotFound {
                provider: "soilgrids",
            }) as fn() -> _,
            Duration::from_hours(168),
        ),
        (
            || domain::ports::ProviderError::Unavailable {
                provider: "soilgrids",
                source: "timeout".into(),
            },
            Duration::from_mins(15),
        ),
    ] {
        let mut store = FakeStore::default();
        store
            .garden_repo
            .expect_soil_profile_failed()
            .times(1)
            .returning(move |g, retry_at| {
                assert_eq!(g, garden);
                assert_eq!(retry_at, now.plus(wait));
                Ok(())
            });
        store.garden_repo.expect_set_soil_profile().never();
        let app_state = soil_state(store, error);
        crate::scheduler::fetch_soil_profile(&app_state, garden, at)
            .await
            .unwrap();
    }
}

/// The S2 exit check's soil half, with the adapter `wiring.rs` builds: a central Toronto
/// garden, where `SoilGrids` answers null (as live on 2026-10-20), is `ready` with Soil
/// Landscapes of Canada's profile after one `SoilGrids` call.
#[tokio::test]
async fn a_toronto_garden_gets_its_soil_profile_from_slc() {
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
            "../../../crates/integrations/fixtures/soilgrids_toronto.json"
        )))
        .expect(1)
        .mount(&server)
        .await;
    let garden: GardenId = GARDEN.parse().unwrap();
    let mut store = FakeStore::default();
    store
        .garden_repo
        .expect_set_soil_profile()
        .times(1)
        .returning(move |g, status, profile| {
            assert_eq!(g, garden);
            assert_eq!(status, domain::garden::gardens::SoilProfileStatus::Ready);
            let top = profile.expect("a profile").depths[0];
            assert!((top.ph_h2o.unwrap() - 7.4).abs() < 1e-9);
            assert!((top.clay_pct.unwrap() - 29.0).abs() < 1e-9);
            Ok(())
        });
    store.garden_repo.expect_soil_profile_failed().never();
    let shared = state(
        store,
        MockAuthVerifier::new(),
        MockBlobStore::new(),
        no_images(),
    );
    let mut owned = Arc::try_unwrap(shared).ok().expect("one owner");
    owned.ports.soil = Arc::new(
        integrations::soilgrids::SoilGrids::new(
            integrations::http::client(&integrations::http::HttpConfig::default()).unwrap(),
            integrations::soilgrids::SoilGridsConfig {
                query_url: server.uri(),
                min_interval: Duration::ZERO,
                ..integrations::soilgrids::SoilGridsConfig::default()
            },
            Arc::clone(&owned.ports.clock),
        )
        .with_local_fallback(Arc::new(
            integrations::slc::SoilLandscapes::embedded().unwrap(),
        )),
    );
    let at = domain::records::weather::Coordinates::new(43.65, -79.38).unwrap();
    crate::scheduler::fetch_soil_profile(&owned, garden, at)
        .await
        .unwrap();
}

/// `backend/openapi.json` is committed for the app's `openapi-typescript`, and the `domain` types
/// are the contracts' source of truth, so the file must be what those types generate now.
/// Regenerate with `cargo run -p api -- openapi > openapi.json` from `backend/`.
#[test]
fn the_committed_openapi_document_matches_the_types() {
    let committed = include_str!("../../../openapi.json");
    let generated = serde_json::to_string_pretty(&crate::router::openapi()).unwrap();
    assert!(
        committed.trim_end() == generated,
        "backend/openapi.json is stale: run `cargo run -p api -- openapi > openapi.json` in backend/"
    );
}
