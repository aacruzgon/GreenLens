//! Conversations, messages, chat images, pending actions and message feedback ([contracts 8];
//! [database 4], cleanup).

use chrono::{DateTime, Utc};
use domain::{
    authz::{ConversationOwner, Editor, Membership, Viewer},
    chat::{
        Anchor, ConfirmResult, Conversation, ConversationDetail, ConversationQuery, Message,
        MessageFeedbackRequest, PendingAction,
    },
    error::{DomainError, InvalidStateDetails},
    id::{ChatImageId, ConversationId, MessageId, PendingActionId},
    ports::{
        ChatRepo, ContentPart, ContentType, ContextMessage, NewChatImage, NewConversation,
        NewMessage, NewPendingAction, ObjectKey, ObjectRef, PendingActionRecord, StoreError,
    },
    url::Stored,
    wire::{Page, PageRequest, Timestamp},
};
use serde_json::{Value as Json, json};
use surrealdb::types::{RecordId, SurrealValue, Value};

use crate::{
    SurrealStore,
    error::{DbFailure, RowError, take_failed, thrown},
    ids::{from_rid, message_feedback_rid, rid},
    read::{json_of, page as page_of, take_one},
    repo::analysis::enum_str,
    rows::{dt, from_json, to_json},
    tx::run_tx,
};

/// The statements. `tool_calls` stores only text, tool-call and tool-result parts (the agent
/// never passes reasoning or images there); the write refuses the rest.
pub mod sql {
    /// The projection of a `conversation` into `Conversation`.
    macro_rules! conversation_fields {
        () => {
            "id, garden AS garden_id, { type: record::tb(anchor), id: anchor } AS anchor, title, created_at, last_message_at"
        };
    }

    /// The projection of a `message` into `Message`, with its cards.
    macro_rules! message_fields {
        () => {
            "id, role, text, attachments, citations, farm_citations, blocks, interrupted, created_at, tool_calls,
  (SELECT id, conversation AS conversation_id, tool, status, card, preview, expires_at FROM pending_action
     WHERE message = $parent.id ORDER BY id) AS pending_actions"
        };
    }

    /// The projection of a `pending_action` into `PendingAction`, with its stored result.
    macro_rules! pending_fields {
        () => {
            "id, conversation AS conversation_id, tool, status, card, preview, expires_at, result"
        };
    }

