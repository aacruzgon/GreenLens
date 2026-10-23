// The sideways cards under the diagnosis (roadmap 10 item 3; design 3.1, the featured album with the next one
// peeking): the risk of spread and what else the photos found, each 84% of the width so the next one shows.
import { Pressable, ScrollView, View, useWindowDimensions } from "react-native";

import { LineIcon, Text, gutter, radii, space, useTheme, type LineIconName } from "@/ui";

import type { FeatureCard } from "../model/result";

export interface FeatureCardsProps {
  cards: FeatureCard[];
  fill: string;
  onReport: (url: string) => void;
}

const GAP = 10;

export function FeatureCards({ cards, fill, onReport }: FeatureCardsProps) {
  const { colors } = useTheme();
  const { width } = useWindowDimensions();
  if (cards.length === 0) return null;
  const cardWidth = cards.length === 1 ? width - gutter * 2 : Math.round((width - gutter * 2) * 0.84);
  return (
    <ScrollView
      horizontal
      showsHorizontalScrollIndicator={false}
      snapToInterval={cardWidth + GAP}
      decelerationRate="fast"
      contentContainerStyle={{ paddingHorizontal: gutter, gap: GAP }}
      testID="feature-cards"
    >
      {cards.map((c) => {
        const icon: LineIconName = c.kind === "risk" ? "drop" : "bug";
        const fg = c.tone === "healthy" ? colors.healthy : c.tone === "watch" ? colors.watch : colors.textMuted;
        const bg = c.tone === "healthy" ? colors.healthySoft : c.tone === "watch" ? colors.watchSoft : colors.surfaceMuted;
        return (
          <View
            key={c.key}
            accessible
            accessibilityLabel={[c.eyebrow, c.title, c.detail].filter(Boolean).join(". ")}
            style={{ width: cardWidth, flexDirection: "row", alignItems: "center", gap: space[3], padding: space[3], borderRadius: 22, borderCurve: "continuous", backgroundColor: fill }}
          >
            <View style={{ width: 56, height: 56, borderRadius: 14, alignItems: "center", justifyContent: "center", backgroundColor: bg }}>
              <LineIcon name={icon} color={fg} size={24} />
            </View>
            <View style={{ flex: 1, gap: 1 }}>
              <Text variant="caption1" style={{ fontWeight: "600", color: c.kind === "risk" ? fg : colors.textMuted }}>
                {c.eyebrow}
              </Text>
              <Text variant="callout" emphasis>
                {c.title}
              </Text>
              {c.detail ? (
                <Text variant="footnote" color="textMuted">
                  {c.detail}
                </Text>
              ) : null}
              {c.reportUrl ? (
                <Pressable accessibilityRole="link" onPress={() => onReport(c.reportUrl!)} hitSlop={10} style={{ marginTop: 2, alignSelf: "flex-start", borderRadius: radii.small }}>
                  <Text variant="footnote" color="accent" emphasis>
                    How to report
                  </Text>
                </Pressable>
              ) : null}
            </View>
          </View>
        );
      })}
    </ScrollView>
  );
}
