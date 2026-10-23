// A photo cut down to a few dozen pixels a side, as RGBA, for the diagnosis's photo tint (D9: computed on the
// phone). expo-image loads the photo through the same disk cache, under the same cache key, as the photo on screen,
// so a signed URL is downloaded once for both; the image manipulator then shrinks it. Works in Expo Go: nothing here
// needs a development build.
import { Image, type ImageSource } from "expo-image";
import { ImageManipulator, SaveFormat } from "expo-image-manipulator";

import { base64ToBytes, decodePng, type Pixels } from "@/lib/png";

/** The sample's width; enough for a hue, small enough to decode in a few milliseconds. */
export const SAMPLE_WIDTH = 32;

/** @param source a bundled photo, or a signed URL with its cache key (`imageSource`). */
export async function samplePixels(source: number | ImageSource): Promise<Pixels> {
  const ref = await Image.loadAsync(source);
  const image = await ImageManipulator.manipulate(ref).resize({ width: SAMPLE_WIDTH }).renderAsync();
  const saved = await image.saveAsync({ format: SaveFormat.PNG, base64: true });
  if (!saved.base64) throw new Error("no pixels from the image manipulator");
  return decodePng(base64ToBytes(saved.base64));
}
