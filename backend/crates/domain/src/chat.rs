//! Chat and memory ([contracts 8]): conversations, messages, the chat SSE events, pending
//! actions (confirmation cards), memories and the agent tools' return shapes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    connectors::FarmCitation,
    error::ErrorCode,
    garden::map::GardenMap,
    garden::plants::{GrowthQuantity, MeasurementPoint},
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
        weather::{ForecastDay, SoilNow},
    },
    scan::{ImageKind, PlantStatus},
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
    /// The farm records an assistant message cites ([connectors 7]); absent when it cites none,
    /// and read as empty from a message stored before connectors.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub farm_citations: Vec<FarmCitation>,
    /// Confirmation cards.
    pub pending_actions: Vec<PendingAction>,
    /// The answer blocks an assistant message sent, in order, at most 3 ([contracts 8], answer
    /// blocks); absent when it sent none, and read as empty from a message stored before them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<AnswerBlock>,
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
            farm_citations: self.farm_citations,
            pending_actions: self.pending_actions,
            blocks: self.blocks,
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

/// `answer_block` data: sent when `show_block` succeeds, so always after its anchor's
/// `tool_finished`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct AnswerBlockEvent {
    /// `1`.
    pub v: V1,
    /// The block, as stored in `Message.blocks`.
    pub block: AnswerBlock,
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
    /// `answer_block`
    AnswerBlock(AnswerBlockEvent),
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
            Self::AnswerBlock(_) => "answer_block",
            Self::MessageCompleted(_) => "message_completed",
            Self::Error(_) => "error",
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Answer blocks
// ---------------------------------------------------------------------------------------------

/// `AnswerBlock.kind`, and the `kind` argument of `show_block`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnswerBlockKind {
    /// The garden map and its pinned plants; anchored on `get_garden_map`.
    GardenMap,
    /// Daily weather rows; anchored on `get_forecast`, or on `get_scan`'s weather snapshot.
    WeatherHours,
    /// The frost nights and the garden's plants to cover; anchored on `get_forecast`.
    FrostPlanner,
    /// One plant's measured points; anchored on `get_plant_measurements`.
    MeasurementTrend,
    /// A problem and its look-alikes; anchored on `compare_lookalikes`.
    Candidates,
}

impl AnswerBlockKind {
    /// The wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GardenMap => "garden_map",
            Self::WeatherHours => "weather_hours",
            Self::FrostPlanner => "frost_planner",
            Self::MeasurementTrend => "measurement_trend",
            Self::Candidates => "candidates",
        }
    }
}

/// `BlockPlant`: a plant as a block shows it, filled in from the store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct BlockPlant {
    /// The plant.
    pub plant_id: PlantId,
    /// Its name.
    pub name: String,
    /// Its health status.
    pub status: PlantStatus,
    /// cm: its latest `plant_height_cm` point; absent when it has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub height_cm: Option<f64>,
}

/// `FrostPlant = BlockPlant & { frost_tender?: boolean }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct FrostPlant {
    /// Every field of `BlockPlant`.
    #[serde(flatten)]
    pub plant: BlockPlant,
    /// `true` for a confirmed species with `frost: tender`, `false` for `hardy`; absent when the
    /// species is only suggested or unknown, or its sources don't say. Never `false` for "don't
    /// know".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub frost_tender: Option<bool>,
}

/// `AnswerBlock` ([contracts 8], answer blocks): a native component the app draws in an answer.
/// The API builds it from one tool result of the turn (its anchor, `call_id`) and the store, so
/// it holds records and ids only, never text the model wrote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct AnswerBlock {
    /// The anchor: the tool call of the same turn whose result the block shows.
    pub call_id: String,
    /// The kind and its records.
    #[serde(flatten)]
    pub body: BlockBody,
}

impl AnswerBlock {
    /// The block's kind.
    pub const fn kind(&self) -> AnswerBlockKind {
        self.body.kind()
    }
}

