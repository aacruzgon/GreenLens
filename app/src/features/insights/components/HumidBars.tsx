// Hours over 90% humidity in the week before each scan (analytics 3, G4; the mockup's "Weather before the
// diagnosis"): a bar per scan from zero, the diagnosis' own in the watch colour, the hours on each. Hidden from
// VoiceOver behind its summary. Props only.
import { useState } from "react";
import { View } from "react-native";
import Svg, { G, Line, Rect, Text as SvgText } from "react-native-svg";

import { hiddenSvg, useTheme } from "@/ui";

import type { HumidBar } from "../model/insights";

const HEIGHT = 136;
const BASE = 116;
const TOP = 24;

export function HumidBars({ bars, summary }: { bars: readonly HumidBar[]; summary: string }) {
  const { colors } = useTheme();
  const [width, setWidth] = useState(0);
  const max = Math.max(24, ...bars.map((b) => b.hours));
  const slot = bars.length ? (width - 20) / bars.length : 0;
  const barW = Math.min(40, slot * 0.6);
  return (
    <View accessible accessibilityRole="image" accessibilityLabel={summary} onLayout={(e) => setWidth(e.nativeEvent.layout.width)} style={{ height: HEIGHT }} testID="humid-bars">
      {width ? (
        <Svg width={width} height={HEIGHT} {...hiddenSvg}>
          <Line x1={20} x2={width} y1={BASE} y2={BASE} stroke={colors.border} strokeWidth={1} />
          {bars.map((b, i) => {
            const cx = 20 + slot * (i + 0.5);
            const h = Math.max(4, (b.hours / max) * (BASE - TOP));
            return (
              <G key={b.key}>
                <Rect x={cx - barW / 2} y={BASE - h} width={barW} height={h} rx={5} fill={b.highlight ? colors.watch : colors.surfaceMuted} stroke={b.highlight ? undefined : colors.border} />
                <SvgText x={cx} y={BASE - h - 6} fontSize={12} fontWeight="700" fill={colors.text} textAnchor="middle">
                  {`${b.hours} h`}
                </SvgText>
                <SvgText x={cx} y={HEIGHT - 4} fontSize={11} fill={colors.textMuted} textAnchor="middle">
                  {b.date}
                </SvgText>
              </G>
            );
          })}
        </Svg>
      ) : null}
    </View>
  );
}
