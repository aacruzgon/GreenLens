//! The loop on paused time: a fake broker and fake models, no network, no GPU.
#![allow(
    clippy::unused_async_trait_impl,
    clippy::struct_excessive_bools,
    clippy::single_match_else,
    reason = "fakes implement async traits without awaiting, and set flags per test"
)]

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use bytes::Bytes;
use domain::{
    events::Stage,
    jobs::{
        ClaimRequest, ClaimedJob, CompleteJob, GuidedPayload, GuidedResult, Heartbeat,
        HeartbeatResponse, HeldJob, Job, JobErrorCode, JobHead, JobImage, JobOutcome, JobSpec,
        PerceptionPayload, PerceptionResult, PerceptionStage, VerifyPayload, VerifyResult,
        VlmPayload, VlmResult, WorkerEvent, WorkerEvents, WorkerImagePath,
    },
    ports::{JobContext, LoadedImage, ModelError, ProgressSink, VisionModel},
    scan::ImageKind,
    vlm::{GuidedTexture, JarTexture},
    wire::{Confidence, V1},
};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::{
    api::{CallError, JobLease, WorkerApi},
    config::StubArgs,
    stub::{StubConfig, StubModel},
};

#[derive(Default)]
struct Fake {
    claim_fails: bool,
    claim_retry_after: Option<Duration>,
    heartbeat_hangs: bool,
    complete_hangs: bool,
    complete_retry_after: Option<Duration>,
    complete_calls: Mutex<Vec<Instant>>,
    unsupported: bool,
    events_lease_lost: bool,
    image_fails: bool,
    complete_failures: Mutex<u32>,
    claims: Mutex<u32>,
    jobs_left: Mutex<u32>,
    completes: Mutex<Vec<CompleteJob>>,
    heartbeats: Mutex<Vec<Vec<HeldJob>>>,
    cancel_job_ids: Vec<JobId>,
    /// Hand out `perception` jobs instead of guided ones.
    perception: bool,
    events_seen: Mutex<Vec<WorkerEvent>>,
}

const IMG: &str = "01JAX6BBBBBBBBBBBBBBBBBBBB";

fn job_id(n: u32) -> JobId {
    format!(
        "01JAX5AAAAAAAAAAAAAAAAAAA{}",
        char::from(b'0' + u8::try_from(n % 10).unwrap())
    )
    .parse()
    .unwrap()
}

fn job(n: u32, max_runtime_s: u32) -> Job {
    Job {
        v: V1,
        job_id: job_id(n),
        scan_id: "01JAX3K9T6R2B8V0QZ4M7D1E5F".parse().unwrap(),
        run: None,
        attempt: 1,
        lease_token: "01JAX7CCCCCCCCCCCCCCCCCCCC".parse().unwrap(),
        lease_until: "2026-10-18T14:05:22Z".parse().unwrap(),
        max_runtime_s,
        spec: JobSpec::Guided(GuidedPayload {
            image: JobImage {
                image_id: IMG.parse().unwrap(),
                kind: ImageKind::JarTest,
                width: 100,
                height: 100,
                url: WorkerImagePath::new(job_id(n), IMG.parse().unwrap()),
            },
        }),
    }
}

impl WorkerApi for Fake {
    async fn heartbeat(&self, hb: &Heartbeat) -> Result<HeartbeatResponse, CallError> {
        self.heartbeats
            .lock()
            .unwrap()
            .push(hb.current_jobs.clone());
        if self.heartbeat_hangs {
            std::future::pending::<()>().await;
        }
        Ok(HeartbeatResponse {
            cancel_job_ids: self.cancel_job_ids.clone(),
        })
    }

    async fn claim(&self, _req: &ClaimRequest) -> Result<Option<ClaimedJob>, CallError> {
        let reply = {
            let mut claims = self.claims.lock().unwrap();
            *claims += 1;
            let mut left = self.jobs_left.lock().unwrap();
            if self.claim_fails {
                Some(Err(CallError::Unavailable {
                    reason: "connection refused".into(),
                    retry_after: self.claim_retry_after,
                }))
            } else if *left > 0 {
                *left -= 1;
                let mut j = job(*claims, 120);
                if self.perception {
                    j.spec = perception_spec(j.job_id);
                }
                Some(Ok(Some(if self.unsupported {
                    ClaimedJob::Unsupported(JobHead {
                        v: 2,
                        job_id: j.job_id,
                        lease_token: j.lease_token,
                    })
                } else {
                    ClaimedJob::Job(Box::new(j))
                })))
            } else {
                None
            }
        };
        match reply {
            Some(r) => r,
            None => {
                tokio::time::sleep(Duration::from_secs(25)).await; // an empty long poll
                Ok(None)
            }
        }
    }

