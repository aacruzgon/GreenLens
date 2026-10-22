// One photo for a scan after the main capture (roadmap F2 item 7; screens 3.2; flows 6 and 8): a photo that answers
// a question (question mode) or a guided soil check (guided mode). Both have one slot and send on Send, not on the
// shutter: a question photo makes the question `answering` at once and a guided photo starts its own job, so the
// user sees the photo before it goes (contracts 3, POST /scans/{id}/images). Pure functions only.
import { TZDate } from "@date-fns/tz";
import { addHours, format } from "date-fns";

import { GUIDED_KINDS } from "@/domain/scan";
import type { CreateTaskRequest, GuidedKind, ImageKind, LocalDate, QualityHint, ScanDetail, Task } from "@/domain/types";

import type { ShotFile } from "./slot";

export interface GuidedSteps {
  title: string;
  /** What to do before the photo, shown first with Start (screens 3.2, guided mode). */
  steps: readonly string[];
  /** The drop card's instruction once the camera is up. */
  instruction: string;
}

// The photo each check needs is what the guided prompt looks for (vision vlm.md 7, guided@1): the squeezed soil in
// an open hand, and the jar's layers from the side once the water has cleared.
export const GUIDED: Record<GuidedKind, GuidedSteps> = {
  squeeze_test: {
    title: "Squeeze test",
    steps: [
      "Take a handful of soil from about 10 cm down. If it's dry, add a few drops of water.",
      "Squeeze it hard in your fist, then open your hand.",
      "Try to push it out between your thumb and finger into a ribbon.",
    ],
    instruction: "Photograph the soil in your open hand, from about 30 cm.",
  },
  jar_test: {
    title: "Jar test",
    steps: [
      "Fill a clear jar a third full with soil, then nearly to the top with water.",
      "Add a drop of dish soap, close the lid and shake it for 3 minutes.",
      "Leave it to settle for at least 6 hours. We'll add a task to photograph it.",
    ],
    instruction: "Once the water has cleared, photograph the jar from the side, level with the layers.",
  },
};

/** The jar settles for at least 6 hours, so its task is due on the garden's date 6 hours from now. */
export const JAR_SETTLE_HOURS = 6;

export const JAR_TASK_TITLE = "Photograph the jar test";

export function jarTaskDue(timeZone: string, now: Date = new Date()): LocalDate {
  return format(new TZDate(addHours(now, JAR_SETTLE_HOURS).getTime(), timeZone), "yyyy-MM-dd");
}

/** The jar test's task (flows 8): built once per Start, so a retry sends the same body under the same key. */
export function jarTaskRequest(
  scan: Pick<ScanDetail, "id" | "garden_id" | "plant_id">,
  timeZone: string,
  now: Date = new Date(),
): CreateTaskRequest {
  return { garden_id: scan.garden_id, plant_id: scan.plant_id, scan_id: scan.id, title: JAR_TASK_TITLE, due: jarTaskDue(timeZone, now) };
}

export type OneShotPurpose =
  | { type: "question"; questionId: string }
  | { type: "guided"; kind: GuidedKind };

/** What the screen asks for, or why it can't: the scan decides (contracts 3, upload rules). */
export type OneShotTarget =
  | { ok: true; kind: ImageKind; title: string; instruction: string; questionId?: string; guided?: GuidedSteps }
  | { ok: false; message: string };

export function isGuidedKind(kind: string): kind is GuidedKind {
  return (GUIDED_KINDS as readonly string[]).includes(kind);
}

