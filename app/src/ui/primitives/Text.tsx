import { Text as RNText, type TextProps as RNTextProps } from "react-native";

import { useTheme } from "../theme";
import type { ColorRoles, TypeVariant } from "../tokens";

export interface TextProps extends RNTextProps {
  variant?: TypeVariant;
  color?: keyof ColorRoles;
  weight?: "400" | "500" | "600" | "700";
  align?: "left" | "center" | "right";
}

// The display sizes are already large, so they grow less with the system text size.
const DISPLAY_MAX_SCALE: Partial<Record<TypeVariant, number>> = { hero: 1.2, display: 1.3, displaySmall: 1.6 };

/** Scales with the system text size (allowFontScaling stays on). */
export function Text({ variant = "body", color = "text", weight, align, style, ...rest }: TextProps) {
  const t = useTheme();
  return (
    <RNText
      maxFontSizeMultiplier={DISPLAY_MAX_SCALE[variant]}
      {...rest}
      style={[t.type[variant], { color: t.colors[color] }, weight && { fontWeight: weight }, align && { textAlign: align }, style]}
    />
  );
}
