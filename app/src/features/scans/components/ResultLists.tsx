// The diagnosis's lists on the tint (roadmap 10 item 4; screens 3.3): what matched, with "measured" on a
// LiDAR-answered indicator and what was contradicted, then rows of values (weather, soil, measurements,
// organisms) with their bands. Bands only on findings and measurements, never the diagnosis (screens 6).
import { View } from "react-native";

import { ConfidenceBadge, LineIcon, Text, space, useTheme } from "@/ui";

import type { EvidenceRow, ValueRow } from "../model/result";
import { TintCard } from "./TintCard";

function Row({ first, separator, children }: { first: boolean; separator: string; children: React.ReactNode }) {
  return (
    <View style={{ flexDirection: "row", alignItems: "center", gap: space[3], paddingVertical: space[3], paddingHorizontal: space[4], borderTopWidth: first ? 0 : 1, borderTopColor: separator }}>
      {children}
    </View>
  );
}

export function EvidenceList({ rows, fill, separator }: { rows: EvidenceRow[]; fill: string; separator: string }) {
  const { colors } = useTheme();
  return (
    <TintCard fill={fill} testID="evidence-list">
      {rows.map((r, i) => (
        <Row key={r.key} first={i === 0} separator={separator}>
          <View accessible accessibilityLabel={r.matched ? "Matched" : "Contradicted"}>
            <LineIcon name={r.matched ? "check" : "x"} color={r.matched ? colors.healthy : colors.textMuted} size={20} strokeWidth={2} />
          </View>
          <View style={{ flex: 1, gap: 2 }}>
            <View style={{ flexDirection: "row", alignItems: "center", flexWrap: "wrap", gap: space[2] }}>
              <Text variant="body" color={r.matched ? "text" : "textMuted"}>
                {r.text}
              </Text>
              {r.measured ? (
                <View style={{ paddingHorizontal: 7, paddingVertical: 1, borderRadius: 6, backgroundColor: colors.infoSoft }}>
                  <Text variant="caption1" style={{ color: colors.info, fontWeight: "600" }}>
                    measured
                  </Text>
                </View>
              ) : null}
            </View>
            <Text variant="footnote" color="textMuted">
              {r.source}
            </Text>
          </View>
        </Row>
      ))}
    </TintCard>
  );
}

export function ValueList({ rows, fill, separator, testID }: { rows: ValueRow[]; fill: string; separator: string; testID?: string }) {
  return (
    <TintCard fill={fill} testID={testID}>
      {rows.map((r, i) => (
        <Row key={r.key} first={i === 0} separator={separator}>
          <Text variant="body" style={{ flex: 1 }}>
            {r.label}
          </Text>
          <Text variant="body" color="textMuted" style={{ flexShrink: 1, textAlign: "right", fontVariant: ["tabular-nums"] }}>
            {r.value}
          </Text>
          {r.band ? <ConfidenceBadge band={r.band} /> : null}
        </Row>
      ))}
    </TintCard>
  );
}
