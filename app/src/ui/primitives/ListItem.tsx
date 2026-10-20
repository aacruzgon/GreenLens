// A list row (styles.css .row: 12 × 16 padding, 12 pt gap). At least 44 pt tall; a chevron when it navigates.
import type { ReactNode } from "react";
import { Pressable, View, type StyleProp, type ViewStyle } from "react-native";
import Svg, { Path } from "react-native-svg";

import { hiddenSvg } from "../svgId";
import { useTheme } from "../theme";
import { minTouch, space } from "../tokens";
import { Text } from "./Text";

export interface ListItemProps {
  title: string;
  subtitle?: string;
  leading?: ReactNode;
  /** Shown at the right end, before the chevron. */
  trailing?: ReactNode;
  onPress?: () => void;
  /** Draws the chevron; defaults to true when onPress is set. */
  chevron?: boolean;
  accessibilityLabel?: string;
  accessibilityHint?: string;
  style?: StyleProp<ViewStyle>;
}

function Chevron({ color }: { color: string }) {
  return (
    <Svg width={8} height={14} viewBox="0 0 8 14" {...hiddenSvg}>
      <Path d="M1 1l6 6-6 6" stroke={color} strokeWidth={2} fill="none" strokeLinecap="round" strokeLinejoin="round" />
    </Svg>
  );
}

export function ListItem({ title, subtitle, leading, trailing, onPress, chevron, accessibilityLabel, accessibilityHint, style }: ListItemProps) {
  const { colors } = useTheme();
  const showChevron = chevron ?? onPress !== undefined;
  const row: ViewStyle = {
    flexDirection: "row",
    alignItems: "center",
    gap: space[3],
    minHeight: minTouch,
    paddingVertical: space[3],
    paddingHorizontal: space[4],
  };
  const content = (
    <>
      {leading}
      <View style={{ flex: 1, minWidth: 0, gap: 2 }}>
        <Text variant="body">{title}</Text>
        {subtitle ? (
          <Text variant="subheadline" color="textMuted">
            {subtitle}
          </Text>
        ) : null}
      </View>
      {trailing}
      {showChevron ? <Chevron color={colors.textMuted} /> : null}
    </>
  );
  const label = accessibilityLabel ?? (subtitle ? `${title}, ${subtitle}` : title);
  if (onPress) {
    return (
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={label}
        accessibilityHint={accessibilityHint}
        onPress={onPress}
        style={({ pressed }) => [row, pressed ? { backgroundColor: colors.surfaceMuted } : null, style]}
      >
        {content}
      </Pressable>
    );
  }
  return (
    <View accessible accessibilityLabel={label} style={[row, style]}>
      {content}
    </View>
  );
}
