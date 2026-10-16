//! Conversations, messages, chat images, pending actions and message feedback ([contracts 8]).

use crate::{
    authz::{ConversationOwner, Editor, Membership, Viewer},
    chat::{
        Anchor, ChatRoute, ConfirmResult, Conversation, ConversationDetail, ConversationQuery,
        Message, MessageFeedbackRequest, MessageRole, PendingAction,
    },
    id::{ChatImageId, ConversationId, MessageId, PendingActionId, UserId},
    ports::{ContentPart, ObjectKey, ObjectRef, StoreError},
    records::diagnosis::Citation,
    url::Stored,
    wire::{Page, PageRequest, Timestamp},
};

/// `POST /conversations`, with its id and owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewConversation {
    /// Its id.
    pub id: ConversationId,
    /// Its owner.
    pub user: UserId,
    /// Its anchor; checked live and in the garden.
    pub anchor: Anchor,
    /// Now.
    pub created_at: Timestamp,
}

/// A chat image written to `RustFS`, to record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewChatImage {
    /// Its id.
    pub id: ChatImageId,
    /// Stored width.
    pub width: u32,
    /// Stored height.
    pub height: u32,
    /// Its storage key.
    pub storage_key: ObjectKey,
    /// Now.
    pub uploaded_at: Timestamp,
}

/// A message to store. Tool calls and results are stored but never sent to the app.
#[derive(Debug, Clone, PartialEq)]
pub struct NewMessage {
    /// Its id.
    pub id: MessageId,
    /// Its author.
    pub role: MessageRole,
    /// The text.
    pub text: String,
    /// The user's images; each must belong to the conversation.
    pub attachments: Vec<ChatImageId>,
    /// Sources cited.
    pub citations: Vec<Citation>,
    /// Tool calls and results, in order.
    pub tool_calls: Vec<ContentPart>,
    /// The classifier's route.
    pub route: Option<ChatRoute>,
    /// The stream dropped before it finished.
    pub interrupted: bool,
    /// The model.
    pub model: Option<String>,
    /// The prompt version.
    pub prompt_version: Option<String>,
    /// Prompt tokens.
    pub tokens_in: u32,
    /// Completion tokens.
    pub tokens_out: u32,
    /// Now.
    pub created_at: Timestamp,
}

/// A stored message with its tool calls, for rebuilding the context.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextMessage {
    /// The message as the app sees it.
    pub message: Message<Stored>,
    /// Its tool calls and results.
    pub tool_calls: Vec<ContentPart>,
}

/// A card to store with the message that proposed it.
#[derive(Debug, Clone, PartialEq)]
pub struct NewPendingAction {
    /// The card; `status: proposed`.
    pub action: PendingAction,
    /// The assistant message.
    pub message: MessageId,
}

/// A stored card with the result of its confirmation, replayed on a confirm retry.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingActionRecord {
    /// The card.
    pub action: PendingAction,
    /// What confirming stored.
    pub result: Option<ConfirmResult>,
}

/// The chat tables.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait ChatRepo: Send + Sync {
    /// `POST /conversations`: the anchor must be a live record of the garden.
    async fn create_conversation(
        &self,
        member: Membership<Viewer>,
        conversation: NewConversation,
    ) -> Result<Conversation, StoreError>;

    /// `GET /conversations`: the caller's own, newest `last_message_at` first.
    async fn conversations(
        &self,
        member: Membership<Viewer>,
        query: ConversationQuery,
        page: PageRequest,
    ) -> Result<Page<Conversation>, StoreError>;

    /// `GET /conversations/{id}`: messages newest first.
    async fn conversation_detail(
        &self,
        owner: ConversationOwner<Viewer>,
        page: PageRequest,
    ) -> Result<ConversationDetail<Stored>, StoreError>;

    /// `POST /conversations/{id}/images`, after the object is written.
    async fn add_chat_image(
        &self,
        owner: ConversationOwner<Viewer>,
        image: NewChatImage,
    ) -> Result<ChatImageId, StoreError>;

    /// `GET /conversations/{id}/images/{image_id}`, behind a signed URL.
    async fn chat_image_object(
        &self,
        conversation: ConversationId,
        image: ChatImageId,
    ) -> Result<Option<ObjectRef>, StoreError>;

    /// Stores a user or assistant message and sets `last_message_at`.
    async fn store_message(
        &self,
        owner: ConversationOwner<Viewer>,
        message: NewMessage,
    ) -> Result<Message<Stored>, StoreError>;

    /// The summary and the most recent messages, for the context builder.
    async fn context(
        &self,
        owner: ConversationOwner<Viewer>,
        recent: u32,
    ) -> Result<(Option<String>, Vec<ContextMessage>), StoreError>;

    /// The number of stored messages of the conversation (user and assistant), for the summary
    /// trigger (architecture 8.2).
    async fn message_count(&self, owner: ConversationOwner<Viewer>) -> Result<u32, StoreError>;

    /// Stores a new conversation summary.
    async fn set_summary(
        &self,
        owner: ConversationOwner<Viewer>,
        summary: String,
    ) -> Result<(), StoreError>;

    /// A write tool's card. Editors and owners only.
    async fn create_pending_action(
        &self,
        owner: ConversationOwner<Editor>,
        action: NewPendingAction,
    ) -> Result<PendingAction, StoreError>;

    /// A card of the conversation, with its stored result.
    async fn pending_action(
        &self,
        owner: ConversationOwner<Editor>,
        action: PendingActionId,
    ) -> Result<PendingActionRecord, StoreError>;

    /// Marks a `proposed` card `confirmed` with what it stored, in the transaction that stores
    /// it; a card already `confirmed` returns its stored result.
    async fn mark_confirmed(
        &self,
        owner: ConversationOwner<Editor>,
        action: PendingActionId,
        result: ConfirmResult,
        now: Timestamp,
    ) -> Result<PendingAction, StoreError>;

    /// `POST /pending-actions/{id}/reject`.
    async fn reject_pending_action(
        &self,
        owner: ConversationOwner<Editor>,
        action: PendingActionId,
        now: Timestamp,
    ) -> Result<PendingAction, StoreError>;

    /// The cleanup: `proposed` cards past `expires_at` become `expired`.
    async fn expire_pending_actions(&self, now: Timestamp) -> Result<u32, StoreError>;

    /// `POST /messages/{id}/feedback`: an upsert on `message_feedback:[message]`, assistant
    /// messages only.
    async fn rate_message(
        &self,
        owner: ConversationOwner<Viewer>,
        message: MessageId,
        feedback: MessageFeedbackRequest,
    ) -> Result<(), StoreError>;

    /// The conversation a message belongs to, for the feedback route's check.
    async fn message_conversation(
        &self,
        message: MessageId,
    ) -> Result<Option<ConversationId>, StoreError>;
}
