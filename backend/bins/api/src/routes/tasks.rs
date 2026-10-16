//! Tasks ([contracts 9], tasks; [contracts 7], task).

use axum::{extract::State, http::StatusCode, response::Response};

use domain::{
    authz::{Editor, Viewer},
    error::{ErrorBody, InvalidStateDetails},
    garden::tasks::{CreateTaskRequest, TaskQuery, UpdateTaskRequest},
    id::table,
    ports::{IdGenExt, NewTask},
    records::task::{Task, TaskSource},
    wire::{Page, PageRequest},
};

use crate::{
    error::ApiError,
    extract::{ApiJson, ApiQuery, access::Access, auth::AuthUser, idempotency::IdemJson},
    reply::Wire,
    state::SharedState,
};

/// `GET /tasks?garden_id=&source=&state=`: by `due` (none last), then `created_at`.
/// `?source=forecast_alert` is the alert inbox. Any member.
#[utoipa::path(get, path = "/tasks", tag = "tasks", params(TaskQuery, PageRequest),
    responses((status = 200, body = Page<Task>), (status = 404, body = ErrorBody)))]
pub async fn list_tasks(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    ApiQuery(query): ApiQuery<TaskQuery>,
    ApiQuery(page): ApiQuery<PageRequest>,
) -> Result<Wire<Page<Task>>, ApiError> {
    let member = state.member::<Viewer>(&user, query.garden_id).await?;
    Ok(Wire::ok(
        state.ports.store.tasks(member, query, page).await?,
    ))
}

/// `POST /tasks`: `source: "user"`. A plant or scan that isn't live in the garden is `404`,
/// checked in the insert's transaction. Editor.
#[utoipa::path(post, path = "/tasks", tag = "tasks", request_body = CreateTaskRequest,
    params(("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    responses((status = 201, body = Task), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub async fn create_task(
    State(state): State<SharedState>,
    request: IdemJson<CreateTaskRequest>,
) -> Response {
    let IdemJson { user, body, begun } = request;
    let result = async {
        let editor = state.member::<Editor>(&user, body.garden_id).await?;
        let title = body.title.trim().to_owned();
        if title.is_empty() || title.chars().count() > 200 {
            return Err(ApiError::invalid_request("title", "1-200 characters"));
        }
        let task = NewTask {
            id: state.ports.ids.id(),
            plant: body.plant_id,
            plan: None,
            plan_step: None,
            scan: body.scan_id,
            title,
            detail: body.detail,
            due: body.due,
            repeat: body.repeat,
            source: TaskSource::User,
            trigger: None,
            alert_key: None,
            created_by: Some(user.id()),
            created_at: state.ports.clock.now(),
        };
        Ok(Wire::created(
            state.ports.store.create_task(editor, task).await?,
        ))
    }
    .await;
    begun.finish(result).await
}

/// `PATCH /tasks/{id}`: complete, dismiss, or edit a `user` or `chat` task. Completing a repeating
/// task creates the next occurrence in the same transaction. Editor.
#[utoipa::path(patch, path = "/tasks/{id}", tag = "tasks", params(("id" = String, Path)),
    request_body = UpdateTaskRequest,
    responses((status = 200, body = Task), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub async fn update_task(
    State(state): State<SharedState>,
    access: Access<Editor, table::Task>,
    ApiJson(patch): ApiJson<UpdateTaskRequest>,
) -> Result<Wire<Task>, ApiError> {
    let current = state
        .ports
        .store
        .task(access.member.at_least(), access.id)
        .await?;
    patch.check(current.source)?;
    let now = state.ports.clock.now();
    let next = match (patch.done, current.repeat, current.due) {
        (Some(true), Some(repeat), Some(due)) if current.done_at.is_none() => {
            repeat.next_due(due).map(|next_due| NewTask {
                id: state.ports.ids.id(),
                plant: current.plant_id,
                plan: current.plan_id,
                plan_step: current.plan_step_id.clone(),
                scan: None,
                title: current.title.clone(),
                detail: current.detail.clone(),
                due: Some(next_due),
                repeat: Some(repeat),
                source: current.source,
                trigger: None,
                alert_key: None,
                created_by: Some(access.user.id()),
                created_at: now,
            })
        }
        _ => None,
    };
    Ok(Wire::ok(
        state
            .ports
            .store
            .update_task(access.member, access.id, patch, next, now)
            .await?,
    ))
}

/// `DELETE /tasks/{id}`: `user` and `chat` tasks only; plan and forecast-alert tasks are
/// dismissed instead (`409 invalid_state`), so the scheduler never recreates one. Editor.
#[utoipa::path(delete, path = "/tasks/{id}", tag = "tasks", params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn delete_task(
    State(state): State<SharedState>,
    access: Access<Editor, table::Task>,
) -> Result<StatusCode, ApiError> {
    let current = state
        .ports
        .store
        .task(access.member.at_least(), access.id)
        .await?;
    if !current.source.editable() {
        return Err(ApiError::invalid_state(InvalidStateDetails {
            status: Some(format!("{:?}", current.source).to_lowercase()),
            ..InvalidStateDetails::default()
        }));
    }
    state
        .ports
        .store
        .delete_task(access.member, access.id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
