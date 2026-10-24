// One chat turn as the conversation screen shows it while it streams (README 10, chat messages; flows 9 and 18).
// Pure: the screen feeds it the stream's events and what happened to the request, and renders the state. Once
// the stored conversation holds the turn's assistant message, the screen shows that instead (README 12: a
// streaming message is not cached), keeping only what the stored message lacks: remembered facts and rescans.
import type { AnswerBlock, ChatMessage } from "@/domain/proposed";
import type { Attachment, ChatRoute, Citation, ErrorCode, Id, ImageKind, Memory, PendingAction } from "@/domain/types";

/**
 * composing → sending → streaming → complete; `errored` for an `error` event, `interrupted` when the stream
 * dropped, `failed` for an HTTP error before the stream started (flows 18).
 */
export type TurnStatus = "sending" | "streaming" | "complete" | "errored" | "interrupted" | "failed";

export interface ToolChip {
  callId: string;
  label: string;
  state: "running" | "done" | "failed";
}

export interface RescanRequest {
  plantId: Id;
  photoKind: ImageKind;
  instructions: string;
}

export interface TurnError {
  code: ErrorCode | "network";
  message: string;
  retryable: boolean;
  /** `budget_exceeded`'s reset time, when the server gave one. */
  resetsAt?: string;
}

export interface TurnRequest {
  text: string;
  attachments: Attachment[];
}

export interface Turn {
  /** The client's id for the pair of bubbles until the stored messages replace them. */
  localId: string;
  request: TurnRequest;
  status: TurnStatus;
  /** From `message_started`: the assistant message's id, which the stored conversation will hold. */
  messageId?: Id;
  route?: ChatRoute;
  text: string;
  tools: ToolChip[];
  citations: Citation[];
  pendingActions: PendingAction[];
  memories: Memory[];
  rescans: RescanRequest[];
  blocks: AnswerBlock[];
  error?: TurnError;
}

/** The decoded stream events the reducer reads (api's ChatStreamEvent, narrowed to what it needs). */
export type TurnEvent =
  | { type: "message_started"; message_id: Id; route: ChatRoute }
  | { type: "text_delta"; text: string }
  | { type: "tool_started"; call_id: string; label: string }
  | { type: "tool_finished"; call_id: string; ok: boolean }
  | { type: "citation"; citation: Citation }
  | { type: "pending_action"; pending_action: PendingAction }
  | { type: "memory_saved"; memory: Memory }
  | { type: "rescan_requested"; plant_id: Id; photo_kind: ImageKind; instructions: string }
  | { type: "answer_block"; block: AnswerBlock }
  | { type: "message_completed"; message_id: Id }
  | { type: "error"; code: ErrorCode; message: string; retryable: boolean };

export type TurnAction =
  | { type: "event"; event: TurnEvent }
  /** The stream ended, or threw, without `message_completed` or `error`. */
  | { type: "dropped" }
  /** The request failed before a stream started: an HTTP error, or the network after its retries. */
  | { type: "failed"; error: TurnError }
  /** `200 application/json { message }`: the retried send had already finished (contracts 1). */
  | { type: "replayed"; message: ChatMessage };

export function startTurn(localId: string, request: TurnRequest): Turn {
  return { localId, request, status: "sending", text: "", tools: [], citations: [], pendingActions: [], memories: [], rescans: [], blocks: [] };
}

const ENDED: ReadonlySet<TurnStatus> = new Set(["complete", "errored", "interrupted", "failed"]);

export function isTurnOver(turn: Turn): boolean {
  return ENDED.has(turn.status);
}

function onEvent(t: Turn, e: TurnEvent): Turn {
  switch (e.type) {
    case "message_started":
      return { ...t, status: "streaming", messageId: e.message_id, route: e.route };
    case "text_delta":
      return { ...t, status: "streaming", text: t.text + e.text };
    case "tool_started":
      if (t.tools.some((c) => c.callId === e.call_id)) return t;
      return { ...t, status: "streaming", tools: [...t.tools, { callId: e.call_id, label: e.label, state: "running" }] };
    case "tool_finished":
      return { ...t, tools: t.tools.map((c) => (c.callId === e.call_id ? { ...c, state: e.ok ? "done" : "failed" } : c)) };
    case "citation":
      if (t.citations.some((c) => c.source_id === e.citation.source_id)) return t;
      return { ...t, citations: [...t.citations, e.citation] };
    case "pending_action":
      return { ...t, pendingActions: [...t.pendingActions.filter((p) => p.id !== e.pending_action.id), e.pending_action] };
    case "memory_saved":
      return { ...t, memories: [...t.memories.filter((m) => m.id !== e.memory.id), e.memory] };
    case "rescan_requested":
      return { ...t, rescans: [...t.rescans, { plantId: e.plant_id, photoKind: e.photo_kind, instructions: e.instructions }] };
    case "answer_block":
      return { ...t, blocks: [...t.blocks, e.block] };
    case "message_completed":
      return { ...t, status: "complete", messageId: e.message_id, tools: t.tools.map((c) => (c.state === "running" ? { ...c, state: "done" } : c)) };
    case "error":
      return { ...t, status: "errored", error: { code: e.code, message: e.message, retryable: e.retryable } };
  }
}

export function turnReducer(t: Turn, a: TurnAction): Turn {
  // Nothing moves a turn once it has ended: a late event from a stream the screen already gave up on is dropped.
  if (isTurnOver(t)) return t;
  switch (a.type) {
    case "event":
      return onEvent(t, a.event);
    case "dropped":
      return { ...t, status: "interrupted" };
    case "failed":
      return { ...t, status: "failed", error: a.error };
    case "replayed":
      return {
        ...t,
        status: "complete",
        messageId: a.message.id,
        text: a.message.text,
        citations: a.message.citations,
        pendingActions: a.message.pending_actions,
        blocks: a.message.blocks ?? [],
      };
  }
}

/** The tool chips to show: running ones while the answer streams; none once it is over (screens 3.6). */
export function visibleTools(t: Turn): ToolChip[] {
  return isTurnOver(t) ? [] : t.tools;
}
