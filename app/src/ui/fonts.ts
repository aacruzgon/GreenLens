// The display face: Urbanist 600, content heroes only, 22 pt and above (design.md 3.1).
// app.config.ts embeds the file through the expo-font config plugin for development and store builds;
// useFonts also loads it at run time, so it works in Expo Go where the plugin doesn't apply.
import { Urbanist_600SemiBold } from "@expo-google-fonts/urbanist/600SemiBold";
import { useFonts } from "expo-font";

/** The family name the display styles use once the font is loaded. */
export const DISPLAY_FAMILY = "Urbanist_600SemiBold";

/** Loads Urbanist. Returns true once it is ready; false while loading or if it failed. */
export function useDisplayFontLoaded(): boolean {
  const [loaded, error] = useFonts({ [DISPLAY_FAMILY]: Urbanist_600SemiBold });
  return loaded && !error;
}

/**
 * The font family for a display style: Urbanist when loaded, otherwise undefined, which leaves
 * React Native's system font (SF Pro) in place at weight 600.
 */
export function displayFamily(loaded: boolean): string | undefined {
  return loaded ? DISPLAY_FAMILY : undefined;
}
