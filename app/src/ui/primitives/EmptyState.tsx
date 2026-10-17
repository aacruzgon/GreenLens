import { View } from "react-native";
import { Ionicons } from "@expo/vector-icons";

import { useTheme } from "../theme";
import { Button } from "./Button";
import { Text } from "./Text";

/** One sentence that says what goes here, and the one action that fills it when the role allows (screens.md 4). */
export function EmptyState({
  icon = "leaf-outline",
  title,
  body,
  action,
}: {
  icon?: keyof typeof Ionicons.glyphMap;
  title: string;
  body?: string;
  action?: { label: string; onPress: () => void; icon?: keyof typeof Ionicons.glyphMap };
}) {
  const t = useTheme();
  return (
    <View style={{ alignItems: "center", gap: t.space.md, paddingVertical: t.space.xxl, paddingHorizontal: t.space.xl }}>
      <View style={{ width: 64, height: 64, borderRadius: 32, backgroundColor: t.colors.accentSoft, alignItems: "center", justifyContent: "center" }}>
        <Ionicons name={icon} size={30} color={t.colors.accent} />
      </View>
      <Text variant="title3" align="center">
        {title}
      </Text>
      {body ? (
        <Text color="textMuted" align="center">
          {body}
        </Text>
      ) : null}
      {action ? <Button label={action.label} icon={action.icon} onPress={action.onPress} /> : null}
    </View>
  );
}
