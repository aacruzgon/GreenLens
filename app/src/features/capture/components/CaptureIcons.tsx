// Line icons for capture, in the home's style (gardens/components/Icons): 24 pt grid, round caps, 1.6 stroke.
// Decorative: the control or text beside them carries the meaning.
import Svg, { Circle, Path, Rect } from "react-native-svg";

import type { QualityHint } from "@/domain/types";
import { hiddenSvg } from "@/ui";

export type CaptureIconName = "close" | "library" | QualityHint;

export function CaptureIcon({ name, color, size = 22 }: { name: CaptureIconName; color: string; size?: number }) {
  const line = { stroke: color, strokeWidth: 1.6, strokeLinecap: "round" as const, strokeLinejoin: "round" as const, fill: "none" };
  return (
    <Svg width={size} height={size} viewBox="0 0 24 24" {...hiddenSvg}>
      {name === "close" ? <Path d="M6 6l12 12M18 6L6 18" {...line} strokeWidth={2} /> : null}
      {name === "library" ? (
        <>
          <Rect x={3.5} y={5} width={17} height={14} rx={3} {...line} />
          <Path d="M3.5 16l4.5-4.5 3.5 3.5 3-3 6 6" {...line} />
          <Circle cx={15.5} cy={9.5} r={1.5} {...line} />
        </>
      ) : null}
      {/* tap_to_focus: the focus square with a point at its centre. */}
      {name === "tap_to_focus" ? (
        <>
          <Path d="M4 9V5.5A1.5 1.5 0 0 1 5.5 4H9M15 4h3.5A1.5 1.5 0 0 1 20 5.5V9M20 15v3.5a1.5 1.5 0 0 1-1.5 1.5H15M9 20H5.5A1.5 1.5 0 0 1 4 18.5V15" {...line} />
          <Circle cx={12} cy={12} r={1.6} fill={color} />
        </>
      ) : null}
      {/* more_light: a sun. */}
      {name === "more_light" ? (
        <>
          <Circle cx={12} cy={12} r={4} {...line} />
          <Path d="M12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4M5.3 18.7l1.4-1.4M17.3 6.7l1.4-1.4" {...line} />
        </>
      ) : null}
      {/* avoid_glare: a sun half behind a shade. */}
      {name === "avoid_glare" ? (
        <>
          <Path d="M8 14a4 4 0 0 1 8 0" {...line} />
          <Path d="M12 5v2M5.5 8.5l1.4 1.4M18.5 8.5l-1.4 1.4M3 18h18" {...line} />
        </>
      ) : null}
    </Svg>
  );
}