/// `AnswerBlock`'s kinds, tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BlockBody {
    /// From `get_garden_map`.
    GardenMap {
        /// The map, from the result.
        map: GardenMap,
        /// Plant → plants within `NEIGHBOUR_RADIUS_M`, from the result.
        neighbours: BTreeMap<PlantId, Vec<PlantId>>,
        /// Every live plant pinned on the map, from the store.
        plants: Vec<BlockPlant>,
        /// The plants the answer is about, from the `show_block` call, once each: each is one of
        /// `plants`. May be empty.
        focus_plant_ids: Vec<PlantId>,
    },
    /// From `get_forecast`, or from `get_scan`'s weather snapshot.
    WeatherHours {
        /// Daily rows: the forecast, or the scan's snapshot of it at scan time.
        days: Vec<ForecastDay>,
        /// Set when the anchor is `get_scan`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schema(nullable = false)]
        scan_id: Option<ScanId>,
    },
    /// From `get_forecast`.
    FrostPlanner {
        /// The result's days with `frost_risk`, first first.
        nights: Vec<ForecastDay>,
        /// The garden's live plants that aren't dead, from the store, with their confirmed
        /// species' frost tenderness.
        plants: Vec<FrostPlant>,
    },
    /// From `get_plant_measurements`.
    MeasurementTrend {
        /// The plant, from the call's arguments.
        plant_id: PlantId,
        /// Its name, from the store.
        plant_name: String,
        /// The quantity the points measure.
        quantity: GrowthQuantity,
        /// As returned, oldest first.
        points: Vec<MeasurementPoint>,
    },
    /// From `compare_lookalikes`.
    Candidates {
        /// The problem asked about: its id from the call, its name from the knowledge base.
        disease: ProblemRef,
        /// The look-alikes, from the result.
        lookalikes: Vec<Lookalike>,
    },
}

impl BlockBody {
    /// The kind.
    pub const fn kind(&self) -> AnswerBlockKind {
        match self {
            Self::GardenMap { .. } => AnswerBlockKind::GardenMap,
            Self::WeatherHours { .. } => AnswerBlockKind::WeatherHours,
            Self::FrostPlanner { .. } => AnswerBlockKind::FrostPlanner,
            Self::MeasurementTrend { .. } => AnswerBlockKind::MeasurementTrend,
            Self::Candidates { .. } => AnswerBlockKind::Candidates,
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

/// `Memory.topic`: what a memory is about, from a fixed list. With the kind it names the
/// memory's belief key in Agent Memory ([`crate::ports::belief_key`]), so a newer memory on the
/// same topic supersedes the older one instead of standing beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryTopic {
    /// How and how often it is watered.
    Watering,
    /// Fertiliser and compost.
    Feeding,
    /// Sprays, treatments and pest control.
    Treatment,
    /// Pruning, pinching and staking.
    Pruning,
    /// Sowing, planting and transplanting.
    Planting,
    /// Where it grows: bed, balcony, indoors.
    Location,
    /// The pot or container.
    Container,
    /// Soil, mulch and pH.
    Soil,
    /// Sun and shade.
    Light,
    /// Health, symptoms and the scan summaries.
    Health,
    /// Harvests and yield.
    Harvest,
    /// Anything else; never supersedes another memory.
    #[default]
    Other,
}

impl MemoryTopic {
    /// The wire name, also the belief key's stem.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Watering => "watering",
            Self::Feeding => "feeding",
            Self::Treatment => "treatment",
            Self::Pruning => "pruning",
            Self::Planting => "planting",
            Self::Location => "location",
            Self::Container => "container",
            Self::Soil => "soil",
            Self::Light => "light",
            Self::Health => "health",
            Self::Harvest => "harvest",
            Self::Other => "other",
        }
    }
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
    /// What it is about.
    pub topic: MemoryTopic,
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
    /// What the note is about; `other` when absent.
    #[serde(default)]
    pub topic: MemoryTopic,
}

/// A shared memory as Agent Memory holds it ([contracts 8], beliefs): the current value of one
/// key of a plant or of its garden, or, in `PlantBeliefs.history`, an earlier one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Belief {
    /// The belief key: `watering`, `last_treatment`, `scan_summary`, `note_<memory id>`.
    pub key: String,
    /// The memory's text.
    pub value: String,
    /// `false` for a garden-level fact.
    pub about_plant: bool,
    /// The memory it was recorded from; absent if Agent Memory lost the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub memory_id: Option<MemoryId>,
    /// When it was learned: the memory's `created_at`.
    pub learned_at: Timestamp,
    /// When a newer memory on the same key replaced it; absent while it is current.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub superseded_at: Option<Timestamp>,
}

