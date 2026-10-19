import type { ReactNode } from "react";
import { View } from "react-native";

import { Screen, Text, space } from "@/ui";

// Sign-in screens start on the wash with the name in the display face (design 3.1; screens 3.10).
export function AuthShell({ title, subtitle, children }: { title: string; subtitle?: string; children: ReactNode }) {
  return (
    <Screen>
      <View style={{ paddingTop: space[12], gap: space[2] }}>
        <Text variant="hero" accessibilityRole="header">
          PDM
        </Text>
        <Text variant="title2">{title}</Text>
        {subtitle ? (
          <Text variant="body" color="textMuted">
            {subtitle}
          </Text>
        ) : null}
      </View>
      <View style={{ marginTop: space[8], gap: space[4] }}>{children}</View>
    </Screen>
  );
}
