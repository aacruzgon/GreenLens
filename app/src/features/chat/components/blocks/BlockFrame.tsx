// The frame every answer block sits in: a solid card (no content card is glass, design 6) with a small heading,
// read by VoiceOver as one summary when the block has nothing to tap.
import type { ReactNode } from "react";
import { View } from "react-native";

import { LineIcon, Text, space, useCardStyle, useTheme, type LineIconName } from "@/ui";

export interface BlockFrameProps {
  title: string;
  icon: LineIconName;
  /** The block in words (blocks.ts, blockSummary). */
  summary: string;
  children: ReactNode;
  testID?: string;
}

export function BlockFrame({ title, icon, summary, children, testID }: BlockFrameProps) {
  const { colors } = useTheme();
  const card = useCardStyle();
  return (
    <View style={[card, { padding: space[4], gap: space[3] }]} testID={testID} accessibilityHint={summary}>
      <View style={{ flexDirection: "row", alignItems: "center", gap: space[2] }} accessible accessibilityRole="header" accessibilityLabel={`${title}. ${summary}`}>
        <LineIcon name={icon} color={colors.textMuted} size={16} />
        <Text variant="footnote" color="textMuted" emphasis style={{ textTransform: "uppercase", letterSpacing: 0.6 }}>
          {title}
        </Text>
      </View>
      {children}
    </View>
  );
}
