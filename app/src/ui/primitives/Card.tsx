import { Pressable, View, type ViewProps } from "react-native";

import { useTheme } from "../theme";

export interface CardProps extends ViewProps {
  onPress?: () => void;
  accessibilityLabel?: string;
  padded?: boolean;
  tone?: "surface" | "muted";
}

export function Card({ onPress, padded = true, tone = "surface", style, children, accessibilityLabel, ...rest }: CardProps) {
  const t = useTheme();
  const base = {
    backgroundColor: tone === "muted" ? t.colors.surfaceMuted : t.colors.surface,
    borderRadius: t.radius.lg,
    borderWidth: 1,
    borderColor: t.colors.border,
    padding: padded ? t.space.lg : 0,
    overflow: "hidden" as const,
  };
  if (onPress) {
    return (
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={accessibilityLabel}
        onPress={onPress}
        style={({ pressed }) => [base, { opacity: pressed ? 0.85 : 1 }, style as object]}
      >
        {children}
      </Pressable>
    );
  }
  return (
    <View accessibilityLabel={accessibilityLabel} style={[base, style]} {...rest}>
      {children}
    </View>
  );
}
