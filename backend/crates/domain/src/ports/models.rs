//! Model ports: vision on the workers, species identification, the text classifier, text
//! embeddings, the chat/LLM API and the spot segmenter used by the measurement stage.

use std::{collections::BTreeMap, sync::Arc};

use bytes::Bytes;
use futures::stream::BoxStream;

use crate::{
    chat::Usage,
    id::{JobId, ScanId, SpeciesId},
    jobs::{
        GuidedPayload, GuidedResult, JobImage, PerceptionPayload, PerceptionResult, SpeciesResult,
        VerifyPayload, VerifyResult, VlmPayload, VlmResult, WorkerEvent,
    },
    ports::{BoxError, ModelError},
    wire::Run,
};

// ---------------------------------------------------------------------------------------------
// Vision (workers)
// ---------------------------------------------------------------------------------------------

/// The job a model call belongs to, for logs and progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobContext {
    /// The job.
    pub job_id: JobId,
    /// Its scan.
    pub scan_id: ScanId,
    /// Its run; absent for guided jobs.
    pub run: Option<Run>,
    /// 1 on the first try.
    pub attempt: u32,
}

/// An image the worker downloaded from its `JobImage.url`, still encoded (WebP).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedImage {
    /// The payload's description of it.
    pub image: JobImage,
    /// The encoded bytes.
    pub bytes: Bytes,
}

/// Why progress couldn't be sent.
#[derive(Debug, thiserror::Error)]
pub enum ProgressError {
    /// `409 lease_lost`: the job was requeued, expired or cancelled; stop now.
    #[error("lease lost")]
    LeaseLost,
    /// The API couldn't be reached; the worker keeps going and `complete` repeats every stage.
    #[error("progress not delivered")]
    Transport(#[source] BoxError),
}

/// Where a model sends stage events and partial results while a job runs
/// (`POST /internal/jobs/{id}/events`, [contracts 5]). Passed as `Arc<dyn ProgressSink>`: the
/// worker keeps one per job and the model may hand a clone to a blocking task.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait ProgressSink: Send + Sync {
    /// Sends events in order.
    async fn send(&self, events: Vec<WorkerEvent>) -> Result<(), ProgressError>;
}

/// The vision backend of a worker ([contracts 5], [contracts 6]): the perception models on
/// the RTX 3070, the VLM sidecar on the 5070 Ti, or the LLM API on the fallback worker.
///
/// **One method per job kind**, not one generic dispatch: each kind has its own payload and
/// result type, so the compiler checks that a `vlm` payload produces a `VlmResult`, and the
/// worker matches `JobSpec` once. An adapter answers the kinds its capability claims and
/// returns [`ModelError::Unsupported`] for the others; the fallback adapter answers all four.
///
/// Cancellation is by drop: when a heartbeat lists the job in `cancel_job_ids`, the worker
/// drops the future. Adapters keep blocking work (`ort` sessions, image decoding) on
/// `spawn_blocking` and must tolerate being dropped at any `.await`.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait VisionModel: Send + Sync {
    /// Model name → version, sent in `complete.model_versions`.
    fn model_versions(&self) -> BTreeMap<String, String>;

    /// A `perception` job: each model once over the batch, one partial result per finished
    /// stage.
    async fn perceive(
        &self,
        job: JobContext,
        payload: &PerceptionPayload,
        images: &[LoadedImage],
        progress: Arc<dyn ProgressSink>,
    ) -> Result<PerceptionResult, ModelError>;

    /// A `vlm` job: the findings call, then the indicators call unless measurements answered
    /// every indicator.
    async fn analyse(
        &self,
        job: JobContext,
        payload: &VlmPayload,
        images: &[LoadedImage],
        progress: Arc<dyn ProgressSink>,
    ) -> Result<VlmResult, ModelError>;

    /// A `verify` job: the indicators call only.
    async fn verify(
        &self,
        job: JobContext,
        payload: &VerifyPayload,
        images: &[LoadedImage],
        progress: Arc<dyn ProgressSink>,
    ) -> Result<VerifyResult, ModelError>;

    /// A `guided` job: one squeeze or jar test image, alone.
    async fn read_guided_check(
        &self,
        job: JobContext,
        payload: &GuidedPayload,
        image: &LoadedImage,
    ) -> Result<GuidedResult, ModelError>;
}

