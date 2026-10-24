//! The chat turn (architecture 8.2; [contracts 8], sending a message): validation, the token
//! budget, routing and storing the user's message before the stream exists; then, in a task,
//! the context, the model's stream checked one sentence at a time, tool calls, the stored reply,
//! memory extraction and `message_completed`.
//!
//! **Who owns what.** axum needs a `'static` response body, so nothing the stream yields may
//! borrow from the request handler. [`ChatAgent::start_turn`] moves everything the turn needs
//! into a [`Turn`] (owned: ids, the proof, the text, the route) and spawns [`run`] with an
//! `Arc<Inner>` clone. Inside the task the turn borrows `&AgentDeps` from that `Arc` and lends
//! it to the context builder, the tools (`ToolCtx<'t>`) and the model (`ChatModel::stream<'a>`
//! borrows the `ChatRequest` the task owns). Those borrows live inside one `async` block, which
//! the compiler turns into a self-contained future: a borrow held across an `.await` is just a
//! field of that future, so it is allowed and the future stays `'static` because it owns the
//! `Arc` it borrows from. The handler gets a [`TurnStream`]: a channel receiver, `'static` by
//! construction.
//!
//! **Disconnects.** Dropping the stream (the client went away) closes the channel. The task
//! notices at its next send or while waiting for the model (`Sender::closed`), stops the model
//! call by dropping its stream, and stores the message with `interrupted: true`.
//!
//! **Tool results are data.** A successful result goes back to the model inside a `data` block
//! ([`crate::prompt::tool_result`]), like every other text the server didn't write; the stored
//! copy keeps the tool's JSON. A refusal stays plain: the server wrote it for the model to act on.
//!
//! **Answer blocks.** `show_block` is the one call the turn runs itself ([`crate::tools::blocks`]):
//! it reads the turn's own tool results and the blocks already sent, which the reply holds, so
//! only a call of this turn can be a block's anchor. It sends no `tool_started` or
//! `tool_finished`; a built block goes out as `answer_block`, after its anchor's
//! `tool_finished`, and is stored in `Message.blocks`.

use std::sync::Arc;

use domain::{
    authz::{ConversationOwner, Membership, Viewer},
    chat::{
        Anchor, AnswerBlock, AnswerBlockEvent, ChatErrorEvent, ChatEvent, ChatRoute, CitationEvent,
        MemorySaved, Message, MessageCompleted, MessageRole, MessageStarted, PendingActionEvent,
        RescanRequested, SendMessageRequest, TextDelta, ToolFinished, ToolName, ToolStarted, Usage,
    },
    connectors::FarmCitation,
    error::{Budget, DomainError, ErrorCode},
    id::{ChatImageId, MessageId},
    ports::{
        ChatChunk, ChatMessage, ChatPurpose, ChatRequest, ChatRole, ClassifierTask, ContentPart,
        IdGenExt as _, ModelError, NewMessage, ObjectKey, PriorExchange, StopReason,
    },
    records::diagnosis::Citation,
    rules::limits::{MESSAGE_MAX_CHARS, MESSAGE_MAX_IMAGES},
    url::Stored,
    wire::{PageRequest, Timestamp, V1},
};
use futures::{Stream, StreamExt as _};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::{
    context,
    deps::AgentDeps,
    error::{TurnError, stream_error},
    memory,
    prompt::{
        BUDGET_TEXT, CHAT_PROMPT_VERSION, DataKind, LOOP_CAP_TEXT, OFF_TOPIC_PROMPT_VERSION,
        POISON_CONTROL_VERSION, REFUSAL_TEXT, chat_system, data, off_topic_system, tool_result,
    },
    safety::{
        REPLACED_SENTENCE, RouteDecision, SAFETY_POLICY, SentenceSplitter, decide_route, dose_check,
    },
    tools::{
        Offer, ToolCtx, ToolEffect, ToolRegistry,
        blocks::{self, ShowBlockArgs},
        tool_name,
    },
};

/// Events buffered between the turn task and the response body. Small: the body drains it as
/// fast as the network allows, and a full buffer makes the task wait instead of running ahead.
const EVENT_BUFFER: usize = 32;

/// The chat agent `api` holds: cheap to clone.
#[derive(Clone)]
pub struct ChatAgent {
    inner: Arc<Inner>,
}

struct Inner {
    deps: AgentDeps,
    tools: ToolRegistry,
    tracker: TaskTracker,
}

impl std::fmt::Debug for ChatAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatAgent")
            .field("tools", &self.inner.tools)
            .finish_non_exhaustive()
    }
}

