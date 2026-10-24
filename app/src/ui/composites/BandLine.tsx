// A leaf quantity over the checks (analytics 2, P3; styles.css .chart .band): each point with its min–max bar over
// the measured leaves and its value, joined by a line when the caller's rule allows one, with dashed ticks at the
// dates something was done (the plan's steps, as `StepLanes` lists them). Hidden from VoiceOver behind
// `accessibilityLabel`, which says the values and ranges (HIG Charts, Enhancing the accessibility of a chart).
import { useState } from "react";
import { View, type StyleProp, type ViewStyle } from "react-native";
import Svg, { Circle, Line, Polyline, Rect, Text as SvgText } from "react-native-svg";

import { hiddenSvg } from "../svgId";
import { useTheme } from "../theme";
import { CHART_PAD, dateAnchor, fittingLabels, niceTicks, timeDomain, timeScale } from "./chart";

export interface BandPoint {
  key: string;
  /** Epoch ms. */
  t: number;
  value: number;
  /** The range over the measured items, when known. */
  min?: number;
  max?: number;
}

export interface BandLineProps {
  points: readonly BandPoint[];
  accessibilityLabel: string;
  /** Joins the points. P3's rule is the caller's: 3 points, the last range below the first. */
  line: boolean;
  /** Dashed ticks on the time axis, epoch ms. */
  marks?: readonly number[];
  formatTick: (value: number) => string;
  formatValue: (value: number) => string;
  formatDate: (t: number) => string;
  /** The lower bound; 0 by default, as for a count or a percentage. */
  yMin?: number;
  domain?: readonly [number, number];
  height?: number;
  style?: StyleProp<ViewStyle>;
  testID?: string;
}

const DATES = 20;
const TOP = 12;
const BAR = 18;

export function BandLine({ points, accessibilityLabel, line, marks = [], formatTick, formatValue, formatDate, yMin = 0, domain, height = 150, style, testID }: BandLineProps) {
  const { colors } = useTheme();
  const [width, setWidth] = useState(0);
  const x = timeScale(domain ?? timeDomain(points.map((p) => p.t), marks), width);
  const ticks = niceTicks(yMin, Math.max(...points.map((p) => Math.max(p.value, p.max ?? p.value))));
  const lo = ticks[0]!;
  const hi = ticks.at(-1)!;
  const bottom = height - DATES;
  const y = (v: number) => bottom - ((v - lo) / (hi - lo || 1)) * (bottom - TOP);
  const xs = points.map((p) => x(p.t));
  const labels = fittingLabels(xs);

  return (
    <View
      accessible
      accessibilityRole="image"
      accessibilityLabel={accessibilityLabel}
      onLayout={(e) => setWidth(e.nativeEvent.layout.width)}
      style={[{ height }, style]}
      testID={testID}
    >
      {width && points.length ? (
        <Svg width={width} height={height} {...hiddenSvg}>
          {ticks.map((v) => (
            <Line key={`g${v}`} x1={CHART_PAD.left} x2={width - CHART_PAD.right} y1={y(v)} y2={y(v)} stroke={colors.border} strokeWidth={1} />
          ))}
          {ticks.map((v) => (
            <SvgText key={`l${v}`} x={CHART_PAD.left - 8} y={y(v) + 4} fontSize={11} fill={colors.textMuted} textAnchor="end">
              {formatTick(v)}
            </SvgText>
          ))}
          {marks.map((t) => (
            <Line key={`m${t}`} x1={x(t)} x2={x(t)} y1={TOP} y2={bottom} stroke={colors.textMuted} strokeWidth={1} strokeDasharray="2 3" opacity={0.6} />
          ))}
          {points.map((p, i) =>
            p.min !== undefined && p.max !== undefined && p.max > p.min ? (
              <Rect key={`b${p.key}`} x={xs[i]! - BAR / 2} y={y(p.max)} width={BAR} height={y(p.min) - y(p.max)} rx={5} fill={colors.accentSoft} />
            ) : null,
          )}
          {line && points.length > 1 ? (
            <Polyline points={points.map((p, i) => `${xs[i]},${y(p.value)}`).join(" ")} fill="none" stroke={colors.accent} strokeWidth={2.5} strokeLinecap="round" strokeLinejoin="round" />
          ) : null}
          {points.map((p, i) => (
            <Circle key={p.key} cx={xs[i]} cy={y(p.value)} r={5} fill={colors.surface} stroke={colors.accent} strokeWidth={2.5} />
          ))}
          {points.map((p, i) => {
            // The value to the right of its point, or to the left for the last one.
            const end = i === points.length - 1 && points.length > 1;
            return (
              <SvgText key={`v${p.key}`} x={xs[i]! + (end ? -12 : 12)} y={y(p.value) - 6} fontSize={12} fontWeight="700" fill={colors.text} textAnchor={end ? "end" : "start"}>
                {formatValue(p.value)}
              </SvgText>
            );
          })}
          {labels.map((i) => (
            // A date too close to the right edge to centre ends at the edge, so "Oct 24" isn't cut.
            <SvgText key={`d${points[i]!.key}`} {...dateAnchor(xs[i]!, width)} y={height - 4} fontSize={11} fill={colors.textMuted}>
              {formatDate(points[i]!.t)}
            </SvgText>
          ))}
        </Svg>
      ) : null}
    </View>
  );
}
