import { Text as RNText, type TextProps as RNTextProps } from "react-native";

import { useTheme } from "../theme";
import type { ColorRoles, TypeVariant } from "../tokens";

export interface TextProps extends RNTextProps {
  variant?: TypeVariant;
  color?: keyof ColorRoles;
  weight?: "400" | "500" | "600" | "700";
  align?: "left" | "center" | "right";
}

/** Scales with the system text size (allowFontScaling stays on). */
export function Text({ variant = "body", color = "text", weight, align, style, ...rest }: TextProps) {
  const t = useTheme();
  return (
    <RNText
      {...rest}
      style={[t.type[variant], { color: t.colors[color] }, weight && { fontWeight: weight }, align && { textAlign: align }, style]}
    />
  );
}