/// One `POST /conversations/{id}/messages`.
#[derive(Debug, Clone)]
pub struct TurnRequest {
    /// From `domain::authz::check_conversation`: the caller owns the conversation and is a
    /// member of its garden. The real role (for the tool list) is `owner.membership().role()`.
    pub owner: ConversationOwner<Viewer>,
    /// The body.
    pub message: SendMessageRequest,
    /// Server shutdown: the turn stops and stores what it has, like a disconnect.
    pub shutdown: CancellationToken,
}

/// How a turn ended, for `api`'s idempotency record: a completed message is stored under the
/// key and replayed as `200 { message }`; anything else releases the key ([contracts 1]).
#[derive(Debug, Clone, PartialEq)]
pub enum TurnOutcome {
    /// `message_completed` was sent; the stored assistant message.
    Completed(Box<Message<Stored>>),
    /// The client went away or the server shut down; stored with `interrupted: true`.
    Interrupted,
    /// An `error` event ended the stream.
    Failed,
}

/// The turn's events, `'static`: a channel receiver plus the outcome. Dropping it interrupts
/// the turn.
#[derive(Debug)]
pub struct TurnStream {
    events: ReceiverStream<ChatEvent>,
    outcome: oneshot::Receiver<TurnOutcome>,
}

impl TurnStream {
    /// The events and, once the task has stored the message, how the turn ended. Dropping the
    /// event half still interrupts the turn; dropping the outcome half is fine.
    pub fn into_parts(self) -> (ReceiverStream<ChatEvent>, oneshot::Receiver<TurnOutcome>) {
        (self.events, self.outcome)
    }
}

impl Stream for TurnStream {
    type Item = ChatEvent;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<ChatEvent>> {
        self.events.poll_next_unpin(cx)
    }
}

/// Everything the task needs, owned.
#[derive(Debug)]
struct Turn {
    owner: ConversationOwner<Viewer>,
    anchor: Anchor,
    text: String,
    image_keys: Vec<ObjectKey>,
    user_message: MessageId,
    message_id: MessageId,
    decision: RouteDecision,
    /// The garden has farm records to search, read once before routing (gaps G6, G10).
    farm_records: bool,
    now: Timestamp,
    /// Tokens left in the user's daily budget when the turn started.
    budget_left: u64,
    shutdown: CancellationToken,
}

impl ChatAgent {
    /// The agent over its ports. `tracker` is the process's task tracker, so shutdown waits for
    /// turns to store their messages.
    pub fn new(deps: AgentDeps, tracker: TaskTracker) -> Self {
        Self {
            inner: Arc::new(Inner {
                deps,
                tools: ToolRegistry::standard(),
                tracker,
            }),
        }
    }

    /// The ports.
    pub fn deps(&self) -> &AgentDeps {
        &self.inner.deps
    }

    /// The tool registry.
    pub fn tools(&self) -> &ToolRegistry {
        &self.inner.tools
    }

    /// The work before the stream: errors here are ordinary HTTP errors ([contracts 8]).
    pub async fn start_turn(&self, req: TurnRequest) -> Result<TurnStream, TurnError> {
        let deps = &self.inner.deps;
        let owner = req.owner;
        let member = owner.membership();
        let text = req.message.text;
        let image_ids = req.message.image_ids.unwrap_or_default();
        validate(&text, &image_ids)?;

        let now = deps.clock.now();
        let used = deps
            .stores
            .usage
            .usage(member.user(), now.utc_day())
            .await?;
        let spent = used.tokens_in.saturating_add(used.tokens_out);
        let budget = deps.config.daily_chat_tokens;
        if spent >= budget {
            return Err(DomainError::BudgetExceeded {
                budget: Budget::ChatTokens,
                resets_at: next_utc_midnight(now),
            }
            .into());
        }

        // The two newest messages: the anchor, and the exchange the router reads as context.
        let newest = PageRequest {
            cursor: None,
            limit: Some(2),
        };
        let detail = deps.stores.chats.conversation_detail(owner, newest).await?;
        let anchor = detail.conversation.anchor;
        let prior = prior_exchange(&detail.messages.items).unwrap_or_default();

        let mut image_keys = Vec::with_capacity(image_ids.len());
        for id in &image_ids {
            let object = deps
                .stores
                .chats
                .chat_image_object(owner.conversation(), *id)
                .await?
                .ok_or_else(|| DomainError::invalid("image_ids", "unknown_image"))?;
            image_keys.push(object.key);
        }

        let (decision, farm_records) = route(deps, member, &text, &prior).await;

        let user_message = deps.ids.id();
        deps.stores
            .chats
            .store_message(
                owner,
                NewMessage {
                    id: user_message,
                    role: MessageRole::User,
                    text: text.clone(),
                    attachments: image_ids,
                    citations: Vec::new(),
                    farm_citations: Vec::new(),
                    blocks: Vec::new(),
                    tool_calls: Vec::new(),
                    route: Some(decision.route()),
                    interrupted: false,
                    model: None,
                    prompt_version: None,
                    tokens_in: 0,
                    tokens_out: 0,
                    created_at: now,
                },
            )
            .await?;

        let turn = Turn {
            owner,
            anchor,
            text,
            image_keys,
            user_message,
            message_id: deps.ids.id(),
            decision,
            farm_records,
            now,
            budget_left: budget - spent,
            shutdown: req.shutdown,
        };
        let (tx, rx) = mpsc::channel(EVENT_BUFFER);
        let (outcome_tx, outcome_rx) = oneshot::channel();
        let inner = Arc::clone(&self.inner);
        self.inner
            .tracker
            .spawn(async move { run(inner, turn, tx, outcome_tx).await });
        Ok(TurnStream {
            events: ReceiverStream::new(rx),
            outcome: outcome_rx,
        })
    }
}

