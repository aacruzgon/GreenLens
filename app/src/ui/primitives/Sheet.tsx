// The content of a sheet. The sheet itself, its grabber, glass and curve are the system's (expo-router
// formSheet; design.md 2); this wrapper gives the content its surface, padding and an optional title.
import type { ReactNode } from "react";
import { ScrollView, View, type StyleProp, type ViewStyle } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { gutter, space } from "../tokens";
import { Text } from "./Text";

export interface SheetProps {
  title?: string;
  children: ReactNode;
  /** Scrolls the content; off for short sheets sized to their content. */
  scroll?: boolean;
  /** Pinned under the content, such as the sheet's one primary pill. */
  footer?: ReactNode;
  style?: StyleProp<ViewStyle>;
}

export function Sheet({ title, children, scroll = true, footer, style }: SheetProps) {
  const insets = useSafeAreaInsets();
  const body = (
    <View style={{ gap: space[4] }}>
      {title ? <Text variant="title3" emphasis>{title}</Text> : null}
      {children}
    </View>
  );
  return (
    <View style={[{ flex: scroll ? 1 : undefined, paddingTop: space[6] }, style]}>
      {scroll ? (
        <ScrollView contentContainerStyle={{ paddingHorizontal: gutter, paddingBottom: space[4] }} keyboardShouldPersistTaps="handled">
          {body}
        </ScrollView>
      ) : (
        <View style={{ paddingHorizontal: gutter, paddingBottom: space[4] }}>{body}</View>
      )}
      {footer ? <View style={{ paddingHorizontal: gutter, paddingBottom: Math.max(insets.bottom, space[4]) }}>{footer}</View> : null}
    </View>
  );
}
