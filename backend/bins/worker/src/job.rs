//! One job, from claim to `complete` ([contracts 5]): the version check, image fetch, the
//! model call under the job's soft deadline, the `complete` body with `model_versions` and
//! `timings_ms`, and `complete` retried idempotently until `max_runtime_s`.

use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant as StdInstant},
};

use async_trait::async_trait;
use domain::{
    id::WorkerId,
    jobs::{
        ClaimedJob, CompleteJob, Job, JobError, JobErrorCode, JobImage, JobOutcome, JobResult,
        JobSpec, ModelVersionKey, WorkerEvent, WorkerEvents,
    },
    ports::{JobContext, LoadedImage, ModelError, ProgressError, ProgressSink, VisionModel},
    wire::V1,
};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::{
    api::{CallError, JobLease, WorkerApi},
    config::Timings,
    lifecycle::{Backoff, retry_delay},
};

/// Runs jobs on the process's model and labels results with its versions.
pub struct JobRunner {
    model: Arc<dyn VisionModel>,
    worker_id: WorkerId,
}

impl JobRunner {
    /// A runner for `model`.
    pub fn new(model: Arc<dyn VisionModel>, worker_id: WorkerId) -> Self {
        Self { model, worker_id }
    }

    /// The model's version keys plus `worker_id` ([contracts 5], `model_versions` keys).
    pub fn versions(&self) -> BTreeMap<String, String> {
        let mut v = self.model.model_versions();
        v.insert(
            ModelVersionKey::WORKER_ID.to_owned(),
            self.worker_id.to_string(),
        );
        v
    }