    async fn events(&self, _lease: &JobLease, events: &WorkerEvents) -> Result<(), CallError> {
        if self.events_lease_lost {
            Err(CallError::LeaseLost)
        } else {
            self.events_seen
                .lock()
                .unwrap()
                .extend(events.events.iter().cloned());
            Ok(())
        }
    }

    async fn complete(&self, _lease: &JobLease, body: &CompleteJob) -> Result<(), CallError> {
        self.complete_calls.lock().unwrap().push(Instant::now());
        if self.complete_hangs {
            std::future::pending::<()>().await;
        }
        {
            let mut left = self.complete_failures.lock().unwrap();
            if *left > 0 {
                *left -= 1;
                return Err(CallError::Unavailable {
                    reason: "timed out".into(),
                    retry_after: self.complete_retry_after,
                });
            }
        }
        self.completes.lock().unwrap().push(body.clone());
        Ok(())
    }

    async fn image(&self, _lease: &JobLease, _image: &JobImage) -> Result<Bytes, CallError> {
        if self.image_fails {
            Err(CallError::Rejected("404".into()))
        } else {
            Ok(Bytes::from_static(b"webp"))
        }
    }
}

/// A model whose guided reading hangs, or sends an event then answers.
struct Model {
    hangs: bool,
}

#[async_trait]
impl VisionModel for Model {
    fn model_versions(&self) -> BTreeMap<String, String> {
        BTreeMap::from([("vlm_model".to_owned(), "test@1".to_owned())])
    }
    async fn perceive(
        &self,
        _: JobContext,
        _: &PerceptionPayload,
        _: &[LoadedImage],
        _: Arc<dyn ProgressSink>,
    ) -> Result<PerceptionResult, ModelError> {
        Err(ModelError::Unsupported {
            model: "test",
            what: "perception",
        })
    }
    async fn analyse(
        &self,
        _: JobContext,
        _: &VlmPayload,
        _: &[LoadedImage],
        _: Arc<dyn ProgressSink>,
    ) -> Result<VlmResult, ModelError> {
        Err(ModelError::Unsupported {
            model: "test",
            what: "vlm",
        })
    }
    async fn verify(
        &self,
        _: JobContext,
        _: &VerifyPayload,
        _: &[LoadedImage],
        _: Arc<dyn ProgressSink>,
    ) -> Result<VerifyResult, ModelError> {
        Err(ModelError::Unsupported {
            model: "test",
            what: "verify",
        })
    }
    async fn read_guided_check(
        &self,
        _: JobContext,
        _: &GuidedPayload,
        _: &LoadedImage,
    ) -> Result<GuidedResult, ModelError> {
        if self.hangs {
            std::future::pending::<()>().await;
        }
        Ok(GuidedResult {
            texture: GuidedTexture::JarTest {
                settled: true,
                texture: JarTexture::Loamy,
                confidence: Confidence::saturating(0.65),
            },
        })
    }
}

fn fake(jobs: u32) -> Fake {
    Fake {
        jobs_left: Mutex::new(jobs),
        ..Fake::default()
    }
}

fn worker(fake: Fake, hangs: bool, slots: u32) -> Arc<Worker<Fake>> {
    worker_of(
        fake,
        Arc::new(Model { hangs }),
        "rig-5070ti-vlm-1",
        Capability::Vlm,
        slots,
    )
}

fn worker_of(
    fake: Fake,
    model: Arc<dyn VisionModel>,
    id: &str,
    cap: Capability,
    slots: u32,
) -> Arc<Worker<Fake>> {
    let identity = Identity {
        worker_id: id.parse().unwrap(),
        caps: vec![cap],
        slots,
    };
    Arc::new(Worker::new(
        fake,
        model,
        identity,
        Timings::new(Duration::from_secs(20)),
    ))
}

/// A `perception` payload for every stage: one whole-plant photo and one leaf close-up.
fn perception_spec(job: JobId) -> JobSpec {
    let image = |id: &str, kind| JobImage {
        image_id: id.parse().unwrap(),
        kind,
        width: 100,
        height: 100,
        url: WorkerImagePath::new(job, id.parse().unwrap()),
    };
    JobSpec::Perception(PerceptionPayload {
        images: vec![
            image(IMG, ImageKind::WholePlant),
            image("01JAX6BBBBBBBBBBBBBBBBBBBC", ImageKind::LeafCloseup),
        ],
        stages: PerceptionStage::ALL.to_vec(),
        species_id: None,
        skip_species_identification: false,
        regions: None,
        label_set_version: "labels-1".to_owned(),
        classifier_labels_version: "classes-1".to_owned(),
        question_id: None,
    })
}

