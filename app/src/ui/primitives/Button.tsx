// The pill (design.md 3.1, styles.css .btn and data-buttons="pill").
// primary: the one green action per screen; with an icon, the icon sits in a white disc at the left end.
// secondary: a pale surface pill. ghost: the surface at 80% over a tinted fill, no shadow (.btn-ghost under
// data-buttons="pill", as on the frost banner). plain: text only, in the text colour, since controls are neutral.
// destructive: the secondary pill with its text in `danger` (HIG Buttons, Role: "a destructive button uses the system
// red color"), for deleting or leaving; check:contrast covers danger on surface.
// small is .btn-sm: 15 pt text and 14 pt sides at the same 44 pt height.
import type { ReactNode } from "react";
import { ActivityIndicator, Pressable, View, type PressableProps, type StyleProp, type ViewStyle } from "react-native";

import { useTheme } from "../theme";
import { elevation, minTouch, radii, type ColorRole } from "../tokens";
import { Text } from "./Text";

export type ButtonVariant = "primary" | "secondary" | "destructive" | "ghost" | "plain";
export type ButtonSize = "small" | "regular" | "large";

export interface IconRenderProps {
  color: string;
  size: number;
}

export interface ButtonProps extends Omit<PressableProps, "children" | "style"> {
  label: string;
  variant?: ButtonVariant;
  /** large is the full-width hero pill (60 pt); regular is 44 pt. */
  size?: ButtonSize;
  /** Draws the icon; on primary it sits in the white disc. */
  icon?: (props: IconRenderProps) => ReactNode;
  busy?: boolean;
  disabled?: boolean;
  style?: StyleProp<ViewStyle>;
}

const FG: Record<ButtonVariant, ColorRole> = { primary: "accentText", secondary: "text", destructive: "danger", ghost: "text", plain: "text" };

export function Button({
  label,
  variant = "primary",
  size = "regular",
  icon,
  busy = false,
  disabled = false,
  style,
  accessibilityLabel,
  accessibilityHint,
  ...rest
}: ButtonProps) {
  const { colors, dark, reducedMotion } = useTheme();
  const inactive = disabled || busy;
  const large = size === "large";
  const small = size === "small";
  const side = variant === "plain" ? 10 : small ? 14 : 18;
  const height = large ? 60 : minTouch;
  const fg = colors[FG[variant]];
  // .btn-xl > svg.i: a 48 pt disc in accentText with the icon in accent; scaled to the 44 pt pill.
  const disc = large ? 48 : 32;
  const iconSize = large ? 24 : 18;
  const withDisc = variant === "primary" && icon !== undefined;

  const fill: ViewStyle =
    variant === "primary"
      ? { backgroundColor: colors.accent, boxShadow: large && !dark ? elevation.pillPrimary : undefined }
      : variant === "secondary" || variant === "destructive"
        ? { backgroundColor: colors.surface, boxShadow: dark ? undefined : elevation.pillSecondary }
        : variant === "ghost"
          ? { backgroundColor: `${colors.surface}CC` }
          : { backgroundColor: "transparent" };

  return (
    <Pressable
      {...rest}
      accessibilityRole="button"
      accessibilityLabel={accessibilityLabel ?? label}
      accessibilityHint={accessibilityHint}
      accessibilityState={{ disabled: inactive, busy }}
      disabled={inactive}
      style={({ pressed }) => [
        {
          minHeight: height,
          minWidth: minTouch,
          borderRadius: radii.pill,
          flexDirection: "row",
          alignItems: "center",
          justifyContent: withDisc ? "flex-start" : "center",
          gap: withDisc ? 14 : 8,
          paddingLeft: withDisc ? (height - disc) / 2 : side,
          paddingRight: withDisc ? 24 : side,
          alignSelf: large ? "stretch" : "flex-start",
          opacity: disabled ? 0.4 : 1,
        },
        fill,
        // .btn:active scales to 0.97; under reduced motion the press dims instead.
        pressed && !inactive ? (reducedMotion ? { opacity: 0.7 } : { transform: [{ scale: 0.97 }] }) : null,
        style,
      ]}
    >
      {withDisc ? (
        <View
          style={{
            width: disc,
            height: disc,
            borderRadius: disc / 2,
            backgroundColor: colors.accentText,
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          {busy ? <ActivityIndicator color={colors.accent} /> : icon({ color: colors.accent, size: iconSize })}
        </View>
      ) : icon && !busy ? (
        icon({ color: fg, size: iconSize })
      ) : null}
      {busy && !withDisc ? <ActivityIndicator color={fg} /> : null}
      <Text
        emphasis
        color={FG[variant]}
        numberOfLines={2}
        style={large ? { fontSize: 18 } : small ? { fontSize: 15, lineHeight: 20 } : undefined}
      >
        {label}
      </Text>
    </Pressable>
  );
}
