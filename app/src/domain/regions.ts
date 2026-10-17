import type { BBox } from "./types";

/** A normalised [x, y, w, h] box (contracts 1, regions) in a view of the given size. */
export function toViewRect(bbox: BBox, viewWidth: number, viewHeight: number) {
  const [x = 0, y = 0, w = 0, h = 0] = bbox;
  return { x: x * viewWidth, y: y * viewHeight, width: w * viewWidth, height: h * viewHeight };
}
