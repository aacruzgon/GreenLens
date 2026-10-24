// A plant's insights (roadmap F7 item 3; design 8; the mockup's t-insights): the plant's name, its date range and
// status; health over time (StatusStrip); spots on the measured leaves with their ranges, under the steps done
// (BandLine, StepLanes); height with its noise band (LineChart); the weather before the diagnosis; and the
// nearest plant. Solid cards, entering on the damped spring, a fade under reduced motion (design 6).
import { useLocalSearchParams } from "expo-router";
import type { ReactNode } from "react";
import { Pressable, View } from "react-native";

import { formatMonthDay } from "@/domain/format";
import {
  BandLine,
  Card,
  EmptyState,
  Enter,
  ErrorState,
  LineChart,
  LineIcon,
  Screen,
  Section,
  Skeleton,
  StatusChip,
  StatusStrip,
  StepLanes,
  Text,
  minTouch,
  radii,
  space,
  timeDomain,
  useTheme,
} from "@/ui";

import { HumidBars } from "../components/HumidBars";
import { useInsights } from "../hooks/useInsights";

const STAGGER = 60;

function InsightCard({ eyebrow, headline, children, testID }: { eyebrow: string; headline: string; children?: ReactNode; testID?: string }) {
  return (
    <Card testID={testID} style={{ gap: space[2] }}>
      <View>
        <Text variant="footnote" color="textMuted" emphasis>
          {eyebrow}
        </Text>
        <Text variant="title3" emphasis accessibilityRole="header">
          {headline}
        </Text>
      </View>
      {children}
    </Card>
  );
}

function Caption({ children }: { children: string }) {
  return (
    <Text variant="footnote" color="textMuted">
      {children}
    </Text>
  );
}