/// The longest previous user message the router reads, in characters.
pub const ROUTE_CONTEXT_USER_CHARS: usize = 300;

/// The longest previous assistant reply the router reads, in characters.
pub const ROUTE_CONTEXT_ASSISTANT_CHARS: usize = 1_000;

/// The first `max` characters of `text`, marked with `…` when cut.
fn cut_start(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => format!("{}…", text[..end].trim_end()),
        None => text.to_owned(),
    }
}

/// The last `max` characters of `text`, marked with `…` when cut: a long question usually
/// ends with what it asks.
fn cut_end(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_owned();
    }
    let start = text.char_indices().nth(count - max).map_or(0, |(i, _)| i);
    format!("…{}", text[start..].trim_start())
}

/// The exchange before this message, from the conversation's newest messages (newest first),
/// so a follow-up is routed with what it refers to. `None` in a conversation's first message.
///
/// - The assistant half is the last reply, its first [`ROUTE_CONTEXT_ASSISTANT_CHARS`]. It is
///   left out when empty, and when the reply cites farm records: such a reply can quote Slack
///   messages or Drive files that people outside PDM wrote, which must not reach a classifier
///   that has no data framing (Jev's `state`).
/// - The user half is the user message before it, or the newest message itself when no reply
///   followed it (an interrupted turn): its last [`ROUTE_CONTEXT_USER_CHARS`].
pub fn prior_exchange<U: domain::url::UrlState>(newest: &[Message<U>]) -> Option<PriorExchange> {
    fn said<U: domain::url::UrlState>(m: &Message<U>) -> Option<&str> {
        Some(m.text.trim()).filter(|t| !t.is_empty())
    }
    let user_half = |m: Option<&Message<U>>| {
        m.filter(|m| m.role == MessageRole::User)
            .and_then(said)
            .map(|t| cut_end(t, ROUTE_CONTEXT_USER_CHARS))
    };
    let first = newest.first()?;
    let prior = if first.role == MessageRole::Assistant {
        PriorExchange {
            assistant: said(first)
                .filter(|_| first.farm_citations.is_empty())
                .map(|t| cut_start(t, ROUTE_CONTEXT_ASSISTANT_CHARS)),
            user: user_half(newest.get(1)),
        }
    } else {
        PriorExchange {
            assistant: None,
            user: user_half(Some(first)),
        }
    };
    (!prior.is_empty()).then_some(prior)
}

/// Routes the message. Whether the garden has farm records is read first, once per turn: the
/// classifier is asked for the `farm_records` label only then, so a question about the garden's
/// own records is on topic only then (gap G10), and the same fact offers `search_farm_records`
/// (gap G6). The exchange before the message goes with it, so a follow-up such as "what did you
/// just tell me?" is routed as the conversation it continues. A classifier that fails or is
/// slower than `classifier_timeout` leaves no scores.
async fn route(
    deps: &AgentDeps,
    member: Membership<Viewer>,
    text: &str,
    prior: &PriorExchange,
) -> (RouteDecision, bool) {
    let farm_records = context::has_farm_records(deps, member).await;
    let scores = tokio::time::timeout(
        deps.config.classifier_timeout,
        deps.classifier
            .classify_with_context(ClassifierTask::Route { farm_records }, text, prior),
    )
    .await
    .ok()
    .and_then(Result::ok);
    let decision = decide_route(
        text,
        scores.as_deref(),
        &deps.config.calibration,
        farm_records,
    );
    (decision, farm_records)
}

/// `SendMessageRequest`'s limits: at most 2,000 characters and 3 images.
fn validate(text: &str, images: &[ChatImageId]) -> Result<(), DomainError> {
    if text.trim().is_empty() {
        return Err(DomainError::invalid("text", "empty"));
    }
    if text.chars().count() > MESSAGE_MAX_CHARS {
        return Err(DomainError::invalid("text", "too_long"));
    }
    if images.len() > MESSAGE_MAX_IMAGES {
        return Err(DomainError::invalid("image_ids", "too_many"));
    }
    Ok(())
}

