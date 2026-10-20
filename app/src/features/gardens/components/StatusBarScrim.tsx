// A strip of the page's background behind the status bar, shown once the hero photo has scrolled away, so the
// clock and battery never sit on a card. It fades out over its last 12 pt, like iOS's soft scroll edge effect
// (HIG Scroll views: scroll edge effects). Over the photo it is hidden: the light status bar reads on the photo.
import Svg, { Defs, LinearGradient, Rect, Stop } from "react-native-svg";

import { Fade, hiddenSvg, useSvgId, useTheme } from "@/ui";

const FADE = 12;

export interface StatusBarScrimProps {
  /** The status bar's height. */
  topInset: number;
  visible: boolean;
}

export function StatusBarScrim({ topInset, visible }: StatusBarScrimProps) {
  const { colors } = useTheme();
  const id = useSvgId("scrim");
  return (
    <Fade
      visible={visible}
      testID="status-bar-scrim"
      style={{ position: "absolute", top: 0, left: 0, right: 0, height: topInset + FADE }}
    >
      <Svg width="100%" height="100%" pointerEvents="none" {...hiddenSvg}>
        <Defs>
          <LinearGradient id={id} x1="0" y1="0" x2="0" y2="1">
            <Stop offset="0" stopColor={colors.background} />
            <Stop offset={topInset / (topInset + FADE)} stopColor={colors.background} />
            <Stop offset="1" stopColor={colors.background} stopOpacity={0} />
          </LinearGradient>
        </Defs>
        <Rect x="0" y="0" width="100%" height="100%" fill={`url(#${id})`} />
      </Svg>
    </Fade>
  );
}
