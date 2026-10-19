import { View } from "react-native";
import { Ionicons } from "@expo/vector-icons";
import { format, parseISO } from "date-fns";

import type { ForecastDay, WeatherSummary } from "@/domain/types";
import { Card, Text, useTheme } from "@/ui";

type IconName = keyof typeof Ionicons.glyphMap;

export function dayIcon(d: Pick<ForecastDay, "frost_risk" | "precip_mm" | "mean_rh_pct">): IconName {
  if (d.frost_risk) return "snow-outline";
  if (d.precip_mm >= 1) return "rainy-outline";
  if (d.mean_rh_pct >= 80) return "cloudy-outline";
  return d.mean_rh_pct >= 70 ? "partly-sunny-outline" : "sunny-outline";
}

const degrees = (c: number) => `${Math.round(c)}°`.replace("-", "−");

/** Now, and the days ahead, with frost called out in words (architecture 8.3; design 5, garden home). */
export function WeatherStrip({ weather }: { weather: WeatherSummary }) {
  const t = useTheme();
  const c = weather.current;
  const days = weather.forecast.slice(0, 6);
  const frost = days.find((d) => d.frost_risk);
  const today = days[0];
  const condition = c.precip_mm_last_24h >= 1 ? "Light rain" : c.rh_pct >= 85 ? "Cloudy" : "Clear";
  return (
    <Card>
      <View style={{ flexDirection: "row", alignItems: "center", gap: t.space.md }}>
        <Ionicons name={today ? dayIcon(today) : "partly-sunny-outline"} size={30} color={t.colors.textMuted} />
        <Text variant="largeTitle" weight="600" accessibilityLabel={`${Math.round(c.temp_c)} degrees`}>
          {degrees(c.temp_c)}
        </Text>
        <View style={{ flex: 1 }}>
          <Text variant="headline">{condition}</Text>
          <Text variant="footnote" color="textMuted">
            {`${Math.round(c.rh_pct)}% humidity · ${Math.round(c.precip_mm_last_24h)} mm today`}
          </Text>
        </View>
      </View>
      <View style={{ flexDirection: "row", justifyContent: "space-between", marginTop: t.space.lg }}>
        {days.map((d, i) => {
          const label = i === 0 ? "Today" : format(parseISO(d.date), "EEE");
          return (
            <View
              key={d.date}
              accessible
              accessibilityLabel={`${label}: high ${Math.round(d.t_max_c)}, low ${Math.round(d.t_min_c)} degrees${d.frost_risk ? ", frost possible" : ""}`}
              style={{
                alignItems: "center",
                gap: 6,
                minWidth: 46,
                paddingVertical: t.space.sm,
                borderRadius: t.radius.pill,
                backgroundColor: d.frost_risk ? t.colors.infoSoft : "transparent",
              }}
            >
              <Text variant="footnote" color={d.frost_risk ? "info" : "textMuted"} weight="600">
                {i === 0 ? "Now" : label}
              </Text>
              <Ionicons name={dayIcon(d)} size={20} color={d.frost_risk ? t.colors.info : t.colors.text} />
              <Text variant="subhead" weight="600">
                {degrees(d.t_max_c)}
              </Text>
              <Text variant="footnote" color={d.frost_risk ? "info" : "textMuted"}>
                {degrees(d.t_min_c)}
              </Text>
            </View>
          );
        })}
      </View>
      <View style={{ flexDirection: "row", justifyContent: "space-between", marginTop: t.space.md }}>
        <Text variant="caption" color={frost ? "info" : "textMuted"} weight={frost ? "600" : undefined}>
          {frost ? `Frost possible ${format(parseISO(frost.date), "EEEE")}` : "No frost in the next 6 days"}
        </Text>
        <Text variant="caption" color="textMuted">
          {`Updated ${format(new Date(weather.fetched_at), "HH:mm")}`}
        </Text>
      </View>
    </Card>
  );
}
