// A fade in or out. Fades are the still version of every motion, so this runs under reduced motion too
// (design.md 6). Timing: F0's 250 ms on the iOS sheet curve (tokens motion.fade, motion.sheet).
import { useEffect, type ReactNode } from "react";
import type { StyleProp, ViewStyle } from "react-native";
import Animated, { Easing, useAnimatedStyle, useSharedValue, withDelay, withTiming } from "react-native-reanimated";

import { motion } from "../tokens";

export interface FadeProps {
  children: ReactNode;
  /** Fades to 0 when false; the content stays mounted. */
  visible?: boolean;
  duration?: number;
  delay?: number;
  style?: StyleProp<ViewStyle>;
  testID?: string;
}

const sheetEase = Easing.bezier(...motion.sheet.bezier);

export function Fade({ children, visible = true, duration = motion.fade.duration, delay = 0, style, testID }: FadeProps) {
  const opacity = useSharedValue(0);

  useEffect(() => {
    const to = withTiming(visible ? 1 : 0, { duration, easing: sheetEase });
    opacity.value = delay > 0 ? withDelay(delay, to) : to;
  }, [delay, duration, opacity, visible]);

  const animated = useAnimatedStyle(() => ({ opacity: opacity.value }));

  return (
    <Animated.View
      testID={testID}
      pointerEvents={visible ? "auto" : "none"}
      accessibilityElementsHidden={!visible}
      importantForAccessibility={visible ? "auto" : "no-hide-descendants"}
      style={[style, animated]}
    >
      {children}
    </Animated.View>
  );
}