export function oneShotTarget(scan: Pick<ScanDetail, "status" | "open_question">, purpose: OneShotPurpose): OneShotTarget {
  if (purpose.type === "guided") {
    // A guided check is taken in any status except failed (contracts 3).
    if (scan.status === "failed") return { ok: false, message: "This scan failed, so it can't take a soil check. Start a new scan." };
    const guided = GUIDED[purpose.kind];
    return { ok: true, kind: purpose.kind, title: guided.title, instruction: guided.instruction, guided };
  }
  const q = scan.open_question;
  // Only an open photo request takes a photo; answered, superseded or answering ones get 409 (contracts 3). A scan
  // keeps only its open question, so one that's gone may have been answered or superseded: it is no longer open.
  if (!q || q.id !== purpose.questionId) return { ok: false, message: "This question is no longer open." };
  if (q.kind !== "photo_request" || !q.photo_kind) return { ok: false, message: "This question doesn't need a photo." };
  if (q.status !== "open") return { ok: false, message: "A photo for this question is already being checked." };
  return {
    ok: true,
    kind: q.photo_kind,
    title: q.text,
    instruction: q.photo_hint ?? "Take the photo the question asks for.",
    questionId: q.id,
  };
}

/**
 * The one slot (flows 14, cut to one shot that waits for Send). `intro` is guided mode's instructions before Start;
 * `aim` is the live camera; `review` holds the photo with Retake and Send; `sending` is the upload, which keeps its
 * file and key so a failed send is retried as the same action.
 */
export type OneShotState =
  | { step: "intro" }
  | { step: "aim"; problem?: ShotProblem }
  | { step: "review"; file: ShotFile; key: string; problem?: ShotProblem }
  | { step: "sending"; file: ShotFile; key: string }
  | { step: "sent"; imageId: string };

export interface ShotProblem {
  message: string;
  hint?: QualityHint;
  /** A failed send: Try again sends the same photo under the same key. */
  retry?: boolean;
}

export type OneShotEvent =
  | { type: "start" }
  | { type: "take"; file: ShotFile; key: string }
  | { type: "retake" }
  | { type: "send" }
  | { type: "stored"; imageId: string }
  /** 422 image_quality, or any other refusal: the photo isn't stored, so it's taken again. */
  | { type: "rejected"; message: string; hint?: QualityHint }
  /** The network or a retryable error: the photo stays for Try again. */
  | { type: "failed"; message: string }
  /** A picked photo re-encoded after 413 or 415: a new body, a new key, sent at once. */
  | { type: "reencoded"; file: ShotFile; key: string };

export function initialOneShot(purpose: OneShotPurpose, skipIntro = false): OneShotState {
  return purpose.type === "guided" && !skipIntro ? { step: "intro" } : { step: "aim" };
}

export function oneShotReducer(state: OneShotState, event: OneShotEvent): OneShotState {
  switch (event.type) {
    case "start":
      return state.step === "intro" ? { step: "aim" } : state;
    case "take":
      return state.step === "aim" || state.step === "review" ? { step: "review", file: event.file, key: event.key } : state;
    case "retake":
      return state.step === "review" ? { step: "aim" } : state;
    case "send":
      return state.step === "review" ? { step: "sending", file: state.file, key: state.key } : state;
    case "stored":
      return state.step === "sending" ? { step: "sent", imageId: event.imageId } : state;
    case "rejected":
      return state.step === "sending" ? { step: "aim", problem: { message: event.message, hint: event.hint } } : state;
    case "failed":
      return state.step === "sending" ? { step: "review", file: state.file, key: state.key, problem: { message: event.message, retry: true } } : state;
    case "reencoded":
      return state.step === "sending" ? { step: "sending", file: event.file, key: event.key } : state;
  }
}

/**
 * A scan's open jar-test task: Start reuses it, and Discard names it (screens 3.2). Only a user's task carries a scan
 * link on the server (contracts 9), and only the jar test makes one; a plan's task never does.
 */
export function jarTaskOf<T extends Pick<Task, "source" | "scan_id" | "done_at" | "dismissed_at">>(tasks: readonly T[], scanId: string): T | undefined {
  return tasks.find((t) => t.source === "user" && t.scan_id === scanId && !t.done_at && !t.dismissed_at);
}
