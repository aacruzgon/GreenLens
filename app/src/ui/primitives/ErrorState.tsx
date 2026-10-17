import { View } from "react-native";
import { Ionicons } from "@expo/vector-icons";

import { useTheme } from "../theme";
import { Button } from "./Button";
import { Text } from "./Text";

/** What happened, what to do, and the request ID in small text when there is one (screens.md 4). */
export function ErrorState({
  title,
  message,
  requestId,
  action,
}: {
  title: string;
  message?: string;
  requestId?: string;
  action?: { label: string; onPress: () => void };
}) {
  const t = useTheme();
  return (
    <View style={{ alignItems: "center", gap: t.space.md, paddingVertical: t.space.xxl, paddingHorizontal: t.space.xl }}>
      <Ionicons name="cloud-offline-outline" size={40} color={t.colors.textMuted} />
      <Text variant="title3" align="center">
        {title}
      </Text>
      {message ? (
        <Text color="textMuted" align="center">
          {message}
        </Text>
      ) : null}
      {action ? <Button label={action.label} variant="secondary" onPress={action.onPress} /> : null}
      {requestId ? (
        <Text variant="caption" color="textMuted" selectable>
          Reference: {requestId}
        </Text>
      ) : null}
    </View>
  );
}