/// When the daily budget resets: the next midnight UTC (usage is counted per UTC day).
fn next_utc_midnight(now: Timestamp) -> Timestamp {
    let day = now.utc_day();
    day.succ_opt()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map_or(now, |t| Timestamp::from(t.and_utc()))
}

// ---------------------------------------------------------------------------------------------
// The task
// ---------------------------------------------------------------------------------------------

/// Sends events until the client goes away.
struct Emitter {
    tx: mpsc::Sender<ChatEvent>,
    gone: bool,
}

impl Emitter {
    /// Sends one event; false once the receiver is gone.
    async fn send(&mut self, event: ChatEvent) -> bool {
        if !self.gone && self.tx.send(event).await.is_err() {
            self.gone = true;
        }
        !self.gone
    }

    async fn text(&mut self, text: String) -> bool {
        self.send(ChatEvent::TextDelta(TextDelta { v: V1, text }))
            .await
    }
}

/// How the reply ended.
#[derive(Debug)]
enum Ended {
    Completed,
    Interrupted,
    Failed(ModelError),
    Store(domain::ports::StoreError),
}

/// What the stored assistant message is built from.
#[derive(Debug, Default)]
struct Reply {
    /// The text as streamed, failing sentences replaced.
    text: String,
    /// The message being written, for the dose check's log line.
    message_id: Option<MessageId>,
    /// A tool round ended since the last text: the next text starts a new paragraph.
    after_tool_round: bool,
    citations: Vec<Citation>,
    /// The farm records the tools returned, each once (`Message.farm_citations`).
    farm_citations: Vec<FarmCitation>,
    /// Tool calls and results, in order (stored, never sent to the app).
    tool_calls: Vec<ContentPart>,
    /// The answer blocks sent, in order (`Message.blocks`).
    blocks: Vec<AnswerBlock>,
    usage: Usage,
    purpose: Option<ChatPurpose>,
    prompt_version: &'static str,
}

impl Reply {
    fn add_usage(&mut self, u: Usage) {
        self.usage.tokens_in = self.usage.tokens_in.saturating_add(u.tokens_in);
        self.usage.tokens_out = self.usage.tokens_out.saturating_add(u.tokens_out);
    }

    fn spent(&self) -> u64 {
        u64::from(self.usage.tokens_in) + u64::from(self.usage.tokens_out)
    }
}

/// The turn task. Owns `inner` (an `Arc`) and `turn`; everything below borrows from them.
async fn run(
    inner: Arc<Inner>,
    turn: Turn,
    tx: mpsc::Sender<ChatEvent>,
    outcome: oneshot::Sender<TurnOutcome>,
) {
    let deps = &inner.deps;
    let mut out = Emitter { tx, gone: false };
    let mut reply = Reply {
        message_id: Some(turn.message_id),
        ..Reply::default()
    };
    let route = turn.decision.route();

    let started = MessageStarted {
        v: V1,
        message_id: turn.message_id,
        route,
    };
    let ended = if out.send(ChatEvent::MessageStarted(started)).await {
        match turn.decision {
            RouteDecision::PoisonControl => {
                reply.prompt_version = POISON_CONTROL_VERSION;
                let text = deps.config.poison_control_text.clone();
                emit_checked(deps, &mut out, &mut reply, text).await
            }
            RouteDecision::OffTopic => off_topic(deps, &turn, &mut out, &mut reply).await,
            RouteDecision::Model(route) => {
                model_turn(&inner, &turn, route, &mut out, &mut reply).await
            }
        }
    } else {
        Ended::Interrupted
    };

    let failed = matches!(ended, Ended::Failed(_) | Ended::Store(_));
    if let Ended::Failed(e) = &ended {
        tracing::warn!(error = %e, message_id = %turn.message_id, "chat model failed mid-turn");
        let (code, message) = stream_error(e);
        error_event(&mut out, code, message).await;
    }
    if let Ended::Store(e) = &ended {
        tracing::warn!(error = %e, message_id = %turn.message_id, "store failed mid-turn");
        error_event(
            &mut out,
            ErrorCode::Internal,
            "Something went wrong while answering.",
        )
        .await;
    }
    let completed = matches!(ended, Ended::Completed) && !out.gone;

    let stored = store_reply(deps, &turn, &reply, route, !completed).await;
    let member = turn.owner.membership();
    if let Err(e) = deps
        .stores
        .usage
        .add_chat_usage(member.user(), turn.now.utc_day(), reply.usage)
        .await
    {
        tracing::warn!(error = %e, "chat usage not recorded");
    }

    let result = match (stored, completed) {
        (Ok(message), true) => {
            if matches!(turn.decision, RouteDecision::Model(_)) {
                let ctx = tool_ctx(deps, &turn, None);
                match memory::after_turn(&ctx, &turn.text, &message.text).await {
                    Ok(Some(memory)) => {
                        out.send(ChatEvent::MemorySaved(MemorySaved { v: V1, memory }))
                            .await;
                    }
                    Ok(None) => {}
                    Err(e) => tracing::warn!(error = %e, "memory extraction failed"),
                }
            }
            let done = MessageCompleted {
                v: V1,
                message_id: turn.message_id,
                usage: reply.usage,
            };
            if out.send(ChatEvent::MessageCompleted(done)).await {
                TurnOutcome::Completed(Box::new(message))
            } else {
                TurnOutcome::Interrupted
            }
        }
        (Ok(_), false) if failed => TurnOutcome::Failed,
        (Ok(_), false) => TurnOutcome::Interrupted,
        (Err(e), _) => {
            tracing::warn!(error = %e, message_id = %turn.message_id, "assistant message not stored");
            if completed {
                error_event(
                    &mut out,
                    ErrorCode::Internal,
                    "The reply could not be saved.",
                )
                .await;
            }
            TurnOutcome::Failed
        }
    };
    let _ = outcome.send(result);
    drop(out);

    if let Err(e) = memory::summarise_if_due(deps, turn.owner).await {
        tracing::warn!(error = %e, "conversation summary failed");
    }
}

