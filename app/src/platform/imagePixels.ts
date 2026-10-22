// A photo cut down to a few dozen pixels a side, as RGBA, for the diagnosis's photo tint (D9: computed on the
// phone). expo-image-manipulator needs a local file, so a bundled mock photo is resolved through expo-asset and
// a signed URL is downloaded to the cache first. Works in Expo Go: nothing here needs a development build.
import { Asset } from "expo-asset";
import { File, Paths } from "expo-file-system";
import { ImageManipulator, SaveFormat } from "expo-image-manipulator";

import { base64ToBytes, decodePng, type Pixels } from "@/lib/png";

/** The sample's width; enough for a hue, small enough to decode in a few milliseconds. */
export const SAMPLE_WIDTH = 32;

async function localUri(source: number | string, key: string): Promise<{ uri: string; temp?: File }> {
  if (typeof source === "number") {
    const asset = await Asset.fromModule(source).downloadAsync();
    return { uri: asset.localUri ?? asset.uri };
  }
  if (/^https?:/.test(source)) {
    const temp = new File(Paths.cache, `tint-${key.replace(/[^\w-]/g, "_")}`);
    await File.downloadFileAsync(source, temp, { idempotent: true });
    return { uri: temp.uri, temp };
  }
  return { uri: source };
}

/** @param key a stable name for the photo (its image ID), for the temporary download. */
export async function samplePixels(source: number | string, key: string): Promise<Pixels> {
  const { uri, temp } = await localUri(source, key);
  try {
    const image = await ImageManipulator.manipulate(uri).resize({ width: SAMPLE_WIDTH }).renderAsync();
    const saved = await image.saveAsync({ format: SaveFormat.PNG, base64: true });
    if (!saved.base64) throw new Error("no pixels from the image manipulator");
    return decodePng(base64ToBytes(saved.base64));
  } finally {
    if (temp?.exists) temp.delete();
  }
}
