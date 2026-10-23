// "Not sure yet" with both candidates as two equal plates (design 3.1; styles.css .unsure, .plates): neither is
// drawn as the answer, since the evidence is split (screens 6). A plate shows a picture when there is one.
import type { ReactNode } from "react";
import { View } from "react-native";

import { Text } from "../primitives/Text";
import { useTheme } from "../theme";
import { radii, space } from "../tokens";

export interface CandidatePlate {
  name: string;
  /** A short line under the name, such as the problem's kind ("Fungal"). */
  caption?: string;
  picture?: ReactNode;
}

export interface CandidateCompareProps {
  /** "Not sure yet: early blight or Septoria leaf spot". */
  title: string;
  plates: readonly [CandidatePlate, CandidatePlate];
  /** "One answer settles it." */
  footnote?: string;
}

export function CandidateCompare({ title, plates, footnote }: CandidateCompareProps) {
  const { colors } = useTheme();
  return (
    <View style={{ gap: space[3] }}>
      <Text variant="headline" accessibilityRole="header">
        {title}
      </Text>
      <View style={{ flexDirection: "row", gap: 10 }}>
        {plates.map((p) => (
          <View
            key={p.name}
            accessible
            accessibilityLabel={p.caption ? `${p.name}, ${p.caption}` : p.name}
            style={{ flex: 1, gap: 4, padding: 10, borderRadius: radii.field, borderCurve: "continuous", backgroundColor: colors.surfaceMuted }}
          >
            {p.picture ? <View style={{ borderRadius: radii.small, overflow: "hidden" }}>{p.picture}</View> : null}
            <Text variant="subheadline" style={{ fontWeight: "600" }} numberOfLines={2}>
              {p.name}
            </Text>
            {p.caption ? (
              <Text variant="caption1" color="textMuted">
                {p.caption}
              </Text>
            ) : null}
          </View>
        ))}
      </View>
      {footnote ? (
        <Text variant="footnote" color="textMuted">
          {footnote}
        </Text>
      ) : null}
    </View>
  );
}
