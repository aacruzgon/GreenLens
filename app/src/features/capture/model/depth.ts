// The depth parts a LiDAR shot uploads (contracts 3, POST /scans/{id}/images; architecture 6.7). Pure.
import type { DepthMeta } from "@/domain/depth";
import type { ImageKind } from "@/domain/types";

/**
 * Whether `DepthMeta.distortion` goes up. The contract sends it only if the LiDAR day-1 check finds the photo and
 * its depth distorted (contracts open question 11); until the check is recorded, it stays on the phone. The module
 * reads the table on every shot either way, so the day-1 check can see it.
 */
export const SEND_DISTORTION = false;

/** Kinds whose depth the API keeps: every plant and soil kind. A guided check's depth is dropped, so it isn't sent. */
export function sendsDepth(kind: ImageKind): boolean {
  return kind !== "squeeze_test" && kind !== "jar_test";
}

/** The `DepthMeta` as uploaded: without the distortion table unless the day-1 check asks for it. */
export function uploadMeta(meta: DepthMeta, sendDistortion = SEND_DISTORTION): DepthMeta {
  if (sendDistortion || !meta.distortion) return meta;
  const { distortion: _, ...rest } = meta;
  return rest;
}
