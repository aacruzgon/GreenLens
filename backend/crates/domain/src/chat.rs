//! Chat and memory ([contracts 8]): conversations, messages, the chat SSE events, pending
//! actions (confirmation cards), memories and the agent tools' return shapes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::ErrorCode,
    garden::map::GardenMap,
    id::{
        ChatImageId, ConversationId, GardenId, ImageId, MemoryId, MessageId, PendingActionId,
        PlanId, PlantId, ProblemId, ScanId, StepId,
    },
    knowledge::SoilProfile,
    records::{
        diagnosis::Citation,
        observation::Observation,
        plan::{ActionPlan, PlanStep},
        task::Task,
        weather::SoilNow,
    },
    scan::ImageKind,
    url::{ImagePath, Sign, Signed, SignedUrl, Signer, Stored, UrlSlot, UrlState},
    wire::{Page, Timestamp, V1},
};

/// `anchor`: `{ type: "scan" | "plant" | "garden"; id: Id }`, adjacently tagged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
pub enum Anchor {
    /// "Ask about this result".
    Scan(ScanId),
    /// The plant timeline.
    Plant(PlantId),
    /// Garden home.
    Garden(GardenId),
}

/// `anchor_type` query values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnchorType {
    /// A scan.
    Scan,
    /// A plant.
    Plant,
    /// The garden.
    Garden,
}

/// `POST /conversations` ([contracts 8]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateConversationRequest {
    /// The garden.
    pub garden_id: GardenId,
    /// What the conversation is about; must be in the garden.
    pub anchor: Anchor,
}

/// `Conversation` ([contracts 8]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Conversation {
    /// The conversation.
    pub id: ConversationId,
    /// Its garden.
    pub garden_id: GardenId,
    /// Its anchor.
    pub anchor: Anchor,
    /// A title, once one is generated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub title: Option<String>,
    /// When it was created.
    pub created_at: Timestamp,
    /// When the last message was sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub last_message_at: Option<Timestamp>,
}

/// `GET /conversations?garden_id=&anchor_type=&anchor_id=`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ConversationQuery {
    /// The garden.
    pub garden_id: GardenId,
    /// Filter by anchor type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_type: Option<AnchorType>,
    /// Filter by anchor id (a ULID of the anchor type's table).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>)]
    pub anchor_id: Option<ulid::Ulid>,
}

/// `Message.role`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// The user.
    User,
    /// The assistant.
    Assistant,
}

/// `Message.attachments[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = Attachment)]
pub struct Attachment<U: UrlState = Signed> {
    /// The chat image.
    pub image_id: ChatImageId,
    /// Signed.
    #[serde(default, skip_serializing_if = "UrlSlot::is_absent")]
    #[schema(value_type = SignedUrl, required = true)]
    pub url: U::Url,
}

impl Sign for Attachment<Stored> {
    type Signed = Attachment<Signed>;
    type Scope = ConversationId;
    fn sign(self, conversation: ConversationId, signer: &Signer<'_>) -> Attachment<Signed> {
        Attachment {
            url: signer.url(&ImagePath::ChatImage {
                conversation,
                image: self.image_id,
            }),
            image_id: self.image_id,
        }
    }
}

/// `Message` ([contracts 8]). Tool calls and results are stored but never sent to the app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = Message)]
pub struct Message<U: UrlState = Signed> {
    /// The message.
    pub id: MessageId,
    /// Who wrote it.
    pub role: MessageRole,
    /// The text, dose-checked when the assistant wrote it.
    pub text: String,
    /// The user's images.
    #[schema(value_type = Vec<Attachment>)]
    pub attachments: Vec<Attachment<U>>,
    /// Sources cited.
    pub citations: Vec<Citation>,
    /// Confirmation cards.
    pub pending_actions: Vec<PendingAction>,
    /// The stream dropped before `message_completed`.
    pub interrupted: bool,
    /// When it was stored.
    pub created_at: Timestamp,
}

