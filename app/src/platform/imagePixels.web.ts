// The web build draws the photo onto a small canvas and reads it back. A signed URL on another origin needs CORS
// from the image host for getImageData; without it the read throws and the diagnosis keeps the wash.
import { Asset } from "expo-asset";
import type { ImageSource } from "expo-image";

import type { Pixels } from "@/lib/png";

export const SAMPLE_WIDTH = 32;

function load(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error("the photo didn't load"));
    img.src = src;
  });
}

export async function samplePixels(source: number | ImageSource): Promise<Pixels> {
  const src = typeof source === "number" ? Asset.fromModule(source).uri : source.uri;
  if (!src) throw new Error("no photo");
  const img = await load(src);
  const width = SAMPLE_WIDTH;
  const height = Math.max(1, Math.round((img.naturalHeight / img.naturalWidth) * width));
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("no 2D canvas");
  ctx.drawImage(img, 0, 0, width, height);
  const { data } = ctx.getImageData(0, 0, width, height);
  return { width, height, data: new Uint8Array(data.buffer) };
}
