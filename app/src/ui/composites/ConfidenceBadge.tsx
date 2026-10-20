// A confidence band as a word and three bars (styles.css .band). Only for findings and measurements,
// never the diagnosis (frontend README 14; screens 6).
import { View, type StyleProp, type ViewStyle } from "react-native";

import type { ConfidenceBand } from "@/domain/types";

import { Text } from "../primitives/Text";
import { useTheme } from "../theme";

const FILLED: Record<ConfidenceBand, number> = { low: 1, medium: 2, high: 3 };
const BAR_HEIGHTS = [6, 9, 12];

export interface ConfidenceBadgeProps {
  band: ConfidenceBand;
  style?: StyleProp<ViewStyle>;
}

export function ConfidenceBadge({ band, style }: ConfidenceBadgeProps) {
  const { colors } = useTheme();
  const filled = FILLED[band];
  return (
    <View
      accessible
      accessibilityLabel={`${band} confidence`}
      style={[{ flexDirection: "row", alignItems: "center", gap: 6, alignSelf: "flex-start" }, style]}
    >
      <View style={{ flexDirection: "row", alignItems: "flex-end", gap: 2 }}>
        {BAR_HEIGHTS.map((h, i) => (
          <View
            key={h}
            testID={i < filled ? "band-bar-on" : "band-bar-off"}
            style={{ width: 4, height: h, borderRadius: 1, backgroundColor: i < filled ? colors.textMuted : colors.border }}
          />
        ))}
      </View>
      <Text variant="footnote" color="textMuted">
        {band}
      </Text>
    </View>
  );
}
