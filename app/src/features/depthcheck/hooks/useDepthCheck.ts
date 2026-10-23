import { useState } from "react";

import type { ImageKind } from "@/domain/types";
import { evaluation, haptics, sampleDepth, type CapturedShot } from "@/platform";

import { addShot, isCaseId, type EvalCaseFile, type EvalRuler } from "../model/evalCase";
import { distance, toSensor, unproject, type Intrinsics, type Norm } from "../model/ruler";

export type RulerResult = { mm: number } | { problem: string };

// The LiDAR day-1 check and the evaluation captures (roadmap F5 items 1–2): the last shot with the module's report,
// two points tapped on it for the ruler, and saving the shot into an evaluation case.
export function useDepthCheck() {
  const [shot, setShot] = useState<CapturedShot>();
  const [points, setPoints] = useState<Norm[]>([]);
  const [ruler, setRuler] = useState<RulerResult>();
  const [saved, setSaved] = useState<string>();
  const [problem, setProblem] = useState<string>();

  function take(next: CapturedShot) {
    setShot(next);
    setPoints([]);
    setRuler(undefined);
    setSaved(undefined);
    setProblem(undefined);
  }

  /** A tap on the photo as shown; the second one measures, a third starts again. */
  async function tap(p: Norm) {
    const next = points.length >= 2 ? [p] : [...points, p];
    setPoints(next);
    setRuler(undefined);
    if (next.length === 2 && shot) setRuler(await measure(shot, next[0], next[1]));
  }

  /** `knownMm`: the length the user typed for the ruler, if any. */
  function save(caseId: string, plantKey: string, kind: ImageKind, knownMm?: number) {
    if (!shot) return;
    if (!isCaseId(caseId)) {
      setProblem("Use a case ID like tomato-0007: lower-case letters, digits and dashes.");
      return;
    }
    try {
      const before = evaluation.read(caseId, "case.json");
      const deviceModel = shot.depth?.meta.device_model;
      const measured = ruler && "mm" in ruler && points.length === 2 ? { points: [points[0]!, points[1]!] as EvalRuler["points"], measured_mm: Math.round(ruler.mm * 10) / 10, ...(knownMm && knownMm > 0 ? { known_mm: knownMm } : {}) } : undefined;
      const file: EvalCaseFile = addShot(before ? JSON.parse(before) : undefined, caseId, plantKey, kind, Boolean(shot.depth), deviceModel, new Date().toISOString(), measured);
      const names = file.images[file.images.length - 1];
      evaluation.copy(caseId, shot.file.uri, names.file);
      if (shot.depth && names.depth && names.depth_meta) {
        evaluation.copy(caseId, shot.depth.depthUri, names.depth);
        // The whole DepthMeta, distortion table included: the evaluation decides what the API needs.
        evaluation.write(caseId, names.depth_meta, JSON.stringify(shot.depth.meta, null, 2));
      }
      evaluation.write(caseId, names.report, JSON.stringify(shot.report ?? {}, null, 2));
      evaluation.write(caseId, "case.json", JSON.stringify(file, null, 2));
      setSaved(`Saved as ${caseId}/${names.file}`);
      setProblem(undefined);
      void haptics.success();
    } catch (error) {
      setProblem(error instanceof Error ? error.message : "Couldn't save the shot.");
    }
  }

  return { shot, points, ruler, saved, problem, actions: { take, tap: (p: Norm) => void tap(p), save, retake: () => setShot(undefined) } };
}

async function measure(shot: CapturedShot, a: Norm, b: Norm): Promise<RulerResult> {
  const depth = shot.depth;
  const k = depth?.meta.intrinsics as Intrinsics | undefined;
  const orientation = shot.report?.photo.orientation ?? 1;
  if (!depth || !k) return { problem: "This shot has no depth." };
  const sa = toSensor(a, orientation);
  const sb = toSensor(b, orientation);
  if (!sa || !sb) return { problem: `Orientation ${orientation} can't come from the rear camera.` };
  const [da, db] = await sampleDepth(depth.depthUri, [
    [sa.x, sa.y],
    [sb.x, sb.y],
  ]);
  if (da == null || db == null) return { problem: "No depth under one of the points. Tap closer to the middle of the card." };
  return { mm: distance(unproject(sa, da, k), unproject(sb, db, k)) * 1000 };
}
