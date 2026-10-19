import type { ReactNode } from "react";
import { View } from "react-native";

import { useTheme } from "../theme";
import { Text } from "./Text";

/** A titled group; the title is in the display face at 22 pt (design 3.1). */
export function Section({ title, trailing, children }: { title: string; trailing?: ReactNode; children: ReactNode }) {
  const t = useTheme();
  return (
    <View style={{ gap: t.space.md }}>
      <View style={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between" }}>
        <Text variant="displaySmall" accessibilityRole="header">
          {title}
        </Text>
        {trailing}
      </View>
      {children}
    </View>
  );
}
