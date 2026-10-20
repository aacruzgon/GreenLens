// A neutral chip (styles.css .status sizing: 13/18 semibold, pill radius). Selected chips are outlined in the
// text colour, never green (design.md 3.1, controls are neutral). Status chips are composites/StatusChip.
import type { ReactNode } from "react";
import { Pressable, View, type StyleProp, type ViewStyle } from "react-native";

import { useTheme } from "../theme";
import { radii, type ColorRole } from "../tokens";
import { Text } from "./Text";

export interface ChipProps {
  label: string;
  /** Drawn before the label, such as a status shape. */
  leading?: ReactNode;
  selected?: boolean;
  onPress?: () => void;
  /** Colour roles for the label and the fill; default text on surfaceMuted. */
  color?: ColorRole;
  fill?: ColorRole;
  accessibilityLabel?: string;
  style?: StyleProp<ViewStyle>;
}

export function Chip({ label, leading, selected = false, onPress, color = "text", fill = "surfaceMuted", accessibilityLabel, style }: ChipProps) {
  const { colors } = useTheme();
  const box: ViewStyle = {
    flexDirection: "row",
    alignItems: "center",
    alignSelf: "flex-start",
    gap: 6,
    paddingVertical: 3,
    paddingLeft: leading ? 8 : 10,
    paddingRight: 10,
    borderRadius: radii.pill,
    backgroundColor: colors[fill],
    borderWidth: selected ? 2 : 0,
    borderColor: colors.text,
  };
  const content = (
    <>
      {leading}
      <Text variant="footnote" emphasis color={color} numberOfLines={1}>
        {label}
      </Text>
    </>
  );
  if (onPress) {
    return (
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={accessibilityLabel ?? label}
        accessibilityState={{ selected }}
        onPress={onPress}
        // A 24 pt chip reaches the 44 pt target through its hit slop (design.md 6).
        hitSlop={{ top: 10, bottom: 10, left: 4, right: 4 }}
        style={({ pressed }) => [box, pressed ? { opacity: 0.7 } : null, style]}
      >
        {content}
      </Pressable>
    );
  }
  return (
    <View accessible accessibilityLabel={accessibilityLabel ?? label} style={[box, style]}>
      {content}
    </View>
  );
}
