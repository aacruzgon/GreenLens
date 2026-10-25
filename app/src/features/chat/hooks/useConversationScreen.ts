// The conversation screen's data and actions (screens 3.6; flows 9, 10, 18): the stored messages, the turns
// streaming now, the composer's text and photos, and sending with the retry and resend rules of README 10.
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AccessibilityInfo } from "react-native";

import { chat, flattenPages, isApiError, newKey, qk, sendChatMessage, useConversation, useGarden, type ChatStreamEvent, type IdempotencyKey } from "@/api";
import type { ChatMessage } from "@/domain/proposed";
import { can } from "@/domain/permissions";
import type { Attachment, Conversation, Memory } from "@/domain/types";
import { backoffMs } from "@/lib/backoff";
import { uuid } from "@/lib/uuid";
import { pickFromLibrary } from "@/platform";

import { startTurn, turnReducer, type RescanRequest, type Turn, type TurnAction, type TurnError, type TurnEvent } from "../model/messageReducer";

export const MAX_TEXT = 2000;
export const MAX_PHOTOS = 3;

/** How many stored questions have this text. */
function sameQuestions(all: readonly { role: string; text?: string | null }[], text: string): number {
  return all.filter((m) => m.role === "user" && m.text === text).length;
}

/** A network error before any response is retried with the same key this many times (flows 9). */
const NETWORK_RETRIES = 2;
/**
 * `409 request_in_progress` (the first attempt is still running) is waited out this many times, same key: about
 * 16 s at the API's 2 s, as the server only notices a closed request on its next send, which an LLM or tool call
 * can hold back. Past that the turn fails, and Try again keeps the key.
 */
const IN_PROGRESS_RETRIES = 8;

/**
 * No bytes for this long, keepalives included, and the stream is taken as dropped (README 10, the scan stream's
 * rule: the API sends keepalives on chat streams too). Mutable for tests.
 */
export const chatTiming = { idleMs: 45_000 };

const wait = (ms: number, signal: AbortSignal) =>
  new Promise<void>((resolve) => {
    const t = setTimeout(resolve, ms);
    signal.addEventListener("abort", () => {
      clearTimeout(t);
      resolve();
    });
  });

export interface ComposerPhoto {
  localId: string;
  uri: string;
  status: "uploading" | "ready" | "error";
  imageId?: string;
  url?: string;
  error?: string;
  /** The upload's key and file, kept for Retry: same key, same bytes (README 9). */
  key: IdempotencyKey;
  file: { uri: string; name: string; type: string };
}

/** What a stored message keeps from its turn for this visit: the stream's extras that `Message` doesn't store. */
export interface TurnExtras {
  memories: Memory[];
  rescans: RescanRequest[];
  /** The `error` event that ended the turn: its text stays, where the stored message only says `interrupted`. */
  error?: TurnError;
}

export type ChatItem =
  | { kind: "message"; key: string; message: ChatMessage; extras?: TurnExtras; resendable: boolean }
  | { kind: "turn"; key: string; turn: Turn };

function turnError(e: unknown): TurnError {
  if (isApiError(e)) {
    const resetsAt = typeof e.details?.resets_at === "string" ? e.details.resets_at : undefined;
    return { code: e.code, message: e.message, retryable: e.retryable, ...(resetsAt ? { resetsAt } : {}) };
  }
  return { code: "network", message: "No connection. Check your network and try again.", retryable: true };
}

/** VoiceOver reads a finished answer and an error out; `accessibilityLiveRegion` is Android's only. */
function announce(text: string) {
  if (text) AccessibilityInfo.announceForAccessibility(text);
}

const errorWords = (e: TurnError) => (e.code === "budget_exceeded" ? "Today's chat limit is used up." : e.message);

/** The api's decoded event is the reducer's event with more fields (`v`, `tool`). */
const toTurnEvent = (e: ChatStreamEvent): TurnEvent => e;

