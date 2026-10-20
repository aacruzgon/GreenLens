// A round 44 pt control in the content (styles.css .icon-btn). Neutral: drawn in the text colour,
// never the accent (design.md 3.1, controls are neutral). Bar buttons over photos are the system's, not this.
import type { ReactNode } from "react";
import { Pressable, type PressableProps, type StyleProp, type ViewStyle } from "react-native";

import { useTheme } from "../theme";
import { minTouch } from "../tokens";
import type { IconRenderProps } from "./Button";

export interface IconButtonProps extends Omit<PressableProps, "children" | "style"> {
  /** Required: the control has no visible text. */
  accessibilityLabel: string;
  icon: (props: IconRenderProps) => ReactNode;
  /** filled draws a surface disc behind the icon. */
  filled?: boolean;
  disabled?: boolean;
  style?: StyleProp<ViewStyle>;
}

export function IconButton({ icon, filled = false, disabled = false, style, ...rest }: IconButtonProps) {
  const { colors, reducedMotion } = useTheme();
  return (
    <Pressable
      {...rest}
      accessibilityRole="button"
      accessibilityState={{ disabled }}
      disabled={disabled}
      hitSlop={4}
      style={({ pressed }) => [
        {
          width: minTouch,
          height: minTouch,
          borderRadius: minTouch / 2,
          alignItems: "center",
          justifyContent: "center",
          backgroundColor: filled ? colors.surface : "transparent",
          opacity: disabled ? 0.4 : 1,
        },
        pressed && !disabled ? (reducedMotion ? { opacity: 0.7 } : { transform: [{ scale: 0.94 }] }) : null,
        style,
      ]}
    >
      {icon({ color: colors.text, size: 22 })}
    </Pressable>
  );
}
