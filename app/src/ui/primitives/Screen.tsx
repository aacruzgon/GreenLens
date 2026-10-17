import type { ReactNode } from "react";
import { RefreshControl, ScrollView, View, type ScrollViewProps } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { useTheme } from "../theme";

export interface ScreenProps extends ScrollViewProps {
  children: ReactNode;
  /** Pull to refresh. */
  onRefresh?: () => void;
  refreshing?: boolean;
  /** Pinned under the content, above the home indicator: the screen's main action. */
  footer?: ReactNode;
  /** Pages without a native header (sign-in, capture) pad the top safe area themselves. */
  padTop?: boolean;
}

/** A scrolling page on the background colour, sized for phones and centred on wide web windows. */
export function Screen({ children, onRefresh, refreshing = false, footer, padTop, contentContainerStyle, ...rest }: ScreenProps) {
  const t = useTheme();
  const insets = useSafeAreaInsets();
  return (
    <View style={{ flex: 1, backgroundColor: t.colors.background }}>
      <ScrollView
        contentInsetAdjustmentBehavior="automatic"
        keyboardShouldPersistTaps="handled"
        refreshControl={onRefresh ? <RefreshControl refreshing={refreshing} onRefresh={onRefresh} tintColor={t.colors.accent} /> : undefined}
        contentContainerStyle={[
          {
            padding: t.space.lg,
            paddingTop: padTop ? insets.top + t.space.lg : t.space.lg,
            paddingBottom: footer ? t.space.lg : insets.bottom + t.space.xl,
            gap: t.space.lg,
            width: "100%",
            maxWidth: 720,
            alignSelf: "center",
          },
          contentContainerStyle,
        ]}
        {...rest}
      >
        {children}
      </ScrollView>
      {footer ? (
        <View
          style={{
            paddingHorizontal: t.space.lg,
            paddingTop: t.space.md,
            paddingBottom: insets.bottom + t.space.md,
            borderTopWidth: 1,
            borderTopColor: t.colors.border,
            backgroundColor: t.colors.background,
            width: "100%",
            maxWidth: 720,
            alignSelf: "center",
          }}
        >
          {footer}
        </View>
      ) : null}
    </View>
  );
}