    /// Runs one decoded job: fetch, model, result. `Err(LeaseLost)` means "send nothing".
    pub async fn execute<A: WorkerApi>(
        &self,
        api: &A,
        job: &Job,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<CompleteJob, LeaseLost> {
        let lease = JobLease::from(job);
        let mut timings = BTreeMap::new();
        let started = StdInstant::now();
        let images = match fetch_images(api, &lease, job_images(&job.spec)).await {
            Ok(images) => images,
            Err(CallError::LeaseLost) => return Err(LeaseLost),
            Err(e) => {
                return Ok(self.failed(
                    job,
                    JobError::new(JobErrorCode::ImageFetchFailed, e.to_string()),
                ));
            }
        };
        timings.insert("image_fetch".to_owned(), elapsed_ms(started));
        let ctx = JobContext {
            job_id: job.job_id,
            scan_id: job.scan_id,
            run: job.run,
            attempt: job.attempt,
        };
        let model_started = StdInstant::now();
        let outcome = match &job.spec {
            JobSpec::Perception(p) => self
                .model
                .perceive(ctx, p, &images, progress)
                .await
                .map(JobResult::Perception),
            JobSpec::Vlm(p) => self
                .model
                .analyse(ctx, p, &images, progress)
                .await
                .map(JobResult::Vlm),
            JobSpec::Verify(p) => self
                .model
                .verify(ctx, p, &images, progress)
                .await
                .map(JobResult::Verify),
            JobSpec::Guided(p) => match images.first() {
                Some(image) => self
                    .model
                    .read_guided_check(ctx, p, image)
                    .await
                    .map(JobResult::Guided),
                None => Err(ModelError::Unsupported {
                    model: "worker",
                    what: "a guided job without its image",
                }),
            },
        };
        timings.insert(
            job.spec.kind().as_str().to_owned(),
            elapsed_ms(model_started),
        );
        let mut body = match outcome.map(|r| r.to_value()) {
            Ok(Ok(result)) => self.complete(job, JobOutcome::Done { result }),
            Ok(Err(e)) => self.failed(
                job,
                JobError::new(
                    JobErrorCode::ModelError,
                    format!("result didn't serialise: {e}"),
                ),
            ),
            Err(e) => self.failed(job, job_error(&e)),
        };
        body.timings_ms.extend(timings);
        Ok(body)
    }

    fn complete(&self, job: &Job, outcome: JobOutcome) -> CompleteJob {
        let mut model_versions = self.versions();
        if let JobSpec::Perception(p) = &job.spec {
            // The versions this job used, not every loaded one ([vision 6], versions in results).
            model_versions.insert(
                ModelVersionKey::LABEL_SET_VERSION.to_owned(),
                p.label_set_version.clone(),
            );
            model_versions.insert(
                ModelVersionKey::CLASSIFIER_LABELS_VERSION.to_owned(),
                p.classifier_labels_version.clone(),
            );
        }
        CompleteJob {
            v: V1,
            outcome,
            model_versions,
            timings_ms: BTreeMap::new(),
        }
    }

    fn failed(&self, job: &Job, error: JobError) -> CompleteJob {
        self.complete(job, JobOutcome::Failed { error })
    }
}

/// The job was lost while it ran; nothing more may be sent about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseLost;

fn elapsed_ms(since: StdInstant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Maps a model's error to the `complete` error ([contracts 5], `JobErrorCode`):
/// `unknown_label_version` (not retryable) when the source is a `vision` label-version error,
/// `model_error` otherwise, retryable unless the adapter doesn't do the job at all.
pub fn job_error(e: &ModelError) -> JobError {
    let source = std::error::Error::source(e).and_then(|s| s.downcast_ref::<vision::VisionError>());
    if source.is_some_and(vision::VisionError::is_unknown_label_version) {
        return JobError::new(JobErrorCode::UnknownLabelVersion, chain(e));
    }
    JobError {
        code: JobErrorCode::ModelError,
        message: chain(e),
        retryable: e.retryable(),
    }
}

/// The error and its sources, `": "`-joined, for the job's log.
fn chain(e: &dyn std::error::Error) -> String {
    let mut message = e.to_string();
    let mut next = e.source();
    while let Some(source) = next {
        message.push_str(": ");
        message.push_str(&source.to_string());
        next = source.source();
    }
    message
}

/// The images a job's payload names, each once, in payload order. Depth maps are never in a
/// payload, so the worker never sees one.
pub fn job_images(spec: &JobSpec) -> Vec<JobImage> {
    let mut out: Vec<JobImage> = Vec::new();
    let mut add = |images: &[JobImage]| {
        for i in images {
            if !out.iter().any(|o| o.image_id == i.image_id) {
                out.push(i.clone());
            }
        }
    };
    match spec {
        JobSpec::Perception(p) => add(&p.images),
        JobSpec::Vlm(p) => {
            add(&p.findings_images);
            add(&p.indicator_images);
        }
        JobSpec::Verify(p) => add(&p.images),
        JobSpec::Guided(p) => add(std::slice::from_ref(&p.image)),
    }
    out
}

/// Fetches every image, each retried twice with backoff after an `Unavailable` answer (a
/// transport error, or an error body with `retryable: true`), waiting at least its
/// `retry_after_s`.
async fn fetch_images<A: WorkerApi>(
    api: &A,
    lease: &JobLease,
    images: Vec<JobImage>,
) -> Result<Vec<LoadedImage>, CallError> {
    let mut out = Vec::with_capacity(images.len());
    for image in images {
        let mut backoff = Backoff::new(Duration::from_millis(250), Duration::from_secs(2));
        let mut tries = 0;
        let bytes = loop {
            match api.image(lease, &image).await {
                Ok(bytes) => break bytes,
                Err(e @ CallError::Unavailable { .. }) if tries < 2 => {
                    tries += 1;
                    tokio::time::sleep(retry_delay(&e, &mut backoff)).await;
                }
                Err(e) => return Err(e),
            }
        };
        out.push(LoadedImage { image, bytes });
    }
    Ok(out)
}

/// The job's progress sink: posts events with the lease token. On `409 lease_lost` it cancels
/// the job's token, so the job task stops at once and sends nothing more; other failures are
/// returned and the model goes on (`complete` repeats every stage).
pub struct ApiProgress<A: WorkerApi> {
    api: Arc<A>,
    lease: JobLease,
    cancel: CancellationToken,
}

impl<A: WorkerApi> ApiProgress<A> {
    /// A sink for one job.
    pub const fn new(api: Arc<A>, lease: JobLease, cancel: CancellationToken) -> Self {
        Self { api, lease, cancel }
    }
}

#[async_trait]
impl<A: WorkerApi> ProgressSink for ApiProgress<A> {
    async fn send(&self, events: Vec<WorkerEvent>) -> Result<(), ProgressError> {
        let body = WorkerEvents { v: V1, events };
        match self.api.events(&self.lease, &body).await {
            Ok(()) => Ok(()),
            Err(CallError::LeaseLost) => {
                self.cancel.cancel();
                Err(ProgressError::LeaseLost)
            }
            Err(e) => Err(ProgressError::Transport(Box::new(e))),
        }
    }
}

/// The job task's body: version check, the model under the soft deadline (15 s before
/// `max_runtime_s`, on the monotonic clock), then `complete` with retries. A cancelled token
/// (`cancel_job_ids`, `lease_lost`, or the drain running out) returns at once without sending
/// anything more, whichever of these it is in.
pub async fn run_job<A: WorkerApi>(
    api: &Arc<A>,
    runner: &JobRunner,
    claimed: ClaimedJob,
    cancel: &CancellationToken,
    timings: Timings,
) {
    let received = Instant::now();
    let (job, lease, max_runtime) = match claimed {
        ClaimedJob::Unsupported(head) => {
            let lease = JobLease {
                job_id: head.job_id,
                lease_token: head.lease_token,
            };
            let body = CompleteJob {
                v: V1,
                outcome: JobOutcome::Failed {
                    error: JobError::new(
                        JobErrorCode::UnsupportedVersion,
                        format!(
                            "job v {} (or its kind or payload) is not supported by this worker",
                            head.v
                        ),
                    ),
                },
                model_versions: runner.versions(),
                timings_ms: BTreeMap::new(),
            };
            // No max_runtime_s without a v1 job: give `complete` the shortest one (2 minutes).
            let until = received + Duration::from_mins(2);
            complete_with_retry(api.as_ref(), &lease, &body, until, cancel).await;
            return;
        }
        ClaimedJob::Job(job) => {
            let lease = JobLease::from(job.as_ref());
            let max = Duration::from_secs(u64::from(job.max_runtime_s));
            (job, lease, max)
        }
    };
    let hard_deadline = received + max_runtime;
    let soft_deadline = received + max_runtime.saturating_sub(timings.stop_before_max_runtime);
    let progress: Arc<dyn ProgressSink> = Arc::new(ApiProgress::new(
        Arc::clone(api),
        lease.clone(),
        cancel.clone(),
    ));
    let run = tokio::time::timeout_at(soft_deadline, runner.execute(api.as_ref(), &job, progress));
    let outcome = tokio::select! {
        () = cancel.cancelled() => return, // drops the model's future: cancellation by drop
        outcome = run => outcome,
    };
    let body = match outcome {
        Ok(Ok(body)) => body,
        Ok(Err(LeaseLost)) => return,
        Err(_elapsed) => CompleteJob {
            v: V1,
            outcome: JobOutcome::Failed {
                error: JobError::new(
                    JobErrorCode::MaxRuntimeExceeded,
                    format!("stopped before max_runtime_s {}", job.max_runtime_s),
                ),
            },
            model_versions: runner.versions(),
            timings_ms: BTreeMap::new(),
        },
    };
    complete_with_retry(api.as_ref(), &lease, &body, hard_deadline, cancel).await;
}

/// Repeating `complete` with the same lease token is safe: a job that already completed gets
/// `204`. Retries an `Unavailable` answer with backoff, not before its `retry_after_s`; stops at
/// the hard deadline, since the lease can't outlive `max_runtime`. The job's token cuts a call
/// in flight too, so a drain that runs out isn't held up by a slow `complete`: if the API
/// didn't commit it, the job is requeued 30 s after the last heartbeat, like any abandoned job.
async fn complete_with_retry<A: WorkerApi>(
    api: &A,
    lease: &JobLease,
    body: &CompleteJob,
    until: Instant,
    cancel: &CancellationToken,
) {
    let mut backoff = Backoff::new(Duration::from_millis(500), Duration::from_secs(8));
    loop {
        let answer = tokio::select! {
            () = cancel.cancelled() => return,
            answer = api.complete(lease, body) => answer,
        };
        let e = match answer {
            Ok(()) | Err(CallError::LeaseLost) => return,
            Err(CallError::Rejected(e)) => {
                tracing::error!(error = %e, job_id = %lease.job_id, "complete rejected");
                return;
            }
            Err(e @ CallError::Unavailable { .. }) => e,
        };
        let wait = retry_delay(&e, &mut backoff);
        if Instant::now() + wait >= until {
            tracing::warn!(error = %e, job_id = %lease.job_id, "giving up on complete");
            return;
        }
        tokio::select! {
            () = cancel.cancelled() => return,
            () = tokio::time::sleep(wait) => {}
        }
    }
}
