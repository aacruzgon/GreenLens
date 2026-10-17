import { View } from "react-native";
import { Ionicons } from "@expo/vector-icons";

import { useTheme } from "../theme";
import type { ColorRoles } from "../tokens";
import { Button } from "./Button";
import { Text } from "./Text";

type Tone = "info" | "watch" | "danger" | "accent";

const roles: Record<Tone, { fg: keyof ColorRoles; bg: keyof ColorRoles; icon: keyof typeof Ionicons.glyphMap }> = {
  info: { fg: "info", bg: "infoSoft", icon: "information-circle" },
  watch: { fg: "watch", bg: "watchSoft", icon: "alert-circle" },
  danger: { fg: "danger", bg: "dangerSoft", icon: "warning" },
  accent: { fg: "accent", bg: "accentSoft", icon: "leaf" },
};

export interface BannerProps {
  title: string;
  body?: string;
  tone?: Tone;
  icon?: keyof typeof Ionicons.glyphMap;
  action?: { label: string; onPress: () => void };
}

export function Banner({ title, body, tone = "info", icon, action }: BannerProps) {
  const t = useTheme();
  const r = roles[tone];
  return (
    <View
      accessibilityRole="summary"
      style={{ flexDirection: "row", gap: t.space.md, backgroundColor: t.colors[r.bg], borderRadius: t.radius.md, padding: t.space.md }}
    >
      <Ionicons name={icon ?? r.icon} size={22} color={t.colors[r.fg]} />
      <View style={{ flex: 1, gap: t.space.xs }}>
        <Text variant="subhead" weight="600" color={r.fg}>
          {title}
        </Text>
        {body ? <Text variant="subhead">{body}</Text> : null}
        {action ? (
          <View style={{ alignItems: "flex-start", marginLeft: -t.space.sm }}>
            <Button label={action.label} variant="plain" onPress={action.onPress} />
          </View>
        ) : null}
      </View>
    </View>
  );
}
