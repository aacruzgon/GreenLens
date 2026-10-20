// The damped entrance (design.md 3.1; directions.js conservatory.motion): a critically damped spring,
// stiffness 170 and damping 26, rising 24 pt and scaling from 0.985 as it fades in.
// Under reduced motion it only fades: no rise, no scale (design.md 6).
import { useEffect, type ReactNode } from "react";
import type { StyleProp, ViewStyle } from "react-native";
import Animated, { Easing, useAnimatedStyle, useSharedValue, withDelay, withSpring, withTiming } from "react-native-reanimated";

import { useReducedMotion } from "../theme";
import { motion } from "../tokens";

export interface EnterProps {
  children: ReactNode;
  /** Staggers a list: ms before this item starts. */
  delay?: number;
  style?: StyleProp<ViewStyle>;
  testID?: string;
}

const sheetEase = Easing.bezier(...motion.sheet.bezier);

export function Enter({ children, delay = 0, style, testID }: EnterProps) {
  const reduced = useReducedMotion();
  const progress = useSharedValue(0);

  useEffect(() => {
    const to = reduced
      ? withTiming(1, { duration: motion.fade.duration, easing: sheetEase })
      : withSpring(1, motion.enter.spring);
    progress.value = delay > 0 ? withDelay(delay, to) : to;
  }, [delay, progress, reduced]);

  const animated = useAnimatedStyle(() => {
    const p = progress.value;
    if (reduced) return { opacity: p };
    return {
      opacity: p,
      transform: [
        { translateY: (1 - p) * motion.enter.fromY },
        { scale: motion.enter.fromScale + (1 - motion.enter.fromScale) * p },
      ],
    };
  });

  return (
    <Animated.View testID={testID} style={[style, animated]}>
      {children}
    </Animated.View>
  );
}
