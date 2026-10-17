import type { GuidedCheck, ImageKind, Question, ScanStatus, Stage } from "./types";

// Statuses, stages, the resting rule and the kind limits (contracts 3 and 4; docs/frontend 10).

/** Pipeline order, as `Stage` lists it. */
export const STAGE_ORDER: readonly Stage[] = [
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
];

/** What each stage is called on screen. Plain words, no model names. */
export const STAGE_LABEL: Record<Stage, string> = {
  plant_check: "Finding the plant",
  segmentation: "Outlining leaves",
  species: "Identifying the species",
  disease: "Looking for disease",
  image_embedding: "Comparing with known cases",
  measurement: "Measuring spots",
  context: "Checking weather and soil",
  retrieve: "Gathering likely problems",
  vlm_findings: "Examining the photos",
  vlm_indicators: "Checking telltale signs",
  guided: "Reading the soil check",
  verify: "Checking your photo",
  scoring: "Weighing the evidence",
  fusion: "Combining results",
  plan: "Writing your plan",
};

export const STATUS_LABEL: Record<ScanStatus, string> = {
  pending: "Not sent yet",
  queued: "Waiting to start",
  analysing: "Analysing",
  awaiting_answer: "Needs your answer",
  diagnosed: "Diagnosed",
  planned: "Plan ready",
  failed: "Couldn't finish",
};

export const KIND_LIMIT: Record<ImageKind, number> = {
  whole_plant: 1,
  leaf_closeup: 2,
  leaf_underside: 1,
  soil_surface: 1,
  flower_fruit: 1,
  squeeze_test: 1,
  jar_test: 1,
};

export const KIND_LABEL: Record<ImageKind, string> = {
  whole_plant: "Whole plant",
  leaf_closeup: "Leaf close-up",
  leaf_underside: "Leaf underside",
  soil_surface: "Soil surface",
  flower_fruit: "Flower or fruit",
  squeeze_test: "Soil squeeze test",
  jar_test: "Soil jar test",
};

/**
 * The resting rule: the same as the server's close table (contracts 4, when the stream closes).
 * At rest when no guided check is analysing, and the scan is failed, or planned with no open question.
 */
export function isAtRest(scan: {
  status: ScanStatus;
  open_question?: Pick<Question, "status"> | null;
  guided_checks: Pick<GuidedCheck, "status">[];
}): boolean {
  if (scan.guided_checks.some((c) => c.status === "analysing")) return false;
  if (scan.status === "failed") return true;
  return scan.status === "planned" && !scan.open_question;
}

export function isInProgress(status: ScanStatus): boolean {
  return status === "queued" || status === "analysing" || status === "awaiting_answer";
}
