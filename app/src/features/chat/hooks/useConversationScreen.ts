// The conversation screen's data and actions (screens 3.6; flows 9, 10, 18): the stored messages, the turns
// streaming now, the composer's text and photos, and sending with the retry and resend rules of README 10.
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { chat, flattenPages, isApiError, newKey, qk, sendChatMessage, useConversation, useGarden, type ChatStreamEvent, type IdempotencyKey } from "@/api";
import type { ChatMessage } from "@/domain/proposed";
import { can } from "@/domain/permissions";
import type { Attachment, Conversation, Memory } from "@/domain/types";
import { uuid } from "@/lib/uuid";
import { pickFromLibrary } from "@/platform";

import { startTurn, turnReducer, type RescanRequest, type Turn, type TurnAction, type TurnError, type TurnEvent } from "../model/messageReducer";

export const MAX_TEXT = 2000;
export const MAX_PHOTOS = 3;

/** A network error before any response is retried with the same key this many times (flows 9). */
const NETWORK_RETRIES = 2;

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

  const refresh = useCallback(() => {
    void qc.invalidateQueries({ queryKey: qk.conversation(conversationId) });
    if (gardenId) void qc.invalidateQueries({ queryKey: qk.conversations(gardenId) });
  }, [qc, conversationId, gardenId]);

  const run = useCallback(
    async (localId: string, body: { text: string; image_ids?: string[] }) => {
      const key = newKey();
      const abort = new AbortController();
      aborts.current.add(abort);
      let sawMemory = false;
      try {
        let result: Awaited<ReturnType<typeof sendChatMessage>> | undefined;
        for (let attempt = 0; ; attempt++) {
          try {
            result = await sendChatMessage(conversationId, body, key, { signal: abort.signal });
            break;
          } catch (e) {
            if (abort.signal.aborted) return;
            // A network error before any response: the same key again, which may return the replay (flows 9).
            if (!isApiError(e) && attempt < NETWORK_RETRIES) continue;
            dispatch(localId, { type: "failed", error: turnError(e) });
            return;
          }
        }
        if (result.kind === "replay") {
          dispatch(localId, { type: "replayed", message: result.message });
          return;
        }
        let ended = false;
        try {
          for await (const event of result.events) {
            dispatch(localId, { type: "event", event: toTurnEvent(event) });
            if (event.type === "memory_saved") sawMemory = true;
            if (event.type === "message_completed" || event.type === "error") ended = true;
          }
        } catch {
          // The stream broke after it started.
        }
        if (!ended && !abort.signal.aborted) dispatch(localId, { type: "dropped" });
      } finally {
        aborts.current.delete(abort);
        if (!abort.signal.aborted) {
          refresh();
          if (sawMemory && gardenId) void qc.invalidateQueries({ queryKey: qk.memories(gardenId) });
        }
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
      setTurns((all) => [...all, startTurn(localId, { text: body, attachments })]);
      if (!override) {
        setText("");
        setPhotos([]);
      }
      void run(localId, { text: body, ...(attachments.length ? { image_ids: attachments.map((a) => a.image_id) } : {}) });
    },
    [text, busy, ready, run],
  );

  /** Resend: a new action with a new key (contracts 8). The old turn or interrupted message stays as it was. */
  const resend = useCallback(
    (request: { text: string; attachments: Attachment[] }, failedLocalId?: string) => {
      if (busy) return;
      if (failedLocalId) setTurns((all) => all.filter((t) => t.localId !== failedLocalId));
      const localId = uuid();
      setTurns((all) => [...all, startTurn(localId, request)]);
      void run(localId, { text: request.text, ...(request.attachments.length ? { image_ids: request.attachments.map((a) => a.image_id) } : {}) });
    },
    [busy, run],
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
  // it, lending it the memories and rescans `Message` doesn't keep. An interrupted assistant message can be resent
  // from the user message before it.
  const items: ChatItem[] = useMemo(() => {
    const byMessage = new Map(turns.flatMap((t) => (t.messageId ? [[t.messageId, t] as const] : [])));
    const out: ChatItem[] = stored.map((m, i) => {
      const t = byMessage.get(m.id);
      return {
        kind: "message",
        key: m.id,
        message: m,
        extras: t && (t.memories.length || t.rescans.length) ? { memories: t.memories, rescans: t.rescans } : undefined,
        resendable: m.role === "assistant" && m.interrupted && i > 0 && stored[i - 1]!.role === "user",
      };
    });
    const ids = new Set(stored.map((m) => m.id));
    for (const t of turns) if (!(t.messageId && ids.has(t.messageId))) out.push({ kind: "turn", key: t.localId, turn: t });
    return out;
  }, [stored, turns]);

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
    error: query.isError ? (isApiError(query.error) ? query.error.message : "This conversation couldn't be loaded.") : undefined,
    conversation,
    gardenId,
    items,
    hasOlder: query.hasNextPage,
    loadOlder: () => void query.fetchNextPage(),
    loadingOlder: query.isFetchingNextPage,
    canWrite: can(role, "chat.write_tools"),
    canShare: can(role, "memory.share"),
    canScan: can(role, "scan.create"),
    composer: { text, setText, photos, canSend, busy, uploading },
    actions: { send, resend, requestBefore, addPhoto, retryPhoto, removePhoto, retry: () => void query.refetch() },
  };
}