/// `GET /plants/{plant_id}/beliefs`: what is believed about a plant now, and what was believed
/// before. Shared memories only, so every member may read it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PlantBeliefs {
    /// `false` when Agent Memory isn't configured or didn't answer; both lists are then empty.
    pub available: bool,
    /// The plant's current beliefs and its garden's, one per key.
    pub current: Vec<Belief>,
    /// The plant's superseded beliefs, newest first.
    pub history: Vec<Belief>,
}

/// A `search_memory` hit from Agent Memory: a current shared belief that matched the query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeliefHit {
    /// The belief key.
    pub key: String,
    /// "watering is every 2 days".
    pub text: String,
    /// `false` for a garden-level fact.
    pub about_plant: bool,
    /// Agent Memory's fused score.
    pub score: f32,
}

/// What `search_memory` returns: the local hits (shared memories and the caller's own chat
/// memories) and, when Agent Memory answers in time, the matching current beliefs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemorySearch {
    /// From the database, exact within the garden.
    pub memories: Vec<Memory>,
    /// From Agent Memory; empty when it is off, slow or down.
    pub beliefs: Vec<BeliefHit>,
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
    /// [`MemorySearch`].
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
    /// `FarmRecordHit` list, at most 8: the garden's synced Slack and Drive records
    /// ([connectors 7]). Every role.
    SearchFarmRecords,
    /// `Memory` and a `memory_saved` event.
    Remember,
    /// [`RescanRequest`] and a `rescan_requested` event.
    RequestRescan,
    /// An [`AnswerBlock`] and an `answer_block` event, built from one tool result of the turn.
    /// Every role.
    ShowBlock,
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

