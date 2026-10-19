import type { ReactNode } from "react";
import { RefreshControl, ScrollView, View, type ScrollViewProps } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { useTheme } from "../theme";
import { Fade } from "./Fade";

export interface ScreenProps extends ScrollViewProps {
  children: ReactNode;
  /** Pull to refresh. */
  onRefresh?: () => void;
  refreshing?: boolean;
  /** Pinned under the content, above the home indicator: the screen's main action. */
  footer?: ReactNode;
  /** Pages without a native header (sign-in, capture) pad the top safe area themselves. */
  padTop?: boolean;
  /** Full-bleed content above the padded column, drawn from the top edge under the status bar. */
  hero?: ReactNode;
}

/**
 * A scrolling page that starts with the sage wash at the top and melts into the background (design 3.1),
 * sized for phones and centred on wide windows.
 */
export function Screen({ children, onRefresh, refreshing = false, footer, padTop, hero, contentContainerStyle, ...rest }: ScreenProps) {
  const t = useTheme();
  const insets = useSafeAreaInsets();
  const column = {
    padding: t.space.lg,
    paddingTop: hero ? t.space.xl : padTop ? insets.top + t.space.lg : t.space.lg,
    paddingBottom: footer ? t.space.lg : insets.bottom + t.space.xl,
    gap: t.space.lg,
    width: "100%" as const,
    maxWidth: 720,
    alignSelf: "center" as const,
  };
  return (
    <View style={{ flex: 1, backgroundColor: t.colors.background }}>
      {hero ? null : (
        <Fade
          style={{ position: "absolute", top: 0, left: 0, right: 0, height: 560 }}
          stops={[
            { at: 0, color: t.colors.wash, opacity: 1 },
            { at: 1, color: t.colors.wash, opacity: 0 },
          ]}
        />
      )}
      <ScrollView
        contentInsetAdjustmentBehavior={hero ? "never" : "automatic"}
        keyboardShouldPersistTaps="handled"
        refreshControl={onRefresh ? <RefreshControl refreshing={refreshing} onRefresh={onRefresh} tintColor={t.colors.accent} /> : undefined}
        contentContainerStyle={hero ? undefined : [column, contentContainerStyle]}
        {...rest}
      >
        {hero ? (
          <>
            {hero}
            <View style={[column, contentContainerStyle]}>{children}</View>
          </>
        ) : (
          children
        )}
      </ScrollView>
      {footer ? (
        <View
          style={{
            paddingHorizontal: t.space.lg,
            paddingTop: t.space.md,
            paddingBottom: insets.bottom + t.space.md,
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