export function useConversationScreen(conversationId: string) {
  const qc = useQueryClient();
  const query = useConversation(conversationId);
  const first = query.data?.pages[0];
  const conversation: Conversation | undefined = first
    ? { id: first.id, garden_id: first.garden_id, anchor: first.anchor, title: first.title, created_at: first.created_at, last_message_at: first.last_message_at }
    : undefined;
  const gardenId = conversation?.garden_id ?? "";
  const garden = useGarden(gardenId);
  const role = garden.data?.my_role;

  // Pages are newest first; the screen reads oldest first.
  const stored = useMemo(
    () => flattenPages(query.data ? { pages: query.data.pages.map((p) => p.messages) } : undefined).reverse() as ChatMessage[],
    [query.data],
  );

  const [turns, setTurns] = useState<Turn[]>([]);
  const [text, setText] = useState("");
  const [photos, setPhotos] = useState<ComposerPhoto[]>([]);
  const aborts = useRef(new Set<AbortController>());
  /**
   * A failed turn's key, when the question may have reached the server (a network error, or `request_in_progress`
   * given up on). Try again sends it again with that key, which waits for or replays the first attempt: a new key
   * could start a second turn while the first is still running (flows 9).
   */
  const keptKeys = useRef(new Map<string, IdempotencyKey>());
  /**
   * How many stored questions had a turn's text when it was sent. A failed turn gives way to the stored copy of
   * its question only when there is one more than this: an earlier question with the same text isn't it (review of
   * #205: asking the same question twice lost Try again).
   */
  const [askedBefore, setAskedBefore] = useState<Readonly<Record<string, number>>>({});
  /** Failed turns whose question may have reached the server: their key was kept. */
  const [maybeSent, setMaybeSent] = useState<ReadonlySet<string>>(() => new Set());

  // Leaving the screen closes its streams: the server stops the turn and stores it as interrupted (contracts 8).
  useEffect(() => {
    const open = aborts.current;
    return () => {
      for (const a of open) a.abort();
      open.clear();
    };
  }, []);

  const dispatch = useCallback((localId: string, action: TurnAction) => {
    setTurns((all) => all.map((t) => (t.localId === localId ? turnReducer(t, action) : t)));
  }, []);

  /**
   * After a turn the stored conversation is fetched again. A turn closed by leaving the screen still changed it
   * (the server stores the question and an interrupted reply), so its cache is only marked stale, for the next
   * visit to fetch.
   */
  const refresh = useCallback(
    (left = false) => {
      const refetchType = left ? "none" : "active";
      void qc.invalidateQueries({ queryKey: qk.conversation(conversationId), refetchType });
      if (gardenId) void qc.invalidateQueries({ queryKey: qk.conversations(gardenId), refetchType });
    },
    [qc, conversationId, gardenId],
  );

  const run = useCallback(
    async (localId: string, body: { text: string; image_ids?: string[] }, kept?: IdempotencyKey) => {
      const key = kept ?? newKey();
      // `abort` is leaving the screen; `conn` is this request, which a silent stream also closes.
      const abort = new AbortController();
      const conn = new AbortController();
      abort.signal.addEventListener("abort", () => conn.abort());
      aborts.current.add(abort);
      let sawMemory = false;
      let idle: ReturnType<typeof setTimeout> | undefined;
      const onActivity = () => {
        clearTimeout(idle);
        idle = setTimeout(() => conn.abort(), chatTiming.idleMs);
      };
      try {
        let result: Awaited<ReturnType<typeof sendChatMessage>> | undefined;
        let network = 0;
        let inProgress = 0;
        for (;;) {
          try {
            result = await sendChatMessage(conversationId, body, key, { signal: conn.signal, onActivity });
            break;
          } catch (e) {
            if (abort.signal.aborted) return;
            // A network error before any response: the same key again after a short wait, which may return the
            // replay (flows 9). If the first attempt did reach the server, the retry hears `409
            // request_in_progress` and waits `retry_after_s` before asking again with that key: a new key would
            // ask the question twice.
            if (!isApiError(e) && network < NETWORK_RETRIES) {
              await wait(backoffMs(network++, { baseMs: 500 }), abort.signal);
              continue;
            }
            if (isApiError(e) && e.code === "request_in_progress" && inProgress < IN_PROGRESS_RETRIES) {
              inProgress++;
              await wait((e.retryAfterS ?? 2) * 1_000, abort.signal);
              continue;
            }
            if (!isApiError(e) || e.code === "request_in_progress") {
              keptKeys.current.set(localId, key);
              setMaybeSent((all) => new Set(all).add(localId));
            }
            dispatch(localId, { type: "failed", error: turnError(e) });
            announce(errorWords(turnError(e)));
            return;
          }
        }
        if (result.kind === "replay") {
          dispatch(localId, { type: "replayed", message: result.message });
          announce(result.message.text);
          return;
        }
        let ended = false;
        let said = "";
        onActivity();
        try {
          for await (const event of result.events) {
            dispatch(localId, { type: "event", event: toTurnEvent(event) });
            if (event.type === "memory_saved") sawMemory = true;
            if (event.type === "text_delta") said += event.text;
            if (event.type === "message_completed") {
              ended = true;
              announce(said);
            }
            if (event.type === "error") {
              ended = true;
              announce(event.message);
            }
          }
        } catch {
          // The stream broke after it started.
        }
        if (!ended && !abort.signal.aborted) {
          // A drop, or no bytes for `idleMs`: the request is closed, and Resend is offered.
          conn.abort();
          dispatch(localId, { type: "dropped" });
          announce("The answer was interrupted.");
        }
      } finally {
        clearTimeout(idle);
        aborts.current.delete(abort);
        refresh(abort.signal.aborted);
        if (!abort.signal.aborted && sawMemory && gardenId) void qc.invalidateQueries({ queryKey: qk.memories(gardenId) });
      }
    },
    [conversationId, dispatch, refresh, qc, gardenId],
  );

  const busy = turns.some((t) => t.status === "sending" || t.status === "streaming");
  const uploading = photos.some((p) => p.status === "uploading");
  const ready = photos.filter((p) => p.status === "ready");
  const trimmed = text.trim();
  const canSend = trimmed.length > 0 && trimmed.length <= MAX_TEXT && !busy && !uploading && conversation !== undefined;

  /** Sends `override` (a block's request, "Ask again") or the composer's text with its ready photos. */
  const send = useCallback(
    (override?: string) => {
      const body = (override ?? text).trim();
      if (!body || body.length > MAX_TEXT || busy) return;
      const attachments: Attachment[] = override ? [] : ready.map((p) => ({ image_id: p.imageId!, url: p.url! }));
      const localId = uuid();
      setAskedBefore((all) => ({ ...all, [localId]: sameQuestions(stored, body) }));
      // A failed turn's Try again only makes sense as the last thing in the conversation, so a new question
      // replaces it; a question that did reach the server comes back with the stored conversation.
      setTurns((all) => [...all.filter((t) => t.status !== "failed"), startTurn(localId, { text: body, attachments })]);
      if (!override) {
        setText("");
        setPhotos([]);
      }
      void run(localId, { text: body, ...(attachments.length ? { image_ids: attachments.map((a) => a.image_id) } : {}) });
    },
    [text, busy, ready, run, stored],
  );

  /**
   * Resend: a new action with a new key (contracts 8). The old turn or interrupted message stays as it was. Try
   * again on a turn that failed before any answer keeps that turn's key, when it has one (`keptKeys`).
   */
  const resend = useCallback(
    (request: { text: string; attachments: Attachment[] }, failedLocalId?: string) => {
      if (busy) return;
      const kept = failedLocalId ? keptKeys.current.get(failedLocalId) : undefined;
      if (failedLocalId) {
        keptKeys.current.delete(failedLocalId);
        setTurns((all) => all.filter((t) => t.localId !== failedLocalId));
      }
      const localId = uuid();
      setAskedBefore((all) => ({ ...all, [localId]: sameQuestions(stored, request.text) }));
      setTurns((all) => [...all.filter((t) => t.status !== "failed"), startTurn(localId, request)]);
      void run(localId, { text: request.text, ...(request.attachments.length ? { image_ids: request.attachments.map((a) => a.image_id) } : {}) }, kept);
    },
    [busy, run, stored],
  );

  const upload = useCallback(
    async (photo: ComposerPhoto) => {
      setPhotos((all) => all.map((p) => (p.localId === photo.localId ? { ...p, status: "uploading", error: undefined } : p)));
      try {
        const res = await chat.uploadImage(conversationId, photo.file, photo.key);
        setPhotos((all) => all.map((p) => (p.localId === photo.localId ? { ...p, status: "ready", imageId: res.image_id, url: res.url } : p)));
      } catch (e) {
        const message = isApiError(e) ? e.message : "The photo didn't upload. Check your network.";
        setPhotos((all) => all.map((p) => (p.localId === photo.localId ? { ...p, status: "error", error: message } : p)));
      }
    },
    [conversationId],
  );

  const addPhoto = useCallback(async () => {
    if (photos.length >= MAX_PHOTOS) return;
    const shot = await pickFromLibrary();
    if (!shot) return;
    const ext = shot.file.mimeType.split("/")[1] ?? "jpeg";
    const photo: ComposerPhoto = {
      localId: uuid(),
      uri: shot.file.uri,
      status: "uploading",
      key: newKey(),
      file: { uri: shot.file.uri, name: `photo.${ext === "jpeg" ? "jpg" : ext}`, type: shot.file.mimeType },
    };
    setPhotos((all) => [...all, photo].slice(0, MAX_PHOTOS));
    await upload(photo);
  }, [photos.length, upload]);

  const retryPhoto = useCallback((localId: string) => {
    const photo = photos.find((p) => p.localId === localId);
    if (photo) void upload(photo);
  }, [photos, upload]);

  const removePhoto = useCallback((localId: string) => setPhotos((all) => all.filter((p) => p.localId !== localId)), []);

  // The list: stored messages, then the turns not stored yet. A turn whose assistant message is stored gives way to
  // it, lending it what `Message` doesn't keep: memories, rescans and an `error` event's text. The server stores a
  // question before its stream starts, so a refetch during a turn (a card confirmed mid-answer, a drop) brings it
  // in while the turn still draws it: a stored question after the last stored answer that a live turn asks is
  // left to the turn. An interrupted assistant message can be resent from the user message before it, while it is
  // the last thing in the conversation: once a resend or another question follows it, a second Resend would only
  // ask twice (seen on the simulator). A failed turn whose question the server did store (its first attempt got
  // through, then `request_in_progress` was given up on) gives way to the stored question and its answer, and a
  // failed turn is never live: it doesn't hold back the stored messages' Resend.
  const items: ChatItem[] = useMemo(() => {
    const byMessage = new Map(turns.flatMap((t) => (t.messageId ? [[t.messageId, t] as const] : [])));
    const storedIds = new Set(stored.map((m) => m.id));
    const lastQuestion = stored.findLast((m) => m.role === "user");
    const answered = lastQuestion !== undefined && stored.at(-1)?.role === "assistant";
    // A failed turn the server may have got (its key was kept) gives way once a newer stored copy of its question
    // is answered; one that never reached the server, or a repeat of an older question, keeps its Try again.
    const superseded = (t: (typeof turns)[number]) =>
      t.status === "failed" &&
      answered &&
      lastQuestion.text === t.request.text &&
      maybeSent.has(t.localId) &&
      sameQuestions(stored, t.request.text) > (askedBefore[t.localId] ?? 0);
    const liveTurns = turns.filter((t) => !(t.messageId && storedIds.has(t.messageId)) && !superseded(t));
    const asked = liveTurns.map((t) => t.request.text);
    const hidden = new Set<string>();
    for (let i = stored.length - 1; i >= 0 && stored[i]!.role === "user"; i--) {
      const at = asked.indexOf(stored[i]!.text);
      if (at === -1) continue;
      asked.splice(at, 1);
      hidden.add(stored[i]!.id);
    }
    const shown = stored.filter((m) => !hidden.has(m.id));
    const live = liveTurns.some((t) => t.status !== "failed");
    const out: ChatItem[] = shown.map((m, i) => {
      const t = byMessage.get(m.id);
      const error = t?.status === "errored" ? t.error : undefined;
      return {
        kind: "message",
        key: m.id,
        message: m,
        extras: t && (t.memories.length || t.rescans.length || error) ? { memories: t.memories, rescans: t.rescans, ...(error ? { error } : {}) } : undefined,
        resendable:
          m.role === "assistant" && m.interrupted && i === shown.length - 1 && !live && shown[i - 1]?.role === "user" && (error ? error.retryable : true),
      };
    });
    for (const t of liveTurns) out.push({ kind: "turn", key: t.localId, turn: t });
    return out;
  }, [stored, turns, maybeSent, askedBefore]);

  /** The user message an interrupted assistant message answered, for Resend. */
  const requestBefore = useCallback(
    (messageId: string): { text: string; attachments: Attachment[] } | undefined => {
      const i = stored.findIndex((m) => m.id === messageId);
      const prev = i > 0 ? stored[i - 1] : undefined;
      return prev && prev.role === "user" ? { text: prev.text, attachments: prev.attachments } : undefined;
    },
    [stored],
  );

  return {
    loading: query.isPending,
    /** `404`: deleted with its plant or garden, or never the caller's (contracts 8); there is nothing to retry. */
    gone: query.isError && isApiError(query.error) && query.error.status === 404,
    error: query.isError ? (isApiError(query.error) ? query.error.message : "This conversation couldn't be loaded.") : undefined,
    conversation,
    gardenId,
    items,
    /** The questions asked so far, so a suggestion isn't offered twice. */
    asked: [...stored.filter((m) => m.role === "user").map((m) => m.text), ...turns.map((t) => t.request.text)],
    hasOlder: query.hasNextPage,
    loadOlder: () => void query.fetchNextPage(),
    loadingOlder: query.isFetchingNextPage,
    canWrite: can(role, "chat.write_tools"),
    canShare: can(role, "memory.share"),
    canScan: can(role, "scan.create"),
    canAddTasks: can(role, "task.write"),
    composer: { text, setText, photos, canSend, busy, uploading },
    actions: { send, resend, requestBefore, addPhoto, retryPhoto, removePhoto, retry: () => void query.refetch() },
  };
}
