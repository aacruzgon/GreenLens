// What a screen shows when it has nothing yet: a picture or icon, a plain title, one line on what to do,
// and at most one action (the screen's one primary pill when it has none other).
import type { ReactNode } from "react";
import { View, type StyleProp, type ViewStyle } from "react-native";

import { space } from "../tokens";
import { Button, type ButtonProps } from "./Button";
import { Text } from "./Text";

export interface EmptyStateProps {
  title: string;
  message?: string;
  icon?: ReactNode;
  action?: Pick<ButtonProps, "label" | "onPress" | "icon" | "variant">;
  style?: StyleProp<ViewStyle>;
}

export function EmptyState({ title, message, icon, action, style }: EmptyStateProps) {
  return (
    <View style={[{ alignItems: "center", gap: space[3], paddingVertical: space[10], paddingHorizontal: space[6] }, style]}>
      {icon}
      <Text variant="title3" emphasis align="center">
        {title}
      </Text>
      {message ? (
        <Text variant="body" color="textMuted" align="center">
          {message}
        </Text>
      ) : null}
      {action ? <Button {...action} style={{ alignSelf: "center", marginTop: space[2] }} /> : null}
    </View>
  );
}
