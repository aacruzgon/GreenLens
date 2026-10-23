// Line icons for the live analysis, in the home's style (gardens/components/Icons): 24 pt grid, round caps, 1.6
// stroke, from the mockup's symbols (docs/design/mockups/index.html, i-back, i-leaf, i-more). Decorative: the
// control beside or around them carries the meaning.
import Svg, { Circle, Path } from "react-native-svg";

import { hiddenSvg } from "@/ui";

export type ScanIconName = "back" | "leaf" | "more" | "check";

export function ScanIcon({ name, color, size = 22 }: { name: ScanIconName; color: string; size?: number }) {
  const line = { stroke: color, strokeWidth: 1.6, strokeLinecap: "round" as const, strokeLinejoin: "round" as const, fill: "none" };
  return (
    <Svg width={size} height={size} viewBox="0 0 24 24" {...hiddenSvg}>
      {name === "back" ? <Path d="M15 5l-7 7 7 7" {...line} strokeWidth={2} /> : null}
      {name === "leaf" ? (
        <>
          <Path d="M5 19c0-8.5 5.5-13.5 14-14 0 9-5.5 14-14 14z" {...line} />
          <Path d="M5 19l7.5-7.5" {...line} />
        </>
      ) : null}
      {name === "more" ? (
        <>
          <Circle cx={6} cy={12} r={1.4} fill={color} />
          <Circle cx={12} cy={12} r={1.4} fill={color} />
          <Circle cx={18} cy={12} r={1.4} fill={color} />
        </>
      ) : null}
      {name === "check" ? <Path d="M5 12.5l4.5 4.5L19 7.5" {...line} strokeWidth={2.4} /> : null}
    </Svg>
  );
}
