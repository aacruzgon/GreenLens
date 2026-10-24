import type { AnswerBlockEvent, ChatMessage } from "@/domain/proposed";
import type {
  ChatErrorEvent,
  CitationEvent,
  MemorySavedEvent,
  MessageCompletedEvent,
  MessageStartedEvent,
  PendingActionEvent,
  RescanRequestedEvent,
  SendMessageRequest,
  TextDeltaEvent,
  ToolFinishedEvent,
  ToolStartedEvent,
} from "@/domain/types";

import { client } from "./client";
import type { IdempotencyKey } from "./idempotency";
import { readSse } from "./sse";

// `POST /conversations/{id}/messages` (contracts 8; README 10, chat messages). Unlike the scan stream, the event
// name is the SSE `event:` line and `data:` holds the variant's fields only, with no `id:` lines: the stream can't
// be resumed. A retried send whose first attempt finished is answered `200 application/json { message }`.

interface ChatEventData {
  message_started: MessageStartedEvent;
  text_delta: TextDeltaEvent;
  tool_started: ToolStartedEvent;
  tool_finished: ToolFinishedEvent;
  citation: CitationEvent;
  pending_action: PendingActionEvent;
  memory_saved: MemorySavedEvent;
  rescan_requested: RescanRequestedEvent;
  message_completed: MessageCompletedEvent;
  error: ChatErrorEvent;
  /** Proposed (domain/proposed.ts): only mock mode sends it until the backend has it. */
  answer_block: AnswerBlockEvent;
}

export type ChatEventType = keyof ChatEventData;

/** One chat event, tagged with its SSE event name. */
export type ChatStreamEvent = { [K in ChatEventType]: { type: K } & ChatEventData[K] }[ChatEventType];

const KNOWN: ReadonlySet<string> = new Set<ChatEventType>([
  "message_started", "text_delta", "tool_started", "tool_finished", "citation", "pending_action",
  "memory_saved", "rescan_requested", "message_completed", "error", "answer_block",
]);

/** One SSE message as a chat event, or undefined for one the app must ignore (an unknown name, `v` not 1). */
export function decodeChatEvent(event: string, data: string): ChatStreamEvent | undefined {
  if (!KNOWN.has(event)) return undefined;
  let parsed: unknown;
  try {
    parsed = JSON.parse(data);
  } catch {
    return undefined;
  }
  if (typeof parsed !== "object" || parsed === null || (parsed as { v?: unknown }).v !== 1) return undefined;
  return { ...(parsed as object), type: event } as ChatStreamEvent;
}

export type SendResult =
  /** The turn, live. The iteration ends when the server closes the stream, the stream drops or `signal` aborts. */
  | { kind: "stream"; events: AsyncGenerator<ChatStreamEvent> }
  /** The idempotent replay of a turn that already finished: take it as complete. */
  | { kind: "replay"; message: ChatMessage };

export interface SendOptions {
  signal?: AbortSignal;
  /** Every chunk of bytes, keepalives included, so the caller can tell a quiet stream from a dead one. */
  onActivity?: () => void;
}

/**
 * Sends one message. Errors before the stream starts (budget, validation, 404) throw an ApiError; a network error
 * before any response throws as is, and the caller retries with the same key. A drop after the stream started
 * surfaces as the iteration throwing or ending without `message_completed`.
 */
export async function sendChatMessage(
  conversationId: string,
  body: SendMessageRequest,
  key: IdempotencyKey,
  opts: SendOptions = {},
): Promise<SendResult> {
  const { data, response } = await client().POST("/conversations/{id}/messages", {
    params: { path: { id: conversationId }, header: { "Idempotency-Key": key } },
    body,
    headers: { Accept: "text/event-stream" },
    parseAs: "stream",
    signal: opts.signal,
  });
  const type = response.headers.get("content-type") ?? "";
  if (type.includes("application/json")) {
    const replay = (await response.json()) as { message: ChatMessage };
    return { kind: "replay", message: replay.message };
  }
  const stream = data as ReadableStream<Uint8Array> | null | undefined;
  async function* events(): AsyncGenerator<ChatStreamEvent> {
    if (!stream) return;
    for await (const msg of readSse(stream, { signal: opts.signal, onActivity: opts.onActivity })) {
      const event = decodeChatEvent(msg.event, msg.data);
      if (event) yield event;
    }
  }
  return { kind: "stream", events: events() };
}
