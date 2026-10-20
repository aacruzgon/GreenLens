// Line icons for the garden home, drawn from the mockup's symbols (docs/design/mockups/index.html, i-*)
// at Conservatory's 1.6 stroke (directions.js shape.iconStroke). Decorative: the text beside them carries the meaning.
import Svg, { Circle, Path } from "react-native-svg";

import type { WeatherKind } from "../model/weather";

const PATHS: Record<WeatherKind | "leaf" | "plus" | "chevron", string[]> = {
  cloud: ["M7 18h10.5a4 4 0 0 0 .4-8 6 6 0 0 0-11.5 1.6A3.3 3.3 0 0 0 7 18z"],
  rain: ["M7 15h10.5a4 4 0 0 0 .4-8 6 6 0 0 0-11.5 1.6A3.3 3.3 0 0 0 7 15z", "M8.5 18l-1 2.5M12.5 18l-1 2.5M16.5 18l-1 2.5"],
  sun: ["M12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4M5.3 18.7l1.4-1.4M17.3 6.7l1.4-1.4"],
  partly: ["M9 5.5a4 4 0 0 1 6.2 2.6", "M5 9.5l1 .6M9 2.8v1.4M3.3 6l1.2.8", "M8 20h9.5a3.6 3.6 0 0 0 .3-7.2 5.4 5.4 0 0 0-10.3 1.5A3 3 0 0 0 8 20z"],
  snow: ["M12 3v18M4.2 7.5l15.6 9M4.2 16.5l15.6-9", "M9.5 4.5L12 7l2.5-2.5M9.5 19.5L12 17l2.5 2.5"],
  leaf: ["M5 19c0-8.5 5.5-13.5 14-14 0 9-5.5 14-14 14z", "M5 19l7.5-7.5"],
  plus: ["M12 5v14M5 12h14"],
  chevron: ["M6 9l6 6 6-6"],
};

export type IconName = keyof typeof PATHS;

export function Icon({ name, color, size = 20, strokeWidth = 1.6 }: { name: IconName; color: string; size?: number; strokeWidth?: number }) {
  return (
    <Svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke={color}
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      testID={`icon-${name}`}
    >
      {name === "sun" ? <Circle cx={12} cy={12} r={4} /> : null}
      {PATHS[name].map((d) => (
        <Path key={d} d={d} />
      ))}
    </Svg>
  );
}
