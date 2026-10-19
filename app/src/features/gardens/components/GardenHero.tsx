import { View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { format } from "date-fns";

import type { Garden, WeatherSummary } from "@/domain/types";
import { Fade, MeltPhoto, Text, useTheme } from "@/ui";

/**
 * The garden home's hero (design 3.1, melt): a photo from the garden dissolves into the ground, with the
 * garden's name set large over its faded edge and one line on the weather.
 */
export function GardenHero({ garden, weather, photo }: { garden: Garden; weather?: WeatherSummary; photo?: { uri: string; cacheKey?: string } }) {
  const t = useTheme();
  const insets = useSafeAreaInsets();
  const city = garden.location_label?.split(",")[0];
  const eyebrow = [city, format(new Date(), "EEEE")].filter(Boolean).join(" · ");
  const frost = weather?.forecast.slice(0, 6).find((d) => d.frost_risk);
  const line = weather
    ? [
        `${Math.round(weather.current.temp_c)} °C, ${Math.round(weather.current.rh_pct)}% humidity.`,
        frost ? `Frost possible ${format(new Date(`${frost.date}T12:00:00`), "EEEE")} night.` : undefined,
      ]
        .filter(Boolean)
        .join(" ")
    : undefined;

  return (
    <View style={{ height: 420 + insets.top, justifyContent: "flex-end" }}>
      {photo ? (
        <MeltPhoto source={photo} ground={t.colors.background} from={0.34} to={0.8} shadeTop style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0 }} />
      ) : (
        <Fade
          stops={[
            { at: 0, color: t.colors.wash, opacity: 1 },
            { at: 1, color: t.colors.background, opacity: 1 },
          ]}
        />
      )}
      <View style={{ paddingHorizontal: 20, paddingBottom: t.space.xs, width: "100%", maxWidth: 720, alignSelf: "center" }}>
        <Text variant="subhead" color="textMuted">
          {eyebrow}
        </Text>
        <Text variant="hero" accessibilityRole="header" style={{ marginTop: t.space.xs, marginBottom: 10 }}>
          {garden.name}
        </Text>
        {line ? (
          <Text variant="subhead" color="textMuted" style={{ maxWidth: 300 }}>
            {line}
          </Text>
        ) : null}
      </View>
    </View>
  );
}
