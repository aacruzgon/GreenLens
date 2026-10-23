// The distance hint on LiDAR phones (README 11, capture; architecture 6.7): the good range per kind, and the
// chip's words. The ranges move with the LiDAR day-1 check (architecture 10).
import type { CaptureKind } from "./shotPlan";

/** Metres. */
export const DISTANCE_RANGES: Record<CaptureKind, { min: number; max: number }> = {
  whole_plant: { min: 0.5, max: 3 },
  leaf_closeup: { min: 0.25, max: 0.45 },
  leaf_underside: { min: 0.25, max: 0.45 },
  soil_surface: { min: 0.3, max: 0.8 },
  flower_fruit: { min: 0.3, max: 0.8 },
};

/** Kinds with a distance range: every capture kind, not the guided checks. */
export function hasDistanceRange(kind: string): kind is CaptureKind {
  return kind in DISTANCE_RANGES;
}

export type DistanceFit = "good" | "closer" | "back";

export interface DistanceHint {
  fit: DistanceFit;
  /** "32 cm · good distance", "1.2 m · move closer". */
  text: string;
}

function length(m: number): string {
  return m < 1 ? `${Math.round(m * 100)} cm` : `${(Math.round(m * 10) / 10).toFixed(1)} m`;
}

export function distanceHint(kind: CaptureKind, metres: number): DistanceHint {
  const { min, max } = DISTANCE_RANGES[kind];
  const fit: DistanceFit = metres < min ? "back" : metres > max ? "closer" : "good";
  const words = fit === "good" ? "good distance" : fit === "closer" ? "move closer" : "move back";
  return { fit, text: `${length(metres)} · ${words}` };
}