/// The label list `BioCLIP` scores against, generated from the knowledge base ([contracts 10]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeciesLabelSet {
    /// `label_set_version`.
    pub version: String,
    /// Each species' `bioclip_label`.
    pub labels: Vec<(SpeciesId, String)>,
}

/// Species identification (architecture 6.2): `BioCLIP` inside the perception worker today, an
/// external identification API behind the same trait later.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait SpeciesIdentifier: Send + Sync {
    /// The top candidates for the plant in these images.
    async fn identify(
        &self,
        images: &[LoadedImage],
        labels: &SpeciesLabelSet,
    ) -> Result<SpeciesResult, ModelError>;
}

// ---------------------------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------------------------

/// What the text classifier is asked (architecture 7, router and safety gate).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClassifierTask {
    /// The chat route: `diagnosis_question`, `care_question`, `task_change`, `safety_sensitive`,
    /// `off_topic`, and `farm_records` when the garden has farm records to search, so a
    /// question about its own Slack channels or documents isn't taken for off-topic (gap G10).
    Route {
        /// The garden has farm records: a synced record, or a connector that is `active` or
        /// `reauth_needed`, the condition that offers `search_farm_records`.
        farm_records: bool,
    },
    /// Whether a turn holds a fact worth remembering.
    MemoryWorthy,
    /// A knowledge chunk's topic, at ingestion.
    ChunkTopic,
}

/// The labels of [`ClassifierTask::Route`] for a garden without farm records, in the order of
/// contracts 8. `agent`'s classifier and Jev share them.
pub const ROUTE_LABELS: [&str; 5] = [
    "diagnosis_question",
    "care_question",
    "task_change",
    "safety_sensitive",
    "off_topic",
];

/// The label for a question about the garden's own farm records (gap G10). It is no route of
/// its own: the router answers it as a care question, and only when the garden has farm records.
pub const FARM_RECORDS_LABEL: &str = "farm_records";

/// The labels of [`ClassifierTask::Route`] for a garden with farm records: [`ROUTE_LABELS`]
/// and [`FARM_RECORDS_LABEL`] before `off_topic`.
pub const FARM_ROUTE_LABELS: [&str; 6] = [
    "diagnosis_question",
    "care_question",
    "task_change",
    "safety_sensitive",
    FARM_RECORDS_LABEL,
    "off_topic",
];

/// The labels of [`ClassifierTask::MemoryWorthy`]: the exchange holds a fact worth remembering,
/// or it doesn't. `agent`'s memory gate compares the two; its classifier and Jev share them.
pub const MEMORY_LABELS: [&str; 2] = ["remember", "skip"];

/// The labels of [`ClassifierTask::ChunkTopic`]: the chunk topics ([contracts 10],
/// `ChunkSeed.topic`), shared by `agent`'s classifier and Jev.
pub const TOPIC_LABELS: [&str; 5] = [
    "symptoms",
    "conditions",
    "management",
    "care",
    "recognition",
];

/// One label's score. Scores are compared with each other, not read as probabilities.
#[derive(Debug, Clone, PartialEq)]
pub struct LabelScore {
    /// The label.
    pub label: String,
    /// Its score.
    pub score: f64,
}

/// The fast text classifier: Jev, or the cheapest LLM with structured output.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait TextClassifier: Send + Sync {
    /// Scores every label of the task, highest first.
    async fn classify(
        &self,
        task: ClassifierTask,
        text: &str,
    ) -> Result<Vec<LabelScore>, ModelError>;
}

