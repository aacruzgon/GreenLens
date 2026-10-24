//! Chat: conversations, images, the message stream, feedback and pending actions
//! ([contracts 8]; [architecture 8.2]).

use std::{convert::Infallible, sync::Arc};

use axum::{
    extract::State,
    http::{HeaderMap, Method, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures::StreamExt;

use domain::{
    authz::{Editor, Viewer},
    chat::{
        ChatEvent, ChatImageUploaded, ConfirmResponse, ConfirmResult, Conversation,
        ConversationDetail, ConversationQuery, CreateConversationRequest, MessageFeedbackRequest,
        MessageReplay, PendingStatus, Preview, RejectResponse, SendMessageRequest, WriteTool,
    },
    error::{ErrorBody, InvalidStateDetails, InvalidStateHint},
    garden::tasks::UpdateTaskRequest,
    id::{ConversationId, table},
    ports::{IdGenExt, NewConversation, NewTask, PlanWrite, StoredResponse},
    records::{
        plan::{ActionPlan, PlanReason},
        task::TaskSource,
    },
    rules::limits::SSE_KEEPALIVE,
    url::Sign,
    wire::{Page, PageRequest},
};

use crate::{
    error::ApiError,
    extract::{
        ApiJson, ApiMultipart, ApiQuery, RequestIdHeader,
        access::ConvAccess,
        auth::AuthUser,
        idempotency::{self, Begin, IdemJson},
    },
    reply::Wire,
    seams::{PreviewError, TurnOutcome, TurnRequest},
    state::SharedState,
    uploads,
};

/// `POST /conversations`: anchored to a scan, plant or garden of the garden. Any member.
#[utoipa::path(post, path = "/conversations", tag = "chat", request_body = CreateConversationRequest,
    params(("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    responses((status = 201, body = Conversation), (status = 404, body = ErrorBody)))]
pub async fn create_conversation(
    State(state): State<SharedState>,
    request: IdemJson<CreateConversationRequest>,
) -> Response {
    let IdemJson { user, body, begun } = request;
    let result = async {
        let member = state.member::<Viewer>(&user, body.garden_id).await?;
        let conversation = NewConversation {
            id: state.ports.ids.id(),
            user: user.id(),
            anchor: body.anchor,
            created_at: state.ports.clock.now(),
        };
        Ok(Wire::created(
            state
                .ports
                .store
                .create_conversation(member, conversation)
                .await?,
        ))
    }
    .await;
    begun.finish(result).await
}

/// `GET /conversations?garden_id=&anchor_type=&anchor_id=`: the caller's own, newest
/// `last_message_at` first.
#[utoipa::path(get, path = "/conversations", tag = "chat", params(ConversationQuery, PageRequest),
    responses((status = 200, body = Page<Conversation>), (status = 404, body = ErrorBody)))]
pub async fn list_conversations(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    ApiQuery(query): ApiQuery<ConversationQuery>,
    ApiQuery(page): ApiQuery<PageRequest>,
) -> Result<Wire<Page<Conversation>>, ApiError> {
    let member = state.member::<Viewer>(&user, query.garden_id).await?;
    Ok(Wire::ok(
        state.ports.store.conversations(member, query, page).await?,
    ))
}

/// `GET /conversations/{id}`: with its messages, newest first. Owner of the conversation.
#[utoipa::path(get, path = "/conversations/{id}", tag = "chat", params(("id" = String, Path), PageRequest),
    responses((status = 200, body = ConversationDetail), (status = 404, body = ErrorBody)))]
pub async fn get_conversation(
    State(state): State<SharedState>,
    access: ConvAccess<Viewer, table::Conversation>,
    ApiQuery(page): ApiQuery<PageRequest>,
) -> Result<Wire<ConversationDetail>, ApiError> {
    let detail = state
        .ports
        .store
        .conversation_detail(access.owner, page)
        .await?;
    Ok(Wire::ok(detail.sign((), &state.signer())))
}

