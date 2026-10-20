// The weather card (screens 3.1; styles.css .weather with the "days" layout, where .w-main is hidden): the
// next six days with the frost day marked, and a footer with the frost day and when it was fetched. The
// hero's sentence above carries now. A 503 shows "Weather unavailable" in its place.
import { View } from "react-native";

import { Button, Card, Skeleton, Text, space, useTheme } from "@/ui";

import type { WeatherCardState } from "../model/views";
import { Icon } from "./Icons";

export function WeatherCard({ state }: { state: WeatherCardState }) {
  const { colors } = useTheme();

  if (state.status === "loading") {
    return (
      <Card testID="weather-loading" accessibilityLabel="Loading the weather">
        <View style={{ flexDirection: "row", gap: space[2] }}>
          {[0, 1, 2, 3, 4, 5].map((i) => (
            <Skeleton key={i} height={110} radius={10} style={{ flex: 1 }} />
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
  // .w-day: 13 pt type on the screen's 22 pt line, 4 pt between the rows and 6 pt above and below.
  const cell = { fontSize: 13, lineHeight: 22, fontVariant: ["tabular-nums" as const] };
  return (
    <Card style={{ paddingTop: 14, paddingBottom: 12 }}>
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
              borderCurve: "continuous",
              backgroundColor: d.frost ? colors.watchSoft : "transparent",
            }}
          >
            <Text color={d.frost ? "watch" : "text"} style={cell}>
              {d.label}
            </Text>
            <Icon name={d.kind} color={d.frost ? colors.watch : colors.textMuted} />
            <Text color={d.frost ? "watch" : "text"} style={cell}>
              {d.high}
            </Text>
            <Text color={d.frost ? "watch" : "textMuted"} style={cell}>
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