/// `compare_lookalikes` items, and `candidates.lookalikes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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
    fn memories_carry_their_topic() {
        let m = Memory {
            id: "01JAX3K9T6R2B8V0QZ4M7D1E5F".parse().unwrap(),
            garden_id: "01JAX3K9T6R2B8V0QZ4M7D1E5G".parse().unwrap(),
            plant_id: None,
            kind: MemoryKind::Fact,
            topic: MemoryTopic::Container,
            text: "Everything is in containers.".to_owned(),
            source: MemorySource::User,
            shared: true,
            created_at: "2026-10-17T15:00:00Z".parse().unwrap(),
        };
        assert_eq!(
            serde_json::to_string(&m).unwrap(),
            r#"{"id":"01JAX3K9T6R2B8V0QZ4M7D1E5F","garden_id":"01JAX3K9T6R2B8V0QZ4M7D1E5G","kind":"fact","topic":"container","text":"Everything is in containers.","source":"user","shared":true,"created_at":"2026-10-17T15:00:00Z"}"#
        );
        let note: CreateMemoryRequest = serde_json::from_str(
            r#"{"garden_id":"01JAX3K9T6R2B8V0QZ4M7D1E5G","text":"Organic only."}"#,
        )
        .unwrap();
        assert_eq!(note.topic, MemoryTopic::Other);
    }

    #[test]
    fn plant_beliefs_write_current_and_history() {
        let b = PlantBeliefs {
            available: true,
            current: vec![Belief {
                key: "watering".to_owned(),
                value: "Watered every 2 days.".to_owned(),
                about_plant: true,
                memory_id: Some("01JAX3K9T6R2B8V0QZ4M7D1E5F".parse().unwrap()),
                learned_at: "2026-10-17T15:00:00Z".parse().unwrap(),
                superseded_at: None,
            }],
            history: vec![Belief {
                key: "watering".to_owned(),
                value: "Watered every 3 days.".to_owned(),
                about_plant: true,
                memory_id: None,
                learned_at: "2026-10-14T15:00:00Z".parse().unwrap(),
                superseded_at: Some("2026-10-17T15:00:00Z".parse().unwrap()),
            }],
        };
        assert_eq!(
            serde_json::to_string(&b).unwrap(),
            r#"{"available":true,"current":[{"key":"watering","value":"Watered every 2 days.","about_plant":true,"memory_id":"01JAX3K9T6R2B8V0QZ4M7D1E5F","learned_at":"2026-10-17T15:00:00Z"}],"history":[{"key":"watering","value":"Watered every 3 days.","about_plant":true,"learned_at":"2026-10-14T15:00:00Z","superseded_at":"2026-10-17T15:00:00Z"}]}"#
        );
    }

    fn message(farm_citations: Vec<FarmCitation>) -> Message {
        Message {
            id: "01JAX3K9T6R2B8V0QZ4M7D1E5F".parse().unwrap(),
            role: MessageRole::Assistant,
            text: "Bed 3 was sprayed on Oct 12.".to_owned(),
            attachments: Vec::new(),
            citations: Vec::new(),
            farm_citations,
            pending_actions: Vec::new(),
            blocks: Vec::new(),
            interrupted: false,
            created_at: "2026-10-19T09:00:00Z".parse().unwrap(),
        }
    }

    #[test]
    fn a_message_without_farm_citations_leaves_the_field_out() {
        let json = serde_json::json!({
            "id": "01JAX3K9T6R2B8V0QZ4M7D1E5F", "role": "assistant",
            "text": "Bed 3 was sprayed on Oct 12.", "attachments": [], "citations": [],
            "pending_actions": [], "interrupted": false, "created_at": "2026-10-19T09:00:00Z"
        });
        assert_eq!(serde_json::to_value(message(Vec::new())).unwrap(), json);
        // A message stored before connectors reads as citing no farm record.
        assert_eq!(
            serde_json::from_value::<Message>(json).unwrap(),
            message(Vec::new())
        );
    }

    #[test]
    fn a_message_citing_a_farm_record_carries_it() {
        let citation = crate::connectors::FarmCitation {
            record_id: "01JAX3K9T6R2B8V0QZ4M7D1E5C.0.1aBcD".parse().unwrap(),
            system: crate::connectors::ConnectorKind::GoogleDrive,
            title: "Spray log 2026".to_owned(),
            author: None,
            at: "2026-10-12T07:30:00Z".parse().unwrap(),
            link: "https://docs.google.com/document/d/1aBcD/edit".to_owned(),
        };
        let m = message(vec![citation]);
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(
            v["farm_citations"],
            serde_json::json!([{
                "record_id": "01JAX3K9T6R2B8V0QZ4M7D1E5C.0.1aBcD", "system": "google_drive",
                "title": "Spray log 2026", "at": "2026-10-12T07:30:00Z",
                "link": "https://docs.google.com/document/d/1aBcD/edit"
            }])
        );
        assert_eq!(serde_json::from_value::<Message>(v).unwrap(), m);
    }

    #[test]
    fn search_farm_records_is_a_read_tool() {
        assert_eq!(
            serde_json::to_value(ToolName::SearchFarmRecords).unwrap(),
            serde_json::json!("search_farm_records")
        );
        assert!(!ToolName::SearchFarmRecords.is_write());
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

    // -----------------------------------------------------------------------------------------
    // Answer blocks: one exact-JSON test per kind, the event and `Message.blocks`.

    const TOMATO: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5A";
    const LETTUCE: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5B";
    const SCAN: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5S";

    fn day(date: &str, t_min_c: f64, frost_risk: bool) -> ForecastDay {
        ForecastDay {
            date: date.parse().unwrap(),
            t_min_c,
            t_max_c: 9.5,
            precip_mm: 0.0,
            mean_rh_pct: 78.0,
            hours_rh_over_90: 4,
            frost_risk,
        }
    }

    const DAY: &str = r#"{"date":"2026-10-22","t_min_c":-1.5,"t_max_c":9.5,"precip_mm":0.0,"mean_rh_pct":78.0,"hours_rh_over_90":4,"frost_risk":true}"#;

    fn plant(id: &str, name: &str, height_cm: Option<f64>) -> BlockPlant {
        BlockPlant {
            plant_id: id.parse().unwrap(),
            name: name.to_owned(),
            status: PlantStatus::Healthy,
            height_cm,
        }
    }

    fn exact(block: &AnswerBlock, json: &str) {
        assert_eq!(serde_json::to_string(block).unwrap(), json);
        assert_eq!(&serde_json::from_str::<AnswerBlock>(json).unwrap(), block);
    }

    #[test]
    fn a_garden_map_block_carries_the_map_its_plants_and_the_focus() {
        let tomato: PlantId = TOMATO.parse().unwrap();
        let lettuce: PlantId = LETTUCE.parse().unwrap();
        let pin = |plant_id, x_m| crate::garden::map::Pin {
            plant_id,
            x_m,
            y_m: 0.5,
            canopy_radius_m: None,
            bed_id: None,
        };
        let block = AnswerBlock {
            call_id: "toolu_map".to_owned(),
            body: BlockBody::GardenMap {
                map: GardenMap {
                    garden_id: "01JAX3K9T6R2B8V0QZ4M7D1E51".parse().unwrap(),
                    version: 2,
                    beds: Vec::new(),
                    pins: vec![pin(tomato, 0.0), pin(lettuce, 0.4)],
                    north_deg: None,
                    created_by: None,
                    created_at: "2026-10-18T10:00:00Z".parse().unwrap(),
                },
                neighbours: BTreeMap::from([(tomato, vec![lettuce]), (lettuce, vec![tomato])]),
                plants: vec![
                    plant(TOMATO, "Tomato 1", Some(42.0)),
                    plant(LETTUCE, "Lettuce", None),
                ],
                focus_plant_ids: vec![tomato],
            },
        };
        exact(
            &block,
            concat!(
                r#"{"call_id":"toolu_map","kind":"garden_map","map":{"garden_id":"01JAX3K9T6R2B8V0QZ4M7D1E51","version":2,"beds":[],"#,
                r#""pins":[{"plant_id":"01JAX3K9T6R2B8V0QZ4M7D1E5A","x_m":0.0,"y_m":0.5},{"plant_id":"01JAX3K9T6R2B8V0QZ4M7D1E5B","x_m":0.4,"y_m":0.5}],"#,
                r#""created_at":"2026-10-18T10:00:00Z"},"#,
                r#""neighbours":{"01JAX3K9T6R2B8V0QZ4M7D1E5A":["01JAX3K9T6R2B8V0QZ4M7D1E5B"],"01JAX3K9T6R2B8V0QZ4M7D1E5B":["01JAX3K9T6R2B8V0QZ4M7D1E5A"]},"#,
                r#""plants":[{"plant_id":"01JAX3K9T6R2B8V0QZ4M7D1E5A","name":"Tomato 1","status":"healthy","height_cm":42.0},"#,
                r#"{"plant_id":"01JAX3K9T6R2B8V0QZ4M7D1E5B","name":"Lettuce","status":"healthy"}],"#,
                r#""focus_plant_ids":["01JAX3K9T6R2B8V0QZ4M7D1E5A"]}"#
            ),
        );
        assert_eq!(block.kind(), AnswerBlockKind::GardenMap);
    }

    #[test]
    fn a_weather_hours_block_names_its_scan_only_when_anchored_on_one() {
        let forecast = AnswerBlock {
            call_id: "toolu_fc".to_owned(),
            body: BlockBody::WeatherHours {
                days: vec![day("2026-10-22", -1.5, true)],
                scan_id: None,
            },
        };
        exact(
            &forecast,
            &format!(r#"{{"call_id":"toolu_fc","kind":"weather_hours","days":[{DAY}]}}"#),
        );
        let snapshot = AnswerBlock {
            call_id: "toolu_scan".to_owned(),
            body: BlockBody::WeatherHours {
                days: vec![day("2026-10-22", -1.5, true)],
                scan_id: Some(SCAN.parse().unwrap()),
            },
        };
        exact(
            &snapshot,
            &format!(
                r#"{{"call_id":"toolu_scan","kind":"weather_hours","days":[{DAY}],"scan_id":"{SCAN}"}}"#
            ),
        );
    }

    #[test]
    fn a_frost_planner_marks_tender_hardy_and_leaves_unknown_out() {
        let frost = |id, name, frost_tender| FrostPlant {
            plant: plant(id, name, None),
            frost_tender,
        };
        let block = AnswerBlock {
            call_id: "toolu_fc".to_owned(),
            body: BlockBody::FrostPlanner {
                nights: vec![day("2026-10-22", -1.5, true)],
                plants: vec![
                    frost(TOMATO, "Tomato 1", Some(true)),
                    frost(LETTUCE, "Lettuce", Some(false)),
                    frost(SCAN, "Mystery", None),
                ],
            },
        };
        exact(
            &block,
            &format!(
                concat!(
                    r#"{{"call_id":"toolu_fc","kind":"frost_planner","nights":[{}],"plants":["#,
                    r#"{{"plant_id":"{}","name":"Tomato 1","status":"healthy","frost_tender":true}},"#,
                    r#"{{"plant_id":"{}","name":"Lettuce","status":"healthy","frost_tender":false}},"#,
                    r#"{{"plant_id":"{}","name":"Mystery","status":"healthy"}}]}}"#
                ),
                DAY, TOMATO, LETTUCE, SCAN
            ),
        );
    }

    #[test]
    fn a_measurement_trend_carries_the_plant_quantity_and_points() {
        let block = AnswerBlock {
            call_id: "toolu_m".to_owned(),
            body: BlockBody::MeasurementTrend {
                plant_id: TOMATO.parse().unwrap(),
                plant_name: "Tomato 1".to_owned(),
                quantity: GrowthQuantity::PlantHeightCm,
                points: vec![MeasurementPoint {
                    scan_id: SCAN.parse().unwrap(),
                    created_at: "2026-10-11T15:00:00Z".parse().unwrap(),
                    quantity: GrowthQuantity::PlantHeightCm,
                    value: 38.5,
                    confidence_band: crate::wire::ConfidenceBand::High,
                }],
            },
        };
        exact(
            &block,
            &format!(
                concat!(
                    r#"{{"call_id":"toolu_m","kind":"measurement_trend","plant_id":"{}","plant_name":"Tomato 1","#,
                    r#""quantity":"plant_height_cm","points":[{{"scan_id":"{}","created_at":"2026-10-11T15:00:00Z","#,
                    r#""quantity":"plant_height_cm","value":38.5,"confidence_band":"high"}}]}}"#
                ),
                TOMATO, SCAN
            ),
        );
    }

    #[test]
    fn a_candidates_block_names_the_problem_and_its_lookalikes() {
        let block = AnswerBlock {
            call_id: "toolu_l".to_owned(),
            body: BlockBody::Candidates {
                disease: ProblemRef {
                    problem_id: "tomato_early_blight".parse().unwrap(),
                    name: "Early blight".to_owned(),
                },
                lookalikes: vec![Lookalike {
                    problem_id: "septoria_leaf_spot".parse().unwrap(),
                    name: "Septoria leaf spot".to_owned(),
                    how_to_tell: "Smaller spots with grey centres and no rings.".to_owned(),
                }],
            },
        };
        exact(
            &block,
            concat!(
                r#"{"call_id":"toolu_l","kind":"candidates","disease":{"problem_id":"tomato_early_blight","name":"Early blight"},"#,
                r#""lookalikes":[{"problem_id":"septoria_leaf_spot","name":"Septoria leaf spot","how_to_tell":"Smaller spots with grey centres and no rings."}]}"#
            ),
        );
    }

    #[test]
    fn an_unknown_block_kind_does_not_decode() {
        let json = r#"{"call_id":"toolu_1","kind":"pie_chart","slices":[]}"#;
        assert!(serde_json::from_str::<AnswerBlock>(json).is_err());
    }

    #[test]
    fn the_answer_block_event_writes_v_and_the_block() {
        let e = ChatEvent::AnswerBlock(AnswerBlockEvent {
            v: V1,
            block: AnswerBlock {
                call_id: "toolu_fc".to_owned(),
                body: BlockBody::WeatherHours {
                    days: Vec::new(),
                    scan_id: None,
                },
            },
        });
        assert_eq!(e.event_name(), "answer_block");
        assert_eq!(
            serde_json::to_string(&e).unwrap(),
            r#"{"v":1,"block":{"call_id":"toolu_fc","kind":"weather_hours","days":[]}}"#
        );
    }

    #[test]
    fn a_message_carries_its_blocks_and_leaves_the_field_out_without_them() {
        let block = AnswerBlock {
            call_id: "toolu_fc".to_owned(),
            body: BlockBody::WeatherHours {
                days: vec![day("2026-10-22", -1.5, true)],
                scan_id: None,
            },
        };
        let mut m = message(Vec::new());
        assert!(serde_json::to_value(&m).unwrap().get("blocks").is_none());
        m.blocks = vec![block];
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(
            v["blocks"],
            serde_json::from_str::<serde_json::Value>(&format!(
                r#"[{{"call_id":"toolu_fc","kind":"weather_hours","days":[{DAY}]}}]"#
            ))
            .unwrap()
        );
        assert_eq!(serde_json::from_value::<Message>(v).unwrap(), m);
    }

    #[test]
    fn show_block_is_a_read_tool() {
        assert_eq!(
            serde_json::to_value(ToolName::ShowBlock).unwrap(),
            serde_json::json!("show_block")
        );
        assert!(!ToolName::ShowBlock.is_write());
        for kind in [
            AnswerBlockKind::GardenMap,
            AnswerBlockKind::WeatherHours,
            AnswerBlockKind::FrostPlanner,
            AnswerBlockKind::MeasurementTrend,
            AnswerBlockKind::Candidates,
        ] {
            assert_eq!(serde_json::to_value(kind).unwrap(), kind.as_str());
        }
    }
}
