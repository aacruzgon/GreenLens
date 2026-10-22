// A picture answer (design 5, after Merlin): a compact pill with a small reference picture and its answer
// ("Yes, rings"), for the question card on the live analysis (styles.css .layer[data-scan="camera"] .live .qpic).
// Without a picture it is the same pill with the label only, so a question the app has no art for still reads.
import type { ReactNode } from "react";
import { Pressable, View } from "react-native";

import { Text } from "../primitives/Text";
import { useTheme } from "../theme";
import { minTouch } from "../tokens";

export interface PictureAnswerProps {
  label: string;
  picture?: ReactNode;
  onPress: () => void;
  disabled?: boolean;
  /** The chosen answer while it is sent. */
  selected?: boolean;
  testID?: string;
}

export function PictureAnswer({ label, picture, onPress, disabled = false, selected = false, testID }: PictureAnswerProps) {
  const { colors, reducedMotion } = useTheme();
  return (
    <Pressable
      testID={testID}
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityState={{ disabled, selected }}
      disabled={disabled}
      onPress={onPress}
      style={({ pressed }) => ({
        flex: 1,
        minHeight: minTouch + 8,
        flexDirection: "row",
        alignItems: "center",
        gap: 10,
        paddingVertical: 6,
        paddingLeft: picture ? 6 : 16,
        paddingRight: 12,
        borderRadius: 999,
        borderWidth: 1.5,
        borderColor: selected ? colors.text : "rgba(255, 255, 255, 0.14)",
        backgroundColor: "rgba(255, 255, 255, 0.08)",
        opacity: disabled && !selected ? 0.5 : 1,
        transform: [{ scale: pressed && !reducedMotion ? 0.97 : 1 }],
      })}
    >
      {picture ? <View style={{ width: 52, height: 39, borderRadius: 14, overflow: "hidden" }}>{picture}</View> : null}
      <Text variant="subheadline" style={{ fontWeight: "600", flexShrink: 1 }} numberOfLines={2}>
        {label}
      </Text>
    </Pressable>
  );
}
