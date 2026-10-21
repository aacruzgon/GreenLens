// Liquid Glass for the controls on the photo (design 3.1): drawn only where iOS draws it, and not with Reduce
// Transparency on, since iOS makes its own bars opaque then. Elsewhere the control keeps its solid pill.
import { isLiquidGlassAvailable } from "expo-glass-effect";
import { useEffect, useState } from "react";
import { AccessibilityInfo } from "react-native";

function glassAvailable(): boolean {
  try {
    return isLiquidGlassAvailable();
  } catch {
    // No native module: the web build, an older Expo Go or Jest.
    return false;
  }
}

/** The system's Reduce Transparency setting, kept current while the app runs. */
export function useSystemReducedTransparency(): boolean {
  const [reduced, setReduced] = useState(false);
  useEffect(() => {
    let alive = true;
    AccessibilityInfo.isReduceTransparencyEnabled()
      .then((value) => {
        if (alive) setReduced(value);
      })
      .catch(() => {});
    const sub = AccessibilityInfo.addEventListener("reduceTransparencyChanged", setReduced);
    return () => {
      alive = false;
      sub.remove();
    };
  }, []);
  return reduced;
}

/** Whether to draw a control as Liquid Glass rather than its solid fallback. */
export function useGlass(): boolean {
  const reduced = useSystemReducedTransparency();
  return glassAvailable() && !reduced;
}