/// `POST /conversations/{id}/images`: the scan upload's pipeline, without the quality check and
/// depth.
#[utoipa::path(post, path = "/conversations/{id}/images", tag = "chat",
    params(("id" = String, Path), ("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    request_body(content_type = "multipart/form-data", description = "file"),
    responses((status = 201, body = ChatImageUploaded), (status = 413, body = ErrorBody), (status = 415, body = ErrorBody),
        (status = 503, body = ErrorBody)))]
pub async fn upload_chat_image(
    State(state): State<SharedState>,
    access: ConvAccess<Viewer, table::Conversation>,
    headers: HeaderMap,
    request_id: RequestIdHeader,
    ApiMultipart(multipart): ApiMultipart,
) -> Result<Response, ApiError> {
    let form = uploads::read_form(multipart).await?;
    let path = format!("/conversations/{}/images", access.id);
    let begun = match idempotency::begin(
        &state,
        access.user.id(),
        &Method::POST,
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
    let result = uploads::store_chat_image(&state, access.owner, form).await;
    Ok(begun.finish(result).await)
}

/// `POST /conversations/{id}/messages`: the agent's turn as SSE, events without `id:` lines (not
/// resumable). Errors before the stream starts (budget, validation) are ordinary error bodies.
///
/// The stream is the agent's `'static` event stream mapped to SSE frames. If the client goes
/// away, hyper drops the body, which drops the agent's receiver; the turn task sees its next send
/// fail, stops the LLM call and stores the message with `interrupted: true`. Shutdown ends the
/// stream through `take_until`, the same way.
///
/// The idempotency record finishes when the turn task reports its outcome: a completed message is
/// stored as the replay `200 { message }`; an interrupted or failed one releases the key, so the
/// app's "resend", with a new key, runs again.
#[utoipa::path(post, path = "/conversations/{id}/messages", tag = "chat", request_body = SendMessageRequest,
    params(("id" = String, Path), ("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    responses((status = 200,
            content((ChatEvent = "text/event-stream"), (MessageReplay = "application/json")),
            description = "`event: name`, `data: ChatEvent` as SSE; a finished turn's replay is `application/json` MessageReplay"),
        (status = 400, body = ErrorBody), (status = 404, body = ErrorBody), (status = 429, body = ErrorBody)))]
pub async fn send_message(
    State(state): State<SharedState>,
    access: ConvAccess<Viewer, table::Conversation>,
    request: IdemJson<SendMessageRequest>,
) -> Response {
    let IdemJson { body, begun, .. } = request;
    let turn = state
        .chat
        .start_turn(TurnRequest {
            owner: access.owner,
            message: body,
            shutdown: state.shutdown.child_token(),
        })
        .await;
    let parts = match turn {
        Ok(parts) => parts,
        Err(error) => return begun.finish::<MessageReplay>(Err(error.into())).await,
    };
    let conversation: ConversationId = access.id;
    let outcome = parts.outcome;
    let finisher = Arc::clone(&state);
    state.tasks.spawn(async move {
        match outcome.await {
            Ok(TurnOutcome::Completed(message)) => {
                let replay = MessageReplay {
                    message: message.sign(conversation, &finisher.signer()),
                };
                match serde_json::to_value(&replay) {
                    Ok(body) => begun.complete(StoredResponse { status: 200, body }).await,
                    Err(_) => begun.release().await,
                }
            }
            _ => begun.release().await,
        }
    });
    let frames = parts
        .events
        .map(|event| {
            Ok::<_, Infallible>(
                Event::default()
                    .event(event.event_name())
                    .json_data(&event)
                    .unwrap_or_else(|_| Event::default().comment("unserialisable event")),
            )
        })
        .take_until(state.shutdown.clone().cancelled_owned());
    Sse::new(frames)
        .keep_alive(KeepAlive::new().interval(SSE_KEEPALIVE).text("keepalive"))
        .into_response()
}

/// `POST /messages/{id}/feedback`: `204`; the conversation's owner, assistant messages only.
#[utoipa::path(post, path = "/messages/{id}/feedback", tag = "chat", params(("id" = String, Path)),
    request_body = MessageFeedbackRequest,
    responses((status = 204), (status = 404, body = ErrorBody)))]
