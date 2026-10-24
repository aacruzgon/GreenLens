// Depth beyond the upload (roadmap F5 items 1–2): reading a depth map back for the day-1 check's ruler, and the
// evaluation captures the vision track takes off the phone (evaluation, the set and its files). The captures go to
// Documents/evaluation, which the Files app shows (UIFileSharingEnabled), so they leave the phone by AirDrop or a
// cable, never through the API.
import { Directory, File, Paths } from "expo-file-system";

import { sampleDepth as sample } from "@modules/lidar/src/LidarCamera";

/** The median depth in metres around normalised sensor-frame points of a depth PNG; null where there is none. */
export function sampleDepth(uri: string, points: [number, number][], halfSide = 2): Promise<(number | null)[]> {
  return sample(uri, points, halfSide);
}

const root = () => new Directory(Paths.document, "evaluation");

export const evaluation = {
  /** Copies a file into a case's folder under `name`. */
  copy(caseId: string, uri: string, name: string): void {
    const dir = new Directory(root(), caseId);
    dir.create({ intermediates: true, idempotent: true });
    const target = new File(dir, name);
    if (target.exists) target.delete();
    new File(uri).copySync(target);
  },

  /** Writes a text file (JSON) into a case's folder. */
  write(caseId: string, name: string, text: string): void {
    const dir = new Directory(root(), caseId);
    dir.create({ intermediates: true, idempotent: true });
    const target = new File(dir, name);
    if (target.exists) target.delete();
    target.create();
    target.write(text);
  },

  /** A case's saved text file, or undefined. */
  read(caseId: string, name: string): string | undefined {
    const f = new File(new Directory(root(), caseId), name);
    return f.exists ? f.textSync() : undefined;
  },
};
