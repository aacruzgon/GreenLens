import { useId } from "react";
import { Platform } from "react-native";

/** A gradient id unique to this component, safe inside url(#…). */
export function useSvgId(prefix: string): string {
  return `${prefix}-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
}

/**
 * Hides a decorative Svg from assistive technology. react-native-svg hands its props to the DOM on the web,
 * where only aria-hidden is a valid attribute (React warns about the native props there).
 */
export const hiddenSvg =
  Platform.OS === "web"
    ? ({ "aria-hidden": true } as const)
    : ({ accessibilityElementsHidden: true, importantForAccessibility: "no-hide-descendants" } as const);
