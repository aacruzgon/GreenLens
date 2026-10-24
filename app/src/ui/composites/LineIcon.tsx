// Line icons for the diagnosis, the plan, the plant and the Tasks tab, drawn from the mockup's symbols
// (docs/design/mockups/index.html, i-*) at Conservatory's 1.6 stroke. Circles are written as arcs so every icon
// is a list of paths. Decorative: the control or text beside them carries the meaning.
import Svg, { Path } from "react-native-svg";

import { hiddenSvg } from "../svgId";

const PATHS = {
  back: ["M15 5l-7 7 7 7"],
  map: ["M3.5 6.5l5.5-2 6 2 5.5-2v13l-5.5 2-6-2-5.5 2z", "M9 4.5v13M15 6.5v13"],
  play: ["M8 5.2v13.6L18.6 12z"],
  chat: ["M4 5.5h16v11H10l-5 4v-4H4z"],
  x: ["M6.5 6.5l11 11M17.5 6.5l-11 11"],
  bug: ["M6.5 13.5a5.5 6.5 0 1 0 11 0a5.5 6.5 0 1 0 -11 0", "M12 7v13M9 4.5l1.5 2.5M15 4.5L13.5 7M6.5 11H4M20 11h-2.5M6.5 16H4M20 16h-2.5"],
  book: ["M4 5h5.5A2.5 2.5 0 0 1 12 7.5V19a2 2 0 0 0-2-2H4z", "M20 5h-5.5A2.5 2.5 0 0 0 12 7.5V19a2 2 0 0 1 2-2h6z"],
  info: ["M3 12a9 9 0 1 0 18 0a9 9 0 1 0 -18 0", "M12 11v5.5M12 7.8v.4"],
  repeat: ["M17 3.5l3 3-3 3M4 11V9.5a3 3 0 0 1 3-3h13M7 20.5l-3-3 3-3M20 13v1.5a3 3 0 0 1-3 3H4"],
  rain: ["M7 15h10.5a4 4 0 0 0 .4-8 6 6 0 0 0-11.5 1.6A3.3 3.3 0 0 0 7 15z", "M8.5 18l-1 2.5M12.5 18l-1 2.5M16.5 18l-1 2.5"],
  flask: ["M9.5 3.5h5M10.5 3.5v5.5L5 18.5A1.6 1.6 0 0 0 6.4 21h11.2a1.6 1.6 0 0 0 1.4-2.5L13.5 9V3.5", "M7.5 15h9"],
  hand: ["M8 13V6.5a1.5 1.5 0 0 1 3 0V12M11 11V5a1.5 1.5 0 0 1 3 0v6M14 11V6.5a1.5 1.5 0 0 1 3 0V14c0 4-2.5 6.5-6 6.5-2.5 0-4-1.2-5.2-3.2L4 13.8a1.4 1.4 0 0 1 2.3-1.5L8 14"],
  pin: ["M12 21s-6.5-6-6.5-11a6.5 6.5 0 0 1 13 0c0 5-6.5 11-6.5 11z", "M9.7 10a2.3 2.3 0 1 0 4.6 0a2.3 2.3 0 1 0 -4.6 0"],
  drop: ["M12 3.5c4 4.8 6 8 6 10.8a6 6 0 0 1-12 0c0-2.8 2-6 6-10.8z"],
  check: ["M5 12.5l4.5 4.5L19 7.5"],
  plus: ["M12 5v14M5 12h14"],
  camera: ["M3.5 8h3.2l1.8-2.6h7l1.8 2.6h3.2v11h-17z", "M8.4 13.2a3.6 3.6 0 1 0 7.2 0a3.6 3.6 0 1 0 -7.2 0"],
  chevronRight: ["M9 6l6 6-6 6"],
  send: ["M12 19V5.5M6 11.5l6-6 6 6"],
  thumbUp: ["M7.5 10.5v9h-3v-9z", "M7.5 10.5l3.5-6.5a2 2 0 0 1 2.6 2.3L13 9.5h5.3a1.8 1.8 0 0 1 1.7 2.2l-1.6 6.3a2 2 0 0 1-1.9 1.5H7.5"],
  thumbDown: ["M7.5 13.5v-9h-3v9z", "M7.5 13.5l3.5 6.5a2 2 0 0 0 2.6-2.3L13 14.5h5.3a1.8 1.8 0 0 0 1.7-2.2l-1.6-6.3a2 2 0 0 0-1.9-1.5H7.5"],
  snow: ["M12 3v18M4.2 7.5l15.6 9M4.2 16.5l15.6-9", "M9.5 4.5L12 6.5l2.5-2M9.5 19.5l2.5-2 2.5 2"],
  ruler: ["M3.5 15.5l12-12 5 5-12 12z", "M7.5 11.5l2 2M10.5 8.5l2 2M13.5 5.5l2 2"],
  external: ["M14 4.5h5.5V10M19.5 4.5L11 13", "M17 14v5.5H4.5V7H10"],
  leaf: ["M5 19c0-8.5 5.5-13.5 14-14 0 9-5.5 14-14 14z", "M5 19l7.5-7.5"],
} as const satisfies Record<string, readonly string[]>;

export type LineIconName = keyof typeof PATHS;

export interface LineIconProps {
  name: LineIconName;
  color: string;
  size?: number;
  strokeWidth?: number;
}

export function LineIcon({ name, color, size = 20, strokeWidth = 1.6 }: LineIconProps) {
  const filled = name === "play";
  return (
    <Svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill={filled ? color : "none"}
      stroke={color}
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      {...hiddenSvg}
      testID={`line-icon-${name}`}
    >
      {PATHS[name].map((d) => (
        <Path key={d} d={d} />
      ))}
    </Svg>
  );
}
