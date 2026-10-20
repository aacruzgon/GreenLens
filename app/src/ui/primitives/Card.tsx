// A solid card, never glass (design.md 6): 28 pt radius, the two-layer warm shadow in light mode,
// a border in dark (directions.js shape.cardStyle "shadow").
import { Pressable, View, type StyleProp, type ViewProps, type ViewStyle } from "react-native";

import { useTheme } from "../theme";
import { elevation, radii, space } from "../tokens";

export interface CardProps extends ViewProps {
  /** Inner padding; styles.css .card uses 14 × 16. Set false for full-bleed content like a melted photo. */
  padded?: boolean;
  onPress?: () => void;
  accessibilityLabel?: string;
  style?: StyleProp<ViewStyle>;
}

export function useCardStyle(): ViewStyle {
  const { colors, scheme } = useTheme();
  const e = elevation.card[scheme];
  return {
    backgroundColor: colors.surface,
    borderRadius: radii.card,
    borderColor: colors.border,
    borderWidth: e.borderWidth,
    boxShadow: e.boxShadow,
    borderCurve: "continuous",
  };
}

export function Card({ padded = true, onPress, style, children, accessibilityLabel, ...rest }: CardProps) {
  const card = useCardStyle();
  const padding: ViewStyle | null = padded ? { paddingVertical: 14, paddingHorizontal: space[4] } : { overflow: "hidden" };
  if (onPress) {
    return (
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={accessibilityLabel}
        onPress={onPress}
        style={({ pressed }) => [card, padding, pressed ? { opacity: 0.85 } : null, style]}
        {...rest}
      >
        {children}
      </Pressable>
    );
  }
  return (
    <View accessibilityLabel={accessibilityLabel} style={[card, padding, style]} {...rest}>
      {children}
    </View>
  );
}
