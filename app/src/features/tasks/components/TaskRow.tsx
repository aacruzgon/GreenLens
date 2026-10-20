// A task in a list (screens 5: TaskRow, used on the garden home and the Tasks tab; styles.css .task, .cb).
// The round check springs in on F1's check spring (stiffness 300, damping 30); under reduced motion it
// only fades (design 6). The check is a control, so it is drawn in the text colour, not the accent: green
// is kept for the screen's one primary action (design 3.1). Without `onCheck` the row is read-only and
// shows no check box at all (screens 4: hidden, not disabled).
import { useEffect } from "react";
import { Pressable, View } from "react-native";
import Animated, { useAnimatedStyle, useSharedValue, withSpring, withTiming } from "react-native-reanimated";
import Svg, { Path } from "react-native-svg";

import { Text, motion, minTouch, space, useTheme } from "@/ui";

export interface TaskRowProps {
  title: string;
  /** "Tomato 2 · weekly". */
  meta?: string;
  done: boolean;
  /** Marks it done; absent for viewers. */
  onCheck?: () => void;
  /** Opens what the task leads to, such as a scan of its plant. */
  onPress?: () => void;
  /** What a press does, for VoiceOver ("Starts a scan of Tomato 2"). */
  pressHint?: string;
  testID?: string;
}

const BOX = 24;

function Check({ done, title, onCheck }: { done: boolean; title: string; onCheck: () => void }) {
  const { colors, reducedMotion } = useTheme();
  const p = useSharedValue(done ? 1 : 0);

  useEffect(() => {
    const to = done ? 1 : 0;
    p.value = reducedMotion ? withTiming(to, { duration: motion.fade.duration }) : withSpring(to, motion.check.spring);
  }, [done, p, reducedMotion]);

  // .task.done .cb: the box fills and grows to 1.08; the tick scales in with it.
  const fill = useAnimatedStyle(() =>
    reducedMotion ? { opacity: p.value } : { opacity: p.value, transform: [{ scale: 1 + 0.08 * p.value }] },
  );
  const tick = useAnimatedStyle(() =>
    reducedMotion ? { opacity: p.value } : { opacity: p.value, transform: [{ scale: 0.4 + 0.6 * p.value }] },
  );

  return (
    <Pressable
      accessibilityRole="checkbox"
      accessibilityState={{ checked: done, disabled: done }}
      accessibilityLabel={done ? `${title}, done` : `Mark done: ${title}`}
      disabled={done}
      onPress={onCheck}
      hitSlop={(minTouch - BOX) / 2}
      testID="task-check"
      style={{ width: BOX, height: BOX, marginTop: -1 }}
    >
      <View
        style={{
          position: "absolute",
          inset: 0,
          borderRadius: BOX / 2,
          borderWidth: 2,
          borderColor: colors.textMuted,
        }}
      />
      <Animated.View
        style={[
          { position: "absolute", inset: 0, borderRadius: BOX / 2, backgroundColor: colors.text, alignItems: "center", justifyContent: "center" },
          fill,
        ]}
      >
        <Animated.View style={tick}>
          <Svg width={16} height={16} viewBox="0 0 24 24">
            <Path d="M5 12.5l4.5 4.5L19 7.5" stroke={colors.background} strokeWidth={3} fill="none" strokeLinecap="round" strokeLinejoin="round" />
          </Svg>
        </Animated.View>
      </Animated.View>
    </Pressable>
  );
}

export function TaskRow({ title, meta, done, onCheck, onPress, pressHint, testID }: TaskRowProps) {
  const body = (
    <View style={{ flex: 1, gap: 2 }}>
      <Text
        variant="body"
        color={done ? "textMuted" : "text"}
        style={done ? { textDecorationLine: "line-through" } : undefined}
      >
        {title}
      </Text>
      {meta ? (
        <Text variant="footnote" color="textMuted">
          {meta}
        </Text>
      ) : null}
    </View>
  );
  return (
    <View testID={testID} style={{ flexDirection: "row", alignItems: "flex-start", gap: space[3], paddingVertical: space[3], paddingHorizontal: space[4] }}>
      {onCheck ? <Check done={done} title={title} onCheck={onCheck} /> : null}
      {onPress ? (
        <Pressable
          accessibilityRole="button"
          accessibilityHint={pressHint}
          onPress={onPress}
          style={({ pressed }) => [{ flex: 1, minHeight: BOX }, pressed ? { opacity: 0.6 } : null]}
        >
          {body}
        </Pressable>
      ) : (
        body
      )}
    </View>
  );
}