async fn error_event(out: &mut Emitter, code: ErrorCode, message: &str) {
    out.send(ChatEvent::Error(ChatErrorEvent {
        v: V1,
        code,
        message: message.to_owned(),
        retryable: code.default_retryable(),
    }))
    .await;
}

fn tool_ctx<'t>(
    deps: &'t AgentDeps,
    turn: &Turn,
    plant: Option<domain::id::PlantId>,
) -> ToolCtx<'t> {
    ToolCtx {
        deps,
        owner: turn.owner,
        anchor: turn.anchor,
        plant: plant.or(match turn.anchor {
            Anchor::Plant(p) => Some(p),
            Anchor::Scan(_) | Anchor::Garden(_) => None,
        }),
        message_id: turn.message_id,
        now: turn.now,
    }
}

async fn store_reply(
    deps: &AgentDeps,
    turn: &Turn,
    reply: &Reply,
    route: ChatRoute,
    interrupted: bool,
) -> Result<Message<Stored>, domain::ports::StoreError> {
    deps.stores
        .chats
        .store_message(
            turn.owner,
            NewMessage {
                id: turn.message_id,
                role: MessageRole::Assistant,
                text: reply.text.clone(),
                attachments: Vec::new(),
                citations: reply.citations.clone(),
                farm_citations: reply.farm_citations.clone(),
                blocks: reply.blocks.clone(),
                tool_calls: reply.tool_calls.clone(),
                route: Some(route),
                interrupted,
                model: reply.purpose.map(|p| deps.chat.model_name(p)),
                prompt_version: Some(reply.prompt_version.to_owned()),
                tokens_in: reply.usage.tokens_in,
                tokens_out: reply.usage.tokens_out,
                created_at: deps.clock.now(),
            },
        )
        .await
}

/// Dose-checks one streamed piece and sends it; a failing sentence is replaced by the fixed
/// safety note, keeping the piece's surrounding whitespace ([contracts 7]).
async fn emit_piece(deps: &AgentDeps, out: &mut Emitter, reply: &mut Reply, piece: String) -> bool {
    let checked = if let Some(found) = dose_check(&piece, &deps.products) {
        // The rule and whether the sentence held a quantity (`Rate` without one: the unit
        // started the sentence, as after a line break); never the matched text or the
        // sentence, which may hold a rate.
        tracing::info!(
            message_id = reply.message_id.as_ref().map(tracing::field::display),
            rule = ?found.rule,
            has_quantity = found.has_quantity(),
            "dose check replaced a sentence"
        );
        let lead = &piece[..piece.len() - piece.trim_start().len()];
        let trail = &piece[piece.trim_end().len()..];
        format!("{lead}{REPLACED_SENTENCE}{trail}")
    } else {
        piece
    };
    if std::mem::take(&mut reply.after_tool_round)
        && !reply.text.is_empty()
        && !reply.text.ends_with(char::is_whitespace)
        && !checked.starts_with(char::is_whitespace)
    {
        reply.text.push_str(PARAGRAPH_BREAK);
        if !out.text(PARAGRAPH_BREAK.to_owned()).await {
            return false;
        }
    }
    reply.text.push_str(&checked);
    out.text(checked).await
}