    /// `POST /conversations`: the anchor must be a live record of the garden (the garden
    /// itself, or one of its plants or scans).
    pub const CREATE_CONVERSATION: &str = concat!(
        "BEGIN;
IF array::len(SELECT VALUE id FROM $anchor WHERE (id = $garden OR garden = $garden) AND deleting_at = NONE) = 0 { THROW 'not_found' };
CREATE $conversation SET user = $user, garden = $garden, anchor = $anchor, created_at = $now;
SELECT ", conversation_fields!(), " FROM ONLY $conversation;
COMMIT;"
    );
    /// `GET /conversations`: the caller's own, newest `last_message_at` first (index
    /// `conversation_user`), paged by `(last activity, id)`.
    pub const CONVERSATIONS: &str = concat!(
        "SELECT ",
        conversation_fields!(),
        ", last_message_at ?? created_at AS sort_at FROM conversation
WHERE user = $user AND garden = $garden AND ($anchor = NONE OR anchor = $anchor)
  AND ($after = NONE OR [last_message_at ?? created_at, id] < $after)
ORDER BY sort_at DESC, id DESC LIMIT $limit;"
    );
    /// The cursor key of a conversation.
    pub const CONVERSATION_CURSOR: &str =
        "SELECT VALUE [last_message_at ?? created_at, id] FROM ONLY $conversation;";
    /// A conversation.
    pub const CONVERSATION: &str = concat!(
        "SELECT ",
        conversation_fields!(),
        " FROM ONLY $conversation;"
    );
    /// Its messages newest first (index `message_conversation`), paged by id (ULIDs sort by
    /// time).
    pub const MESSAGES: &str = concat!(
        "SELECT ",
        message_fields!(),
        " FROM message WHERE conversation = $conversation
  AND ($after = NONE OR id < $after) ORDER BY id DESC LIMIT $limit;"
    );
    /// `POST /conversations/{id}/images`, after the object is written.
    pub const ADD_CHAT_IMAGE: &str = "CREATE $image SET conversation = $conversation, width = $width, height = $height, storage_key = $key, uploaded_at = $now;";
    /// The object behind a signed chat image URL.
    pub const CHAT_IMAGE_OBJECT: &str =
        "SELECT VALUE storage_key FROM $image WHERE conversation = $conversation;";
    /// A message, and `last_message_at`; every attachment must be the conversation's.
    pub const STORE_MESSAGE: &str = concat!(
        "BEGIN;
IF array::len(SELECT VALUE id FROM chat_image WHERE id IN $row.attachments AND conversation = $conversation) != array::len($row.attachments) { THROW 'not_found' };
CREATE $message CONTENT $row;
UPDATE $conversation SET last_message_at = $row.created_at;
SELECT ", message_fields!(), " FROM ONLY $message;
COMMIT;"
    );
    /// The summary and the most recent messages.
    pub const CONTEXT: &str = concat!(
        "SELECT VALUE summary FROM ONLY $conversation;
SELECT ",
        message_fields!(),
        " FROM message WHERE conversation = $conversation ORDER BY id DESC LIMIT $recent;"
    );
    /// The conversation's message count (index `message_conversation`).
    pub const MESSAGE_COUNT: &str =
        "RETURN count(SELECT VALUE id FROM message WHERE conversation = $conversation);";
    /// A new summary.
    pub const SET_SUMMARY: &str = "UPDATE $conversation SET summary = $summary;";
    /// A write tool's card, with the assistant message that proposed it. The tool runs inside
    /// the turn's tool loop, before the turn stores that message (agent `store_reply`), so the
    /// message usually doesn't exist yet; only a message of another conversation is refused.
    pub const CREATE_PENDING_ACTION: &str = "BEGIN;
IF record::exists($row.message) AND $row.message.conversation != $conversation { THROW 'not_found' };
CREATE $action CONTENT $row;
COMMIT;";
    /// A card of the conversation, with its stored result.
    pub const PENDING_ACTION: &str = concat!(
        "SELECT ",
        pending_fields!(),
        " FROM ONLY $action WHERE conversation = $conversation;"
    );
    /// Marks a `proposed` card `confirmed` with its result; a `confirmed` one keeps its result
    /// (a confirm retry, contracts 8).
    pub const MARK_CONFIRMED: &str = concat!(
        "BEGIN;
LET $st = (SELECT VALUE status FROM ONLY $action WHERE conversation = $conversation);
IF $st = NONE { THROW 'not_found' };
IF !($st IN ['proposed', 'confirmed']) { THROW 'invalid_state' };
IF $st = 'proposed' { UPDATE $action SET status = 'confirmed', result = $result, decided_at = $now; };
SELECT ", pending_fields!(), " FROM ONLY $action;
COMMIT;"
    );
    /// `POST /pending-actions/{id}/reject`: only a `proposed` card.
    pub const REJECT_PENDING_ACTION: &str = concat!(
        "BEGIN;
LET $st = (SELECT VALUE status FROM ONLY $action WHERE conversation = $conversation);
IF $st = NONE { THROW 'not_found' };
IF $st != 'proposed' { THROW 'invalid_state' };
UPDATE $action SET status = 'rejected', decided_at = $now;
SELECT ",
        pending_fields!(),
        " FROM ONLY $action;
COMMIT;"
    );
    /// Cleanup ([database 4] table): `proposed` cards past `expires_at` become `expired`.
    pub const EXPIRE_PENDING_ACTIONS: &str = "UPDATE pending_action SET status = 'expired' WHERE status = 'proposed' AND expires_at < $now RETURN VALUE id;";
    /// `POST /messages/{id}/feedback`: an upsert on `message_feedback:[message]`, assistant
    /// messages only.
    pub const RATE_MESSAGE: &str = "BEGIN;
IF (SELECT VALUE role FROM ONLY $message WHERE conversation = $conversation) != 'assistant' { THROW 'not_found' };
UPSERT $feedback SET rating = $rating, notes = $notes;
COMMIT;";
    /// The conversation a message belongs to.
    pub const MESSAGE_CONVERSATION: &str = "SELECT VALUE conversation FROM $message;";
}