/// A stub worker pausing `delay_ms` inside each stage.
fn stub_worker(fake: Fake, delay_ms: u64) -> Arc<Worker<Fake>> {
    let config = StubConfig::new(&StubArgs {
        stub_vision: true,
        allow_stub_vision: false,
        label_set_version: Some("labels-1".to_owned()),
        classifier_labels_version: Some("classes-1".to_owned()),
        species: vec!["tomato".to_owned()],
        problems: Vec::new(),
        plant_present: true,
        stage_delay_ms: delay_ms,
    })
    .unwrap();
    worker_of(
        Fake {
            perception: true,
            ..fake
        },
        Arc::new(StubModel::new(config)),
        "dev-stub-perception-1",
        Capability::Perception,
        1,
    )
}

fn spawn_claims(
    w: &Arc<Worker<Fake>>,
    shutdown: &CancellationToken,
) -> tokio::task::JoinHandle<()> {
    let (w, shutdown) = (Arc::clone(w), shutdown.clone());
    tokio::spawn(async move { w.claim_loop(shutdown).await })
}

async fn stop(
    w: &Arc<Worker<Fake>>,
    shutdown: &CancellationToken,
    claims: tokio::task::JoinHandle<()>,
) {
    shutdown.cancel();
    claims.await.unwrap();
    w.drain().await;
}

