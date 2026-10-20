// A content screen. Every screen starts with the wash at the top, fading into the background over
// 560 pt (design.md 3.1, the melt; styles.css .scr under data-melt). An optional hero runs full bleed
// under the status bar; without one the content starts below the safe area.
import type { ReactElement, ReactNode } from "react";
import { ScrollView, View, type RefreshControlProps, type StyleProp, type ViewStyle } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import Svg, { Defs, LinearGradient, Rect, Stop } from "react-native-svg";

import { useSvgId } from "../svgId";
import { useTheme } from "../theme";
import { gutter, space } from "../tokens";

/** The wash's fade length (styles.css: linear-gradient(wash 0, background 560px)). */
export const WASH_HEIGHT = 560;

export interface ScreenProps {
  children: ReactNode;
  /** Full-bleed content at the top, such as a MeltPhoto; it sits under the status bar. */
  hero?: ReactNode;
  /** Scrolls the content (default). */
  scroll?: boolean;
  /** Side gutters on the content (default 16 pt). */
  padded?: boolean;
  /** True when a native header sits above, so the top inset is already taken. */
  underHeader?: boolean;
  refreshControl?: ReactElement<RefreshControlProps>;
  contentStyle?: StyleProp<ViewStyle>;
  testID?: string;
}

export function Wash() {
  const { colors } = useTheme();
  const id = useSvgId("wash");
  return (
    <Svg
      width="100%"
      height={WASH_HEIGHT}
      style={{ position: "absolute", top: 0, left: 0, right: 0 }}
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      pointerEvents="none"
    >
      <Defs>
        <LinearGradient id={id} x1="0" y1="0" x2="0" y2="1">
          <Stop offset="0" stopColor={colors.wash} />
          <Stop offset="1" stopColor={colors.background} />
        </LinearGradient>
      </Defs>
      <Rect x="0" y="0" width="100%" height="100%" fill={`url(#${id})`} />
    </Svg>
  );
}

export function Screen({ children, hero, scroll = true, padded = true, underHeader = false, refreshControl, contentStyle, testID }: ScreenProps) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const top = hero || underHeader ? 0 : insets.top;
  const body = (
    <>
      <Wash />
      {hero}
      <View style={[{ paddingTop: top + (hero ? space[4] : space[2]), paddingHorizontal: padded ? gutter : 0, gap: space[6] }, contentStyle]}>
        {children}
      </View>
    </>
  );
  return (
    <View testID={testID} style={{ flex: 1, backgroundColor: colors.background }}>
      {scroll ? (
        <ScrollView
          contentInsetAdjustmentBehavior={underHeader ? "automatic" : "never"}
          contentContainerStyle={{ flexGrow: 1, paddingBottom: insets.bottom + space[8] }}
          refreshControl={refreshControl}
          keyboardShouldPersistTaps="handled"
        >
          {body}
        </ScrollView>
      ) : (
        <View style={{ flex: 1, paddingBottom: insets.bottom }}>{body}</View>
      )}
    </View>
  );
}
