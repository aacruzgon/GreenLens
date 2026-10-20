// The card that drops from the top on the camera screens (design 3.1, after the Camera app's visual intelligence
// card): capture's instruction (F2) and the live analysis's stage card (F3). Solid dark at 92% with a hairline
// and a grabber at its foot (styles.css .cap-sheet), never glass, since it carries content (design 6). It drops
// 16 pt on the entrance spring as it fades in; under reduced motion it only fades (design 6).
import { useEffect, type ReactNode } from "react";
import { View, type LayoutChangeEvent, type StyleProp, type ViewStyle } from "react-native";
import Animated, { Easing, useAnimatedStyle, useSharedValue, withSpring, withTiming } from "react-native-reanimated";

import { useReducedMotion } from "../theme";
import { camera, motion, radii } from "../tokens";

const DROP = 16;
const sheetEase = Easing.bezier(...motion.sheet.bezier);

export interface DropCardProps {
  children: ReactNode;
  /** A new value drops the card again: the chosen mode on capture, the running stage on the analysis. */
  dropKey?: string;
  style?: StyleProp<ViewStyle>;
  testID?: string;
  /** The card's frame, for what must stay clear of it (capture's framing guide). */
  onLayout?: (event: LayoutChangeEvent) => void;
}

export function DropCard({ children, dropKey, style, testID, onLayout }: DropCardProps) {
  const reduced = useReducedMotion();
  const progress = useSharedValue(0);

  useEffect(() => {
    progress.value = 0;
    progress.value = reduced
      ? withTiming(1, { duration: motion.fade.duration, easing: sheetEase })
      : withSpring(1, motion.enter.spring);
  }, [dropKey, progress, reduced]);

  const animated = useAnimatedStyle(() => {
    const p = progress.value;
    return reduced ? { opacity: p } : { opacity: p, transform: [{ translateY: (p - 1) * DROP }] };
  });

  return (
    <Animated.View
      testID={testID}
      onLayout={onLayout}
      style={[
        {
          paddingTop: 14,
          paddingHorizontal: 18,
          paddingBottom: 24,
          borderRadius: radii.sheet,
          borderCurve: "continuous",
          backgroundColor: camera.card,
          borderWidth: 1,
          borderColor: camera.hairline,
        },
        style,
        animated,
      ]}
    >
      {children}
      <View
        accessibilityElementsHidden
        importantForAccessibility="no-hide-descendants"
        style={{ position: "absolute", left: "50%", bottom: 8, width: 36, height: 5, marginLeft: -18, borderRadius: 3, backgroundColor: camera.grabber }}
      />
    </Animated.View>
  );
}
