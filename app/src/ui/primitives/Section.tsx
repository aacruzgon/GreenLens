import type { ReactNode } from "react";
import { View } from "react-native";

import { useTheme } from "../theme";
import { Text } from "./Text";

export function Section({ title, trailing, children }: { title: string; trailing?: ReactNode; children: ReactNode }) {
  const t = useTheme();
  return (
    <View style={{ gap: t.space.sm }}>
      <View style={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between" }}>
        <Text variant="title3" accessibilityRole="header">
          {title}
        </Text>
        {trailing}
      </View>
      {children}
    </View>
  );
}