/// Separates the text before a tool call from the text after it.
const PARAGRAPH_BREAK: &str = "\n\n";

/// Sends a fixed or complete text sentence by sentence.
async fn emit_checked(
    deps: &AgentDeps,
    out: &mut Emitter,
    reply: &mut Reply,
    text: String,
) -> Ended {
    let mut splitter = SentenceSplitter::new();
    let pieces = splitter.push(&text).into_iter().chain(splitter.finish());
    for piece in pieces {
        if !emit_piece(deps, out, reply, piece).await {
            return Ended::Interrupted;
        }
    }
    Ended::Completed
}

/// What one model call produced.
#[derive(Debug, Default)]
struct CallResult {
    /// The assistant turn exactly as the model wrote it (raw text, reasoning, tool calls), to
    /// replay in the next request of the loop.
    parts: Vec<ContentPart>,
    calls: Vec<(String, String, serde_json::Value)>,
    stop: Option<StopReason>,
}

/// Streams one model call: text goes out sentence by sentence, everything is recorded.
async fn stream_call(
    deps: &AgentDeps,
    turn: &Turn,
    request: &ChatRequest,
    out: &mut Emitter,
    reply: &mut Reply,
    splitter: &mut SentenceSplitter,
) -> Result<CallResult, Ended> {
    let mut result = CallResult::default();
    let mut raw = String::new();
    let mut usage = Usage::default();
    let mut stream = deps.chat.stream(request);
    loop {
        let next = tokio::select! {
            biased;
            () = turn.shutdown.cancelled() => return Err(Ended::Interrupted),
            () = out.tx.closed() => return Err(Ended::Interrupted),
            next = stream.next() => next,
        };
        let chunk = match next {
            None => break,
            Some(Err(e)) => {
                reply.add_usage(usage);
                return Err(Ended::Failed(e));
            }
            Some(Ok(c)) => c,
        };
        match chunk {
            ChatChunk::TextDelta(t) => {
                raw.push_str(&t);
                for piece in splitter.push(&t) {
                    if !emit_piece(deps, out, reply, piece).await {
                        reply.add_usage(usage);
                        return Err(Ended::Interrupted);
                    }
                }
            }
            ChatChunk::Reasoning(block) => {
                flush_raw(&mut result.parts, &mut raw);
                result.parts.push(ContentPart::Reasoning(block));
            }
            ChatChunk::ToolCall {
                call_id,
                name,
                arguments,
            } => {
                flush_raw(&mut result.parts, &mut raw);
                result.parts.push(ContentPart::ToolCall {
                    call_id: call_id.clone(),
                    name: name.clone(),
                    arguments: arguments.clone(),
                });
                result.calls.push((call_id, name, arguments));
            }
            // Usage is cumulative within a call: keep the latest.
            ChatChunk::Usage(u) => usage = u,
            ChatChunk::Done(s) => result.stop = Some(s),
        }
    }
    drop(stream);
    flush_raw(&mut result.parts, &mut raw);
    reply.add_usage(usage);
    // Text before a tool call may not end with a full stop: check and send it now.
    if let Some(rest) = splitter.finish()
        && !emit_piece(deps, out, reply, rest).await
    {
        return Err(Ended::Interrupted);
    }
    Ok(result)
}

fn flush_raw(parts: &mut Vec<ContentPart>, raw: &mut String) {
    if !raw.is_empty() {
        parts.push(ContentPart::Text(std::mem::take(raw)));
    }
}

/// The off-topic route: the cheapest model, no tools, a short redirect.
async fn off_topic(deps: &AgentDeps, turn: &Turn, out: &mut Emitter, reply: &mut Reply) -> Ended {
    reply.purpose = Some(ChatPurpose::OffTopic);
    reply.prompt_version = OFF_TOPIC_PROMPT_VERSION;
    let request = ChatRequest {
        purpose: ChatPurpose::OffTopic,
        system: off_topic_system(),
        messages: vec![ChatMessage {
            role: ChatRole::User,
            content: vec![ContentPart::Text(data(DataKind::UserMessage, &turn.text))],
        }],
        tools: Vec::new(),
        response_schema: None,
        max_tokens: deps.config.cheap_max_tokens,
    };
    let mut splitter = SentenceSplitter::new();
    match stream_call(deps, turn, &request, out, reply, &mut splitter).await {
        Ok(_) => Ended::Completed,
        Err(e) => e,
    }
}