impl Sign for Message<Stored> {
    type Signed = Message<Signed>;
    type Scope = ConversationId;
    fn sign(self, conversation: ConversationId, signer: &Signer<'_>) -> Message<Signed> {
        Message {
            attachments: self.attachments.sign(conversation, signer),
            id: self.id,
            role: self.role,
            text: self.text,
            citations: self.citations,
            pending_actions: self.pending_actions,
            interrupted: self.interrupted,
            created_at: self.created_at,
        }
    }
}

/// `ConversationDetail extends Conversation` ([contracts 8]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = ConversationDetail)]
pub struct ConversationDetail<U: UrlState = Signed> {
    /// Every field of `Conversation`.
    #[serde(flatten)]
    pub conversation: Conversation,
    /// Newest first; `?cursor=` for older.
    #[schema(value_type = Page<Message>)]
    pub messages: Page<Message<U>>,
}

impl Sign for ConversationDetail<Stored> {
    type Signed = ConversationDetail<Signed>;
    type Scope = ();
    fn sign(self, (): (), signer: &Signer<'_>) -> ConversationDetail<Signed> {
        let id = self.conversation.id;
        ConversationDetail {
            conversation: self.conversation,
            messages: self.messages.sign(id, signer),
        }
    }
}

/// `201` of `POST /conversations/{id}/images`: `{ image_id, url }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ChatImageUploaded {
    /// The new chat image.
    pub image_id: ChatImageId,
    /// Signed.
    pub url: SignedUrl,
}

/// `POST /messages/{id}/feedback`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Rating {
    /// Thumbs up.
    Up,
    /// Thumbs down.
    Down,
}

/// `POST /messages/{id}/feedback`: owner only, assistant messages only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MessageFeedbackRequest {
    /// The rating.
    pub rating: Rating,
    /// Notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub notes: Option<String>,
}

/// `SendMessageRequest` ([contracts 8]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SendMessageRequest {
    /// At most 2,000 characters.
    pub text: String,
    /// From `POST /conversations/{id}/images`, at most 3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub image_ids: Option<Vec<ChatImageId>>,
}

/// The replay of a finished chat message under its idempotency key: `200 { message }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = MessageReplay)]
pub struct MessageReplay<U: UrlState = Signed> {
    /// The stored assistant message.
    #[schema(value_type = Message)]
    pub message: Message<U>,
}

// ---------------------------------------------------------------------------------------------
// Chat SSE events
// ---------------------------------------------------------------------------------------------

/// `message_started.route` and `message.route`: the text classifier's label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChatRoute {
    /// About a diagnosis.
    DiagnosisQuestion,
    /// A care question.
    CareQuestion,
    /// Write tools are likely.
    TaskChange,
    /// Pesticide doses, a person or pet poisoned: the safety policy first.
    SafetySensitive,
    /// A short, polite redirect.
    OffTopic,
}

/// `message_completed.usage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct Usage {
    /// Prompt tokens.
    pub tokens_in: u32,
    /// Completion tokens.
    pub tokens_out: u32,
}

/// `message_started` data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MessageStarted {
    /// `1`.
    pub v: V1,
    /// The assistant message being written.
    pub message_id: MessageId,
    /// Its route.
    pub route: ChatRoute,
}

/// `text_delta` data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TextDelta {
    /// `1`.
    pub v: V1,
    /// One dose-checked sentence or part of one.
    pub text: String,
}

/// `tool_started` data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ToolStarted {
    /// `1`.
    pub v: V1,
    /// The call.
    pub call_id: String,
    /// The tool name.
    pub tool: String,
    /// For the UI: "Checking the forecast".
    pub label: String,
}

/// `tool_finished` data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ToolFinished {
    /// `1`.
    pub v: V1,
    /// The call.
    pub call_id: String,
    /// Whether it succeeded.
    pub ok: bool,
}

/// `citation` data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CitationEvent {
    /// `1`.
    pub v: V1,
    /// The source.
    pub citation: Citation,
}

/// `pending_action` data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PendingActionEvent {
    /// `1`.
    pub v: V1,
    /// The card.
    pub pending_action: PendingAction,
}

