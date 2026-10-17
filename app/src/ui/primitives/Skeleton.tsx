import { useEffect } from "react";
import { type DimensionValue } from "react-native";
import Animated, { useAnimatedStyle, useReducedMotion, useSharedValue, withRepeat, withTiming } from "react-native-reanimated";

import { useTheme } from "../theme";

/** A placeholder in the shape of the content (screens.md 4: never a full-screen spinner). */
export function Skeleton({ width = "100%", height = 16, radius }: { width?: DimensionValue; height?: number; radius?: number }) {
  const t = useTheme();
  const reduceMotion = useReducedMotion();
  const opacity = useSharedValue(1);
  useEffect(() => {
    if (!reduceMotion) opacity.value = withRepeat(withTiming(0.45, { duration: 800 }), -1, true);
  }, [opacity, reduceMotion]);
  const animated = useAnimatedStyle(() => ({ opacity: opacity.value }));
  return (
    <Animated.View
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      style={[{ width, height, borderRadius: radius ?? t.radius.sm, backgroundColor: t.colors.surfaceMuted }, animated]}
    />
  );
}
