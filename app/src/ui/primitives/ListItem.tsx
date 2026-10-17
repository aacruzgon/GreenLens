import { Pressable, View } from "react-native";
import { Ionicons } from "@expo/vector-icons";
import type { ReactNode } from "react";

import { useTheme } from "../theme";
import { MIN_TOUCH } from "../tokens";
import { Text } from "./Text";

export interface ListItemProps {
  title: string;
  subtitle?: string;
  leading?: ReactNode;
  trailing?: ReactNode;
  onPress?: () => void;
  chevron?: boolean;
  destructive?: boolean;
}

export function ListItem({ title, subtitle, leading, trailing, onPress, chevron = !!onPress, destructive }: ListItemProps) {
  const t = useTheme();
  const content = (
    <View style={{ flexDirection: "row", alignItems: "center", gap: t.space.md, minHeight: MIN_TOUCH + 8, paddingVertical: t.space.sm }}>
      {leading}
      <View style={{ flex: 1, gap: 2 }}>
        <Text color={destructive ? "danger" : "text"}>{title}</Text>
        {subtitle ? (
          <Text variant="footnote" color="textMuted">
            {subtitle}
          </Text>
        ) : null}
      </View>
      {trailing}
      {chevron ? <Ionicons name="chevron-forward" size={18} color={t.colors.textMuted} /> : null}
    </View>
  );
  if (!onPress) return content;
  return (
    <Pressable accessibilityRole="button" accessibilityLabel={subtitle ? `${title}, ${subtitle}` : title} onPress={onPress} style={({ pressed }) => ({ opacity: pressed ? 0.6 : 1 })}>
      {content}
    </Pressable>
  );
}