/// `memory_saved` data: shown as "Remembered: …" with Undo and Share with garden.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MemorySaved {
    /// `1`.
    pub v: V1,
    /// The private chat memory.
    pub memory: Memory,
}

/// `rescan_requested` data: the app offers the capture flow for a new scan of that plant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RescanRequested {
    /// `1`.
    pub v: V1,
    /// The plant.
    pub plant_id: PlantId,
    /// Which shot.
    pub photo_kind: ImageKind,
    /// How to take it; dose-checked.
    pub instructions: String,
}

/// `message_completed` data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MessageCompleted {
    /// `1`.
    pub v: V1,
    /// The stored message.
    pub message_id: MessageId,
    /// Token counts.
    pub usage: Usage,
}

/// `error` data: an error after the stream started; the stream ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ChatErrorEvent {
    /// `1`.
    pub v: V1,
    /// A contracts 2 code.
    pub code: ErrorCode,
    /// Safe to show.
    pub message: String,
    /// Whether resending may work.
    pub retryable: bool,
}

/// One chat SSE event ([contracts 8], sending a message). The `data:` line carries the
/// variant's fields only, and the SSE `event:` line is [`ChatEvent::event_name`]; the stream is
/// not resumable, so events have no `id:`. Serialize only: the API writes these and the app
/// reads them.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(untagged)]
pub enum ChatEvent {
    /// `message_started`
    MessageStarted(MessageStarted),
    /// `text_delta`
    TextDelta(TextDelta),
    /// `tool_started`
    ToolStarted(ToolStarted),
    /// `tool_finished`
    ToolFinished(ToolFinished),
    /// `citation`
    Citation(CitationEvent),
    /// `pending_action`
    PendingAction(PendingActionEvent),
    /// `memory_saved`
    MemorySaved(MemorySaved),
    /// `rescan_requested`
    RescanRequested(RescanRequested),
    /// `message_completed`
    MessageCompleted(MessageCompleted),
    /// `error`
    Error(ChatErrorEvent),
}

impl ChatEvent {
    /// The SSE `event:` name.
    pub const fn event_name(&self) -> &'static str {
        match self {
            Self::MessageStarted(_) => "message_started",
            Self::TextDelta(_) => "text_delta",
            Self::ToolStarted(_) => "tool_started",
            Self::ToolFinished(_) => "tool_finished",
            Self::Citation(_) => "citation",
            Self::PendingAction(_) => "pending_action",
            Self::MemorySaved(_) => "memory_saved",
            Self::RescanRequested(_) => "rescan_requested",
            Self::MessageCompleted(_) => "message_completed",
            Self::Error(_) => "error",
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Pending actions
// ---------------------------------------------------------------------------------------------

/// `PendingAction.tool`: the write tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WriteTool {
    /// Add a task.
    CreateTask,
    /// Complete a task.
    CompleteTask,
    /// Propose a new plan version.
    RevisePlan,
}

/// `PendingAction.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PendingStatus {
    /// Waiting for the user.
    Proposed,
    /// Confirmed; a retry returns the same result.
    Confirmed,
    /// Rejected.
    Rejected,
    /// Past `expires_at`.
    Expired,
}

impl PendingStatus {
    /// The wire name, for `InvalidStateDetails.status`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Confirmed => "confirmed",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
        }
    }
}

/// `PendingAction.card`; the labels are written by the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Card {
    /// "Add a task?"
    pub title: String,
    /// "Spray the tomato with water to knock off aphids".
    pub body: String,
    /// "Add task".
    pub confirm_label: String,
    /// "Not now".
    pub reject_label: String,
}

/// `PendingAction.preview`, tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Preview {
    /// A task to create or complete.
    Task {
        /// The task as it would be stored.
        task: Box<Task>,
    },
    /// A plan revision, stored exactly as shown when confirmed.
    Plan {
        /// The plan the revision is based on.
        base_plan_id: PlanId,
        /// The new summary.
        plan_summary: String,
        /// Replace the base plan's step with the same id, or add a new one.
        changed_steps: Vec<PlanStep>,
        /// Steps dropped.
        removed_step_ids: Vec<StepId>,
        /// "No copper products".
        constraints_added: Vec<String>,
    },
}

