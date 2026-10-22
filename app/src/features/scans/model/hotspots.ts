// Findings as chips on the photo (roadmap F3 item 4; design 3.1, after the Camera app's chip on the subject).
// A disease finding is drawn as what it looks like ("Leaf spots · 8–12 mm"), never as the problem's name: the
// name waits for the result (screens 6). Organisms keep their name, since a finding is not a diagnosis.
import type { BBox, ImageKind, Observation, ScanImage } from "@/domain/types";

import { lesionSize } from "./phases";
import type { ScanState } from "./scanState";

export interface Hotspot {
  /** Stable across re-renders: the region's image and box. */
  key: string;
  imageId: string;
  bbox: BBox;
  label: string;
  /** "8–12 mm", from the run's lesion measurement on the same photo. */
  size?: string;
  kind: "symptom" | "organism";
  /** The findings it stands for, for VoiceOver and the run log. */
  observationIds: string[];
}

const SYMPTOM_STAGES = new Set(["disease", "damage"]);

function regionKey(imageId: string, bbox: BBox): string {
  return `${imageId}:${bbox.map((n) => n.toFixed(3)).join(",")}`;
}

function symptomWord(kind: ImageKind | undefined): string {
  return kind === "leaf_closeup" || kind === "leaf_underside" ? "Leaf spots" : "Affected area";
}

/**
 * One hotspot per region. Several disease findings on one leaf (the classifier's top two) share it; findings
 * without a region, and the leaf detector's and the plant check's own boxes, aren't drawn.
 */
export function hotspotsOf(state: Pick<ScanState, "findings">, images: readonly Pick<ScanImage, "id" | "kind">[]): Hotspot[] {
  const kindOf = new Map(images.map((i) => [i.id, i.kind]));
  const out = new Map<string, Hotspot>();
  const size = lesionSize(state);
  const sizeImage = state.findings.find((f) => f.detail?.measurement?.quantity === "lesion_diameter_mm")?.region?.image_id;
  for (const f of state.findings) {
    const region = f.region;
    if (!region) continue;
    const symptom = SYMPTOM_STAGES.has(f.stage) && isProblemFinding(f);
    const organism = f.stage === "organism";
    if (!symptom && !organism) continue;
    const key = regionKey(region.image_id, region.bbox);
    const existing = out.get(key);
    if (existing) {
      existing.observationIds.push(f.id);
      continue;
    }
    out.set(key, {
      key,
      imageId: region.image_id,
      bbox: region.bbox,
      label: symptom ? symptomWord(kindOf.get(region.image_id)) : f.label,
      size: symptom && (sizeImage === undefined || sizeImage === region.image_id) ? size : undefined,
      kind: symptom ? "symptom" : "organism",
      observationIds: [f.id],
    });
  }
  return [...out.values()];
}

/** A damage finding, or a disease finding that names a problem: the classifier's "Healthy" finding is no symptom. */
function isProblemFinding(f: Observation): boolean {
  return f.stage === "damage" || f.ref?.table === "disease";
}

/** The oval inside a region: the box's centre, at 70% of its size (styles.css .phone[data-region="hotspot"]). */
export function ovalOf(rect: { x: number; y: number; width: number; height: number }) {
  const width = rect.width * 0.7;
  const height = rect.height * 0.7;
  return { x: rect.x + (rect.width - width) / 2, y: rect.y + (rect.height - height) / 2, width, height };
}

/**
 * The photo the analysis is reading: the one with the newest drawn finding, else the first close-up, else the
 * whole plant. The user's tap on the strip overrides it (the screen keeps that choice).
 */
export function readingImageId(hotspots: readonly Hotspot[], images: readonly Pick<ScanImage, "id" | "kind">[]): string | undefined {
  const newest = hotspots[hotspots.length - 1];
  if (newest) return newest.imageId;
  return (images.find((i) => i.kind === "leaf_closeup") ?? images.find((i) => i.kind === "whole_plant") ?? images[0])?.id;
}

/** The species check's chip ("Tomato · Change"): while the plant's species waits for the user, from the run. */
export function speciesChip(state: Pick<ScanState, "species" | "findings">, plant: { species_status: string; species?: { common_name: string } }) {
  if (state.species) {
    const top = state.species.candidates.find((c) => c.species_id === state.species?.workingSpeciesId) ?? state.species.candidates[0];
    return top ? { name: top.name, needsConfirmation: state.species.needsConfirmation } : undefined;
  }
  if (plant.species_status === "suggested") {
    const name = plant.species?.common_name ?? state.findings.find((f) => f.pipeline_stage === "species")?.label;
    return name ? { name, needsConfirmation: true } : undefined;
  }
  return undefined;
}
