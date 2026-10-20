// Scan statuses, stage order, the resting rule and image kind limits
// (contracts 3 and 4; docs/frontend/README.md §10).
import type { GuidedCheck, ImageKind, Question, ScanStatus, Stage } from "./types";

export const SCAN_STATUSES = [
  "pending",
  "queued",
  "analysing",
  "awaiting_answer",
  "diagnosed",
  "planned",
  "failed",
] as const satisfies readonly ScanStatus[];

/** Pipeline stages in the order a run starts them (contracts 4). */
export const STAGE_ORDER = [
  "plant_check",
  "segmentation",
  "species",
  "disease",
  "image_embedding",
  "measurement",
  "context",
  "retrieve",
  "vlm_findings",
  "vlm_indicators",
  "guided",
  "verify",
  "scoring",
  "fusion",
  "plan",
] as const satisfies readonly Stage[];

export function stageIndex(stage: Stage): number {
  return STAGE_ORDER.indexOf(stage);
}

/** Shots per kind before submit (contracts 3). A photo that answers a question doesn't count. */
export const IMAGE_KIND_LIMITS: Record<ImageKind, number> = {
  whole_plant: 1,
  leaf_closeup: 2,
  leaf_underside: 1,
  soil_surface: 1,
  flower_fruit: 1,
  squeeze_test: 1,
  jar_test: 1,
};

/** Kinds that are guided soil checks: unlimited after submit. */
export const GUIDED_KINDS = ["squeeze_test", "jar_test"] as const satisfies readonly ImageKind[];

/** The only kind a scan can't be submitted without. */
export const REQUIRED_KIND = "whole_plant" as const satisfies ImageKind;

export function imageLimit(kind: ImageKind): number {
  return IMAGE_KIND_LIMITS[kind];
}

/** Whether another shot of `kind` fits before submit, given how many are held. */
export function canAddImage(kind: ImageKind, held: number): boolean {
  return held < IMAGE_KIND_LIMITS[kind];
}

export function canSubmit(held: Partial<Record<ImageKind, number>>): boolean {
  return (held[REQUIRED_KIND] ?? 0) >= 1;
}

/**
 * The resting rule, the same as the server's close table (contracts 4): no guided check is
 * `analysing`, and the scan is either `failed`, or `planned` with no open question
 * (`open` or `answering`).
 */
export function isAtRest(scan: {
  status: ScanStatus;
  open_question?: Question | null;
  guided_checks: readonly Pick<GuidedCheck, "status">[];
}): boolean {
  if (scan.guided_checks.some((c) => c.status === "analysing")) return false;
  if (scan.status === "failed") return true;
  return scan.status === "planned" && !scan.open_question;
}
