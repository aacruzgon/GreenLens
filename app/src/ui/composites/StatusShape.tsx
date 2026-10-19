import Svg, { Circle, Path, Rect } from "react-native-svg";

import type { PlantStatus } from "@/domain/types";

import { useTheme } from "../theme";

/** Status is a word, a colour and a shape (design 6): healthy ●, watch ▲, sick ◆, dead □. */
export function StatusShape({ status, size = 10, color }: { status: PlantStatus; size?: number; color?: string }) {
  const t = useTheme();
  const fill = color ?? t.colors[status];
  return (
    <Svg width={size} height={size} viewBox="0 0 10 10" accessibilityElementsHidden importantForAccessibility="no-hide-descendants">
      {status === "healthy" ? (
        <Circle cx={5} cy={5} r={4.5} fill={fill} />
      ) : status === "watch" ? (
        <Path d="M5 0.6 L9.6 9.2 L0.4 9.2 Z" fill={fill} />
      ) : status === "sick" ? (
        <Path d="M5 0 L10 5 L5 10 L0 5 Z" fill={fill} />
      ) : (
        <Rect x={1} y={1} width={8} height={8} fill="none" stroke={fill} strokeWidth={1.8} />
      )}
    </Svg>
  );
}
