import type { ReactNode } from "react";
import { View } from "react-native";
import { Ionicons } from "@expo/vector-icons";

import { useTheme } from "../theme";
import type { ColorRoles } from "../tokens";
import { Text } from "./Text";

type Tone = "neutral" | "accent" | "healthy" | "watch" | "sick" | "dead" | "info" | "danger";

const toneRoles: Record<Tone, [keyof ColorRoles, keyof ColorRoles]> = {
  neutral: ["textMuted", "surfaceMuted"],
  accent: ["accent", "accentSoft"],
  healthy: ["healthy", "healthySoft"],
  watch: ["watch", "watchSoft"],
  sick: ["sick", "sickSoft"],
  dead: ["dead", "deadSoft"],
  info: ["info", "infoSoft"],
  danger: ["danger", "dangerSoft"],
};

export interface ChipProps {
  label: string;
  tone?: Tone;
  icon?: keyof typeof Ionicons.glyphMap;
  /** Drawn before the label instead of an icon, such as a status shape. */
  leading?: ReactNode;
  accessibilityLabel?: string;
}

/** A small label. Its meaning is always in its words; colour only reinforces it. */
export function Chip({ label, tone = "neutral", icon, leading, accessibilityLabel }: ChipProps) {
  const t = useTheme();
  const [fg, bg] = toneRoles[tone];
  return (
    <View
      accessible
      accessibilityLabel={accessibilityLabel ?? label}
      style={{
        flexDirection: "row",
        alignItems: "center",
        alignSelf: "flex-start",
        gap: 6,
        backgroundColor: t.colors[bg],
        borderRadius: t.radius.pill,
        paddingHorizontal: t.space.sm + 2,
        paddingVertical: t.space.xs,
      }}
    >
      {leading ?? (icon ? <Ionicons name={icon} size={14} color={t.colors[fg]} /> : null)}
      <Text variant="footnote" weight="600" style={{ color: t.colors[fg] }}>
        {label}
      </Text>
    </View>
  );
}

export type { Tone as ChipTone };