/// The pinned text-embedding model (architecture 10): its name, version and dimension, which
/// must equal the `chunk_vec` index's `DIMENSION` ([database 5]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingModel {
    /// Stored as `embedding_model` beside every vector.
    pub name: String,
    /// The model revision.
    pub version: String,
    /// The vector length.
    pub dimension: usize,
}

/// A text or image embedding.
#[derive(Debug, Clone, PartialEq)]
pub struct Embedding(pub Vec<f32>);

impl Embedding {
    /// The vector length.
    pub fn dimension(&self) -> usize {
        self.0.len()
    }

    /// Cosine similarity; 0 when either vector is zero or the lengths differ.
    pub fn cosine(&self, other: &Self) -> f32 {
        if self.0.len() != other.0.len() {
            return 0.0;
        }
        let dot: f32 = self.0.iter().zip(&other.0).map(|(a, b)| a * b).sum();
        let norm = |v: &[f32]| v.iter().map(|x| x * x).sum::<f32>().sqrt();
        let denom = norm(&self.0) * norm(&other.0);
        if denom == 0.0 { 0.0 } else { dot / denom }
    }
}

/// Which side of a retrieval the texts are on. Asymmetric retrieval models (arctic-embed-xs,
/// architecture 10) embed the two sides differently: a query gets the model's instruction
/// prefix, a document gets none. Mixing them up still returns vectors, only worse matches, so
/// the caller names the side on every call instead of being handed a pre-set embedder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EmbedMode {
    /// What is searched with: a chat question, a `search_knowledge` query, a memory search.
    Query,
    /// What is searched: a knowledge chunk, a memory being saved.
    Document,
}

/// Text embeddings on the control plane's CPU (architecture 10), for memory and knowledge
/// queries and the knowledge loader. The model's identity is data returned by a method, not an
/// associated `const`: a trait with an associated const is not dyn compatible (Rust Reference,
/// dyn compatibility), and the model is chosen at start-up. One instance serves both
/// [`EmbedMode`]s; a model without a query prefix ignores the mode.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait TextEmbedder: Send + Sync {
    /// The pinned model.
    fn model(&self) -> EmbeddingModel;

    /// One embedding per text, in order, embedded as `mode`. Runs the model on a blocking
    /// thread.
    async fn embed(&self, mode: EmbedMode, texts: &[String]) -> Result<Vec<Embedding>, ModelError>;
}

// ---------------------------------------------------------------------------------------------
// Chat / LLM API
// ---------------------------------------------------------------------------------------------

/// Which caller is asking, so the adapter picks the model tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChatPurpose {
    /// A chat turn: the strongest model.
    Chat,
    /// The plan writer, with structured output.
    PlanWriter,
    /// Memory extraction: a cheap model.
    MemoryExtraction,
    /// An off-topic redirect: the cheapest model.
    OffTopic,
    /// The fallback worker's vision calls.
    VisionFallback,
    /// The text classifier's LLM fallback (routing, the memory gate, chunk topics): the
    /// cheapest model with structured output.
    Classification,
}

/// A message's author.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChatRole {
    /// The user.
    User,
    /// The model.
    Assistant,
    /// A tool result.
    Tool,
}

/// One part of a message.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentPart {
    /// Text.
    Text(String),
    /// An image sent to the model (`view_image`, the fallback worker).
    Image {
        /// `image/webp`.
        media_type: &'static str,
        /// The bytes.
        data: Bytes,
    },
    /// A tool call the model made earlier in the conversation.
    ToolCall {
        /// The call.
        call_id: String,
        /// The tool.
        name: String,
        /// Its arguments.
        arguments: serde_json::Value,
    },
    /// A tool's result.
    ToolResult {
        /// The call it answers.
        call_id: String,
        /// The result, the same records the app sees, or `{ "error": reason }`.
        content: serde_json::Value,
        /// The call was refused (a dose in its arguments, a scope or validation failure): the
        /// model reads the reason and may rewrite the call (`tool_result.is_error`).
        is_error: bool,
    },
    /// A reasoning block the model produced earlier in this turn's tool loop. Replayed
    /// unchanged on the next request of the loop; never stored, never sent to the app.
    Reasoning(ReasoningBlock),
}