export function InsightsScreen() {
  const { plantId } = useLocalSearchParams<{ plantId: string }>();
  const { colors } = useTheme();
  const v = useInsights(plantId);
  const p = v.plant;
  const date = (t: number) => formatMonthDay(new Date(t).toISOString(), v.tz);

  const header = (
    <View style={{ flexDirection: "row", alignItems: "center", gap: space[2] }}>
      <Pressable accessibilityRole="button" accessibilityLabel="Back" onPress={v.actions.back} style={{ width: minTouch, height: minTouch, alignItems: "center", justifyContent: "center", marginLeft: -space[2] }} testID="insights-back">
        <LineIcon name="back" color={colors.text} size={22} strokeWidth={2} />
      </Pressable>
      <View style={{ flex: 1 }}>
        {p ? (
          <>
            <Text variant="headline">{p.name}</Text>
            <Text variant="footnote" color="textMuted">
              {v.range ? `Insights · ${v.range}` : "Insights"}
              {v.stale ? " · Not checked recently" : ""}
            </Text>
          </>
        ) : (
          <Skeleton height={36} width="60%" />
        )}
      </View>
      {p ? <StatusChip status={p.status} /> : null}
    </View>
  );

  if (v.notFound || v.error) {
    return (
      <Screen testID="insights-screen">
        {header}
        {v.notFound ? (
          <EmptyState title="We can't find this" message="This plant was deleted, or you no longer have access to it." action={{ label: "Back", onPress: v.actions.back, variant: "secondary" }} />
        ) : (
          <ErrorState message="The insights didn't load." onRetry={v.retry} />
        )}
      </Screen>
    );
  }

  const spots = v.spots;
  const spotDomain = spots ? timeDomain(spots.points.map((pt) => Date.parse(pt.created_at)), v.lanes.flatMap((l) => l.times)) : undefined;
  const sections: { key: string; node: ReactNode }[] = [];

  sections.push({
    key: "health",
    node: (
      <InsightCard eyebrow="Health over time" headline={v.health.headline} testID="insights-health">
        {v.health.cells.length ? (
          <StatusStrip cells={v.health.cells} formatDate={date} accessibilityLabel={v.health.summary} onPressCell={v.actions.openScan} testID="insights-strip" />
        ) : null}
      </InsightCard>
    ),
  });

  if (spots) {
    sections.push({
      key: "spots",
      node: (
        <InsightCard eyebrow="Spots on the measured leaves" headline={spots.headline} testID="insights-spots">
          {spots.kpi ? (
            <View style={{ flexDirection: "row", gap: space[6] }}>
              <View>
                <Text variant="title3" emphasis style={{ fontVariant: ["tabular-nums"] }} testID="insights-spots-kpi">
                  {spots.kpi}
                </Text>
                <Text variant="caption1" color="textMuted">
                  of leaf area with spots
                </Text>
              </View>
              <View>
                <Text variant="title3" emphasis style={{ fontVariant: ["tabular-nums"] }}>
                  {String(spots.checks)}
                </Text>
                <Text variant="caption1" color="textMuted">
                  {spots.checks === 1 ? "check" : "checks"}
                </Text>
              </View>
            </View>
          ) : null}
          <BandLine
            points={spots.points.map((pt) => ({ key: pt.scan_id, t: Date.parse(pt.created_at), value: pt.value, min: pt.min, max: pt.max }))}
            line={spots.line}
            marks={v.lanes.flatMap((l) => l.times)}
            domain={spotDomain}
            formatTick={(n) => (n === 0 ? "0" : `${n}%`)}
            formatValue={(n) => `${Math.round(n)}%`}
            formatDate={date}
            accessibilityLabel={spots.summary}
            testID="insights-spots-chart"
          />
          {v.lanes.length ? <StepLanes steps={v.lanes} domain={spotDomain} testID="insights-lanes" /> : null}
          {v.after ? <Text variant="subheadline">{v.after}</Text> : null}
          <Caption>{spots.caption}</Caption>
        </InsightCard>
      ),
    });
  } else if (v.weather) {
    // A current problem with no close-up measured: say how spots get measured.
    sections.push({
      key: "spots",
      node: (
        <InsightCard eyebrow="Spots on the measured leaves" headline="Not measured yet" testID="insights-spots">
          <Caption>Take a leaf close-up on a LiDAR device to measure spots.</Caption>
        </InsightCard>
      ),
    });
  }

  if (v.heightLoaded) {
    const h = v.height;
    sections.push({
      key: "height",
      node: h ? (
        <InsightCard eyebrow="Height" headline={h.headline} testID="insights-height">
          <LineChart
            points={h.points.map((pt) => ({ key: pt.scan_id, t: Date.parse(pt.created_at), value: pt.value }))}
            band={h.band}
            line={h.trend}
            formatTick={(n) => `${n}`}
            formatDate={date}
            accessibilityLabel={h.summary}
            testID="insights-height-chart"
          />
          <Caption>{h.caption}</Caption>
        </InsightCard>
      ) : (
        <InsightCard eyebrow="Height" headline="Not measured yet" testID="insights-height">
          <Caption>Growth is measured from whole-plant photos on an iPhone or iPad with LiDAR.</Caption>
        </InsightCard>
      ),
    });
  }

  if (v.weather) {
    sections.push({
      key: "weather",
      node: (
        <InsightCard eyebrow="Weather before the diagnosis" headline={v.weather.headline} testID="insights-weather">
          <HumidBars bars={v.weather.bars} summary={v.weather.summary} />
          <Caption>{v.weather.caption}</Caption>
        </InsightCard>
      ),
    });
  }

  if (v.nearby) {
    const n = v.nearby;
    sections.push({
      key: "nearby",
      node: (
        <Section title="Nearby">
          <Card onPress={() => v.actions.openPlant(n.plantId)} accessibilityLabel={`${n.name}, ${n.cm} centimetres away. ${n.line}`} testID="insights-nearby">
            <View style={{ flexDirection: "row", alignItems: "center", gap: space[3] }}>
              <View style={{ width: 36, height: 36, borderRadius: radii.pill, alignItems: "center", justifyContent: "center", backgroundColor: colors.surfaceMuted }}>
                <LineIcon name="pin" color={colors.textMuted} size={18} />
              </View>
              <View style={{ flex: 1, gap: 2 }}>
                <Text variant="headline">{`${n.name}, ${n.cm} cm away`}</Text>
                <Text variant="footnote" color="textMuted">
                  {n.line}
                </Text>
              </View>
              <StatusChip status={n.status} />
            </View>
          </Card>
        </Section>
      ),
    });
  }

  return (
    <Screen testID="insights-screen" contentStyle={{ gap: space[4] }}>
      {header}
      {v.loading ? (
        <>
          <Skeleton height={120} radius={radii.card} />
          <Skeleton height={220} radius={radii.card} />
        </>
      ) : (
        sections.map((s, i) => (
          <Enter key={s.key} delay={i * STAGGER}>
            {s.node}
          </Enter>
        ))
      )}
    </Screen>
  );
}
