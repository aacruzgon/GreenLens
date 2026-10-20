// The status shapes (design.md 6): healthy ● circle, watch ▲ triangle, sick ◆ diamond, dead □ hollow square.
// They carry the difference F0's colours don't. Proportions from styles.css (.status::before, .sh-*).
// Decorative: the word next to it is what assistive technology reads.
import Svg, { Circle, Polygon, Rect } from "react-native-svg";

import { useTheme } from "../theme";
import { statusRoles, type PlantStatus } from "../tokens";

export const STATUS_SHAPE: Record<PlantStatus, "circle" | "triangle" | "diamond" | "square"> = {
  healthy: "circle",
  watch: "triangle",
  sick: "diamond",
  dead: "square",
};

export interface StatusShapeProps {
  status: PlantStatus;
  /** Side of the square it is drawn in, in pt (default 9). */
  size?: number;
  /** Overrides the status colour, such as white on a photo. */
  color?: string;
}

export function StatusShape({ status, size = 9, color }: StatusShapeProps) {
  const { colors } = useTheme();
  const fill = color ?? colors[statusRoles[status].fg];
  const shape = STATUS_SHAPE[status];
  return (
    <Svg
      width={size}
      height={size}
      viewBox="0 0 10 10"
      testID={`status-shape-${shape}`}
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
    >
      {shape === "circle" ? <Circle cx={5} cy={5} r={4.2} fill={fill} /> : null}
      {shape === "triangle" ? <Polygon points="5,0.6 9.8,9.2 0.2,9.2" fill={fill} /> : null}
      {shape === "diamond" ? <Polygon points="5,0.4 9.6,5 5,9.6 0.4,5" fill={fill} /> : null}
      {shape === "square" ? <Rect x={1.5} y={1.5} width={7} height={7} rx={1} fill="none" stroke={fill} strokeWidth={1.8} /> : null}
    </Svg>
  );
}