/// A model's reasoning block, opaque to `agent`. The LLM API requires the blocks of the
/// current tool loop to be sent back unchanged; the signature binds each block to the
/// conversation prefix that produced it, so a rebuilt context drops every earlier one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReasoningBlock {
    /// A `thinking` block: its (possibly empty) text and its signature.
    Thinking {
        /// The summarised or omitted reasoning text.
        text: String,
        /// The provider's signature, replayed byte for byte.
        signature: String,
    },
    /// A `redacted_thinking` block.
    Redacted {
        /// The encrypted payload.
        data: String,
    },
}

/// One message of the context.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessage {
    /// Its author.
    pub role: ChatRole,
    /// Its parts.
    pub content: Vec<ContentPart>,
}

/// A tool the model may call, filtered by role before each turn ([contracts 8]).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    /// The tool name.
    pub name: String,
    /// What it does, for the model.
    pub description: String,
    /// JSON Schema of its arguments.
    pub parameters: serde_json::Value,
    /// Ask the provider to guarantee the arguments match the schema. Each strict tool adds to a
    /// grammar the provider compiles per request, and nineteen are too many for Anthropic's
    /// (a 400), so only the tools that propose changes set it.
    pub strict: bool,
}

/// One LLM call. Owned, so it can be built once and borrowed by the stream for its whole life.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    /// Who is asking.
    pub purpose: ChatPurpose,
    /// The system prompt.
    pub system: String,
    /// The context, oldest first.
    pub messages: Vec<ChatMessage>,
    /// The tools offered.
    pub tools: Vec<ToolSpec>,
    /// Structured output: the JSON Schema the answer must match.
    pub response_schema: Option<serde_json::Value>,
    /// The output budget.
    pub max_tokens: u32,
}

/// Why the model stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// The answer is complete.
    EndTurn,
    /// It wants the tool results.
    ToolUse,
    /// `max_tokens` was reached.
    MaxTokens,
    /// The provider's safety classifier declined (HTTP 200 with `stop_reason: "refusal"`):
    /// any text so far is incomplete and any tool call must not run.
    Refusal,
}

/// One call's token counts as the provider bills them: [`Usage`] (every input token, cached or
/// not, as `message_completed` and the stored message carry it) with the part of the input
/// read from and written to the prompt cache. Only the daily chat budget reads the split
/// ([`CallUsage::budget_tokens`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CallUsage {
    /// All input and output tokens.
    pub usage: Usage,
    /// Of `usage.tokens_in`, the tokens read from the prompt cache.
    pub cache_read: u32,
    /// Of `usage.tokens_in`, the tokens written to the prompt cache.
    pub cache_write: u32,
}

impl CallUsage {
    /// A cache read counts as this fraction of an input token in the budget: Anthropic's
    /// usual ratio (0.1×; Opus 5.5 bills 0.05×, so the budget errs high).
    pub const CACHE_READ_WEIGHT: (u64, u64) = (1, 10);
    /// A cache write counts as 1.25 input tokens: Anthropic's price for the 5-minute cache,
    /// the one the agent uses.
    pub const CACHE_WRITE_WEIGHT: (u64, u64) = (5, 4);
    /// An output token (thinking included) counts as 5 input tokens: Opus 5.5's $20 to $4 per
    /// million, so a budget token costs about an input token's price (US$4 per million).
    pub const OUTPUT_WEIGHT: u64 = 5;

    /// Counts that carry no cache split (a provider that reports none, a test).
    #[must_use]
    pub const fn plain(usage: Usage) -> Self {
        Self {
            usage,
            cache_read: 0,
            cache_write: 0,
        }
    }

