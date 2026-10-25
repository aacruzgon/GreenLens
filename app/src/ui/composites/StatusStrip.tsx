// The plant's status at each check (analytics 2, P1; the mockup's Insights, "Health over time"): a cell per scan
// on a date axis, in the status colour with its shape (design 6), and the dates under them. A gap of more than
// 14 days between checks stays a gap: the baseline isn't drawn across it. A cell can be tapped to open its scan;
// each one is its own VoiceOver element with its date and status (HIG Charts, Enhancing the accessibility).
import { useState } from "react";
import { Pressable, View, type StyleProp, type ViewStyle } from "react-native";
import Svg, { Circle, Line, Polygon, Rect, Text as SvgText } from "react-native-svg";

import { hiddenSvg } from "../svgId";
import { useTheme } from "../theme";
import { statusRoles, type PlantStatus } from "../tokens";
import { DAY_MS, fittingLabels, timeDomain, timeScale } from "./chart";

/** A plant status, or `unknown`: a failed scan, or "Not sure" with nothing pointing to a problem (grey). */
export type StripStatus = PlantStatus | "unknown";

export interface StripCell {
  key: string;
  /** Epoch ms. */
  t: number;
  status: StripStatus;
  /** The cell in words, with its date: "Oct 11: Sick, early blight". */
  label: string;
}

export interface StatusStripProps {
  cells: readonly StripCell[];
  formatDate: (t: number) => string;
  /** The whole strip in words, read when the cells aren't tappable. */
  accessibilityLabel: string;
  onPressCell?: (key: string) => void;
  style?: StyleProp<ViewStyle>;
  testID?: string;
}

const HEIGHT = 46;
const CELL_H = 24;
const GAP_DAYS = 14;

function Glyph({ status, cx, cy, color }: { status: StripStatus; cx: number; cy: number; color: string }) {
  const r = 4.5;
  switch (status) {
    case "healthy":
      return <Circle cx={cx} cy={cy} r={3.5} fill={color} />;
    case "watch":
      return <Polygon points={`${cx},${cy - r} ${cx + r},${cy + r - 1} ${cx - r},${cy + r - 1}`} fill={color} />;
    case "sick":
      return <Polygon points={`${cx},${cy - r} ${cx + r},${cy} ${cx},${cy + r} ${cx - r},${cy}`} fill={color} />;
    case "dead":
      return <Rect x={cx - 3.5} y={cy - 3.5} width={7} height={7} rx={1} fill="none" stroke={color} strokeWidth={1.8} />;
    case "unknown":
      return <Line x1={cx - 4} x2={cx + 4} y1={cy} y2={cy} stroke={color} strokeWidth={2} strokeLinecap="round" />;
  }
}

export function StatusStrip({ cells, formatDate, accessibilityLabel, onPressCell, style, testID }: StatusStripProps) {
  const { colors } = useTheme();
  const [width, setWidth] = useState(0);
  // The strip has no value labels, so it uses the whole width.
  const x = timeScale(timeDomain(cells.map((c) => c.t)), width, 0);
  const xs = cells.map((c) => x(c.t));
  const spacing = Math.min(...xs.slice(1).map((v, i) => v - xs[i]!), Infinity);
  const cellW = Math.max(12, Math.min(26, spacing - 4));
  const labels = fittingLabels(xs);
  const fill = (s: StripStatus) => (s === "unknown" ? colors.textMuted : colors[statusRoles[s].fg]);
  const mid = 2 + CELL_H / 2;

  return (
    <View
      accessible={!onPressCell}
      accessibilityRole={onPressCell ? undefined : "image"}
      accessibilityLabel={onPressCell ? undefined : accessibilityLabel}
      onLayout={(e) => setWidth(e.nativeEvent.layout.width)}
      style={[{ height: HEIGHT }, style]}
      testID={testID}
    >
      {width && cells.length ? (
        <>
          <Svg width={width} height={HEIGHT} {...hiddenSvg}>
            {cells.slice(1).map((c, i) =>
              c.t - cells[i]!.t <= GAP_DAYS * DAY_MS ? (
                <Line key={`g${c.key}`} x1={xs[i]} x2={xs[i + 1]} y1={mid} y2={mid} stroke={colors.border} strokeWidth={1} />
              ) : null,
            )}
            {cells.map((c, i) => (
              <Rect key={c.key} x={xs[i]! - cellW / 2} y={2} width={cellW} height={CELL_H} rx={6} fill={fill(c.status)} opacity={0.9} />
            ))}
            {cells.map((c, i) => (
              <Glyph key={`s${c.key}`} status={c.status} cx={xs[i]!} cy={mid} color={colors.surface} />
            ))}
            {labels.map((i) => (
              <SvgText
                key={`d${cells[i]!.key}`}
                x={i === 0 ? Math.max(0, xs[i]! - cellW / 2) : i === cells.length - 1 && cells.length > 1 ? Math.min(width, xs[i]! + cellW / 2) : xs[i]}
                y={42}
                fontSize={11}
                fill={colors.textMuted}
                textAnchor={i === 0 ? "start" : i === cells.length - 1 && cells.length > 1 ? "end" : "middle"}
              >
                {formatDate(cells[i]!.t)}
              </SvgText>
            ))}
          </Svg>
          {onPressCell
            ? cells.map((c, i) => {
                // Touch targets as wide as the cells' spacing allows, so neighbours don't overlap.
                const w = Math.max(cellW, Math.min(44, spacing));
                return (
                  <Pressable
                    key={`p${c.key}`}
                    accessibilityRole="button"
                    accessibilityLabel={c.label}
                    onPress={() => onPressCell(c.key)}
                    style={{ position: "absolute", left: xs[i]! - w / 2, top: 0, width: w, height: HEIGHT }}
                    testID={`strip-cell-${c.key}`}
                  />
                );
              })
            : null}
        </>
      ) : null}
    </View>
  );
}
