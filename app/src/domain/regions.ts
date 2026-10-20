// Normalised boxes to view coordinates (contracts 1, regions).
import type { BBox } from "./types";

export type Size = { width: number; height: number };
export type Rect = { x: number; y: number; width: number; height: number };

/** Where a photo of `image` size lands inside a `view` when it is scaled to fit or to fill. */
export function fitRect(image: Size, view: Size, mode: "contain" | "cover" = "contain"): Rect {
  if (image.width <= 0 || image.height <= 0) return { x: 0, y: 0, width: view.width, height: view.height };
  const sx = view.width / image.width;
  const sy = view.height / image.height;
  const scale = mode === "contain" ? Math.min(sx, sy) : Math.max(sx, sy);
  const width = image.width * scale;
  const height = image.height * scale;
  return { x: (view.width - width) / 2, y: (view.height - height) / 2, width, height };
}

const clamp01 = (n: number) => Math.min(1, Math.max(0, n));

/** `[x, y, w, h]` in 0..1 of the image, to a rectangle within the drawn image `rect`. */
export function bboxToView(bbox: BBox | readonly number[], rect: Rect): Rect {
  const [x = 0, y = 0, w = 0, h = 0] = bbox;
  const x0 = clamp01(x);
  const y0 = clamp01(y);
  const x1 = clamp01(x + w);
  const y1 = clamp01(y + h);
  return {
    x: rect.x + x0 * rect.width,
    y: rect.y + y0 * rect.height,
    width: (x1 - x0) * rect.width,
    height: (y1 - y0) * rect.height,
  };
}

/** Convenience: the image's stored size and the view's size straight to a view rectangle. */
export function regionToView(
  bbox: BBox | readonly number[],
  image: Size,
  view: Size,
  mode: "contain" | "cover" = "contain",
): Rect {
  return bboxToView(bbox, fitRect(image, view, mode));
}