    /// The call's weight in the daily chat budget, in input-token prices: uncached input at 1,
    /// cache reads at 0.1, cache writes at 1.25 (each rounded up) and output at 5. Anthropic's
    /// prices, from the claude-api skill (cache reads 0.1× the input price, 5-minute writes
    /// 1.25×, Opus 5.5 output 5×), so the budget is a cost cap: most of a tool turn's input is
    /// the cached prefix read again on each call.
    #[must_use]
    pub fn budget_tokens(&self) -> u64 {
        let read = u64::from(self.cache_read);
        let write = u64::from(self.cache_write);
        let uncached = u64::from(self.usage.tokens_in)
            .saturating_sub(read)
            .saturating_sub(write);
        let (rn, rd) = Self::CACHE_READ_WEIGHT;
        let (wn, wd) = Self::CACHE_WRITE_WEIGHT;
        uncached
            + (read * rn).div_ceil(rd)
            + (write * wn).div_ceil(wd)
            + u64::from(self.usage.tokens_out) * Self::OUTPUT_WEIGHT
    }
}

/// One piece of a streamed answer.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatChunk {
    /// More text. `agent` buffers it into sentences for the dose check before streaming.
    TextDelta(String),
    /// A complete tool call.
    ToolCall {
        /// The call.
        call_id: String,
        /// The tool.
        name: String,
        /// Its arguments.
        arguments: serde_json::Value,
    },
    /// A complete reasoning block, to replay on the next request of the tool loop.
    Reasoning(ReasoningBlock),
    /// Token counts, once known. The counts are cumulative for the call: a later `Usage`
    /// replaces an earlier one (it is not added to it), and the last one before `Done` is the
    /// call's total.
    Usage(CallUsage),
    /// The end.
    Done(StopReason),
}

/// A whole answer, for the non-streaming callers (the plan writer, memory extraction).
#[derive(Debug, Clone, PartialEq)]
pub struct ChatCompletion {
    /// The text, or the structured output as JSON text.
    pub text: String,
    /// Tool calls.
    pub tool_calls: Vec<ChatChunk>,
    /// Token counts.
    pub usage: Usage,
    /// Why it stopped.
    pub stop: StopReason,
}

/// The LLM API (architecture 7): streaming for chat, whole answers for the plan writer.
///
/// `stream` is a plain method returning a boxed stream, so the trait stays dyn compatible
/// without `#[async_trait]`. The named lifetime `'a` ties the stream to both `&self` and the
/// borrowed request: the adapter can keep references into the request (messages, tool schemas,
/// image bytes) while it streams instead of cloning them, and the borrow checker guarantees the
/// request outlives the stream. A caller that needs a `'static` stream (the SSE response body)
/// moves the `Arc<dyn ChatModel>` and the owned request into an `async_stream` generator and
/// borrows them inside it.
#[async_trait::async_trait]
pub trait ChatModel: Send + Sync {
    /// The model name, stored on every message and plan.
    fn model_name(&self, purpose: ChatPurpose) -> String;

    /// Streams the answer.
    fn stream<'a>(
        &'a self,
        request: &'a ChatRequest,
    ) -> BoxStream<'a, Result<ChatChunk, ModelError>>;

    /// The whole answer.
    async fn complete(&self, request: &ChatRequest) -> Result<ChatCompletion, ModelError>;
}

// ---------------------------------------------------------------------------------------------
// Spot segmentation (measurement stage)
// ---------------------------------------------------------------------------------------------

/// A borrowed RGB8 image, row-major: the stored photo the measurement stage already decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbView<'a> {
    width: u32,
    height: u32,
    pixels: &'a [u8],
}

impl<'a> RgbView<'a> {
    /// Checks `pixels.len() == width × height × 3`.
    pub fn new(width: u32, height: u32, pixels: &'a [u8]) -> Option<Self> {
        let expected = usize::try_from(u64::from(width) * u64::from(height) * 3).ok()?;
        (pixels.len() == expected).then_some(Self {
            width,
            height,
            pixels,
        })
    }

    /// Width.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The pixels.
    pub const fn pixels(&self) -> &'a [u8] {
        self.pixels
    }
}

