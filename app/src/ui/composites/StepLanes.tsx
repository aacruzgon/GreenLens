// What was done, and when (analytics 2, P4; styles.css .lanes): a marker on the time axis at each date a plan step
// was done, lined up with the chart above it when both take the same `domain`, and a list of the steps with their
// dates. The markers are decoration; the list is what VoiceOver reads.
import { useState } from "react";
import { View, type StyleProp, type ViewStyle } from "react-native";
import Svg, { Polygon } from "react-native-svg";

import { Text } from "../primitives/Text";
import { hiddenSvg } from "../svgId";
import { useTheme } from "../theme";
import { space } from "../tokens";
import { timeDomain, timeScale } from "./chart";

export interface StepLane {
  key: string;
  title: string;
  /** Each time it was done, epoch ms. */
  times: readonly number[];
  /** The dates in words: "Oct 14, 17". */
  dates: string;
}

export interface StepLanesProps {
  steps: readonly StepLane[];
  /** The chart's domain, so the markers sit under its dates. */
  domain?: readonly [number, number];
  style?: StyleProp<ViewStyle>;
  testID?: string;
}

const STRIP = 16;

function Diamond({ cx, cy, r, fill }: { cx: number; cy: number; r: number; fill: string }) {
  return <Polygon points={`${cx},${cy - r} ${cx + r},${cy} ${cx},${cy + r} ${cx - r},${cy}`} fill={fill} />;
}

export function StepLanes({ steps, domain, style, testID }: StepLanesProps) {
  const { colors } = useTheme();
  const [width, setWidth] = useState(0);
  const times = steps.flatMap((s) => s.times);
  const x = timeScale(domain ?? timeDomain(times), width);
  return (
    <View style={[{ gap: space[2] }, style]} testID={testID}>
      <View onLayout={(e) => setWidth(e.nativeEvent.layout.width)} style={{ height: STRIP }} accessible={false}>
        {width ? (
          <Svg width={width} height={STRIP} {...hiddenSvg}>
            {times.map((t, i) => (
              <Diamond key={`${t}-${i}`} cx={x(t)} cy={STRIP / 2} r={5} fill={colors.accent} />
            ))}
          </Svg>
        ) : null}
      </View>
      <View style={{ gap: space[1] }}>
        {steps.map((s) => (
          <View key={s.key} style={{ flexDirection: "row", alignItems: "center", gap: space[2] }} accessible accessibilityLabel={`${s.title}, done ${s.dates}`}>
            <Svg width={12} height={12} {...hiddenSvg}>
              <Diamond cx={6} cy={6} r={5} fill={colors.accent} />
            </Svg>
            <Text variant="footnote" color="textMuted" style={{ minWidth: 72, fontVariant: ["tabular-nums"] }}>
              {s.dates}
            </Text>
            <Text variant="footnote" style={{ flex: 1 }}>
              {s.title}
            </Text>
          </View>
        ))}
      </View>
    </View>
  );
}
