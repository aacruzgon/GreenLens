// The card that drops from the top on the camera screens (design 3.1, after the Camera app's visual intelligence
// card): capture's instruction (F2) and the live analysis's stage card (F3). Solid dark at 92% with a hairline
// and a grabber at its foot (styles.css .cap-sheet), never glass, since it carries content (design 6). The first
// time, it slides down into the screen from above the top edge, once the screen has come up; a new `dropKey` drops it
// again by 16 pt. Both run on the entrance spring as it fades in; under reduced motion it only fades (design 6).
import { useEffect, useRef, useState, type ReactNode } from "react";
import { View, type LayoutChangeEvent, type StyleProp, type ViewStyle } from "react-native";
import Animated, { Easing, useAnimatedStyle, useSharedValue, withDelay, withSpring, withTiming } from "react-native-reanimated";

import { useReducedMotion } from "../theme";
import { camera, motion, radii } from "../tokens";

const DROP = 16;
// The camera screens come up as a full-screen modal; the first slide waits for it, or it would play unseen.
const ENTER_DELAY = 280;
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
  // How far the card travels: from above the screen's top edge the first time, then DROP.
  const distance = useSharedValue(0);
  // The card's bottom edge from its first layout; the first slide starts once it is known.
  const [enterFrom, setEnterFrom] = useState<number>();
  const entered = useRef(false);

  useEffect(() => {
    if (enterFrom === undefined) return;
    progress.value = 0;
    if (reduced) {
      progress.value = withTiming(1, { duration: motion.fade.duration, easing: sheetEase });
    } else if (!entered.current) {
      distance.value = enterFrom;
      progress.value = withDelay(ENTER_DELAY, withSpring(1, motion.enter.spring));
      entered.current = true;
    } else {
      distance.value = DROP;
      progress.value = withSpring(1, motion.enter.spring);
    }
  }, [dropKey, enterFrom, progress, distance, reduced]);

  const animated = useAnimatedStyle(() => {
    const p = progress.value;
    return reduced ? { opacity: p } : { opacity: Math.min(1, p * 2), transform: [{ translateY: (p - 1) * distance.value }] };
  });

  return (
    <Animated.View
      testID={testID}
      onLayout={(e) => {
        const { y, height } = e.nativeEvent.layout;
        if (enterFrom === undefined) setEnterFrom(y + height);
        onLayout?.(e);
      }}
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
