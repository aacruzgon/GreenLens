//! The worker API ([contracts 5]): heartbeat, the claim long-poll, progress events, completion
//! and job images. Authenticated by worker token; every call about a job also carries
//! `X-Lease-Token`, and any call about a requeued, expired or cancelled job is `409 lease_lost`.
//! Worker bodies ignore unknown fields, because older and newer workers run side by side.

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};

use domain::{
    error::ErrorBody,
    events::{ScanEventBody, Stage},
    id::{ImageId, JobId},
    jobs::{ClaimRequest, CompleteJob, Heartbeat, HeartbeatResponse, Job, WorkerEvents},
    scan::ScanStatus,
};

use crate::{
    error::ApiError,
    extract::{
        ApiJson, ApiPath,
        auth::{JobLease, WorkerAuth},
    },
    media,
    reply::Wire,
    seams::ScanWriter,
    state::SharedState,
};

/// `POST /internal/workers/heartbeat`: every 10 s. Extends the listed leases (only the worker's
/// own, never past `claimed_at + max_runtime`), records `versions` for `/status`, and answers
/// the jobs to stop.
#[utoipa::path(post, path = "/internal/workers/heartbeat", tag = "worker", security(("worker" = [])), request_body = Heartbeat,
    responses((status = 200, body = HeartbeatResponse), (status = 401, body = ErrorBody)))]
pub async fn heartbeat(
    State(state): State<SharedState>,
    WorkerAuth(worker): WorkerAuth,
    ApiJson(body): ApiJson<Heartbeat>,
) -> Result<Wire<HeartbeatResponse>, ApiError> {
    if body.worker_id != worker {
        return Err(ApiError::unauthenticated().message("the token belongs to another worker"));
    }
    Ok(Wire::ok(
        state
            .ports
            .store
            .heartbeat(body, state.ports.clock.now())
            .await?,
    ))
}

/// `POST /internal/jobs/claim`: the long poll, up to `wait_s` (default 25, at most 30). `200 Job`,
/// or `204` when nothing arrived (also on shutdown, so the worker reconnects). A claim that moved
/// the scan from `queued` to `analysing` emits that `status_changed` here.
#[utoipa::path(post, path = "/internal/jobs/claim", tag = "worker", security(("worker" = [])), request_body = ClaimRequest,
    responses((status = 200, body = Job), (status = 204), (status = 401, body = ErrorBody)))]
pub async fn claim(
    State(state): State<SharedState>,
    WorkerAuth(worker): WorkerAuth,
    ApiJson(body): ApiJson<ClaimRequest>,
) -> Result<Response, ApiError> {
    if body.worker_id != worker {
        return Err(ApiError::unauthenticated().message("the token belongs to another worker"));
    }
    let wait = std::time::Duration::from_secs(u64::from(body.wait_s()));
    let claimed = state
        .broker
        .claim(&worker, &body.caps, body.slots, wait, &state.shutdown)
        .await?;
    let Some(claimed) = claimed else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    if claimed.started_analysing
        && let Some(run) = claimed.job.run
    {
        let event = ScanEventBody::StatusChanged {
            from: ScanStatus::Queued,
            to: ScanStatus::Analysing,
        };
        if let Err(error) = state
            .writers
            .emit(claimed.job.scan_id, run, vec![event])
            .await
        {
            tracing::warn!(job = %claimed.job.job_id, error = %error, "status_changed not emitted");
        }
    }
    Ok((StatusCode::OK, Json(claimed.job)).into_response())
}

/// `POST /internal/jobs/{id}/events`: stage events and partial results, turned into scan events
/// by the pipeline (a partial result's findings, or `finding_retracted` when it changed).
/// `measurement` is never a worker stage: `api` reports it itself. Events that arrive after the
/// job completed with the same token (a late batch) get `204` and are dropped, never forwarded:
/// the result is final and its events were sent at `complete`.
#[utoipa::path(post, path = "/internal/jobs/{id}/events", tag = "worker", security(("worker" = [])), request_body = WorkerEvents,
    params(("id" = String, Path), ("X-Lease-Token" = String, Header)),
    responses((status = 204), (status = 409, body = ErrorBody)))]
pub async fn job_events(
    State(state): State<SharedState>,
    JobLease(lease): JobLease,
    ApiJson(body): ApiJson<WorkerEvents>,
) -> Result<StatusCode, ApiError> {
    let Some(job) = state
        .ports
        .store
        .live_lease(&lease, state.ports.clock.now())
        .await?
    else {
        tracing::debug!(job = %lease.job, "events after complete dropped");
        return Ok(StatusCode::NO_CONTENT);
    };
    let events = body
        .events
        .into_iter()
        .filter(|e| {
            !matches!(
                e,
                domain::jobs::WorkerEvent::StageStarted {
                    stage: Stage::Measurement
                }
            )
        })
        .collect();
    state.pipeline.worker_events(&lease, job, events).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /internal/jobs/{id}/complete`: the result, read by the job's recorded kind (contracts
/// v0.9), or a failure. `204` also for a repeat with the same token after completion. A job out
/// of attempts fails for good inside `Pipeline::job_completed` (the scan, the question or the
/// guided check, by kind), whose follow-up comes back in `handled`: it is applied once.
#[utoipa::path(post, path = "/internal/jobs/{id}/complete", tag = "worker", security(("worker" = [])), request_body = CompleteJob,
    params(("id" = String, Path), ("X-Lease-Token" = String, Header)),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn complete(
    State(state): State<SharedState>,
    JobLease(lease): JobLease,
    ApiJson(body): ApiJson<CompleteJob>,
) -> Result<StatusCode, ApiError> {
    // `check_lease` also accepts a job already completed with this token, so a retried
    // `complete` reaches `complete_job`, which answers `AlreadyCompleted` (contracts 5: `204`,
    // nothing changes).
    let job = state
        .ports
        .store
        .check_lease(&lease, state.ports.clock.now())
        .await?;
    let handled = state.pipeline.job_completed(&lease, job, body).await?;
    state.spawn_follow_up(handled.follow_up);
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /internal/jobs/{id}/images/{image_id}`: a job's image, authorised by its lease: only an
/// image the job's payload names, only while the lease holds.
#[utoipa::path(get, path = "/internal/jobs/{id}/images/{image_id}", tag = "worker",
    params(("id" = String, Path), ("image_id" = String, Path), ("X-Lease-Token" = String, Header)),
    responses((status = 200, content_type = "image/webp"), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn job_image(
    State(state): State<SharedState>,
    JobLease(lease): JobLease,
    ApiPath(ids): ApiPath<(JobId, ImageId)>,
) -> Result<Response, ApiError> {
    let object = state
        .ports
        .store
        .job_image(&lease, ids.1, state.ports.clock.now())
        .await?
        .ok_or_else(ApiError::not_found)?;
    media::stream_object(&*state.ports.blob, object, &state.shutdown).await
}
