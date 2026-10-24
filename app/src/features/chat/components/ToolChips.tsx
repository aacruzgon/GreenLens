// Tool chips while the agent works (screens 3.6): the server's `label` ("Checking the forecast"), a dot while it
// runs, a check when it finished. They go once the answer is complete.
import { ActivityIndicator, View } from "react-native";

import { LineIcon, Text, space, useTheme } from "@/ui";

import type { ToolChip } from "../model/messageReducer";

export function ToolChips({ tools }: { tools: readonly ToolChip[] }) {
  const { colors } = useTheme();
  if (tools.length === 0) return null;
  return (
    <View style={{ gap: space[1] }} accessibilityLiveRegion="polite" testID="tool-chips">
      {tools.map((t) => (
        <View
          key={t.callId}
          accessible
          accessibilityLabel={t.state === "running" ? t.label : `${t.label}, done`}
          style={{ flexDirection: "row", alignItems: "center", gap: space[2], alignSelf: "flex-start", paddingVertical: space[1], paddingHorizontal: space[3], borderRadius: 999, backgroundColor: colors.surfaceMuted }}
        >
          {t.state === "running" ? (
            <ActivityIndicator size="small" color={colors.textMuted} style={{ transform: [{ scale: 0.7 }] }} />
          ) : (
            <LineIcon name={t.state === "done" ? "check" : "x"} color={t.state === "done" ? colors.textMuted : colors.danger} size={14} />
          )}
          <Text variant="footnote" color="textMuted">
            {t.label}
          </Text>
        </View>
      ))}
    </View>
  );
}