/// `PendingAction` ([contracts 8]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PendingAction {
    /// The card.
    pub id: PendingActionId,
    /// Its conversation.
    pub conversation_id: ConversationId,
    /// The write tool.
    pub tool: WriteTool,
    /// Its status.
    pub status: PendingStatus,
    /// What the card shows.
    pub card: Card,
    /// What confirming stores.
    pub preview: Preview,
    /// 24 hours after it was made.
    pub expires_at: Timestamp,
}

/// `result` of a confirm.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ToSchema)]
pub struct ConfirmResult {
    /// The created or completed task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub task: Option<Task>,
    /// The new plan version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plan: Option<ActionPlan>,
}

/// `200` of `POST /pending-actions/{id}/confirm`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ConfirmResponse {
    /// The card, now `confirmed`.
    pub pending_action: PendingAction,
    /// What was stored.
    pub result: ConfirmResult,
}

/// `200` of `POST /pending-actions/{id}/reject`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RejectResponse {
    /// The card, now `rejected`.
    pub pending_action: PendingAction,
}

// ---------------------------------------------------------------------------------------------
// Memory
// ---------------------------------------------------------------------------------------------

/// `Memory.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    /// "It's in a container".
    Fact,
    /// "Applied neem oil Oct 18".
    Event,
    /// The pipeline's scan summary.
    Summary,
}

/// `Memory.source` ([contracts 8], memory).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemorySource {
    /// The pipeline after each scan; every member sees it; the plan writer uses it.
    Pipeline,
    /// Extraction or `remember`; private to its author until shared.
    Chat,
    /// A shared chat memory or an editor's garden note.
    User,
}

/// `Memory` ([contracts 8]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Memory {
    /// The memory.
    pub id: MemoryId,
    /// Its garden.
    pub garden_id: GardenId,
    /// Its plant; absent for a garden-level fact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plant_id: Option<PlantId>,
    /// What kind.
    pub kind: MemoryKind,
    /// The text.
    pub text: String,
    /// Where it came from.
    pub source: MemorySource,
    /// `false` for chat memories until their author shares them.
    pub shared: bool,
    /// When it was stored.
    pub created_at: Timestamp,
}

/// `GET /memories?garden_id=&plant_id=`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MemoryQuery {
    /// The garden.
    pub garden_id: GardenId,
    /// One plant's memories.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plant_id: Option<PlantId>,
}

/// `POST /memories`: a garden note, `source: "user"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateMemoryRequest {
    /// The garden.
    pub garden_id: GardenId,
    /// The plant, or none for the garden.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plant_id: Option<PlantId>,
    /// The note.
    pub text: String,
}

// ---------------------------------------------------------------------------------------------
// Tool return shapes
// ---------------------------------------------------------------------------------------------

/// The agent's tools ([contracts 8], tool return shapes and scope).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolName {
    /// `ScanDetail` without image URLs.
    GetScan,
    /// The image to the model.
    ViewImage,
    /// `WeatherSummary["forecast"]`.
    GetForecast,
    /// `Memory[]`.
    SearchMemory,
    /// [`SimilarCase`] list.
    FindSimilarCases,
    /// [`KnowledgeHit`] list.
    SearchKnowledge,
    /// `SpeciesSeed`.
    GetSpeciesProfile,
    /// `ProblemSeed`.
    GetDiseaseProfile,
    /// `OrganismSeed`.
    GetOrganismProfile,
    /// [`Lookalike`] list.
    CompareLookalikes,
    /// [`SoilProfileView`].
    GetSoilProfile,
    /// `MeasurementPoint[]`.
    GetPlantMeasurements,
    /// [`GardenMapView`].
    GetGardenMap,
    /// `Task[]`.
    ListTasks,
    /// `Memory` and a `memory_saved` event.
    Remember,
    /// [`RescanRequest`] and a `rescan_requested` event.
    RequestRescan,
    /// A `PendingAction` (editors and owners).
    CreateTask,
    /// A `PendingAction` (editors and owners).
    CompleteTask,
    /// A `PendingAction` (editors and owners).
    RevisePlan,
}

