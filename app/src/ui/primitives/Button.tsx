import { ActivityIndicator, Pressable, StyleSheet, View, type PressableProps } from "react-native";
import { Ionicons } from "@expo/vector-icons";

import { useTheme } from "../theme";
import { MIN_TOUCH, shadow } from "../tokens";
import { Text } from "./Text";

export interface ButtonProps extends Omit<PressableProps, "children"> {
  label: string;
  variant?: "primary" | "secondary" | "plain" | "destructive";
  size?: "regular" | "large";
  icon?: keyof typeof Ionicons.glyphMap;
  loading?: boolean;
  fullWidth?: boolean;
  /** For a button in a set of choices: announced as selected. */
  selected?: boolean;
}

/**
 * Conservatory's pill (design 3.1): one accent pill per screen. A large one with an icon carries it in a
 * white disc at its left end. Secondary buttons are pale surface pills; plain ones are text in the accent.
 */
export function Button({ label, variant = "primary", size = "regular", icon, loading, fullWidth, selected, disabled, style, ...rest }: ButtonProps) {
  const t = useTheme();
  const c = t.colors;
  const palette = {
    primary: { bg: c.accent, fg: c.accentText, border: c.accent },
    secondary: { bg: c.surface, fg: c.text, border: t.scheme === "dark" ? c.border : c.surface },
    plain: { bg: "transparent", fg: c.accent, border: "transparent" },
    destructive: { bg: c.dangerSoft, fg: c.danger, border: c.dangerSoft },
  }[variant];
  const inactive = disabled || loading;
  const large = size === "large";
  const disc = large && !!icon && variant === "primary";
  const lift = variant === "primary" && large ? shadow.pill : variant === "secondary" && t.scheme === "light" ? shadow.soft : undefined;

  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label}
      disabled={inactive}
      hitSlop={4}
      style={(state) => [
        styles.base,
        lift,
        {
          minHeight: large ? 60 : MIN_TOUCH,
          borderRadius: t.radius.pill,
          backgroundColor: palette.bg,
          borderColor: palette.border,
          justifyContent: disc ? "flex-start" : "center",
          paddingLeft: disc ? 6 : variant === "plain" ? t.space.sm : t.space.lg + 2,
          paddingRight: variant === "plain" ? t.space.sm : t.space.xl,
          opacity: inactive ? 0.5 : state.pressed ? 0.8 : 1,
          transform: [{ scale: state.pressed && !inactive ? 0.97 : 1 }],
          alignSelf: fullWidth ? "stretch" : "auto",
        },
        typeof style === "function" ? style(state) : style,
      ]}
      {...rest}
      accessibilityState={{ ...rest.accessibilityState, disabled: !!inactive, busy: !!loading, ...(selected !== undefined ? { selected } : {}) }}
    >
      <View style={[styles.row, { gap: disc ? 14 : 8 }]}>
        {disc ? (
          <View style={[styles.disc, { backgroundColor: c.accentText }]}>
            {loading ? <ActivityIndicator color={c.accent} /> : <Ionicons name={icon} size={22} color={c.accent} />}
          </View>
        ) : loading ? (
          <ActivityIndicator color={palette.fg} />
        ) : icon ? (
          <Ionicons name={icon} size={20} color={palette.fg} />
        ) : null}
        <Text variant={large ? "headline" : "subhead"} weight="600" style={{ color: palette.fg, fontSize: large ? 18 : 16 }}>
          {label}
        </Text>
      </View>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  base: { alignItems: "center", flexDirection: "row", borderWidth: StyleSheet.hairlineWidth },
  row: { flexDirection: "row", alignItems: "center" },
  disc: { width: 48, height: 48, borderRadius: 24, alignItems: "center", justifyContent: "center" },
});
