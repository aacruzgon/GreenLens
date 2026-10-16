//! Memories ([contracts 8], memory).

use axum::{extract::State, http::StatusCode, response::Response};

use domain::{
    authz::{Editor, Owner, Viewer},
    chat::{CreateMemoryRequest, Memory, MemoryKind, MemoryQuery, MemorySource},
    error::ErrorBody,
    id::table,
    ports::{IdGenExt, NewMemory},
    wire::{Page, PageRequest},
};

use crate::{
    error::ApiError,
    extract::{ApiQuery, access::Access, auth::AuthUser, idempotency::IdemJson},
    reply::Wire,
    state::SharedState,
};

/// `GET /memories?garden_id=&plant_id=`: shared memories plus the caller's own chat memories,
/// newest first. Any member.
#[utoipa::path(get, path = "/memories", tag = "memories", params(MemoryQuery, PageRequest),
    responses((status = 200, body = Page<Memory>), (status = 404, body = ErrorBody)))]
pub async fn list_memories(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    ApiQuery(query): ApiQuery<MemoryQuery>,
    ApiQuery(page): ApiQuery<PageRequest>,
) -> Result<Wire<Page<Memory>>, ApiError> {
    let member = state.member::<Viewer>(&user, query.garden_id).await?;
    Ok(Wire::ok(
        state.ports.memory.memories(member, query, page).await?,
    ))
}

/// `POST /memories`: a garden note, `source: "user"`, embedded on CPU. Editor.
#[utoipa::path(post, path = "/memories", tag = "memories", request_body = CreateMemoryRequest,
    params(("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    responses((status = 201, body = Memory), (status = 403, body = ErrorBody)))]
pub async fn create_memory(
    State(state): State<SharedState>,
    request: IdemJson<CreateMemoryRequest>,
) -> Response {
    let IdemJson { user, body, begun } = request;
    let result = async {
        let editor = state.member::<Editor>(&user, body.garden_id).await?;
        let text = body.text.trim().to_owned();
        if text.is_empty() || text.chars().count() > 500 {
            return Err(ApiError::invalid_request("text", "1-500 characters"));
        }
        let model = state.ports.embedder.model();
        let embedding = state
            .ports
            .embedder
            .embed(
                domain::ports::EmbedMode::Document,
                std::slice::from_ref(&text),
            )
            .await?
            .pop()
            .ok_or_else(|| ApiError::internal("the embedder returned no vector"))?;
        let memory = NewMemory {
            id: state.ports.ids.id(),
            plant: body.plant_id,
            kind: MemoryKind::Fact,
            text,
            embedding,
            embedding_model: model.name,
            source_msg: None,
            created_at: state.ports.clock.now(),
        };
        Ok(Wire::created(
            state.ports.memory.add_note(editor, memory).await?,
        ))
    }
    .await;
    begun.finish(result).await
}

/// `POST /memories/{id}/share`: the chat memory's author, if an editor or owner.
#[utoipa::path(post, path = "/memories/{id}/share", tag = "memories", params(("id" = String, Path)),
    responses((status = 200, body = Memory), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub async fn share_memory(
    State(state): State<SharedState>,
    access: Access<Editor, table::Memory>,
) -> Result<Wire<Memory>, ApiError> {
    let (source, author) = state
        .ports
        .memory
        .memory_owner(access.id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    // Someone else's private memory is invisible: 404, never 403.
    if source != MemorySource::Chat || author != Some(access.user.id()) {
        return Err(ApiError::not_found());
    }
    Ok(Wire::ok(
        state.ports.memory.share(access.member, access.id).await?,
    ))
}

/// `DELETE /memories/{id}`: the author of a chat memory; an owner for shared memories.
#[utoipa::path(delete, path = "/memories/{id}", tag = "memories", params(("id" = String, Path)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub async fn delete_memory(
    State(state): State<SharedState>,
    access: Access<Viewer, table::Memory>,
) -> Result<StatusCode, ApiError> {
    let (source, author) = state
        .ports
        .memory
        .memory_owner(access.id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    match source {
        MemorySource::Chat if author == Some(access.user.id()) => {}
        MemorySource::Chat => return Err(ApiError::not_found()),
        MemorySource::User | MemorySource::Pipeline => {
            access.member.require::<Owner>()?;
        }
    }
    state.ports.memory.delete(access.member, access.id).await?;
    Ok(StatusCode::NO_CONTENT)
}
