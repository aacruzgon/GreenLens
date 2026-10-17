import { useEffect } from "react";
import { ActivityIndicator, View } from "react-native";
import { Ionicons } from "@expo/vector-icons";
import Animated, { FadeIn, useReducedMotion } from "react-native-reanimated";

import { useTheme } from "../theme";
import { Text } from "../primitives/Text";

export type StageState = "waiting" | "running" | "done" | "skipped";

export interface StageRow {
  key: string;
  label: string;
  state: StageState;
  note?: string;
}

/** The live list of pipeline stages (architecture 1, UI & Design: live progress for each stage). */
export function StageProgress({ stages, onDone }: { stages: StageRow[]; onDone?: () => void }) {
  const t = useTheme();
  const reduceMotion = useReducedMotion();
  const allDone = stages.length > 0 && stages.every((s) => s.state === "done" || s.state === "skipped");
  useEffect(() => {
    if (allDone) onDone?.();
  }, [allDone, onDone]);

  return (
    <View style={{ gap: t.space.xs }} accessibilityRole="progressbar" accessibilityLabel="Analysis progress">
      {stages.map((s) => (
        <Animated.View
          key={s.key}
          entering={reduceMotion ? undefined : FadeIn.duration(250)}
          style={{ flexDirection: "row", alignItems: "center", gap: t.space.md, minHeight: 32 }}
          accessible
          accessibilityLabel={`${s.label}: ${s.state === "done" ? "done" : s.state === "running" ? "in progress" : s.state === "skipped" ? "skipped" : "waiting"}`}
        >
          <View style={{ width: 22, alignItems: "center" }}>
            {s.state === "running" ? (
              <ActivityIndicator size="small" color={t.colors.accent} />
            ) : s.state === "done" ? (
              <Ionicons name="checkmark-circle" size={22} color={t.colors.healthy} />
            ) : s.state === "skipped" ? (
              <Ionicons name="remove-circle-outline" size={22} color={t.colors.textMuted} />
            ) : (
              <Ionicons name="ellipse-outline" size={20} color={t.colors.border} />
            )}
          </View>
          <View style={{ flex: 1 }}>
            <Text variant="callout" color={s.state === "waiting" ? "textMuted" : "text"} weight={s.state === "running" ? "600" : undefined}>
              {s.label}
            </Text>
            {s.note ? (
              <Text variant="footnote" color="textMuted">
                {s.note}
              </Text>
            ) : null}
          </View>
        </Animated.View>
      ))}
    </View>
  );
}
