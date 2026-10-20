import type { ImageSource } from "expo-image";

import { resolveMockImage } from "./mock/images";

// Every image the app shows comes with a signed URL on its record (README 9, signed image URLs).
// The disk cache is keyed by something that survives re-signing: the image ID when the record has
// one, otherwise the URL's path without its query. Mock mode's `mock://` URLs resolve to bundled files.
export function imageSource(url: string | null | undefined, imageId?: string | null): ImageSource | number | null {
  if (!url) return null;
  const bundled = resolveMockImage(url);
  if (bundled !== undefined) return bundled;
  return { uri: url, cacheKey: imageId ?? url.split("?")[0] };
}
