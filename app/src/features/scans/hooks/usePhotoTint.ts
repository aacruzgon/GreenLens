// The diagnosis's tint from its photo (D9: on the phone). The hue is cached per image for the session, so going
// back to a result doesn't sample again. Until the hue arrives, and when the photo can't be read, the ground is the
// wash, which the tint replaces without a jump in lightness (both sit at the same OKLCH L).
import { useQuery } from "@tanstack/react-query";

import { imageSource } from "@/api";
import type { ScanImage } from "@/domain/types";
import { samplePixels } from "@/platform";
import { dominantHue, photoTint, useTheme, type PhotoTintColors } from "@/ui";

export function usePhotoTint(image: Pick<ScanImage, "id" | "url"> | undefined): PhotoTintColors {
  const { scheme } = useTheme();
  const hue = useQuery({
    queryKey: ["photoTint", image?.id ?? ""],
    queryFn: async () => {
      const src = imageSource(image?.url, image?.id);
      if (src === null) return null;
      const source = typeof src === "number" ? src : src.uri;
      if (!source) return null;
      return dominantHue((await samplePixels(source, image?.id ?? "photo")).data) ?? null;
    },
    enabled: Boolean(image?.id),
    staleTime: Infinity,
    gcTime: Infinity,
    retry: false,
  });
  return photoTint(hue.data ?? undefined, scheme);
}
