// A titled group of content. Conservatory sets section titles in the display face at 22 pt
// (directions.js sections "title", sectionFont "display", sectionSize 22; styles.css .sec-h).
import type { ReactNode } from "react";
import { Pressable, View, type StyleProp, type ViewStyle } from "react-native";

import { space } from "../tokens";
import { Text } from "./Text";

export interface SectionProps {
  title: string;
  /** A link at the right of the title ("See all"), in the accent at 15 pt (styles.css .sec-h a). */
  action?: { label: string; onPress: () => void; accessibilityLabel?: string };
  children?: ReactNode;
  style?: StyleProp<ViewStyle>;
}

export function Section({ title, action, children, style }: SectionProps) {
  return (
    <View style={[{ gap: 10 }, style]}>
      <View style={{ flexDirection: "row", alignItems: "baseline", justifyContent: "space-between", marginHorizontal: space[1], gap: space[3] }}>
        <Text variant="section" style={{ flexShrink: 1 }}>
          {title}
        </Text>
        {action ? (
          <Pressable
            accessibilityRole="link"
            accessibilityLabel={action.accessibilityLabel ?? action.label}
            onPress={action.onPress}
            hitSlop={12}
            style={{ minHeight: 28, justifyContent: "center" }}
          >
            <Text variant="subheadline" color="accent">
              {action.label}
            </Text>
          </Pressable>
        ) : null}
      </View>
      {children}
    </View>
  );
}
