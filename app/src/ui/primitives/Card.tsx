import { Pressable, View, type ViewProps } from "react-native";

import { useTheme } from "../theme";
import { shadow } from "../tokens";

export interface CardProps extends ViewProps {
  onPress?: () => void;
  accessibilityLabel?: string;
  padded?: boolean;
  tone?: "surface" | "muted";
}

export function Card({ onPress, padded = true, tone = "surface", style, children, accessibilityLabel, ...rest }: CardProps) {
  const t = useTheme();
  // Large continuous radii; a warm shadow in light mode and a border in dark (design 3.1). The shadow
  // sits on an outer view, because the inner one clips its photos to the radius.
  const light = t.scheme === "light";
  const outer = {
    borderRadius: t.radius.card,
    backgroundColor: tone === "muted" ? t.colors.surfaceMuted : t.colors.surface,
    ...(light && tone === "surface" ? shadow.card : { borderWidth: 1, borderColor: t.colors.border }),
  };
  const inner = { borderRadius: t.radius.card, padding: padded ? t.space.lg : 0, overflow: "hidden" as const, flexGrow: 1 };
  if (onPress) {
    return (
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={accessibilityLabel}
        onPress={onPress}
        style={({ pressed }) => [outer, { transform: [{ scale: pressed ? 0.98 : 1 }] }, style as object]}
      >
        <View style={inner}>{children}</View>
      </Pressable>
    );
  }
  return (
    <View accessibilityLabel={accessibilityLabel} style={[outer, style]} {...rest}>
      <View style={inner}>{children}</View>
    </View>
  );
}
