import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";

import { client, ok } from "../client";
import type { components } from "../generated/schema";
import { useIdempotentMutation, type IdempotencyKey } from "../idempotency";
import { STALE } from "../queryClient";
import { qk } from "../queryKeys";

type S = components["schemas"];
export type Scan = S["Scan"];
export type ScanDetail = S["ScanDetail"];
export type CreateScanRequest = S["CreateScanRequest"];
export type AnswerRequest = S["AnswerRequest"];
export type ImageKind = S["ImageKind"];
export type UploadImageResponse = S["UploadImageResponse"];

/** A file part: a Blob on the web, React Native's `{ uri, name, type }` on phones. */
export type UploadFile = Blob | { uri: string; name: string; type: string };

/** One `POST /scans/{id}/images` (contracts §3). Keep the same object for a retry: same bytes. */
export interface ShotUpload {
  kind: ImageKind;
  file: UploadFile;
  questionId?: string;
  /** LiDAR phones only: the 16-bit depth PNG, its optional confidence PNG and a `DepthMeta`. */
  depth?: UploadFile;
  depthConfidence?: UploadFile;
  depthMeta?: Record<string, unknown>;
}

/** The multipart field names of contracts §3: kind, file, question_id, depth, depth_confidence, depth_meta. */
export function shotFormData(part: ShotUpload): FormData {
  const form = new FormData();
  form.append("kind", part.kind);
  form.append("file", part.file as Blob);
  if (part.questionId) form.append("question_id", part.questionId);
  if (part.depth) form.append("depth", part.depth as Blob);
  if (part.depthConfidence) form.append("depth_confidence", part.depthConfidence as Blob);
  if (part.depthMeta) form.append("depth_meta", JSON.stringify(part.depthMeta));
  return form;
}

const idem = (key: IdempotencyKey) => ({ "Idempotency-Key": key });

export const scans = {
  get: async (id: string): Promise<ScanDetail> => ok(await client().GET("/scans/{id}", { params: { path: { id } } })),
  create: async (body: CreateScanRequest, key: IdempotencyKey): Promise<Scan> =>
    ok(await client().POST("/scans", { body, params: { header: idem(key) } })),
  uploadImage: async (id: string, part: ShotUpload, key: IdempotencyKey): Promise<UploadImageResponse> =>
    ok(
      await client().POST("/scans/{id}/images", {
        params: { path: { id }, header: idem(key) },
        body: shotFormData(part),
        // FormData goes out as is; the platform sets the multipart boundary.
        bodySerializer: (b: unknown) => b as FormData,
      }),
    ),
  /** No body and idempotent on the server (`200` when nothing changes), so no key. */
  submit: async (id: string): Promise<Scan> => ok(await client().POST("/scans/{id}/submit", { params: { path: { id } } })),
  answer: async (id: string, body: AnswerRequest, key: IdempotencyKey): Promise<Scan> =>
    ok(await client().POST("/scans/{id}/answers", { params: { path: { id }, header: idem(key) }, body })),
  /** Only a `pending` scan; `409 invalid_state` otherwise. */
  delete: async (id: string): Promise<void> => {
    await client().DELETE("/scans/{id}", { params: { path: { id } } });
  },
};

export function useScan(id: string) {
  return useQuery({ queryKey: qk.scan(id), queryFn: () => scans.get(id), staleTime: STALE.signedUrls });
}

/** Scan create, submit, delete and `failed` change `Plant.latest_scan` (README §9, invalidation). */
function invalidatePlant(qc: QueryClient, scan: { plant_id: string; garden_id: string }, timeline: boolean) {
  const keys = [qk.plant(scan.plant_id), qk.plants(scan.garden_id), ...(timeline ? [qk.timeline(scan.plant_id)] : [])];
  return Promise.all(keys.map((queryKey) => qc.invalidateQueries({ queryKey })));
}

export function useCreateScan() {
  const qc = useQueryClient();
  return useIdempotentMutation({
    mutationFn: (body: CreateScanRequest, key) => scans.create(body, key),
    onSuccess: (scan) => invalidatePlant(qc, scan, true),
  });
}

export function useUploadImage(scanId: string) {
  const qc = useQueryClient();
  return useIdempotentMutation({
    mutationFn: (part: ShotUpload, key) => scans.uploadImage(scanId, part, key),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.scan(scanId) }),
  });
}

export function useSubmitScan() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => scans.submit(id),
    onSuccess: (scan) =>
      Promise.all([qc.invalidateQueries({ queryKey: qk.scan(scan.id) }), invalidatePlant(qc, scan, false)]),
  });
}

export function useAnswerQuestion(scanId: string) {
  const qc = useQueryClient();
  return useIdempotentMutation({
    mutationFn: (body: AnswerRequest, key) => scans.answer(scanId, body, key),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.scan(scanId) }),
  });
}

export function useDeleteScan() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (scan: Pick<Scan, "id" | "plant_id" | "garden_id">) => scans.delete(scan.id),
    onSuccess: (_data, scan) => {
      qc.removeQueries({ queryKey: qk.scan(scan.id) });
      // A pending scan's jar-test task goes with it.
      return Promise.all([invalidatePlant(qc, scan, true), qc.invalidateQueries({ queryKey: qk.tasks(scan.garden_id) })]);
    },
  });
}
