// The measurement trend (design 5): the plant's measured points with their noise band (analytics 2), joined only
// when there is a trend (3 points over 14 days), with no line past the last point. Opens the plant's page.
import { useState } from "react";
import { View } from "react-native";
import Svg, { Circle, Line, Polyline } from "react-native-svg";

import type { AnswerBlock } from "@/domain/proposed";
import { formatLocalDate } from "@/domain/format";
import { Button, Text, hiddenSvg, useTheme } from "@/ui";

import { blockSummary, chartPoints, hasTrend, plot, trendWord } from "../../model/blocks";
import { BlockFrame } from "./BlockFrame";

type Block = Extract<AnswerBlock, { kind: "measurement_trend" }>;

const HEIGHT = 110;

export function MeasurementTrendBlock({ block, onOpenPlant }: { block: Block; onOpenPlant: (plantId: string) => void }) {
  const { colors } = useTheme();
  // The card's own width, measured, as the map block does.
  const [width, setWidth] = useState(0);
  const pts = plot(block.points, width - 12, HEIGHT - 12).map((p) => ({ ...p, x: p.x + 6, y: p.y + 6, lo: p.lo + 6, hi: p.hi + 6 }));
  const trend = hasTrend(block.points);
  const word = trendWord(block.points);
  const series = chartPoints(block.points);
  const quantity = block.quantity === "plant_height_cm" ? "Height" : "Width";
  return (
    <BlockFrame title={`${quantity} · ${block.plant_name}`} icon="ruler" summary={blockSummary(block)} testID="block-measurement_trend">
      {series.length === 0 ? (
        <Text variant="subheadline" color="textMuted">
          No measurements yet. A scan with the whole plant in view adds one.
        </Text>
      ) : (
        <>
          <View onLayout={(e) => setWidth(e.nativeEvent.layout.width)} style={{ height: HEIGHT }}>
          {width ? <Svg width={width} height={HEIGHT} {...hiddenSvg}>
            {pts.map((p) => (
              <Line key={`b-${p.point.scan_id}`} x1={p.x} x2={p.x} y1={p.lo} y2={p.hi} stroke={colors.accentSoft} strokeWidth={6} strokeLinecap="round" />
            ))}
            {trend ? <Polyline points={pts.map((p) => `${p.x},${p.y}`).join(" ")} fill="none" stroke={colors.accent} strokeWidth={2} /> : null}
            {pts.map((p) => (
              <Circle key={p.point.scan_id} cx={p.x} cy={p.y} r={3.5} fill={colors.surface} stroke={colors.accent} strokeWidth={2} />
            ))}
          </Svg> : null}
          </View>
          <View style={{ flexDirection: "row", justifyContent: "space-between" }}>
            <Text variant="caption1" color="textMuted">
              {formatLocalDate(series[0]!.created_at.slice(0, 10))} · about {Math.round(series[0]!.value)} cm
            </Text>
            <Text variant="caption1" color="textMuted">
              {formatLocalDate(series.at(-1)!.created_at.slice(0, 10))} · about {Math.round(series.at(-1)!.value)} cm
            </Text>
          </View>
          <Text variant="subheadline">
            {word
              ? `The latest is ${word} than the first, over ${series.length} scans.`
              : `${series.length} ${series.length === 1 ? "scan" : "scans"} so far: too few for a trend.`}
          </Text>
        </>
      )}
      <Button label={`Open ${block.plant_name}`} variant="secondary" size="small" onPress={() => onOpenPlant(block.plant_id)} style={{ alignSelf: "flex-start" }} />
    </BlockFrame>
  );
}
