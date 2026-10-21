// The shot plan (README 11, capture): the shots a scan can carry before submit, from the kind limits in
// contracts 3, in the mode bar's order (roadmap F2 item 3). The squeeze and jar tests are guided checks
// with their own screens, so they are not modes here.
import type { ImageKind } from "@/domain/types";

export type CaptureKind = Extract<ImageKind, "whole_plant" | "leaf_closeup" | "leaf_underside" | "soil_surface" | "flower_fruit">;

export interface ShotMode {
  kind: CaptureKind;
  /** The mode bar's word, set in capitals by the bar (mockup .mode). */
  label: string;
  /** The drop card's title. */
  title: string;
  /** The drop card's instruction (mockup t-capture, data-hint). */
  instruction: string;
}

export const MODES: readonly ShotMode[] = [
  {
    kind: "whole_plant",
    label: "Whole plant",
    title: "Whole plant",
    instruction: "Step back about 1 m and get the whole plant in the frame.",
  },
  {
    kind: "leaf_closeup",
    label: "Close-up",
    title: "Leaf close-up",
    instruction: "Fill the frame with one affected leaf, about 30 cm away. Tap the leaf to focus.",
  },
  {
    kind: "leaf_underside",
    label: "Underside",
    title: "Leaf underside",
    instruction: "Suspect pests? Turn a leaf over and photograph the underside.",
  },
  {
    kind: "soil_surface",
    label: "Soil",
    title: "Soil",
    instruction: "Add the soil around the base, from about 40 cm.",
  },
  {
    // FRUIT is the flower_fruit kind; its instruction says flower or fruit (roadmap F2 item 3).
    kind: "flower_fruit",
    label: "Fruit",
    title: "Flower or fruit",
    instruction: "Something wrong with a flower or fruit? Fill the frame with it.",
  },
];

/** The most of each kind a pending scan takes (contracts 3, ImageKind). */
export const KIND_LIMIT: Record<CaptureKind, number> = {
  whole_plant: 1,
  leaf_closeup: 2,
  leaf_underside: 1,
  soil_surface: 1,
  flower_fruit: 1,
};

/** Submit needs these (contracts 3, missing_required_shot). */
export const REQUIRED: readonly CaptureKind[] = ["whole_plant"];

/**
 * The coverage dial's segments: the shots a diagnosis needs, not only the one submit needs. The mockup's dial
 * counts the whole plant and a close-up ("1 of 2 needed shots", docs/design/mockups/index.html t-capture).
 */
export const NEEDED: readonly CaptureKind[] = ["whole_plant", "leaf_closeup"];

export function modeOf(kind: CaptureKind): ShotMode {
  return MODES.find((m) => m.kind === kind)!;
}