pub async fn message_feedback(
    State(state): State<SharedState>,
    access: ConvAccess<Viewer, table::Message>,
    ApiJson(body): ApiJson<MessageFeedbackRequest>,
) -> Result<StatusCode, ApiError> {
    state
        .ports
        .store
        .rate_message(access.owner, access.id, body)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /pending-actions/{id}/confirm`: the conversation's owner, an editor or owner of the
/// garden.
///
/// | State | Result |
/// |---|---|
/// | `confirmed` (a retry) | `200` with the first result |
/// | `rejected`, `expired`, or past `expires_at` | `409 invalid_state` with `status` |
/// | `proposed`, a task card | the task is created (source `chat`) or completed; a plant deleted since: `409` with `hint: "plant_removed"` |
/// | `proposed`, a plan card | under the scan's plan lock: the preview applied to the current plan exactly as shown, stored as a new version with reason `chat_revision`; a base that is no longer current: `409` with `current_plan_id`; a neighbour deleted since: `409` with `hint: "neighbour_removed"`. Never retried |
#[utoipa::path(post, path = "/pending-actions/{id}/confirm", tag = "chat", params(("id" = String, Path)),
    responses((status = 200, body = ConfirmResponse), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody)))]
pub async fn confirm_pending_action(
    State(state): State<SharedState>,
    access: ConvAccess<Editor, table::PendingAction>,
) -> Result<Wire<ConfirmResponse>, ApiError> {
    let record = state
        .ports
        .store
        .pending_action(access.owner, access.id)
        .await?;
    let action = record.action;
    let now = state.ports.clock.now();
    match action.status {
        PendingStatus::Confirmed => {
            return Ok(Wire::ok(ConfirmResponse {
                pending_action: action,
                result: record.result.unwrap_or_default(),
            }));
        }
        PendingStatus::Rejected | PendingStatus::Expired => {
            return Err(ApiError::invalid_state(InvalidStateDetails {
                status: Some(action.status.as_str().to_owned()),
                allowed: Some(vec![PendingStatus::Proposed.as_str().to_owned()]),
                ..InvalidStateDetails::default()
            }));
        }
        PendingStatus::Proposed if action.expires_at <= now => {
            return Err(ApiError::invalid_state(InvalidStateDetails {
                status: Some(PendingStatus::Expired.as_str().to_owned()),
                ..InvalidStateDetails::default()
            }));
        }
        PendingStatus::Proposed => {}
    }
    let editor = access.owner.membership();
    let result = match (&action.tool, &action.preview) {
        (WriteTool::CreateTask, Preview::Task { task }) => {
            let created = confirm_new_task(&state, editor, access.user.id(), task, now).await?;
            ConfirmResult {
                task: Some(created),
                plan: None,
            }
        }
        (WriteTool::CompleteTask, Preview::Task { task }) => {
            let patch = UpdateTaskRequest {
                done: Some(true),
                dismissed: None,
                title: None,
                detail: None,
                due: None,
                repeat: None,
            };
            let done = state
                .ports
                .store
                .update_task(editor, task.id, patch, None, now)
                .await?;
            ConfirmResult {
                task: Some(done),
                plan: None,
            }
        }
        (WriteTool::RevisePlan, Preview::Plan { base_plan_id, .. }) => {
            let plan = confirm_revision(&state, editor, *base_plan_id, &action, now).await?;
            ConfirmResult {
                task: None,
                plan: Some(plan),
            }
        }
        _ => {
            return Err(ApiError::internal(
                "pending action tool and preview disagree",
            ));
        }
    };
    let confirmed = state
        .ports
        .store
        .mark_confirmed(access.owner, access.id, result.clone(), now)
        .await?;
    Ok(Wire::ok(ConfirmResponse {
        pending_action: confirmed,
        result,
    }))
}

/// The task card: creates the task with source `chat`; a plant deleted since answers `409` with
/// `hint: "plant_removed"`.
async fn confirm_new_task(
    state: &SharedState,
    editor: domain::authz::Membership<Editor>,
    user: domain::id::UserId,
    task: &domain::records::task::Task,
    now: domain::wire::Timestamp,
) -> Result<domain::records::task::Task, ApiError> {
    let new = NewTask {
        id: state.ports.ids.id(),
        plant: task.plant_id,
        plan: None,
        plan_step: None,
        scan: task.scan_id,
        title: task.title.clone(),
        detail: task.detail.clone(),
        due: task.due,
        repeat: task.repeat,
        source: TaskSource::Chat,
        trigger: None,
        alert_key: None,
        created_by: Some(user),
        created_at: now,
    };
    state
        .ports
        .store
        .create_task(editor, new)
        .await
        .map_err(|e| match e {
            domain::ports::StoreError::NotFound => ApiError::from(
                domain::error::DomainError::invalid_state_hint(InvalidStateHint::PlantRemoved),
            ),
            other => other.into(),
        })
}

/// The plan card: read the base plan, apply the preview exactly as shown, resolve its citations,
/// and hand the write to the pipeline, which takes the scan's plan lock, compares and sets
/// `scan.plan` (`409` with `current_plan_id` when the base isn't current), and emits
/// `plan_superseded` with reason `chat_revision`.
async fn confirm_revision(
    state: &SharedState,
    editor: domain::authz::Membership<Editor>,
    base_plan_id: domain::id::PlanId,
    action: &domain::chat::PendingAction,
    now: domain::wire::Timestamp,
) -> Result<ActionPlan, ApiError> {
    let base = state
        .ports
        .store
        .plan(editor.at_least(), base_plan_id)
        .await?;
    let revised = state
        .chat
        .apply_preview(&base, &action.preview, action.id, now)
        .map_err(|e| match e {
            PreviewError::BaseMismatch { current, .. } => {
                ApiError::invalid_state(InvalidStateDetails {
                    current_plan_id: Some(current),
                    ..InvalidStateDetails::default()
                })
            }
            PreviewError::NotAPlan => ApiError::internal(e),
        })?;
    let citations = state
        .ports
        .knowledge
        .citations(&revised.citation_ids)
        .await?;
    // The base's farm records that the revised steps still cite (connectors 7).
    let farm_records = base
        .farm_records
        .iter()
        .filter(|c| {
            revised
                .steps
                .iter()
                .any(|s| s.farm_citations.contains(&c.record_id))
        })
        .cloned()
        .collect();
    let plan = ActionPlan {
        id: state.ports.ids.id(),
        scan_id: base.scan_id,
        plant_id: base.plant_id,
        version: base.version + 1,
        supersedes: Some(base.id),
        provisional: base.provisional,
        diagnosis_id: base.diagnosis_id,
        open_question: base.open_question.clone(),
        constraints: revised.constraints,
        summary: revised.summary,
        steps: revised.steps,
        watch_for: revised.watch_for,
        citations,
        farm_records,
        task_ids: Vec::new(),
        llm_model: base.llm_model.clone(),
        prompt_version: base.prompt_version.clone(),
        created_at: now,
    };
    let write = PlanWrite {
        plan,
        reason: PlanReason::ChatRevision,
        expected: Some(base.id),
        tasks: Vec::new(),
        triggers: Vec::new(),
    };
    state
        .pipeline
        .store_revision(editor, base.scan_id, write)
        .await
        .map_err(|e| match ApiError::from(e) {
            error if error.code() == domain::error::ErrorCode::NotFound => ApiError::from(
                domain::error::DomainError::invalid_state_hint(InvalidStateHint::NeighbourRemoved),
            ),
            error => error,
        })
}

/// `POST /pending-actions/{id}/reject`: `200 { pending_action }`.
#[utoipa::path(post, path = "/pending-actions/{id}/reject", tag = "chat", params(("id" = String, Path)),
    responses((status = 200, body = RejectResponse), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn reject_pending_action(
    State(state): State<SharedState>,
    access: ConvAccess<Editor, table::PendingAction>,
) -> Result<Wire<RejectResponse>, ApiError> {
    let pending_action = state
        .ports
        .store
        .reject_pending_action(access.owner, access.id, state.ports.clock.now())
        .await?;
    Ok(Wire::ok(RejectResponse { pending_action }))
}
