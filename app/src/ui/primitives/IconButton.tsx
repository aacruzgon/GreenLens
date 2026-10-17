import { Pressable, type PressableProps } from "react-native";
import { Ionicons } from "@expo/vector-icons";

import { useTheme } from "../theme";
import type { ColorRoles } from "../tokens";
import { MIN_TOUCH } from "../tokens";

export interface IconButtonProps extends Omit<PressableProps, "children"> {
  icon: keyof typeof Ionicons.glyphMap;
  /** Required: an icon-only control needs a spoken label. */
  label: string;
  color?: keyof ColorRoles;
  size?: number;
}

export function IconButton({ icon, label, color = "accent", size = 24, style, ...rest }: IconButtonProps) {
  const t = useTheme();
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label}
      hitSlop={8}
      style={(state) => [
        { width: MIN_TOUCH, height: MIN_TOUCH, alignItems: "center", justifyContent: "center", opacity: state.pressed ? 0.6 : 1 },
        typeof style === "function" ? style(state) : style,
      ]}
      {...rest}
    >
      <Ionicons name={icon} size={size} color={t.colors[color]} />
    </Pressable>
  );
}
