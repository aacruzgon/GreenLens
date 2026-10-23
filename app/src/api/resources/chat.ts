import { useInfiniteQuery, useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";

import { client, ok } from "../client";
import type {
  ChatImageUploaded,
  ConfirmResponse,
  ConnectorList,
  Conversation,
  ConversationDetail,
  CreateConversationRequest,
  MessageFeedbackRequest,
  Page,
  PendingAction,
} from "@/domain/types";
import { useIdempotentMutation, type IdempotencyKey } from "../idempotency";
import { nextCursor } from "../pages";
import { STALE } from "../queryClient";
import { qk, type ConversationAnchor } from "../queryKeys";
import type { UploadFile } from "./scans";

// Conversations, chat photos, message feedback and the confirmation cards (contracts 8; README 10, 11).
// Sending a message is a stream, in chatStream.ts.

const idem = (key: IdempotencyKey) => ({ "Idempotency-Key": key });

export const chat = {
  list: async (gardenId: string, anchor?: ConversationAnchor, cursor?: string): Promise<Page<Conversation>> =>
    ok(
      await client().GET("/conversations", {
        params: { query: { garden_id: gardenId, anchor_type: anchor?.type, anchor_id: anchor?.id, cursor } },
      }),
    ),
  create: async (body: CreateConversationRequest, key: IdempotencyKey): Promise<Conversation> =>
    ok(await client().POST("/conversations", { body, params: { header: idem(key) } })),
  /** With its messages, newest first; `cursor` pages to older ones. */
  get: async (id: string, cursor?: string): Promise<ConversationDetail> =>
    ok(await client().GET("/conversations/{id}", { params: { path: { id }, query: { cursor } } })),
  /** The scan upload's checks without the quality gate (contracts 8). Keep the same file for a retry. */
  uploadImage: async (id: string, file: UploadFile, key: IdempotencyKey): Promise<ChatImageUploaded> => {
    const form = new FormData();
    form.append("file", file as Blob);
    return ok(
      await client().POST("/conversations/{id}/images", {
        params: { path: { id }, header: idem(key) },
        body: form,
        // FormData goes out as is; the platform sets the multipart boundary.
        bodySerializer: (b: unknown) => b as FormData,
      }),
    );
  },
  feedback: async (messageId: string, body: MessageFeedbackRequest): Promise<void> => {
    await client().POST("/messages/{id}/feedback", { params: { path: { id: messageId } }, body });
  },
  confirm: async (id: string): Promise<ConfirmResponse> =>
    ok(await client().POST("/pending-actions/{id}/confirm", { params: { path: { id } } })),
  reject: async (id: string): Promise<{ pending_action: PendingAction }> =>
    ok(await client().POST("/pending-actions/{id}/reject", { params: { path: { id } } })),
  connectors: async (gardenId: string): Promise<ConnectorList> =>
    ok(await client().GET("/gardens/{id}/connectors", { params: { path: { id: gardenId } } })),
};

/** The caller's own conversations in a garden, newest `last_message_at` first; with `anchor`, one anchor's. */
export function useConversations(gardenId: string, anchor?: ConversationAnchor) {
  return useInfiniteQuery({
    queryKey: qk.conversations(gardenId, anchor),
    queryFn: ({ pageParam }) => chat.list(gardenId, anchor, pageParam),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: nextCursor,
    enabled: gardenId !== "",
  });
}

/** A conversation and its messages, a page at a time from the newest. Attachments are signed, so 10 minutes. */
export function useConversation(id: string) {
  return useInfiniteQuery({
    queryKey: qk.conversation(id),
    queryFn: ({ pageParam }) => chat.get(id, pageParam),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (detail) => nextCursor(detail.messages),
    staleTime: STALE.signedUrls,
    enabled: id !== "",
  });
}

export function useCreateConversation() {
  const qc = useQueryClient();
  return useIdempotentMutation({
    mutationFn: (body: CreateConversationRequest, key) => chat.create(body, key),
    onSuccess: (c) => qc.invalidateQueries({ queryKey: qk.conversations(c.garden_id) }),
  });
}

export function useUploadChatImage(conversationId: string) {
  return useIdempotentMutation({
    mutationFn: (file: UploadFile, key) => chat.uploadImage(conversationId, file, key),
  });
}

export function useMessageFeedback() {
  return useMutation({
    mutationFn: ({ messageId, body }: { messageId: string; body: MessageFeedbackRequest }) => chat.feedback(messageId, body),
  });
}

/** What a confirmed card changed: the tasks for a task card, the scan and its plan for a plan card (flows 10). */
export function invalidateConfirmed(qc: QueryClient, gardenId: string, res: ConfirmResponse): void {
  // The stored message holds the card, so its new status must be read again (a remount showed the old one).
  void qc.invalidateQueries({ queryKey: qk.conversation(res.pending_action.conversation_id) });
  void qc.invalidateQueries({ queryKey: qk.tasks(gardenId) });
  const plan = res.result.plan;
  if (plan) {
    void qc.invalidateQueries({ queryKey: qk.scan(plan.scan_id) });
    void qc.invalidateQueries({ queryKey: qk.plant(plan.plant_id) });
    void qc.invalidateQueries({ queryKey: qk.timeline(plan.plant_id) });
  }
}

/** Confirm is never retried by the client: a plan revision must not be applied twice (contracts 8). */
export function useConfirmPendingAction(gardenId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => chat.confirm(id),
    retry: false,
    onSuccess: (res) => invalidateConfirmed(qc, gardenId, res),
  });
}

export function useRejectPendingAction() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => chat.reject(id),
    onSuccess: (res) => qc.invalidateQueries({ queryKey: qk.conversation(res.pending_action.conversation_id) }),
  });
}

/** The garden's farm connectors: chat reads them to tell an `era` record from a vendor's (EON E10.3). */
export function useConnectors(gardenId: string) {
  return useQuery({
    queryKey: qk.connectors(gardenId),
    queryFn: () => chat.connectors(gardenId),
    enabled: gardenId !== "",
  });
}