impl ToolName {
    /// Write tools are offered only to editors and owners; viewers and reviewers get the read
    /// tools and `remember` ([contracts 1], roles and access).
    pub const fn is_write(self) -> bool {
        matches!(
            self,
            Self::CreateTask | Self::CompleteTask | Self::RevisePlan
        )
    }
}

/// `find_similar_cases` items, at most 5.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimilarCase {
    /// The past scan.
    pub scan_id: ScanId,
    /// When it was taken.
    pub created_at: Timestamp,
    /// Its primary problem.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<ProblemRef>,
    /// Cosine similarity.
    pub similarity: f64,
    /// Its `whole_plant` shot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_image_id: Option<ImageId>,
}

/// `{ problem_id, name }`: `SimilarCase.primary` and `TimelineEntry.primary`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ProblemRef {
    /// The problem.
    pub problem_id: ProblemId,
    /// Its name.
    pub name: String,
}

/// `search_knowledge` items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeHit {
    /// The chunk text.
    pub text: String,
    /// Its source.
    pub citation: Citation,
}

/// `compare_lookalikes` items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lookalike {
    /// The look-alike.
    pub problem_id: ProblemId,
    /// Its name.
    pub name: String,
    /// How to tell them apart.
    pub how_to_tell: String,
}

/// `get_soil_profile`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoilProfileView {
    /// `SoilGrids`, `null` until fetched.
    pub soil_profile: Option<SoilProfile>,
    /// Open-Meteo's soil now.
    pub latest: SoilNow,
    /// The latest guided-check texture observation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture: Option<Observation<Stored>>,
}

/// `get_garden_map`: the map and each pinned plant's neighbours.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GardenMapView {
    /// `null` when the garden has no map.
    pub map: Option<GardenMap>,
    /// Plant → plants within `NEIGHBOUR_RADIUS_M`.
    pub neighbours: BTreeMap<PlantId, Vec<PlantId>>,
}

/// `request_rescan`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RescanRequest {
    /// The plant.
    pub plant_id: PlantId,
    /// Which shot.
    pub photo_kind: ImageKind,
    /// How to take it.
    pub instructions: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_are_type_and_id() {
        let a = Anchor::Plant("01JAX3K9T6R2B8V0QZ4M7D1E5F".parse().unwrap());
        assert_eq!(
            serde_json::to_value(a).unwrap(),
            serde_json::json!({"type": "plant", "id": "01JAX3K9T6R2B8V0QZ4M7D1E5F"})
        );
    }

    #[test]
    fn chat_events_write_data_only() {
        let e = ChatEvent::MessageCompleted(MessageCompleted {
            v: V1,
            message_id: "01JAX3K9T6R2B8V0QZ4M7D1E5F".parse().unwrap(),
            usage: Usage {
                tokens_in: 812,
                tokens_out: 140,
            },
        });
        assert_eq!(e.event_name(), "message_completed");
        assert_eq!(
            serde_json::to_string(&e).unwrap(),
            r#"{"v":1,"message_id":"01JAX3K9T6R2B8V0QZ4M7D1E5F","usage":{"tokens_in":812,"tokens_out":140}}"#
        );
    }

    #[test]
    fn plan_preview_is_tagged_by_kind() {
        let p = Preview::Plan {
            base_plan_id: "01JAX3K9T6R2B8V0QZ4M7D1E5F".parse().unwrap(),
            plan_summary: "Without copper".to_owned(),
            changed_steps: vec![],
            removed_step_ids: vec!["s3".parse().unwrap()],
            constraints_added: vec!["No copper products".to_owned()],
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["kind"], "plan");
        assert_eq!(v["removed_step_ids"][0], "s3");
        assert_eq!(serde_json::from_value::<Preview>(v).unwrap(), p);
    }
}
