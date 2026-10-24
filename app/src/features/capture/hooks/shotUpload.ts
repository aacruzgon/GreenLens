import type { ShotUpload } from "@/api";
import type { ImageKind } from "@/domain/types";
import { files } from "@/platform";

import { sendsDepth, uploadMeta } from "../model/depth";
import type { ShotFile } from "../model/slot";

/** One shot's multipart parts: the photo, and on a LiDAR phone its depth map and `DepthMeta` (contracts 3). */
export async function shotUpload(kind: ImageKind, file: ShotFile, questionId?: string): Promise<ShotUpload> {
  const part: ShotUpload = { kind, file: await files.toUploadPart(file.uri), ...(questionId ? { questionId } : {}) };
  if (!file.depth || !sendsDepth(kind)) return part;
  return { ...part, depth: await files.toUploadPart(file.depth.uri), depthMeta: uploadMeta(file.depth.meta) };
}