#[tokio::test(start_paused = true)]
async fn holds_at_most_slots_jobs() {
    let w = worker(fake(10), true, 2);
    let shutdown = CancellationToken::new();
    let _claims = spawn_claims(&w, &shutdown);
    tokio::time::sleep(Duration::from_secs(60)).await;
    assert_eq!(*w.api.claims.lock().unwrap(), 2);
    assert_eq!(w.running().current_jobs().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn a_finished_job_completes_with_versions_and_timings() {
    let w = worker(fake(1), false, 1);
    let shutdown = CancellationToken::new();
    let claims = spawn_claims(&w, &shutdown);
    tokio::time::sleep(Duration::from_millis(5)).await;
    stop(&w, &shutdown, claims).await;
    let completes = w.api.completes.lock().unwrap();
    assert!(matches!(completes[0].outcome, JobOutcome::Done { .. }));
    assert_eq!(completes[0].model_versions["worker_id"], "rig-5070ti-vlm-1");
    assert!(completes[0].timings_ms.contains_key("guided"));
}

#[tokio::test(start_paused = true)]
async fn lease_lost_from_a_progress_event_drops_the_job() {
    // A 409 on events cancels the job's token, which `run_job` selects on.
    let api = Arc::new(Fake {
        events_lease_lost: true,
        ..fake(0)
    });
    let token = CancellationToken::new();
    let sink = crate::job::ApiProgress::new(
        Arc::clone(&api),
        JobLease::from(&job(1, 120)),
        token.clone(),
    );
    let sent = sink
        .send(vec![WorkerEvent::StageStarted {
            stage: Stage::Guided,
        }])
        .await;
    assert!(matches!(sent, Err(domain::ports::ProgressError::LeaseLost)));
    assert!(token.is_cancelled());
}

#[tokio::test(start_paused = true)]
async fn heartbeat_lists_jobs_and_cancels_the_listed_ones() {
    let w = worker(
        Fake {
            cancel_job_ids: vec![job_id(1)],
            ..fake(0)
        },
        true,
        1,
    );
    let token = CancellationToken::new();
    let _registered = w
        .running()
        .register(JobLease::from(&job(1, 120)), token.clone());
    let stop_hb = CancellationToken::new();
    let (w2, s2) = (Arc::clone(&w), stop_hb.clone());
    tokio::spawn(async move { w2.heartbeat_loop(s2).await });
    tokio::time::sleep(Duration::from_millis(1)).await;
    assert!(token.is_cancelled());
    assert_eq!(w.api.heartbeats.lock().unwrap()[0][0].job_id, job_id(1));
    stop_hb.cancel();
}

#[tokio::test(start_paused = true)]
async fn another_version_fails_with_unsupported_version() {
    let w = worker(
        Fake {
            unsupported: true,
            ..fake(1)
        },
        true,
        1,
    );
    let shutdown = CancellationToken::new();
    let claims = spawn_claims(&w, &shutdown);
    tokio::time::sleep(Duration::from_millis(1)).await;
    stop(&w, &shutdown, claims).await;
    let completes = w.api.completes.lock().unwrap();
    let JobOutcome::Failed { error } = &completes[0].outcome else {
        panic!("expected failed")
    };
    assert_eq!(error.code, JobErrorCode::UnsupportedVersion);
    assert!(!error.retryable);
}

#[tokio::test(start_paused = true)]
async fn an_image_that_cannot_be_fetched_fails_the_job_retryably() {
    let w = worker(
        Fake {
            image_fails: true,
            ..fake(1)
        },
        false,
        1,
    );
    let shutdown = CancellationToken::new();
    let claims = spawn_claims(&w, &shutdown);
    tokio::time::sleep(Duration::from_millis(1)).await;
    stop(&w, &shutdown, claims).await;
    let completes = w.api.completes.lock().unwrap();
    let JobOutcome::Failed { error } = &completes[0].outcome else {
        panic!("expected failed")
    };
    assert_eq!(error.code, JobErrorCode::ImageFetchFailed);
    assert!(error.retryable);
}

#[tokio::test(start_paused = true)]
async fn complete_is_retried_after_a_timeout() {
    let w = worker(
        Fake {
            complete_failures: Mutex::new(2),
            ..fake(1)
        },
        false,
        1,
    );
    let shutdown = CancellationToken::new();
    let claims = spawn_claims(&w, &shutdown);
    tokio::time::sleep(Duration::from_secs(5)).await;
    stop(&w, &shutdown, claims).await;
    assert_eq!(w.api.completes.lock().unwrap().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_hung_model_is_stopped_before_max_runtime() {
    let w = worker(fake(1), true, 1);
    let shutdown = CancellationToken::new();
    let _claims = spawn_claims(&w, &shutdown);
    tokio::time::sleep(Duration::from_secs(106)).await; // 120 s − 15 s, plus 1
    let completes = w.api.completes.lock().unwrap().clone();
    let JobOutcome::Failed { error } = &completes[0].outcome else {
        panic!("expected failed")
    };
    assert_eq!(error.code, JobErrorCode::MaxRuntimeExceeded);
    assert!(error.retryable);
}

#[tokio::test(start_paused = true)]
async fn shutdown_abandons_jobs_after_the_drain_bound() {
    let w = worker(fake(1), true, 1);
    let shutdown = CancellationToken::new();
    let claims = spawn_claims(&w, &shutdown);
    tokio::time::sleep(Duration::from_secs(1)).await;
    let started = Instant::now();
    stop(&w, &shutdown, claims).await;
    assert_eq!(started.elapsed(), Duration::from_secs(20));
    assert!(w.api.completes.lock().unwrap().is_empty());
    assert_eq!(w.slots.available_permits(), 1);
}

#[tokio::test(start_paused = true)]
async fn claim_errors_back_off_instead_of_spinning() {
    let w = worker(
        Fake {
            claim_fails: true,
            ..fake(1)
        },
        true,
        1,
    );
    let shutdown = CancellationToken::new();
    let _claims = spawn_claims(&w, &shutdown);
    tokio::time::sleep(Duration::from_secs(60)).await;
    let n = *w.api.claims.lock().unwrap();
    assert!((6..=8).contains(&n), "{n} claims in 60 s");
}

#[tokio::test(start_paused = true)]
async fn complete_waits_for_retry_after() {
    let w = worker(
        Fake {
            complete_failures: Mutex::new(1),
            complete_retry_after: Some(Duration::from_secs(7)), // a 503 with retry_after_s 7
            ..fake(1)
        },
        false,
        1,
    );
    let shutdown = CancellationToken::new();
    let claims = spawn_claims(&w, &shutdown);
    tokio::time::sleep(Duration::from_secs(10)).await;
    stop(&w, &shutdown, claims).await;
    assert_eq!(w.api.completes.lock().unwrap().len(), 1);
    let calls = w.api.complete_calls.lock().unwrap();
    assert_eq!(calls[1] - calls[0], Duration::from_secs(7)); // not the 0.25–0.5 s backoff
}

#[tokio::test(start_paused = true)]
async fn claim_errors_wait_for_retry_after() {
    let w = worker(
        Fake {
            claim_fails: true,
            claim_retry_after: Some(Duration::from_secs(20)), // a 429 with retry_after_s 20
            ..fake(1)
        },
        true,
        1,
    );
    let shutdown = CancellationToken::new();
    let _claims = spawn_claims(&w, &shutdown);
    tokio::time::sleep(Duration::from_secs(50)).await;
    assert_eq!(*w.api.claims.lock().unwrap(), 3); // at 0, 20 and 40 s
}

#[tokio::test(start_paused = true)]
async fn a_complete_in_flight_does_not_hold_up_the_drain() {
    let w = worker(
        Fake {
            complete_hangs: true,
            ..fake(1)
        },
        false,
        1,
    );
    let shutdown = CancellationToken::new();
    let claims = spawn_claims(&w, &shutdown);
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(w.api.complete_calls.lock().unwrap().len(), 1); // the call is in flight
    let started = Instant::now();
    stop(&w, &shutdown, claims).await;
    assert_eq!(started.elapsed(), Duration::from_secs(20));
    assert_eq!(w.slots.available_permits(), 1);
}

#[tokio::test(start_paused = true)]
async fn stopping_the_heartbeat_drops_a_call_in_flight() {
    let w = worker(
        Fake {
            heartbeat_hangs: true,
            ..fake(0)
        },
        true,
        1,
    );
    let stop_hb = CancellationToken::new();
    let (w2, s2) = (Arc::clone(&w), stop_hb.clone());
    let heartbeat = tokio::spawn(async move { w2.heartbeat_loop(s2).await });
    tokio::time::sleep(Duration::from_secs(1)).await; // the first beat hangs
    let started = Instant::now();
    stop_hb.cancel();
    heartbeat.await.unwrap();
    assert_eq!(started.elapsed(), Duration::ZERO); // not the 5 s call timeout
}

#[tokio::test(start_paused = true)]
async fn the_whole_shutdown_ends_within_the_drain_bound() {
    // A job whose `complete` hangs and a heartbeat that hangs: before, 20 s of drain, then the
    // `complete` call's 10 s and the heartbeat's 5 s.
    let w = worker(
        Fake {
            heartbeat_hangs: true,
            complete_hangs: true,
            ..fake(1)
        },
        false,
        1,
    );
    let stopped = async {
        tokio::time::sleep(Duration::from_secs(1)).await; // SIGTERM 1 s in
        Ok(())
    };
    let started = Instant::now();
    run_until(Arc::clone(&w), stopped).await.unwrap();
    assert_eq!(started.elapsed(), Duration::from_secs(21)); // the signal, then exactly 20 s
    assert_eq!(w.api.complete_calls.lock().unwrap().len(), 1);
    assert!(w.api.completes.lock().unwrap().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_stub_job_completes_through_the_loop() {
    let w = stub_worker(fake(1), 150);
    let shutdown = CancellationToken::new();
    let claims = spawn_claims(&w, &shutdown);
    tokio::time::sleep(Duration::from_secs(3)).await; // five stages of 150 ms
    stop(&w, &shutdown, claims).await;
    let completes = w.api.completes.lock().unwrap();
    assert_eq!(completes.len(), 1);
    assert!(matches!(completes[0].outcome, JobOutcome::Done { .. }));
    let versions = &completes[0].model_versions;
    assert_eq!(versions["stub"], "1");
    assert_eq!(versions["worker_id"], "dev-stub-perception-1");
    assert_eq!(versions["label_set_version"], "labels-1");
    assert!(completes[0].timings_ms.contains_key("perception"));
    // The stages arrive in the contract's order.
    let started: Vec<Stage> = w
        .api
        .events_seen
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            WorkerEvent::StageStarted { stage } => Some(*stage),
            _ => None,
        })
        .collect();
    assert_eq!(
        started,
        [
            Stage::PlantCheck,
            Stage::Segmentation,
            Stage::Species,
            Stage::Disease,
            Stage::ImageEmbedding,
        ]
    );
    // What the heartbeat of a started stub lists.
    assert_eq!(w.versions()["classifier_labels_version"], "classes-1");
}

#[tokio::test(start_paused = true)]
async fn a_cancelled_stub_job_stops_at_its_delay() {
    let w = stub_worker(
        Fake {
            cancel_job_ids: vec![job_id(1)],
            ..fake(1)
        },
        10_000,
    );
    let shutdown = CancellationToken::new();
    let claims = spawn_claims(&w, &shutdown);
    // Claimed at once; the heartbeat at 0 s lists it and the next one, at 10 s, is answered
    // with its id in `cancel_job_ids`: the job ends while the first stage is still pausing.
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(w.running().current_jobs().len(), 1);
    let heartbeats = tokio::spawn({
        let w = Arc::clone(&w);
        async move { w.heartbeat_loop(CancellationToken::new()).await }
    });
    tokio::time::sleep(Duration::from_secs(1)).await;
    heartbeats.abort();
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(w.running().current_jobs().len(), 0);
    stop(&w, &shutdown, claims).await;
    assert!(w.api.completes.lock().unwrap().is_empty());
    assert!(
        !w.api
            .events_seen
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, WorkerEvent::StageCompleted { .. }))
    );
}
