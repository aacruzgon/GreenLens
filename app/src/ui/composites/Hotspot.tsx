// The hotspot on a photo (design 3.1, after the furniture concept's white dot): a soft oval inside the finding's
// region, a white dot at its centre and a label chip below, or above where another chip is ("Leaf spots · 8–12 mm"). The dot settles in 520 ms
// and then breathes; under reduced motion it appears at once and holds still (design 6). On the camera screens
// the chip is solid dark (styles.css .layer[data-scan="camera"] .live .region-tag).
import { useEffect } from "react";
import { Pressable, View } from "react-native";
import Animated, {
  Easing,
  cancelAnimation,
  useAnimatedStyle,
  useSharedValue,
  withDelay,
  withRepeat,
  withSequence,
  withTiming,
} from "react-native-reanimated";

import { Text } from "../primitives/Text";
import { useReducedMotion } from "../theme";
import { camera, motion } from "../tokens";

const DOT = 14;
// The chip's height and its gap from the oval; the scan screen places labels with the same numbers.
const TAG_HEIGHT = 34;
const TAG_GAP = 10;
// Wide enough for "Ladybug larva · 6 mm" at 15 pt; the chip itself is as wide as its text.
const TAG_BOX = 280;
const sheetEase = Easing.bezier(...motion.sheet.bezier);

export interface HotspotProps {
  /** The oval in the photo view's points. */
  oval: { x: number; y: number; width: number; height: number };
  label: string;
  /** "8–12 mm", after the label in tabular figures. */
  size?: string;
  /** Where the label chip goes; below by default. */
  labelAt?: "below" | "above";
  /** Animates in when true; a replayed finding is drawn settled (README 10, step 4). */
  animate?: boolean;
  onPress?: () => void;
  testID?: string;
}

export function Hotspot({ oval, label, size, labelAt = "below", animate = true, onPress, testID }: HotspotProps) {
  const reduced = useReducedMotion();
  const settle = useSharedValue(animate && !reduced ? 0 : 1);
  const breathe = useSharedValue(0);

  useEffect(() => {
    if (animate && !reduced) settle.value = withTiming(1, { duration: motion.hotspot.settle, easing: sheetEase });
    else settle.value = 1;
    if (reduced) {
      cancelAnimation(breathe);
      breathe.value = 0;
      return;
    }
    breathe.value = withDelay(
      animate ? motion.hotspot.settle : 0,
      withRepeat(withSequence(withTiming(1, { duration: 1300 }), withTiming(0, { duration: 1300 })), -1),
    );
    return () => cancelAnimation(breathe);
  }, [animate, reduced, settle, breathe]);

  const ovalStyle = useAnimatedStyle(() => ({
    opacity: settle.value,
    transform: [{ scale: 0.92 + 0.08 * settle.value }],
  }));
  const ringStyle = useAnimatedStyle(() => ({ transform: [{ scale: 1 + 0.35 * breathe.value }], opacity: 0.3 * (1 - breathe.value) + 0.15 }));
  const tagStyle = useAnimatedStyle(() => ({ opacity: settle.value, transform: [{ translateY: (1 - settle.value) * 6 }] }));

  const cx = oval.x + oval.width / 2;
  const cy = oval.y + oval.height / 2;
  const name = size ? `${label}, ${size}` : label;

  return (
    <>
      <Animated.View
        pointerEvents="none"
        style={[
          {
            position: "absolute",
            left: oval.x,
            top: oval.y,
            width: oval.width,
            height: oval.height,
            borderRadius: 9999,
            borderWidth: 1.5,
            borderColor: "rgba(255, 255, 255, 0.8)",
            backgroundColor: "rgba(255, 255, 255, 0.1)",
          },
          ovalStyle,
        ]}
      />
      <View pointerEvents="none" style={{ position: "absolute", left: cx - DOT / 2, top: cy - DOT / 2, width: DOT, height: DOT }}>
        <Animated.View style={[{ position: "absolute", left: -6, top: -6, width: DOT + 12, height: DOT + 12, borderRadius: 999, backgroundColor: camera.ink }, ringStyle]} />
        <Animated.View
          style={[
            { width: DOT, height: DOT, borderRadius: DOT / 2, backgroundColor: camera.ink, boxShadow: "0 1px 4px rgba(0, 0, 0, 0.4)" },
            ovalStyle,
          ]}
        />
      </View>
      {/* The chip centred under (or over) the oval: a wide box around the centre line, the chip at its middle. */}
      <Animated.View
        pointerEvents="box-none"
        style={[
          {
            position: "absolute",
            left: cx - TAG_BOX / 2,
            width: TAG_BOX,
            top: labelAt === "below" ? oval.y + oval.height + TAG_GAP : oval.y - TAG_GAP - TAG_HEIGHT,
            height: TAG_HEIGHT,
            alignItems: "center",
            justifyContent: "center",
          },
          tagStyle,
        ]}
      >
        <Pressable
          testID={testID}
          accessibilityRole={onPress ? "button" : "text"}
          accessibilityLabel={name}
          disabled={!onPress}
          onPress={onPress}
          style={{ paddingHorizontal: 12, paddingVertical: 7, borderRadius: 999, backgroundColor: camera.pill }}
        >
          <Text variant="subheadline" style={{ color: camera.ink, fontWeight: "600" }} numberOfLines={1}>
            {label}
            {size ? <Text variant="subheadline" style={{ color: camera.ink, fontVariant: ["tabular-nums"] }}>{` · ${size}`}</Text> : null}
          </Text>
        </Pressable>
      </Animated.View>
    </>
  );
}
