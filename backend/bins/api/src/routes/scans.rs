//! Scans ([contracts 3]; [architecture 8.1]).

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{
        Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures::Stream;

use domain::{
    authz::{Editor, Reviewer, Viewer},
    error::ErrorBody,
    events::ScanEvent,
    id::{FeedbackId, ImageId, ScanId, table},
    ports::{IdGenExt, NewFeedback, NewScan, NewScanPlant},
    rules::limits::SSE_KEEPALIVE,
    scan::{
        AnswerRequest, CreateScanRequest, FeedbackCreated, FeedbackRequest, Scan, ScanDetail,
        ScanPlant, UploadImageResponse,
    },
    url::Sign,
    wire::EventSeq,
};

use crate::{
    error::ApiError,
    events::stream::scan_stream,
    extract::{
        ApiMultipart, ApiPath, RequestIdHeader,
        access::Access,
        idempotency::{self, Begin, IdemJson},
    },
    reply::Wire,
    state::SharedState,
    uploads,
};

/// `POST /scans`: a scan in `pending`, with a new plant when `new_plant` is given. Counts toward
/// the daily scan limit (30), in the scan's transaction (`429 budget_exceeded`).
#[utoipa::path(post, path = "/scans", tag = "scans",
    request_body = CreateScanRequest,
    params(("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    responses((status = 201, body = Scan), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody), (status = 429, body = ErrorBody)))]
pub async fn create_scan(
    State(state): State<SharedState>,
    request: IdemJson<CreateScanRequest>,
) -> Response {
    let IdemJson { user, body, begun } = request;
    let result = async {
        let editor = state.member::<Editor>(&user, body.garden_id).await?;
        let plant = match body.plant()? {
            ScanPlant::Existing(id) => NewScanPlant::Existing(id),
            ScanPlant::New(plant) => NewScanPlant::New {
                id: state.ports.ids.id(),
                plant,
            },
        };
        let scan = state
            .ports
            .store
            .create_scan(
                editor,
                NewScan {
                    id: state.ports.ids.id(),
                    plant,
                    created_by: user.id(),
                    created_at: state.ports.clock.now(),
                },
            )
            .await?;
        Ok(Wire::created(scan.sign((), &state.signer())))
    }
    .await;
    begun.finish(result).await
}

/// `GET /scans/{id}`: the detail, with fresh signed URLs.
#[utoipa::path(get, path = "/scans/{id}", tag = "scans",
    params(("id" = String, Path, description = "Scan id")),
    responses((status = 200, body = ScanDetail), (status = 404, body = ErrorBody)))]
pub async fn get_scan(
    State(state): State<SharedState>,
    access: Access<Viewer, table::Scan>,
) -> Result<Wire<ScanDetail>, ApiError> {
    let detail = state
        .ports
        .store
        .scan_detail(access.member, access.id)
        .await?;
    Ok(Wire::ok(detail.sign((), &state.signer())))
}

/// `DELETE /scans/{id}`: only a `pending` scan (`409 invalid_state` with `status` otherwise,
/// checked in the store's transaction). `deleting_at` is set first, then the cascade runs in the
/// background.
#[utoipa::path(delete, path = "/scans/{id}", tag = "scans",
    params(("id" = String, Path, description = "Scan id")),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn delete_scan(
    State(state): State<SharedState>,
    access: Access<Editor, table::Scan>,
) -> Result<StatusCode, ApiError> {
    let started = state
        .ports
        .store
        .start_scan_deletion(access.member, access.id, state.ports.clock.now())
        .await?;
    state.deletions.spawn(started);
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /scans/{id}/images`: see [`uploads`]. The multipart body is read whole (within its
/// caps) before the idempotency record begins, because the fingerprint covers every part.
#[utoipa::path(post, path = "/scans/{id}/images", tag = "scans",
    params(("id" = String, Path, description = "Scan id"),
        ("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    request_body(content_type = "multipart/form-data", description = "kind, file, question_id?, depth?, depth_confidence?, depth_meta?"),
    responses((status = 201, body = UploadImageResponse), (status = 409, body = ErrorBody),
        (status = 413, body = ErrorBody), (status = 415, body = ErrorBody), (status = 422, body = ErrorBody),
        (status = 503, body = ErrorBody)))]
pub async fn upload_image(
    State(state): State<SharedState>,
    access: Access<Editor, table::Scan>,
    headers: HeaderMap,
    request_id: RequestIdHeader,
    ApiMultipart(multipart): ApiMultipart,
) -> Result<Response, ApiError> {
    let form = uploads::read_form(multipart).await?;
    let path = format!("/scans/{}/images", access.id);
    let begun = match idempotency::begin(
        &state,
        access.user.id(),
        &axum::http::Method::POST,
        &path,
        &headers,
        request_id.0,
        &form.fingerprint(),
    )
    .await?
    {
        Begin::Fresh(begun) => begun,
        Begin::Replay(response) => return Ok(response),
    };
    let result = uploads::store_scan_image(&state, access.member, access.id, form).await;
    Ok(begun.finish(result).await)
}

/// `DELETE /scans/{id}/images/{image_id}`: while `pending`; the photo's depth and confidence
/// maps go with it, deleted detached.
#[utoipa::path(delete, path = "/scans/{id}/images/{image_id}", tag = "scans",
    params(("id" = String, Path, description = "Scan id"), ("image_id" = String, Path, description = "Image id")),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn delete_image(
    State(state): State<SharedState>,
    access: Access<Editor, table::Scan>,
    ApiPath(ids): ApiPath<(ScanId, ImageId)>,
) -> Result<StatusCode, ApiError> {
    let keys = state
        .ports
        .store
        .remove_image(access.member, access.id, ids.1)
        .await?;
    // Through the delete queue: detached, never cancelled, retried; what it can't delete the
    // orphan sweep finds, since the record is gone.
    drop(state.deletes.submit(blob::DeleteJob::Keys(keys)));
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /scans/{id}/submit`: no body. `202` when a run starts, `200` when nothing changes
/// (submit is idempotent); `409 invalid_state` with `hint: "new_scan"` after a non-retryable
/// failure; `422 missing_required_shot`.
#[utoipa::path(post, path = "/scans/{id}/submit", tag = "scans",
    params(("id" = String, Path, description = "Scan id")),
    responses((status = 202, body = Scan), (status = 200, body = Scan), (status = 409, body = ErrorBody),
        (status = 422, body = ErrorBody), (status = 503, body = ErrorBody)))]
