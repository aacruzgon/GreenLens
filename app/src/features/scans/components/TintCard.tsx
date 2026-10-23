// A card on the diagnosis's tint (design 3.1: "Your plan ›" and the other lists on translucent cards over the tint,
// with no shadows; styles.css .card under data-scan="profile"). The fill is the tint's card colour, flattened to
// an opaque colour by ui/tint.ts, so it stays a solid card, never glass.
import type { ReactNode } from "react";
import { View, type StyleProp, type ViewStyle } from "react-native";

import { radii } from "@/ui";

export interface TintCardProps {
  fill: string;
  children: ReactNode;
  style?: StyleProp<ViewStyle>;
  testID?: string;
}

export function TintCard({ fill, children, style, testID }: TintCardProps) {
  return (
    <View testID={testID} style={[{ backgroundColor: fill, borderRadius: radii.card, borderCurve: "continuous", overflow: "hidden" }, style]}>
      {children}
    </View>
  );
}
