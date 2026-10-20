// An inline message across the content: offline, a frost alert, a failed upload. The text stays in the
// text colours on the soft fill; the tone colour marks the edge so text contrast never depends on it.
import type { ReactNode } from "react";
import { View, type StyleProp, type ViewStyle } from "react-native";

import { useTheme } from "../theme";
import { radii, space, type ColorRole } from "../tokens";
import { Button } from "./Button";
import { Text } from "./Text";

export type BannerTone = "info" | "watch" | "danger" | "neutral";

const TONES: Record<BannerTone, { edge: ColorRole; fill: ColorRole }> = {
  info: { edge: "info", fill: "infoSoft" },
  watch: { edge: "watch", fill: "watchSoft" },
  danger: { edge: "danger", fill: "dangerSoft" },
  neutral: { edge: "border", fill: "surfaceMuted" },
};

export interface BannerProps {
  title: string;
  message?: string;
  tone?: BannerTone;
  icon?: ReactNode;
  action?: { label: string; onPress: () => void };
  style?: StyleProp<ViewStyle>;
}

export function Banner({ title, message, tone = "info", icon, action, style }: BannerProps) {
  const { colors } = useTheme();
  const t = TONES[tone];
  return (
    <View
      accessibilityRole={tone === "danger" ? "alert" : "summary"}
      accessibilityLiveRegion="polite"
      style={[
        {
          flexDirection: "row",
          gap: space[3],
          padding: space[4],
          borderRadius: radii.field,
          borderCurve: "continuous",
          backgroundColor: colors[t.fill],
          borderLeftWidth: 4,
          borderLeftColor: colors[t.edge],
        },
        style,
      ]}
    >
      {icon}
      <View style={{ flex: 1, gap: space[1] }}>
        <Text variant="headline">{title}</Text>
        {message ? <Text variant="subheadline">{message}</Text> : null}
        {action ? <Button variant="plain" label={action.label} onPress={action.onPress} style={{ marginLeft: -10 }} /> : null}
      </View>
    </View>
  );
}