pub async fn submit(
    State(state): State<SharedState>,
    access: Access<Editor, table::Scan>,
) -> Result<Wire<Scan>, ApiError> {
    let outcome = state.pipeline.submit(access.member, access.id).await?;
    let status = if outcome.started {
        StatusCode::ACCEPTED
    } else {
        StatusCode::OK
    };
    Ok(Wire::with_status(
        status,
        outcome.scan.sign((), &state.signer()),
    ))
}

/// `POST /scans/{id}/answers`: an indicator question's answer; rescored in the background.
#[utoipa::path(post, path = "/scans/{id}/answers", tag = "scans",
    params(("id" = String, Path, description = "Scan id"),
        ("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    request_body = AnswerRequest,
    responses((status = 202, body = Scan), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn answer(
    State(state): State<SharedState>,
    access: Access<Editor, table::Scan>,
    request: IdemJson<AnswerRequest>,
) -> Response {
    let IdemJson { body, begun, .. } = request;
    let result = async {
        let handled = state
            .pipeline
            .answer(access.member, access.id, body)
            .await?;
        state.spawn_follow_up(handled.follow_up);
        Ok(Wire::accepted(handled.value.sign((), &state.signer())))
    }
    .await;
    begun.finish(result).await
}

/// `POST /scans/{id}/feedback`: reviewer or above; `409 invalid_state` while `pending`.
#[utoipa::path(post, path = "/scans/{id}/feedback", tag = "scans",
    params(("id" = String, Path, description = "Scan id"),
        ("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    request_body = FeedbackRequest,
    responses((status = 201, body = FeedbackCreated), (status = 403, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn feedback(
    State(state): State<SharedState>,
    access: Access<Reviewer, table::Scan>,
    request: IdemJson<FeedbackRequest>,
) -> Response {
    let IdemJson { body, begun, .. } = request;
    let result = async {
        let id: FeedbackId = state
            .ports
            .store
            .record_feedback(
                access.member,
                access.id,
                NewFeedback {
                    id: state.ports.ids.id(),
                    user: access.user.id(),
                    request: body,
                },
            )
            .await?;
        Ok(Wire::created(FeedbackCreated { id }))
    }
    .await;
    begun.finish(result).await
}

/// `GET /scans/{id}/events`: the resumable SSE stream ([`scan_stream`]). Everything that can
/// fail with an `ErrorBody` happens before the first byte: authentication, membership (`404` for
/// non-members) and `Last-Event-ID` parsing.
#[utoipa::path(get, path = "/scans/{id}/events", tag = "scans",
    params(("id" = String, Path, description = "Scan id"),
        ("Last-Event-ID" = Option<u64>, Header, description = "The last seq the app applied")),
    responses((status = 200, content_type = "text/event-stream", body = ScanEvent,
        description = "`id: seq`, `event: type`, `data: ScanEvent`; `: keepalive` every 15 s"),
        (status = 404, body = ErrorBody)))]
pub async fn events(
    State(state): State<SharedState>,
    access: Access<Viewer, table::Scan>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>> + Send + use<>>, ApiError>
{
    let after = match headers.get("last-event-id") {
        None => EventSeq::ZERO,
        Some(value) => value
            .to_str()
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .map(EventSeq)
            .ok_or_else(|| ApiError::invalid_request("Last-Event-ID", "not a sequence number"))?,
    };
    let stream = scan_stream(state, access.member, access.id, after);
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(SSE_KEEPALIVE).text("keepalive")))
}