/// A normal turn: context, then up to `max_model_calls` model calls with tools in between.
async fn model_turn(
    inner: &Inner,
    turn: &Turn,
    route: ChatRoute,
    out: &mut Emitter,
    reply: &mut Reply,
) -> Ended {
    let deps = &inner.deps;
    reply.purpose = Some(ChatPurpose::Chat);
    reply.prompt_version = CHAT_PROMPT_VERSION;

    let query = deps
        .embedder
        .embed(
            domain::ports::EmbedMode::Query,
            std::slice::from_ref(&turn.text),
        )
        .await
        .ok()
        .and_then(|mut v| v.pop());
    let ctx = match context::load(
        deps,
        turn.owner,
        turn.anchor,
        turn.user_message,
        query.as_ref(),
    )
    .await
    {
        Ok(c) => c,
        Err(e) => return Ended::Store(e),
    };

    let mut images = Vec::with_capacity(turn.image_keys.len());
    for key in &turn.image_keys {
        match deps.blob.get_bytes(key).await {
            Ok(data) => images.push(ContentPart::Image {
                media_type: "image/webp",
                data,
            }),
            Err(e) => tracing::warn!(error = %e, "chat image not loaded"),
        }
    }
    let mut last = ctx.turn_message(&turn.text, images);
    if route == ChatRoute::SafetySensitive {
        // Server-written, outside the data blocks, before the user's words.
        last.content
            .insert(0, ContentPart::Text(SAFETY_POLICY.to_owned()));
    }
    let mut messages = ctx.history.clone();
    messages.push(last);

    // Offered by role and by whether the garden has farm records, read once before routing.
    let offer = Offer {
        role: turn.owner.membership().role(),
        farm_records: turn.farm_records,
    };
    let mut request = ChatRequest {
        purpose: ChatPurpose::Chat,
        system: chat_system(),
        messages,
        tools: inner.tools.specs_for(offer),
        response_schema: None,
        max_tokens: deps.config.chat_max_tokens,
    };
    let tool_ctx = tool_ctx(deps, turn, ctx.plant);
    let mut splitter = SentenceSplitter::new();
    let max_calls = deps.config.max_model_calls.max(1);

    for call_no in 1..=max_calls {
        if reply.spent() >= turn.budget_left {
            return emit_checked(deps, out, reply, BUDGET_TEXT.to_owned()).await;
        }
        let result = match stream_call(deps, turn, &request, out, reply, &mut splitter).await {
            Ok(r) => r,
            Err(e) => return e,
        };
        match result.stop {
            Some(StopReason::Refusal) => {
                return emit_checked(deps, out, reply, format!(" {REFUSAL_TEXT}")).await;
            }
            Some(StopReason::ToolUse) if !result.calls.is_empty() => {}
            _ => return Ended::Completed,
        }
        if call_no == max_calls {
            return emit_checked(deps, out, reply, format!(" {LOOP_CAP_TEXT}")).await;
        }

        // The assistant turn goes back unchanged (thinking blocks included), then the results.
        request.messages.push(ChatMessage {
            role: ChatRole::Assistant,
            content: result.parts,
        });
        let mut results = Vec::with_capacity(result.calls.len());
        let mut extra = Vec::new();
        for (call_id, name, arguments) in result.calls {
            let Some((part, mut images)) = run_tool(
                inner, &tool_ctx, offer, out, reply, call_id, name, arguments,
            )
            .await
            else {
                return Ended::Interrupted;
            };
            results.push(part);
            extra.append(&mut images);
        }
        results.append(&mut extra);
        request.messages.push(ChatMessage {
            role: ChatRole::Tool,
            content: results,
        });
    }
    Ended::Completed
}

/// Runs one tool call and sends its events. `None` when the client went away.
#[expect(
    clippy::too_many_arguments,
    reason = "one call's inputs, all borrowed from the turn"
)]
async fn run_tool(
    inner: &Inner,
    ctx: &ToolCtx<'_>,
    offer: Offer,
    out: &mut Emitter,
    reply: &mut Reply,
    call_id: String,
    name: String,
    arguments: serde_json::Value,
) -> Option<(ContentPart, Vec<ContentPart>)> {
    if name == tool_name(ToolName::ShowBlock) {
        return show_block(ctx, out, reply, call_id, arguments).await;
    }
    let tool = inner.tools.find(&name, offer);
    let label = tool.map_or("Working", |t| t.label());
    let started = ToolStarted {
        v: V1,
        call_id: call_id.clone(),
        tool: name.clone(),
        label: label.to_owned(),
    };
    if !out.send(ChatEvent::ToolStarted(started)).await {
        return None;
    }
    reply.after_tool_round = true;
    reply.tool_calls.push(ContentPart::ToolCall {
        call_id: call_id.clone(),
        name: name.clone(),
        arguments: arguments.clone(),
    });
    let result = match tool {
        Some(t) => t.call(ctx, arguments).await,
        None => Err(crate::error::ToolError::Rejected(format!(
            "There is no tool named {name}."
        ))),
    };
    let (content, is_error, images) = match result {
        Ok(output) => {
            for effect in output.effects {
                if !send_effect(out, reply, effect).await {
                    return None;
                }
            }
            (output.content, false, output.images)
        }
        Err(e) => {
            tracing::info!(tool = %name, error = %e, "tool call refused or failed");
            (
                serde_json::json!({ "error": e.for_model() }),
                true,
                Vec::new(),
            )
        }
    };
    // The model reads a successful result inside a `data` block (gap G1); the stored copy keeps
    // the tool's JSON as the app's records have it. A refusal is the server's own text.
    let framed = if is_error {
        content.clone()
    } else {
        let kind = tool.map_or(DataKind::ToolResult, crate::tools::DynTool::data_kind);
        tool_result(kind, &content)
    };
    reply.tool_calls.push(ContentPart::ToolResult {
        call_id: call_id.clone(),
        content,
        is_error,
    });
    let part = ContentPart::ToolResult {
        call_id: call_id.clone(),
        content: framed,
        is_error,
    };
    let finished = ToolFinished {
        v: V1,
        call_id,
        ok: !is_error,
    };
    if !out.send(ChatEvent::ToolFinished(finished)).await {
        return None;
    }
    Some((part, images))
}

