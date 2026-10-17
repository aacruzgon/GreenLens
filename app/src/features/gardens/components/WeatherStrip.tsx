import { ScrollView, View } from "react-native";
import { Ionicons } from "@expo/vector-icons";
import { format, parseISO } from "date-fns";

import { celsius } from "@/domain/format";
import type { WeatherSummary } from "@/domain/types";
import { Card, Text, useTheme } from "@/ui";

/** Now, and the days ahead, with frost and humid days called out in words (architecture 8.3). */
export function WeatherStrip({ weather, locationLabel }: { weather: WeatherSummary; locationLabel?: string }) {
  const t = useTheme();
  const c = weather.current;
  return (
    <Card padded={false}>
      <View style={{ padding: t.space.lg, flexDirection: "row", alignItems: "center", gap: t.space.md }}>
        <Ionicons name="partly-sunny" size={36} color={t.colors.watch} />
        <View style={{ flex: 1 }}>
          <Text variant="title2">{celsius(c.temp_c)}</Text>
          <Text variant="footnote" color="textMuted">
            {[locationLabel, `Humidity ${Math.round(c.rh_pct)}%`, `Wind ${Math.round(c.wind_kmh)} km/h`].filter(Boolean).join(" · ")}
          </Text>
        </View>
      </View>
      <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={{ paddingHorizontal: t.space.lg, paddingBottom: t.space.lg, gap: t.space.sm }}>
        {weather.forecast.slice(0, 7).map((d) => {
          const humid = d.hours_rh_over_90 >= 10;
          const label = format(parseISO(d.date), "EEE");
          return (
            <View
              key={d.date}
              accessible
              accessibilityLabel={`${label}: ${celsius(d.t_min_c)} to ${celsius(d.t_max_c)}${d.frost_risk ? ", frost likely" : ""}${humid ? ", humid" : ""}`}
              style={{
                alignItems: "center",
                gap: 4,
                minWidth: 58,
                paddingVertical: t.space.sm,
                paddingHorizontal: t.space.xs,
                borderRadius: t.radius.md,
                backgroundColor: d.frost_risk ? t.colors.infoSoft : humid ? t.colors.watchSoft : t.colors.surfaceMuted,
              }}
            >
              <Text variant="footnote" weight="600">
                {label}
              </Text>
              <Ionicons
                name={d.frost_risk ? "snow" : d.precip_mm >= 2 ? "rainy" : "sunny"}
                size={18}
                color={d.frost_risk ? t.colors.info : d.precip_mm >= 2 ? t.colors.info : t.colors.watch}
              />
              <Text variant="caption">{`${Math.round(d.t_max_c)}° / ${Math.round(d.t_min_c)}°`}</Text>
              {d.frost_risk ? (
                <Text variant="caption" color="info" weight="600">
                  Frost
                </Text>
              ) : humid ? (
                <Text variant="caption" color="watch" weight="600">
                  Humid
                </Text>
              ) : null}
            </View>
          );
        })}
      </ScrollView>
    </Card>
  );
}
