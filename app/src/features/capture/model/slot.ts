// One shot slot's state machine (flows 14). Pure: the upload runner and the screen send it events, and an event
// a state doesn't take leaves the slot as it was. A slot holds the exact file it uploads and the idempotency key
// of that upload, so a retry sends the same bytes under the same key, and anything new (a retake, a re-encode)
// arrives with a new key (flows 4, same key, same bytes).
import type { QualityHint } from "@/domain/types";

import type { CaptureKind } from "./shotPlan";

export type SlotState = "empty" | "captured" | "uploading" | "stored" | "rejected" | "failed" | "deleting";

/** The file a shot uploads: a photo copied into the draft's folder, so it survives the app closing (README 11). */
export interface ShotFile {
  uri: string;
  name: string;
  type: string;
  /** From the library, not the camera: only these may be re-encoded after a 413 or 415 (flows 4). */
  picked: boolean;
  /** Already re-encoded once; a second refusal is final. */
  reencoded?: boolean;
  /** Pixels, for the re-encode's resize. */
  width: number;
  height: number;
  /** LiDAR phones only: the 16-bit depth PNG kept beside the photo, and its `DepthMeta` (contracts 3). */
  depth?: ShotDepth;
}

export interface ShotDepth {
  uri: string;
  name: string;
  meta: Record<string, unknown>;
}

export interface Slot {
  /** "leaf_closeup:1". */
  id: string;
  kind: CaptureKind;
  index: number;
  state: SlotState;
  file?: ShotFile;
  key?: string;
  /** The stored image's ID, for DELETE /scans/{id}/images/{image_id}. */
  imageId?: string;
  /** A shot waiting while the stored one is deleted: a retake of a full mode (flows 4, replacing). */
  next?: { file: ShotFile; key: string };
  /** Why the last upload didn't store: the server's message, and the retake hint for image_quality. */
  problem?: { message: string; hint?: QualityHint };
  /** Stored without its depth: a small note, never an error (contracts 3). */
  noDepth?: boolean;
}

export type SlotEvent =
  /** A new shot, from the camera or the library. */
  | { type: "capture"; file: ShotFile; key: string }
  | { type: "upload" }
  | { type: "stored"; imageId: string; noDepth?: boolean }
  /** 422 image_quality: not stored, retake. */
  | { type: "rejected"; message: string; hint?: QualityHint }
  /** Network, 5xx, 409 request_in_progress: retried with the same key and bytes. */
  | { type: "failed"; message: string }
  /** 413 or 415 on a picked photo: the re-encoded file goes as a new action with a new key. */
  | { type: "reencoded"; file: ShotFile; key: string }
  /** Any other refusal: the message shows and the slot empties. */
  | { type: "refused"; message: string }
  /** Delete the stored shot; with a file, that shot uploads once the delete is done. */
  | { type: "replace"; next?: { file: ShotFile; key: string } }
  | { type: "deleted" }
  /** The delete didn't go through after its retries: the stored shot stays, and a waiting retake is dropped. */
  | { type: "deleteFailed"; message: string };

export function emptySlot(kind: CaptureKind, index: number): Slot {
  return { id: `${kind}:${index}`, kind, index, state: "empty" };
}

function clear(slot: Slot): Slot {
  return { id: slot.id, kind: slot.kind, index: slot.index, state: "empty" };
}

export function slotReducer(slot: Slot, event: SlotEvent): Slot {
  switch (event.type) {
    case "capture":
      // A retake of a rejected or failed shot, or a slot's first shot. A stored shot is replaced, not overwritten.
      if (slot.state === "empty" || slot.state === "rejected" || slot.state === "failed" || slot.state === "captured") {
        return { ...clear(slot), state: "captured", file: event.file, key: event.key };
      }
      return slot;
    case "upload":
      return slot.state === "captured" || slot.state === "failed" ? { ...slot, state: "uploading", problem: undefined } : slot;
    case "stored":
      return slot.state === "uploading"
        ? { ...slot, state: "stored", imageId: event.imageId, problem: undefined, noDepth: event.noDepth || undefined }
        : slot;
    case "rejected":
      return slot.state === "uploading"
        ? { ...slot, state: "rejected", key: undefined, problem: { message: event.message, hint: event.hint } }
        : slot;
    case "failed":
      // The key stays: the retry is the same action.
      return slot.state === "uploading" ? { ...slot, state: "failed", problem: { message: event.message } } : slot;
    case "reencoded":
      return slot.state === "uploading" ? { ...clear(slot), state: "captured", file: event.file, key: event.key } : slot;
    case "refused":
      return slot.state === "uploading" ? { ...clear(slot), problem: { message: event.message } } : slot;
    case "replace":
      return slot.state === "stored" ? { ...slot, state: "deleting", next: event.next, problem: undefined } : slot;
    case "deleted":
      if (slot.state !== "deleting") return slot;
      return slot.next ? { ...clear(slot), state: "captured", file: slot.next.file, key: slot.next.key } : clear(slot);
    case "deleteFailed":
      return slot.state === "deleting" ? { ...slot, state: "stored", next: undefined, problem: { message: event.message } } : slot;
  }
}
