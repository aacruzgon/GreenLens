import { useEffect, type ReactNode } from "react";
import type { StyleProp, ViewStyle } from "react-native";
import Animated, { useAnimatedStyle, useReducedMotion, useSharedValue, withDelay, withSpring } from "react-native-reanimated";

import { motion } from "../tokens";

/**
 * Content entering a screen: a critically damped spring rising 24 pt, staggered by `index` (design 3.1).
 * With reduced motion on, the content is simply there.
 */
export function Enter({ index = 0, children, style }: { index?: number; children: ReactNode; style?: StyleProp<ViewStyle> }) {
  const reduced = useReducedMotion();
  const { stiffness, damping, mass, rise, stagger } = motion.enter;
  const progress = useSharedValue(reduced ? 1 : 0);
  useEffect(() => {
    if (!reduced) progress.value = withDelay(index * stagger, withSpring(1, { stiffness, damping, mass }));
  }, [reduced, index, stagger, stiffness, damping, mass, progress]);
  const animated = useAnimatedStyle(() => ({
    opacity: Math.min(1, progress.value * 1.4),
    transform: [{ translateY: (1 - progress.value) * rise }],
  }));
  return <Animated.View style={[style, animated]}>{children}</Animated.View>;
}
