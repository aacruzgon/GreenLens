import { useQuery } from "@tanstack/react-query";

import type { AnswerRequest, CreateScanRequest, ImageKind, UploadImageResponse } from "@/domain/types";

import { sendAuthed } from "../auth";
import { apiUrl, call, check, client, idempotent } from "../client";
import { networkError, toApiError } from "../errors";
import { qk } from "../queryKeys";
import { SIGNED_STALE_MS } from "../queryClient";
import { transport, type UploadFile } from "../transport";

export const scans = {
  get: (id: string) => call(client.GET("/scans/{id}", { params: { path: { id } } })),
  create: (body: CreateScanRequest, key: string) => call(client.POST("/scans", { body, params: { header: idempotent(key) } })),
  submit: (id: string) => call(client.POST("/scans/{id}/submit", { params: { path: { id } } })),
  discard: (id: string) => call(client.DELETE("/scans/{id}", { params: { path: { id } } })),
  deleteImage: (id: string, imageId: string) =>
    call(client.DELETE("/scans/{id}/images/{image_id}", { params: { path: { id, image_id: imageId } } })),
  answer: (id: string, body: AnswerRequest, key: string) =>
    call(client.POST("/scans/{id}/answers", { params: { path: { id }, header: idempotent(key) }, body })),

  /** multipart upload of one shot (contracts 3). The same key and the same file on every retry. */
  async uploadImage(
    id: string,
    shot: { kind: ImageKind; file: UploadFile; questionId?: string },
    key: string,
  ): Promise<UploadImageResponse> {
    const parts = [
      { name: "kind", value: shot.kind },
      ...(shot.questionId ? [{ name: "question_id", value: shot.questionId }] : []),
      { name: "file", file: shot.file },
    ];
    let response: Response;
    try {
      response = await sendAuthed((headers) => transport().upload(apiUrl(`/scans/${id}/images`), parts, headers), idempotent(key));
    } catch {
      throw networkError();
    }
    const body = await response.json().catch(() => null);
    return check(response, body as UploadImageResponse, body);
  },

  /** The scan's event stream (contracts 4). Without lastEventId the server replays the whole history. */
  async openEvents(id: string, lastEventId: number | undefined, signal: AbortSignal): Promise<Response> {
    const base = { Accept: "text/event-stream", ...(lastEventId ? { "Last-Event-ID": String(lastEventId) } : {}) };
    let response: Response;
    try {
      response = await sendAuthed((headers) => transport().fetch(apiUrl(`/scans/${id}/events`), { headers, signal }), base);
    } catch (e) {
      if (signal.aborted) throw e;
      throw networkError();
    }
    if (!response.ok) throw toApiError(response.status, await response.json().catch(() => null));
    return response;
  },
};

export function useScan(id: string | undefined) {
  return useQuery({ queryKey: qk.scan(id ?? ""), queryFn: () => scans.get(id!), enabled: !!id, staleTime: SIGNED_STALE_MS });
}
