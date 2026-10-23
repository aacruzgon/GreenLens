// The top of a screen that opens on a photo (design 3.1; HIG scroll views, scroll edge effects). Over the photo, a
// dark scrim keeps the white status bar and bar buttons readable on a bright one. Once the photo has scrolled
// away, the screen's ground fades in behind the status bar (and a floating Back), and the status bar takes the
// scheme's ink.
import { useCallback, useState } from "react";
import type { NativeScrollEvent, NativeSyntheticEvent } from "react-native";
import { StatusBar } from "expo-status-bar";
import Svg, { Defs, LinearGradient, Rect, Stop } from "react-native-svg";

import { Fade } from "../primitives/Fade";
import { hiddenSvg, useSvgId } from "../svgId";
import { useTheme } from "../theme";

/** The scrim's height: the status bar and the bar buttons below it. */
const SCRIM_HEIGHT = 140;

/** A dark gradient over a photo's top, under the status bar and the bar buttons. */
export function TopScrim() {
  const id = useSvgId("scrim");
  return (
    <Svg width="100%" height={SCRIM_HEIGHT} style={{ position: "absolute", top: 0, left: 0 }} pointerEvents="none" {...hiddenSvg}>
      <Defs>
        <LinearGradient id={id} x1="0" y1="0" x2="0" y2="1">
          <Stop offset={0} stopColor="#000000" stopOpacity={0.36} />
          <Stop offset={1} stopColor="#000000" stopOpacity={0} />
        </LinearGradient>
      </Defs>
      <Rect x="0" y="0" width="100%" height="100%" fill={`url(#${id})`} />
    </Svg>
  );
}

/**
 * Whether the photo has scrolled away: past `threshold`, the scroll offset at which the content reaches the top
 * edge. Spread `scrollProps` on the ScrollView.
 */
export function useHeroScroll(threshold: number) {
  const [past, setPast] = useState(false);
  const onScroll = useCallback((e: NativeSyntheticEvent<NativeScrollEvent>) => setPast(e.nativeEvent.contentOffset.y > threshold), [threshold]);
  return { past, scrollProps: { onScroll, scrollEventThrottle: 16 } };
}

export interface TopEdgeProps {
  /** True once the photo has scrolled away. */
  past: boolean;
  /** The screen's ground. */
  color: string;
  /** The hairline under it. */
  separator: string;
  /** Its height: the status bar's, or more when a bar button floats below it. */
  height: number;
}

/** The status bar's style and, once the photo has scrolled away, the ground behind the top edge. */
export function TopEdge({ past, color, separator, height }: TopEdgeProps) {
  const { dark } = useTheme();
  return (
    <>
      <StatusBar style={past && !dark ? "dark" : "light"} />
      <Fade
        visible={past}
        style={{ position: "absolute", top: 0, left: 0, right: 0, height, backgroundColor: color, borderBottomWidth: 1, borderBottomColor: separator }}
        testID="top-edge"
      >
        {null}
      </Fade>
    </>
  );
}
