// Sources, numbered as the plan's steps cite them (screens 5, CitationList; styles.css .sources). Each opens its
// page in the browser; the publisher comes first because that is what the user trusts.
import { Pressable, View } from "react-native";

import type { Citation } from "@/domain/types";

import { Text } from "../primitives/Text";
import { useTheme } from "../theme";
import { minTouch, space } from "../tokens";
import { LineIcon } from "./LineIcon";

export interface CitationListProps {
  citations: readonly Citation[];
  onOpen: (url: string) => void;
  /** Hairline colour between rows; the theme's border by default. */
  separator?: string;
}

/** The small numbered disc a step uses to point at a source. */
export function CiteNumber({ n }: { n: number }) {
  const { colors } = useTheme();
  return (
    <View
      accessibilityLabel={`source ${n}`}
      style={{ minWidth: 18, height: 18, paddingHorizontal: 4, borderRadius: 9, backgroundColor: colors.accentSoft, alignItems: "center", justifyContent: "center" }}
    >
      <Text variant="caption2" style={{ color: colors.accent, fontWeight: "700" }}>
        {n}
      </Text>
    </View>
  );
}

export function CitationList({ citations, onOpen, separator }: CitationListProps) {
  const { colors } = useTheme();
  return (
    <View testID="citation-list">
      {citations.map((c, i) => (
        <Pressable
          key={c.source_id}
          accessibilityRole="link"
          accessibilityLabel={`${i + 1}. ${c.publisher}, ${c.title}`}
          accessibilityHint="Opens in the browser"
          onPress={() => onOpen(c.url)}
          style={({ pressed }) => ({
            flexDirection: "row",
            alignItems: "center",
            gap: space[3],
            minHeight: minTouch,
            paddingVertical: space[2],
            paddingHorizontal: space[4],
            borderTopWidth: i === 0 ? 0 : 1,
            borderTopColor: separator ?? colors.border,
            opacity: pressed ? 0.6 : 1,
          })}
        >
          <CiteNumber n={i + 1} />
          <Text variant="subheadline" style={{ flex: 1 }}>
            {c.publisher}, {c.title}
          </Text>
          <LineIcon name="external" color={colors.textMuted} size={16} />
        </Pressable>
      ))}
    </View>
  );
}