/// A borrowed 8-bit mask of the same size as its image: non-zero inside the leaf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaskView<'a> {
    width: u32,
    height: u32,
    data: &'a [u8],
}

impl<'a> MaskView<'a> {
    /// Checks `data.len() == width × height`.
    pub fn new(width: u32, height: u32, data: &'a [u8]) -> Option<Self> {
        let expected = usize::try_from(u64::from(width) * u64::from(height)).ok()?;
        (data.len() == expected).then_some(Self {
            width,
            height,
            data,
        })
    }

    /// Width.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The mask.
    pub const fn data(&self) -> &'a [u8] {
        self.data
    }
}

/// What the segmenter calls each pixel inside the leaf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SpotClass {
    /// Outside the leaf mask.
    Outside = 0,
    /// Healthy leaf.
    Leaf = 1,
    /// Brown, black or pale against the leaf's own green: part of a spot.
    Necrotic = 2,
    /// Yellowing around a spot: counts toward `affected_area_pct`, never toward a spot's size.
    Halo = 3,
}

/// The segmenter's output, owned: one [`SpotClass`] per pixel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpotMask {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// Row-major classes.
    pub classes: Vec<SpotClass>,
}

/// Marks necrotic and yellowed pixels inside a leaf mask ([contracts 7], geometry: spots;
/// architecture 6.7). Colour thresholds today; a lesion-segmentation model can replace them.
/// Synchronous and CPU-bound: the measurement stage calls it on a blocking thread, over
/// buffers it borrows for the call.
#[cfg_attr(feature = "mock", mockall::automock)]
pub trait SpotSegmenter: Send + Sync {
    /// The algorithm version, part of a measurement's `model_version`.
    fn version(&self) -> String;

    /// Classifies every pixel inside `leaf`.
    fn segment<'a>(&self, image: RgbView<'a>, leaf: MaskView<'a>) -> SpotMask;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The live card turn of the S5 checks as one call with its totals: 8 uncached, 25,792
    /// written and 44,005 read of 69,805 input tokens, and 2,070 out, weighs 46,999 budget
    /// tokens (input prices; output at 5), not 71,875.
    #[test]
    fn the_budget_weighs_reads_writes_and_output_by_price() {
        let call = CallUsage {
            usage: Usage {
                tokens_in: 69_805,
                tokens_out: 2_070,
            },
            cache_read: 44_005,
            cache_write: 25_792,
        };
        assert_eq!(call.budget_tokens(), 8 + 4_401 + 32_240 + 5 * 2_070);
        let plain = CallUsage::plain(Usage {
            tokens_in: 100,
            tokens_out: 7,
        });
        assert_eq!(
            plain.budget_tokens(),
            100 + 35,
            "no cache split: input in full, output at 5"
        );
        let odd = CallUsage {
            usage: Usage {
                tokens_in: 3,
                tokens_out: 0,
            },
            cache_read: 1,
            cache_write: 1,
        };
        assert_eq!(odd.budget_tokens(), 1 + 1 + 2, "each part rounds up");
        let bad = CallUsage {
            cache_read: 10,
            ..CallUsage::default()
        };
        assert_eq!(
            bad.budget_tokens(),
            1,
            "a split larger than the input saturates"
        );
    }

    #[test]
    fn views_check_their_sizes() {
        let px = [0u8; 12];
        assert!(RgbView::new(2, 2, &px).is_some());
        assert!(RgbView::new(3, 2, &px).is_none());
        assert!(MaskView::new(3, 4, &px).is_some());
    }

    #[test]
    fn cosine() {
        let a = Embedding(vec![1.0, 0.0]);
        let b = Embedding(vec![0.0, 2.0]);
        assert!(a.cosine(&b).abs() < 1e-6);
        assert!((a.cosine(&a) - 1.0).abs() < 1e-6);
        assert!(a.cosine(&Embedding(vec![1.0])).abs() < 1e-6);
    }
}