/// `show_block` ([`blocks`]): built from the turn's own results and the blocks already sent,
/// which `reply` holds. It sends no `tool_started` or `tool_finished`, so the app shows no chip
/// for it; a built block goes out as `answer_block` and into `Message.blocks`, and a refused one
/// sends nothing and gives the model the reason. Its one string, `call_id`, must equal a call id
/// the provider wrote, so no text of the model's reaches the app and there is nothing to
/// dose-check. `None` when the client went away.
async fn show_block(
    ctx: &ToolCtx<'_>,
    out: &mut Emitter,
    reply: &mut Reply,
    call_id: String,
    arguments: serde_json::Value,
) -> Option<(ContentPart, Vec<ContentPart>)> {
    reply.after_tool_round = true;
    let built = match serde_json::from_value::<ShowBlockArgs>(arguments.clone()) {
        Ok(args) => blocks::build(ctx, &reply.tool_calls, &reply.blocks, args).await,
        Err(e) => Err(crate::error::ToolError::Rejected(format!(
            "The arguments are invalid: {e}."
        ))),
    };
    reply.tool_calls.push(ContentPart::ToolCall {
        call_id: call_id.clone(),
        name: tool_name(ToolName::ShowBlock).to_owned(),
        arguments,
    });
    let (content, is_error) = match built {
        Ok(block) => {
            let content = serde_json::json!({ "ok": true, "kind": block.kind().as_str() });
            // Stored only once sent, so `Message.blocks` is exactly what the app received.
            let event = ChatEvent::AnswerBlock(AnswerBlockEvent {
                v: V1,
                block: block.clone(),
            });
            if !out.send(event).await {
                return None;
            }
            reply.blocks.push(block);
            (content, false)
        }
        Err(e) => {
            tracing::info!(error = %e, "show_block refused");
            (serde_json::json!({ "error": e.for_model() }), true)
        }
    };
    reply.tool_calls.push(ContentPart::ToolResult {
        call_id: call_id.clone(),
        content: content.clone(),
        is_error,
    });
    // The server wrote both answers, so neither goes in a `data` block.
    Some((
        ContentPart::ToolResult {
            call_id,
            content,
            is_error,
        },
        Vec::new(),
    ))
}

async fn send_effect(out: &mut Emitter, reply: &mut Reply, effect: ToolEffect) -> bool {
    let event = match effect {
        ToolEffect::Citation(citation) => {
            if reply
                .citations
                .iter()
                .any(|c| c.source_id == citation.source_id)
            {
                return true;
            }
            reply.citations.push(citation.clone());
            ChatEvent::Citation(CitationEvent { v: V1, citation })
        }
        ToolEffect::FarmCitation(citation) => {
            // Stored with the message as `farm_citations`; contracts 8 has no event for it.
            if !reply
                .farm_citations
                .iter()
                .any(|c| c.record_id == citation.record_id)
            {
                reply.farm_citations.push(citation);
            }
            return true;
        }
        ToolEffect::PendingAction(a) => ChatEvent::PendingAction(PendingActionEvent {
            v: V1,
            pending_action: *a,
        }),
        ToolEffect::MemorySaved(memory) => ChatEvent::MemorySaved(MemorySaved { v: V1, memory }),
        ToolEffect::RescanRequested(r) => ChatEvent::RescanRequested(RescanRequested {
            v: V1,
            plant_id: r.plant_id,
            photo_kind: r.photo_kind,
            instructions: r.instructions,
        }),
    };
    out.send(event).await
}

#[cfg(test)]
mod tests;
