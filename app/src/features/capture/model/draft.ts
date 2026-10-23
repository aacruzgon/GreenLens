// A capture draft (README 11, capture): the pending scan, its slots and the chosen mode. Persisted, so a capture
// interrupted by the app closing resumes on the still-pending scan (flows 15, suspended). Pure functions only.
import type { QualityHint } from "@/domain/types";

import { KIND_LIMIT, MODES, NEEDED, REQUIRED, type CaptureKind } from "./shotPlan";
import { emptySlot, slotReducer, type ShotFile, type Slot, type SlotEvent } from "./slot";

export interface CaptureDraft {
  scanId: string;
  gardenId: string;
  plantId: string;
  mode: CaptureKind;
  slots: Slot[];
}

/**
 * A new draft for a pending scan. Shots already on the server (a lost draft, another device: flows 16) fill their
 * slots as stored, with no local file; photos that answered a question don't count (contracts 3).
 */
export function createDraft(scan: {
  id: string;
  garden_id: string;
  plant_id: string;
  images?: readonly { id: string; kind: string; answers_question_id?: string }[];
}): CaptureDraft {
  const images = (scan.images ?? []).filter((i) => !i.answers_question_id);
  return {
    scanId: scan.id,
    gardenId: scan.garden_id,
    plantId: scan.plant_id,
    mode: "whole_plant",
    slots: MODES.flatMap((m) => {
      const stored = images.filter((i) => i.kind === m.kind);
      return Array.from({ length: KIND_LIMIT[m.kind] }, (_, i): Slot => {
        const image = stored[i];
        return image ? { ...emptySlot(m.kind, i), state: "stored", imageId: image.id } : emptySlot(m.kind, i);
      });
    }),
  };
}

export function applySlot(draft: CaptureDraft, slotId: string, event: SlotEvent): CaptureDraft {
  return { ...draft, slots: draft.slots.map((s) => (s.id === slotId ? slotReducer(s, event) : s)) };
}

export function slotsOf(draft: CaptureDraft, kind: CaptureKind): Slot[] {
  return draft.slots.filter((s) => s.kind === kind);
}

/** What the shutter does in a mode: fill a free slot, or retake the mode's last stored shot when it is full. */
export type ShutterTarget = { slotId: string; replace: boolean } | undefined;

export function shutterTarget(draft: CaptureDraft, kind: CaptureKind): ShutterTarget {
  const slots = slotsOf(draft, kind);
  const free = slots.find((s) => s.state === "empty" || s.state === "rejected" || s.state === "failed");
  if (free) return { slotId: free.id, replace: false };
  // Every slot is busy or stored; a busy one (uploading, deleting) isn't touched.
  const stored = slots.filter((s) => s.state === "stored");
  const last = stored[stored.length - 1];
  return last && stored.length === slots.length ? { slotId: last.id, replace: true } : undefined;
}

/** Puts a new shot in the slot the shutter targets. Returns the draft unchanged when no slot can take it. */
export function takeShot(draft: CaptureDraft, kind: CaptureKind, file: ShotFile, key: string): CaptureDraft {
  const target = shutterTarget(draft, kind);
  if (!target) return draft;
  return applySlot(draft, target.slotId, target.replace ? { type: "replace", next: { file, key } } : { type: "capture", file, key });
}

const busy = (s: Slot) => s.state === "captured" || s.state === "uploading" || s.state === "deleting";

/** Analyse is enabled when every required shot is stored and nothing is uploading (screens 3.2). */
export function canAnalyse(draft: CaptureDraft): boolean {
  return REQUIRED.every((k) => slotsOf(draft, k).some((s) => s.state === "stored")) && !draft.slots.some(busy);
}

export function hasStored(draft: CaptureDraft, kind: CaptureKind): boolean {
  return slotsOf(draft, kind).some((s) => s.state === "stored");
}

/** The dial: which needed kinds have a stored shot, in NEEDED's order. */
export function coverage(draft: CaptureDraft): { kind: CaptureKind; stored: boolean }[] {
  return NEEDED.map((kind) => ({ kind, stored: hasStored(draft, kind) }));
}

/** A mode's mark on the bar: a dot once stored, busy while a shot is on its way, a warning after a refusal. */
export type ModeMark = "none" | "stored" | "busy" | "problem";

export function modeMark(draft: CaptureDraft, kind: CaptureKind): ModeMark {
  const slots = slotsOf(draft, kind);
  if (slots.some(busy)) return "busy";
  if (slots.some((s) => s.state === "rejected" || s.state === "failed")) return "problem";
  return slots.some((s) => s.state === "stored") ? "stored" : "none";
}

/** Shots the runner must send: captured ones upload, deleting ones delete (on start too, after the app closed). */
export function pendingWork(draft: CaptureDraft): { uploads: Slot[]; deletes: Slot[] } {
  return {
    uploads: draft.slots.filter((s) => s.state === "captured"),
    deletes: draft.slots.filter((s) => s.state === "deleting"),
  };
}