/// A `message` row as written.
#[derive(Debug, Clone, SurrealValue)]
pub struct MessageRow {
    /// Its conversation.
    pub conversation: RecordId,
    /// `user` or `assistant`.
    pub role: String,
    /// The text.
    pub text: String,
    /// The user's images.
    pub attachments: Vec<RecordId>,
    /// `Citation[]`.
    pub citations: Vec<Json>,
    /// `FarmCitation[]` ([connectors 7]). `DEFAULT []` applies only on write, so a message
    /// stored before connectors has none: it reads as empty here and in `Message`.
    ///
    /// [connectors 7]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#7-the-agent
    #[surreal(default)]
    pub farm_citations: Vec<Json>,
    /// `AnswerBlock[]` ([contracts 8], answer blocks). A message stored before answer blocks
    /// has none and reads as empty, like `farm_citations`.
    #[surreal(default)]
    pub blocks: Vec<Json>,
    /// Text, tool-call and tool-result parts, in order; never sent to the app.
    pub tool_calls: Vec<Json>,
    /// The classifier's route.
    pub route: Option<String>,
    /// The stream dropped before it finished.
    pub interrupted: bool,
    /// The model.
    pub model: Option<String>,
    /// The prompt version.
    pub prompt_version: Option<String>,
    /// Prompt tokens.
    pub tokens_in: i64,
    /// Completion tokens.
    pub tokens_out: i64,
    /// When.
    pub created_at: DateTime<Utc>,
}

/// A `pending_action` row as written.
#[derive(Debug, Clone, SurrealValue)]
pub struct PendingActionRow {
    /// Its conversation.
    pub conversation: RecordId,
    /// The assistant message that proposed it.
    pub message: RecordId,
    /// The write tool.
    pub tool: String,
    /// `proposed`.
    pub status: String,
    /// The card.
    pub card: Json,
    /// The preview.
    pub preview: Json,
    /// 24 hours after it was made.
    pub expires_at: DateTime<Utc>,
}

/// A content part as stored in `message.tool_calls`.
pub fn part_json(part: &ContentPart) -> Result<Json, RowError> {
    match part {
        ContentPart::Text(text) => Ok(json!({ "type": "text", "text": text })),
        ContentPart::ToolCall {
            call_id,
            name,
            arguments,
        } => Ok(
            json!({ "type": "tool_call", "call_id": call_id, "name": name, "arguments": arguments }),
        ),
        ContentPart::ToolResult {
            call_id,
            content,
            is_error,
        } => Ok(
            json!({ "type": "tool_result", "call_id": call_id, "content": content, "is_error": is_error }),
        ),
        _ => Err(RowError::BadField {
            field: "tool_calls",
            problem: "only text, tool-call and tool-result parts are stored".to_owned(),
        }),
    }
}

/// A stored part back as a `ContentPart`.
pub fn part_of(json: &Json) -> Result<ContentPart, RowError> {
    let field = |k: &str| json.get(k).cloned().unwrap_or(Json::Null);
    let text = |k: &str| field(k).as_str().map(str::to_owned).unwrap_or_default();
    match json.get("type").and_then(Json::as_str) {
        Some("text") => Ok(ContentPart::Text(text("text"))),
        Some("tool_call") => Ok(ContentPart::ToolCall {
            call_id: text("call_id"),
            name: text("name"),
            arguments: field("arguments"),
        }),
        Some("tool_result") => Ok(ContentPart::ToolResult {
            call_id: text("call_id"),
            content: field("content"),
            is_error: field("is_error").as_bool().unwrap_or(false),
        }),
        other => Err(RowError::BadField {
            field: "tool_calls",
            problem: format!("unknown part {other:?}"),
        }),
    }
}

