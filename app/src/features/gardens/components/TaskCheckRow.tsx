import { useEffect } from "react";
import { Pressable, View } from "react-native";
import Animated, { ReduceMotion, useAnimatedStyle, useSharedValue, withSequence, withSpring } from "react-native-reanimated";
import Svg, { Path } from "react-native-svg";

import type { Task } from "@/domain/types";
import { haptics } from "@/platform";
import { MIN_TOUCH, motion, Text, useTheme } from "@/ui";

/** A task with a round check (design 3.1): the check springs in, with a success haptic. */
export function TaskCheckRow({ task, done, onCheck }: { task: Task; done: boolean; onCheck?: () => void }) {
  const t = useTheme();
  const scale = useSharedValue(1);
  useEffect(() => {
    if (!done) return;
    const spring = { ...motion.check, reduceMotion: ReduceMotion.System };
    scale.value = withSequence(withSpring(0.82, spring), withSpring(1, spring));
  }, [done, scale]);
  const style = useAnimatedStyle(() => ({ transform: [{ scale: scale.value }] }));

  const check = () => {
    if (done || !onCheck) return;
    haptics.success();
    onCheck();
  };

  return (
    <Pressable
      onPress={check}
      disabled={!onCheck}
      accessibilityRole="checkbox"
      accessibilityState={{ checked: done, disabled: !onCheck }}
      accessibilityLabel={`${task.title}${task.detail ? `, ${task.detail}` : ""}`}
      accessibilityHint={onCheck && !done ? "Marks it done" : undefined}
      style={{ flexDirection: "row", alignItems: "center", gap: t.space.md, minHeight: MIN_TOUCH + 12, paddingVertical: t.space.md }}
    >
      {onCheck ? (
        <Animated.View
          style={[
            {
              width: 28,
              height: 28,
              borderRadius: 14,
              borderWidth: done ? 0 : 1.5,
              borderColor: t.colors.textMuted,
              backgroundColor: done ? t.colors.accent : "transparent",
              alignItems: "center",
              justifyContent: "center",
            },
            style,
          ]}
        >
          {done ? (
            <Svg width={16} height={16} viewBox="0 0 24 24">
              <Path d="M5 12.5l4.5 4.5L19 7.5" stroke={t.colors.accentText} strokeWidth={3} fill="none" strokeLinecap="round" strokeLinejoin="round" />
            </Svg>
          ) : null}
        </Animated.View>
      ) : null}
      <View style={{ flex: 1, gap: 2 }}>
        <Text variant="body" weight="600" color={done ? "textMuted" : "text"} style={done ? { textDecorationLine: "line-through" } : undefined}>
          {task.title}
        </Text>
        {task.detail ? (
          <Text variant="footnote" color="textMuted">
            {task.detail}
          </Text>
        ) : null}
      </View>
    </Pressable>
  );
}
