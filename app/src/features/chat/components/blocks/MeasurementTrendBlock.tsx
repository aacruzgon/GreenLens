// The measurement trend (design 5): the plant's measured points with their noise band (analytics 2), drawn with
// Insights' `LineChart`, joined only when there is a trend (3 points over 14 days), with no line past the last
// point. Opens the plant's Insights (roadmap F7 item 3).
import { View } from "react-native";

import type { AnswerBlock } from "@/domain/proposed";
import { formatLocalDate } from "@/domain/format";
import { Button, LineChart, Text } from "@/ui";

import { blockSummary, chartPoints, hasTrend, noise, trendWord } from "../../model/blocks";
import { BlockFrame } from "./BlockFrame";

type Block = Extract<AnswerBlock, { kind: "measurement_trend" }>;

const day = (iso: string) => formatLocalDate(iso.slice(0, 10));

export function MeasurementTrendBlock({ block, onOpenInsights }: { block: Block; onOpenInsights: (plantId: string) => void }) {
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
          <LineChart
            points={series.map((p) => ({ key: p.scan_id, t: Date.parse(p.created_at), value: p.value }))}
            band={series.map((p) => ({ lo: p.value - noise(p.value), hi: p.value + noise(p.value) }))}
            line={hasTrend(block.points)}
            formatTick={(v) => `${v}`}
            formatDate={(t) => day(new Date(t).toISOString())}
            accessibilityLabel={blockSummary(block)}
            height={120}
          />
          <View style={{ flexDirection: "row", justifyContent: "space-between" }}>
            <Text variant="caption1" color="textMuted">
              {day(series[0]!.created_at)} · about {Math.round(series[0]!.value)} cm
            </Text>
            <Text variant="caption1" color="textMuted">
              {day(series.at(-1)!.created_at)} · about {Math.round(series.at(-1)!.value)} cm
            </Text>
          </View>
          <Text variant="subheadline">
            {word
              ? `The latest is ${word} than the first, over ${series.length} scans.`
              : `${series.length} ${series.length === 1 ? "scan" : "scans"} so far: too few for a trend.`}
          </Text>
        </>
      )}
      <Button
        label="See insights"
        accessibilityLabel={`See ${block.plant_name}'s insights`}
        variant="secondary"
        size="small"
        onPress={() => onOpenInsights(block.plant_id)}
        style={{ alignSelf: "flex-start" }}
        testID="block-trend-insights"
      />
    </BlockFrame>
  );
}
