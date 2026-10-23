// An evaluation capture's files (evaluation, the set and its files; roadmap F5 item 2): each shot's photo, depth
// PNG and DepthMeta, with the module's report, in one folder per case, and a case.json the vision track turns into a
// manifest line (`EvalCase.images`). AVFoundation's depth has no confidence map, so none is written. Pure.
import type { ImageKind } from "@/domain/types";

export interface EvalImage {
  file: string;
  kind: ImageKind;
  depth?: string;
  depth_confidence?: string;
  depth_meta?: string;
  report: string;
  /**
   * The day-1 check's ruler on this shot: the two ends tapped (normalised, as shown) and the length measured from
   * the depth, with the length the user said it is. The labeller turns it into `truth.measured` (evaluation 2).
   */
  ruler?: EvalRuler;
}

export interface EvalRuler {
  points: [{ x: number; y: number }, { x: number; y: number }];
  measured_mm: number;
  known_mm?: number;
}

export interface EvalCaseFile {
  case_id: string;
  /** The same physical plant across cases: the repeat scans are 5 plants × 3 scans (evaluation 2). */
  plant_key: string;
  device_model?: string;
  images: EvalImage[];
  saved_at: string;
}

/** "tomato-0007": lower-case letters, digits and dashes, as the manifest's case IDs are. */
export function isCaseId(id: string): boolean {
  return /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(id);
}

/** The next shot's file names in a case: "03-leaf_closeup.jpg" and its companions. */
export function shotNames(index: number, kind: ImageKind, hasDepth: boolean): EvalImage {
  const base = `${String(index + 1).padStart(2, "0")}-${kind}`;
  return {
    file: `${base}.jpg`,
    kind,
    ...(hasDepth ? { depth: `${base}-depth.png`, depth_meta: `${base}-depth_meta.json` } : {}),
    report: `${base}-report.json`,
  };
}

/** The case file with one more shot: a new case, or the saved one read back. */
export function addShot(
  saved: EvalCaseFile | undefined,
  caseId: string,
  plantKey: string,
  kind: ImageKind,
  hasDepth: boolean,
  deviceModel: string | undefined,
  now: string,
  ruler?: EvalRuler,
): EvalCaseFile {
  const images = saved?.images ?? [];
  return {
    case_id: caseId,
    plant_key: plantKey || saved?.plant_key || caseId,
    ...(deviceModel ?? saved?.device_model ? { device_model: deviceModel ?? saved?.device_model } : {}),
    images: [...images, { ...shotNames(images.length, kind, hasDepth), ...(ruler ? { ruler } : {}) }],
    saved_at: now,
  };
}
