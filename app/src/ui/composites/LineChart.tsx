// A measured series over time (analytics 2; styles.css .chart): points at their dates, value gridlines with
// labels, an optional noise band, and a line only when there is a trend (3 points over 14 days), which stops at
// the last point. The drawing is hidden from VoiceOver; `accessibilityLabel` says the values (HIG Charts,
// Enhancing the accessibility of a chart). Its width is measured, as the chat blocks do.
import { useState } from "react";
import { View, type StyleProp, type ViewStyle } from "react-native";
import Svg, { Circle, Line, Polygon, Polyline, Text as SvgText } from "react-native-svg";

import { hiddenSvg } from "../svgId";
import { useTheme } from "../theme";
import { CHART_PAD, dateAnchor, fittingLabels, isTrend, niceTicks, timeDomain, timeScale } from "./chart";

export interface ChartPoint {
  key: string;
  /** Epoch ms. */
  t: number;
  value: number;
}

export interface LineChartProps {
  points: readonly ChartPoint[];
  /** The chart in words, with its values: what VoiceOver reads instead of the drawing. */
  accessibilityLabel: string;
  /** The noise band around each point, in the points' order: drawn as one band from first to last. */
  band?: readonly { lo: number; hi: number }[];
  /** Draws the line; by default only for a trend (3 points over 14 days). */
  line?: boolean;
  formatTick: (value: number) => string;
  formatDate: (t: number) => string;
  /** A fixed lower bound, such as 0 for a percentage (HIG Charts, Axes); otherwise it follows the data. */
  yMin?: number;
  /** A shared time domain, so charts stacked under each other line up. */
  domain?: readonly [number, number];
  height?: number;
  style?: StyleProp<ViewStyle>;
  testID?: string;
}

const DATES = 20;
const TOP = 10;

export function LineChart({ points, accessibilityLabel, band, line, formatTick, formatDate, yMin, domain, height = 136, style, testID }: LineChartProps) {
  const { colors } = useTheme();
  const [width, setWidth] = useState(0);
  const times = points.map((p) => p.t);
  const x = timeScale(domain ?? timeDomain(times), width);
  const values = [...points.map((p) => p.value), ...(band ?? []).flatMap((b) => [b.lo, b.hi])];
  const ticks = niceTicks(yMin ?? Math.min(...values), Math.max(...values));
  const lo = ticks[0]!;
  const hi = ticks.at(-1)!;
  const bottom = height - DATES;
  const y = (v: number) => bottom - ((v - lo) / (hi - lo || 1)) * (bottom - TOP);
  const drawLine = line ?? isTrend(times);
  const xs = points.map((p) => x(p.t));
  const labels = fittingLabels(xs);
  // The band's upper edge left to right, then its lower edge back.
  const bandPts = band && band.length === points.length
    ? [...xs.map((px, i) => `${px},${y(band[i]!.hi)}`), ...xs.map((px, i) => `${px},${y(band[i]!.lo)}`).reverse()].join(" ")
    : undefined;

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
          {bandPts ? <Polygon points={bandPts} fill={colors.accentSoft} /> : null}
          {drawLine && points.length > 1 ? (
            <Polyline points={points.map((p, i) => `${xs[i]},${y(p.value)}`).join(" ")} fill="none" stroke={colors.accent} strokeWidth={2.5} strokeLinecap="round" strokeLinejoin="round" />
          ) : null}
          {points.map((p, i) => (
            <Circle key={p.key} cx={xs[i]} cy={y(p.value)} r={4.5} fill={colors.surface} stroke={colors.accent} strokeWidth={2.5} />
          ))}
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
