// The phase line (design 5, after Flighty): the stage now in semibold, how long it has run in the accent, and what
// comes next on its own line (styles.css .phase). It sits on a solid card, where the accent may go (design 3.1).
import { View } from "react-native";

import { Text } from "../primitives/Text";
import { useTheme } from "../theme";

export interface PhaseLineProps {
  now: string;
  /** "6 s", "in 18 s". */
  time?: string;
  /** "Next: weighing the evidence". */
  next?: string;
}

export function PhaseLine({ now, time, next }: PhaseLineProps) {
  const { colors } = useTheme();
  return (
    <View accessible accessibilityLabel={[now, time, next].filter(Boolean).join(", ")} style={{ gap: 2 }}>
      <View style={{ flexDirection: "row", flexWrap: "wrap", alignItems: "baseline", columnGap: 8 }}>
        <Text variant="subheadline" style={{ fontWeight: "600" }}>
          {now}
        </Text>
        {time ? (
          <Text variant="subheadline" style={{ color: colors.accent, fontWeight: "600", fontVariant: ["tabular-nums"] }}>
            {time}
          </Text>
        ) : null}
      </View>
      {next ? (
        <Text variant="footnote" color="textMuted">
          {next}
        </Text>
      ) : null}
    </View>
  );
}