/** After a restart nothing is in flight: an upload that was running is sent again under its key (flows 15). */
export function resumeDraft(draft: CaptureDraft): CaptureDraft {
  return { ...draft, slots: draft.slots.map((s) => (s.state === "uploading" ? { ...s, state: "captured" } : s)) };
}

/**
 * Points a saved draft's photos at where they are now. iOS can move the app's folder when the app is updated, so
 * a URI saved before the update is gone; the photo's name in the scan's folder still finds it (README 11).
 */
export function relocateDraft(draft: CaptureDraft, locate: (name: string) => string | undefined): CaptureDraft {
  const move = (file: ShotFile): ShotFile => {
    const uri = locate(file.name);
    const depthUri = file.depth && locate(file.depth.name);
    return {
      ...file,
      ...(uri ? { uri } : {}),
      ...(file.depth && depthUri ? { depth: { ...file.depth, uri: depthUri } } : {}),
    };
  };
  return {
    ...draft,
    slots: draft.slots.map((s) => ({
      ...s,
      ...(s.file ? { file: move(s.file) } : {}),
      ...(s.next ? { next: { ...s.next, file: move(s.next.file) } } : {}),
    })),
  };
}

/** A shot's kept files: the photo, and its depth map on a LiDAR phone. */
export function fileUris(file: ShotFile): string[] {
  return file.depth ? [file.uri, file.depth.uri] : [file.uri];
}

const holdsUpload = (s: Slot) => s.state === "captured" || s.state === "uploading" || s.state === "failed";

/**
 * Brings a saved draft in line with its scan on the server, once per visit (flows 16). `known` is the images the
 * draft held as stored before the scan was fetched: one of those the server no longer has (removed on another
 * device, or a scan whose ID the mock reused) empties its slot, and a shot being deleted whose image is gone is
 * done. A server image the draft lacks fills a free slot of its kind, as createDraft does, unless a slot of that
 * kind still holds an upload: the server's image may be that upload, whose retry returns it under the same key.
 * Returns the draft and the kept photos it no longer uses.
 */
export function reconcileDraft(
  draft: CaptureDraft,
  scan: { images?: readonly { id: string; kind: string; answers_question_id?: string }[] },
  known: ReadonlySet<string>,
): { draft: CaptureDraft; unused: string[] } {
  const images = (scan.images ?? []).filter((i) => !i.answers_question_id);
  const onServer = new Set(images.map((i) => i.id));
  const unused: string[] = [];
  let slots = draft.slots.map((s): Slot => {
    if (!s.imageId || onServer.has(s.imageId)) return s;
    if (s.state === "deleting") return slotReducer(s, { type: "deleted" });
    if (s.state === "stored" && known.has(s.imageId)) {
      if (s.file) unused.push(...fileUris(s.file));
      return emptySlot(s.kind, s.index);
    }
    return s;
  });
  const held = new Set(slots.flatMap((s) => (s.imageId ? [s.imageId] : [])));
  for (const image of images) {
    if (held.has(image.id) || slots.some((s) => s.kind === image.kind && holdsUpload(s))) continue;
    const free = slots.find((s) => s.kind === image.kind && s.state === "empty");
    if (!free) continue;
    slots = slots.map((s) => (s === free ? { ...emptySlot(s.kind, s.index), state: "stored", imageId: image.id } : s));
    held.add(image.id);
  }
  return { draft: { ...draft, slots }, unused };
}

/** How an upload ended, as the slot's next event (flows 14). */
export type UploadOutcome =
  | { type: "rejected"; message: string; hint?: QualityHint }
  | { type: "retry"; message: string; afterMs: number }
  | { type: "reencode" }
  | { type: "refused"; message: string };

const QUALITY_HINTS: readonly QualityHint[] = ["tap_to_focus", "more_light", "avoid_glare"];

/** An API refusal as the model reads it: the fields of the api layer's ApiError (contracts 2). */
export interface UploadRefusal {
  code: string;
  status: number;
  message: string;
  retryable: boolean;
  retryAfterS?: number;
  details?: Record<string, unknown>;
}

/** `error` is the API's refusal, or undefined when the request never got an answer (the network). */
export function uploadOutcome(error: UploadRefusal | undefined, file: ShotFile): UploadOutcome {
  if (!error) {
    return { type: "retry", message: "The photo didn't upload. Check your connection.", afterMs: 1000 };
  }
  if (error.code === "image_quality") {
    const hint = error.details?.hint;
    return { type: "rejected", message: error.message, hint: QUALITY_HINTS.find((h) => h === hint) };
  }
  if (error.code === "upload_too_large" || error.code === "unsupported_media_type") {
    // Only a picked photo may change: a camera shot (and later, a LiDAR shot) is sent as taken (contracts 3).
    if (file.picked && !file.reencoded) return { type: "reencode" };
    return { type: "refused", message: error.message };
  }
  if (error.retryable || error.status >= 500 || error.code === "request_in_progress") {
    return { type: "retry", message: error.message, afterMs: (error.retryAfterS ?? 1) * 1000 };
  }
  return { type: "refused", message: error.message };
}
