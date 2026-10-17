import { ActivityIndicator, Pressable, StyleSheet, View, type PressableProps } from "react-native";
import { Ionicons } from "@expo/vector-icons";

import { useTheme } from "../theme";
import { MIN_TOUCH } from "../tokens";
import { Text } from "./Text";

export interface ButtonProps extends Omit<PressableProps, "children"> {
  label: string;
  variant?: "primary" | "secondary" | "plain" | "destructive";
  size?: "regular" | "large";
  icon?: keyof typeof Ionicons.glyphMap;
  loading?: boolean;
  fullWidth?: boolean;
}

export function Button({ label, variant = "primary", size = "regular", icon, loading, fullWidth, disabled, style, ...rest }: ButtonProps) {
  const t = useTheme();
  const c = t.colors;
  const palette = {
    primary: { bg: c.accent, fg: c.accentText, border: c.accent },
    secondary: { bg: c.accentSoft, fg: c.accent, border: c.accentSoft },
    plain: { bg: "transparent", fg: c.accent, border: "transparent" },
    destructive: { bg: c.dangerSoft, fg: c.danger, border: c.dangerSoft },
  }[variant];
  const inactive = disabled || loading;

  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityState={{ disabled: !!inactive, busy: !!loading }}
      disabled={inactive}
      hitSlop={4}
      style={(state) => [
        styles.base,
        {
          minHeight: size === "large" ? 52 : MIN_TOUCH,
          borderRadius: size === "large" ? t.radius.lg : t.radius.md,
          backgroundColor: palette.bg,
          borderColor: palette.border,
          paddingHorizontal: variant === "plain" ? t.space.sm : t.space.lg,
          opacity: inactive ? 0.5 : state.pressed ? 0.75 : 1,
          alignSelf: fullWidth ? "stretch" : "auto",
        },
        typeof style === "function" ? style(state) : style,
      ]}
      {...rest}
    >
      <View style={styles.row}>
        {loading ? (
          <ActivityIndicator color={palette.fg} />
        ) : icon ? (
          <Ionicons name={icon} size={20} color={palette.fg} />
        ) : null}
        <Text variant="headline" style={{ color: palette.fg }}>
          {label}
        </Text>
      </View>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  base: { justifyContent: "center", alignItems: "center", borderWidth: StyleSheet.hairlineWidth },
  row: { flexDirection: "row", alignItems: "center", gap: 8 },
});