/// A message's JSON as `Message`, with its tool calls split off: attachments become
/// `{ image_id }` (the URL slot is empty in the stored state).
fn message_of(mut json: Json) -> Result<(Message<Stored>, Vec<ContentPart>), RowError> {
    let tool_calls = json
        .as_object_mut()
        .and_then(|m| m.remove("tool_calls"))
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .map(part_of)
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(attachments) = json.get_mut("attachments").and_then(Json::as_array_mut) {
        for a in attachments.iter_mut() {
            *a = json!({ "image_id": a.clone() });
        }
    }
    let message = serde_json::from_value(json).map_err(|source| RowError::Json {
        field: "message",
        source,
    })?;
    Ok((message, tool_calls))
}

/// A message with its stored tool calls.
type ParsedMessage = (Message<Stored>, Vec<ContentPart>);

fn messages_of(value: Value) -> Result<Vec<ParsedMessage>, RowError> {
    match json_of(value) {
        Json::Array(items) => items.into_iter().map(message_of).collect(),
        Json::Null => Ok(Vec::new()),
        one => Ok(vec![message_of(one)?]),
    }
}

/// A pending action's JSON split into the card and its stored result.
fn pending_of(mut json: Json) -> Result<PendingActionRecord, RowError> {
    let result = json
        .as_object_mut()
        .and_then(|m| m.remove("result"))
        .map(|r| from_json::<ConfirmResult>("result", r))
        .transpose()?;
    Ok(PendingActionRecord {
        action: from_json("pending_action", json)?,
        result,
    })
}

/// `not_found` and `invalid_state` as the store's errors.
fn chat_failure(f: DbFailure, what: &'static str) -> StoreError {
    if f.thrown() == Some(thrown::INVALID_STATE) {
        StoreError::Rejected(DomainError::InvalidState(InvalidStateDetails::default()))
    } else {
        f.into_store(what)
    }
}

impl SurrealStore {
    async fn pending_by(
        &self,
        statement: &'static str,
        owner_conversation: ConversationId,
        action: PendingActionId,
        extra: impl Fn(
            surrealdb::method::Query<'_, surrealdb::engine::any::Any>,
        ) -> surrealdb::method::Query<'_, surrealdb::engine::any::Any>
        + Send
        + Sync,
        what: &'static str,
    ) -> Result<PendingActionRecord, StoreError> {
        let mut r = run_tx(&self.db, statement, |q| {
            extra(
                q.bind(("action", rid(&action)))
                    .bind(("conversation", rid(&owner_conversation))),
            )
        })
        .await
        .map_err(|f| chat_failure(f, what))?;
        let last = r.num_statements().saturating_sub(2);
        let index = if statement.starts_with("BEGIN") {
            last
        } else {
            0
        };
        let value: Value = r.take(index).map_err(|e| take_failed(what, e))?;
        match json_of(value) {
            Json::Array(mut items) if !items.is_empty() => {
                pending_of(items.remove(0)).map_err(|e| e.into_store(what))
            }
            Json::Null | Json::Array(_) => Err(StoreError::NotFound),
            json => pending_of(json).map_err(|e| e.into_store(what)),
        }
    }
}

