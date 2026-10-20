// The weather card (screens 3.1; styles.css .weather with the "days" layout): now, the next six days with
// the frost day marked, and when it was fetched. A 503 shows "Weather unavailable" in its place.
import { View } from "react-native";

import { Button, Card, Skeleton, Text, space, useTheme } from "@/ui";

import type { WeatherCardState } from "../model/views";
import { Icon } from "./Icons";

export function WeatherCard({ state }: { state: WeatherCardState }) {
  const { colors } = useTheme();

  if (state.status === "loading") {
    return (
      <Card testID="weather-loading" accessibilityLabel="Loading the weather">
        <Skeleton height={44} width="55%" />
        <View style={{ flexDirection: "row", gap: space[2], marginTop: space[4] }}>
          {[0, 1, 2, 3, 4, 5].map((i) => (
            <Skeleton key={i} height={72} radius={10} style={{ flex: 1 }} />
          ))}
        </View>
        <Skeleton height={12} width="40%" style={{ marginTop: space[3] }} />
      </Card>
    );
  }

  if (state.status !== "ready") {
    const title = state.status === "unavailable" ? "Weather unavailable" : "Couldn't load the weather";
    const message = state.status === "error" ? state.message : undefined;
    return (
      <Card style={{ gap: space[2] }}>
        <Text variant="headline">{title}</Text>
        {message ? (
          <Text variant="subheadline" color="textMuted">
            {message}
          </Text>
        ) : null}
        {state.onRetry ? <Button variant="secondary" label="Try again" onPress={state.onRetry} /> : null}
      </Card>
    );
  }

  const v = state.view;
  return (
    <Card>
      <View
        accessible
        accessibilityLabel={`Now ${v.temp}, ${v.humidity}, ${v.detail}`}
        style={{ flexDirection: "row", alignItems: "center", gap: space[3], marginBottom: space[3] }}
      >
        <Icon name={v.nowKind} color={colors.info} size={34} />
        <Text style={{ fontSize: 44, lineHeight: 48, fontWeight: "300", letterSpacing: -0.9, fontVariant: ["tabular-nums"] }}>{v.temp}</Text>
        <View style={{ flex: 1 }}>
          <Text variant="headline">{v.humidity}</Text>
          <Text variant="footnote" color="textMuted">
            {v.detail}
          </Text>
        </View>
      </View>

      <View style={{ flexDirection: "row", gap: 2 }}>
        {v.days.map((d) => (
          <View
            key={d.date}
            accessible
            accessibilityLabel={d.accessibilityLabel}
            testID={d.frost ? "weather-day-frost" : "weather-day"}
            style={{
              flex: 1,
              alignItems: "center",
              gap: space[1],
              paddingVertical: 6,
              borderRadius: 10,
              backgroundColor: d.frost ? colors.watchSoft : "transparent",
            }}
          >
            <Text variant="footnote" color={d.frost ? "watch" : "text"} emphasis={d.frost}>
              {d.label}
            </Text>
            <Icon name={d.kind} color={d.frost ? colors.watch : colors.textMuted} />
            <Text variant="footnote" color={d.frost ? "watch" : "text"} style={{ fontVariant: ["tabular-nums"] }}>
              {d.high}
            </Text>
            <Text variant="footnote" color={d.frost ? "watch" : "textMuted"} style={{ fontVariant: ["tabular-nums"] }}>
              {d.low}
            </Text>
          </View>
        ))}
      </View>

      <View style={{ flexDirection: "row", justifyContent: "space-between", gap: space[3], marginTop: space[2] }}>
        <Text variant="caption1" color="textMuted" style={{ flexShrink: 1 }}>
          {v.frostNote ?? ""}
        </Text>
        <Text variant="caption1" color="textMuted">
          {v.updated}
        </Text>
      </View>
    </Card>
  );
}