#[async_trait::async_trait]
impl ChatRepo for SurrealStore {
    async fn create_conversation(
        &self,
        member: Membership<Viewer>,
        conversation: NewConversation,
    ) -> Result<Conversation, StoreError> {
        const WHAT: &str = "create conversation";
        let anchor = match conversation.anchor {
            Anchor::Scan(id) => rid(&id),
            Anchor::Plant(id) => rid(&id),
            Anchor::Garden(id) => rid(&id),
        };
        let mut r = run_tx(&self.db, sql::CREATE_CONVERSATION, |q| {
            q.bind(("conversation", rid(&conversation.id)))
                .bind(("user", rid(&conversation.user)))
                .bind(("garden", rid(&member.garden())))
                .bind(("anchor", anchor.clone()))
                .bind(("now", dt(conversation.created_at)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let last = r.num_statements().saturating_sub(2);
        take_one(&mut r, last, WHAT)?.ok_or(StoreError::NotFound)
    }

    async fn conversations(
        &self,
        member: Membership<Viewer>,
        query: ConversationQuery,
        page: PageRequest,
    ) -> Result<Page<Conversation>, StoreError> {
        const WHAT: &str = "conversations";
        if query.garden_id != member.garden() {
            return Err(StoreError::NotFound);
        }
        let anchor = match (query.anchor_type, query.anchor_id) {
            (Some(t), Some(id)) => Some(RecordId::new(
                enum_str("anchor_type", &t).map_err(|e| e.into_store(WHAT))?,
                id.to_string(),
            )),
            _ => None,
        };
        let after = self
            .cursor_key(
                sql::CONVERSATION_CURSOR,
                "conversation",
                page.cursor.as_ref(),
                |c| c.parse::<ConversationId>().ok().map(|i| rid(&i)),
            )
            .await?;
        let limit = page.limit();
        let mut r = run_tx(&self.db, sql::CONVERSATIONS, |q| {
            q.bind(("user", rid(&member.user())))
                .bind(("garden", rid(&member.garden())))
                .bind(("anchor", anchor.clone()))
                .bind(("after", after.clone()))
                .bind(("limit", i64::from(limit) + 1))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<Conversation> = crate::read::take_all(&mut r, 0, WHAT)?;
        Ok(page_of(rows, limit, |c| c.id.to_string()))
    }

    async fn conversation_detail(
        &self,
        owner: ConversationOwner<Viewer>,
        page: PageRequest,
    ) -> Result<ConversationDetail<Stored>, StoreError> {
        const WHAT: &str = "conversation detail";
        let conversation = rid(&owner.conversation());
        let mut r = run_tx(&self.db, sql::CONVERSATION, |q| {
            q.bind(("conversation", conversation.clone()))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let conv: Conversation = take_one(&mut r, 0, WHAT)?.ok_or(StoreError::NotFound)?;
        let after = page
            .cursor
            .as_ref()
            .map(|c| c.as_str().parse::<MessageId>().map(|m| rid(&m)))
            .transpose()
            .map_err(|_| StoreError::NotFound)?;
        let limit = page.limit();
        let mut r = run_tx(&self.db, sql::MESSAGES, |q| {
            q.bind(("conversation", conversation.clone()))
                .bind(("after", after.clone()))
                .bind(("limit", i64::from(limit) + 1))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let value: Value = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        let messages: Vec<Message<Stored>> = messages_of(value)
            .map_err(|e| e.into_store(WHAT))?
            .into_iter()
            .map(|(m, _)| m)
            .collect();
        Ok(ConversationDetail {
            conversation: conv,
            messages: page_of(messages, limit, |m| m.id.to_string()),
        })
    }

    async fn add_chat_image(
        &self,
        owner: ConversationOwner<Viewer>,
        image: NewChatImage,
    ) -> Result<ChatImageId, StoreError> {
        const WHAT: &str = "add chat image";
        run_tx(&self.db, sql::ADD_CHAT_IMAGE, |q| {
            q.bind(("image", rid(&image.id)))
                .bind(("conversation", rid(&owner.conversation())))
                .bind(("width", i64::from(image.width)))
                .bind(("height", i64::from(image.height)))
                .bind(("key", image.storage_key.as_str()))
                .bind(("now", dt(image.uploaded_at)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        Ok(image.id)
    }

    async fn chat_image_object(
        &self,
        conversation: ConversationId,
        image: ChatImageId,
    ) -> Result<Option<ObjectRef>, StoreError> {
        const WHAT: &str = "chat image object";
        let mut r = run_tx(&self.db, sql::CHAT_IMAGE_OBJECT, |q| {
            q.bind(("image", rid(&image)))
                .bind(("conversation", rid(&conversation)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let keys: Vec<String> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        Ok(keys
            .first()
            .and_then(|k| ObjectKey::from_stored(k))
            .map(|key| ObjectRef {
                key,
                content_type: ContentType::Webp,
            }))
    }

    async fn store_message(
        &self,
        owner: ConversationOwner<Viewer>,
        message: NewMessage,
    ) -> Result<Message<Stored>, StoreError> {
        const WHAT: &str = "store message";
        let row = (|| {
            Ok::<_, RowError>(MessageRow {
                conversation: rid(&owner.conversation()),
                role: enum_str("role", &message.role)?,
                text: message.text.clone(),
                attachments: message.attachments.iter().map(rid).collect(),
                citations: message
                    .citations
                    .iter()
                    .map(|c| to_json("citations", c))
                    .collect::<Result<_, _>>()?,
                farm_citations: message
                    .farm_citations
                    .iter()
                    .map(|c| to_json("farm_citations", c))
                    .collect::<Result<_, _>>()?,
                blocks: message
                    .blocks
                    .iter()
                    .map(|b| to_json("blocks", b))
                    .collect::<Result<_, _>>()?,
                tool_calls: message
                    .tool_calls
                    .iter()
                    .map(part_json)
                    .collect::<Result<_, _>>()?,
                route: message
                    .route
                    .as_ref()
                    .map(|r| enum_str("route", r))
                    .transpose()?,
                interrupted: message.interrupted,
                model: message.model.clone(),
                prompt_version: message.prompt_version.clone(),
                tokens_in: i64::from(message.tokens_in),
                tokens_out: i64::from(message.tokens_out),
                created_at: dt(message.created_at),
            })
        })()
        .map_err(|e| e.into_store(WHAT))?;
        let mut r = run_tx(&self.db, sql::STORE_MESSAGE, |q| {
            q.bind(("message", rid(&message.id)))
                .bind(("conversation", rid(&owner.conversation())))
                .bind(("row", row.clone()))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let last = r.num_statements().saturating_sub(2);
        let value: Value = r.take(last).map_err(|e| take_failed(WHAT, e))?;
        messages_of(value)
            .map_err(|e| e.into_store(WHAT))?
            .into_iter()
            .next()
            .map(|(m, _)| m)
            .ok_or(StoreError::NotFound)
    }

    async fn context(
        &self,
        owner: ConversationOwner<Viewer>,
        recent: u32,
    ) -> Result<(Option<String>, Vec<ContextMessage>), StoreError> {
        const WHAT: &str = "chat context";
        let mut r = run_tx(&self.db, sql::CONTEXT, |q| {
            q.bind(("conversation", rid(&owner.conversation())))
                .bind(("recent", i64::from(recent)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let summary: Option<String> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        let value: Value = r.take(1).map_err(|e| take_failed(WHAT, e))?;
        // Newest first from the index; the context reads oldest first.
        let mut messages: Vec<ContextMessage> = messages_of(value)
            .map_err(|e| e.into_store(WHAT))?
            .into_iter()
            .map(|(message, tool_calls)| ContextMessage {
                message,
                tool_calls,
            })
            .collect();
        messages.reverse();
        Ok((summary, messages))
    }

    async fn message_count(&self, owner: ConversationOwner<Viewer>) -> Result<u32, StoreError> {
        const WHAT: &str = "message count";
        let mut results = run_tx(&self.db, sql::MESSAGE_COUNT, |q| {
            q.bind(("conversation", rid(&owner.conversation())))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let n: Option<i64> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        Ok(u32::try_from(n.unwrap_or(0)).unwrap_or(u32::MAX))
    }

    async fn set_summary(
        &self,
        owner: ConversationOwner<Viewer>,
        summary: String,
    ) -> Result<(), StoreError> {
        const WHAT: &str = "set summary";
        run_tx(&self.db, sql::SET_SUMMARY, |q| {
            q.bind(("conversation", rid(&owner.conversation())))
                .bind(("summary", summary.clone()))
        })
        .await
        .map(drop)
        .map_err(|f| f.into_store(WHAT))
    }

    async fn create_pending_action(
        &self,
        owner: ConversationOwner<Editor>,
        action: NewPendingAction,
    ) -> Result<PendingAction, StoreError> {
        const WHAT: &str = "create pending action";
        let a = &action.action;
        let row = (|| {
            Ok::<_, RowError>(PendingActionRow {
                conversation: rid(&owner.conversation()),
                message: rid(&action.message),
                tool: enum_str("tool", &a.tool)?,
                status: "proposed".to_owned(),
                card: to_json("card", &a.card)?,
                preview: to_json("preview", &a.preview)?,
                expires_at: dt(a.expires_at),
            })
        })()
        .map_err(|e| e.into_store(WHAT))?;
        run_tx(&self.db, sql::CREATE_PENDING_ACTION, |q| {
            q.bind(("action", rid(&a.id)))
                .bind(("conversation", rid(&owner.conversation())))
                .bind(("row", row.clone()))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        Ok(action.action)
    }

    async fn pending_action(
        &self,
        owner: ConversationOwner<Editor>,
        action: PendingActionId,
    ) -> Result<PendingActionRecord, StoreError> {
        self.pending_by(
            sql::PENDING_ACTION,
            owner.conversation(),
            action,
            |q| q,
            "pending action",
        )
        .await
    }

    async fn mark_confirmed(
        &self,
        owner: ConversationOwner<Editor>,
        action: PendingActionId,
        result: ConfirmResult,
        now: Timestamp,
    ) -> Result<PendingAction, StoreError> {
        const WHAT: &str = "confirm pending action";
        let result = to_json("result", &result).map_err(|e| e.into_store(WHAT))?;
        self.pending_by(
            sql::MARK_CONFIRMED,
            owner.conversation(),
            action,
            |q| q.bind(("result", result.clone())).bind(("now", dt(now))),
            WHAT,
        )
        .await
        .map(|r| r.action)
    }

    async fn reject_pending_action(
        &self,
        owner: ConversationOwner<Editor>,
        action: PendingActionId,
        now: Timestamp,
    ) -> Result<PendingAction, StoreError> {
        self.pending_by(
            sql::REJECT_PENDING_ACTION,
            owner.conversation(),
            action,
            |q| q.bind(("now", dt(now))),
            "reject pending action",
        )
        .await
        .map(|r| r.action)
    }

    async fn expire_pending_actions(&self, now: Timestamp) -> Result<u32, StoreError> {
        const WHAT: &str = "expire pending actions";
        let mut results = run_tx(&self.db, sql::EXPIRE_PENDING_ACTIONS, |q| {
            q.bind(("now", dt(now)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let ids: Vec<RecordId> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        Ok(u32::try_from(ids.len()).unwrap_or(u32::MAX))
    }

    async fn rate_message(
        &self,
        owner: ConversationOwner<Viewer>,
        message: MessageId,
        feedback: MessageFeedbackRequest,
    ) -> Result<(), StoreError> {
        const WHAT: &str = "rate message";
        let rating = enum_str("rating", &feedback.rating).map_err(|e| e.into_store(WHAT))?;
        run_tx(&self.db, sql::RATE_MESSAGE, |q| {
            q.bind(("message", rid(&message)))
                .bind(("conversation", rid(&owner.conversation())))
                .bind(("feedback", message_feedback_rid(message)))
                .bind(("rating", rating.clone()))
                .bind(("notes", feedback.notes.clone()))
        })
        .await
        .map(drop)
        .map_err(|f| f.into_store(WHAT))
    }

    async fn message_conversation(
        &self,
        message: MessageId,
    ) -> Result<Option<ConversationId>, StoreError> {
        const WHAT: &str = "message conversation";
        let mut r = run_tx(&self.db, sql::MESSAGE_CONVERSATION, |q| {
            q.bind(("message", rid(&message)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let ids: Vec<RecordId> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        ids.first()
            .map(from_rid)
            .transpose()
            .map_err(|e: RowError| e.into_store(WHAT))
    }
}
