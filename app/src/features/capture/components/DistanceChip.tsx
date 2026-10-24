// The distance chip on LiDAR phones (architecture 6.7; README 11, capture): how far the centre of the frame is,
// and whether that suits the chosen shot. A solid dark pill on the photo, white text, nothing tinted (design 2).
import { View } from "react-native";

import { Text, camera } from "@/ui";

import { distanceHint } from "../model/distanceRanges";
import type { CaptureKind } from "../model/shotPlan";

export function DistanceChip({ kind, metres }: { kind: CaptureKind; metres: number | null }) {
  const hint = metres === null ? undefined : distanceHint(kind, metres);
  return (
    <View
      testID="distance-chip"
      accessible
      accessibilityLabel={hint ? hint.text : "No distance yet"}
      style={{ alignSelf: "center", flexDirection: "row", alignItems: "center", gap: 6, paddingHorizontal: 12, paddingVertical: 6, borderRadius: 999, backgroundColor: camera.pill }}
    >
      <View style={{ width: 8, height: 8, borderRadius: 4, backgroundColor: hint?.fit === "good" ? camera.ink : "transparent", borderWidth: 1.5, borderColor: camera.ink }} />
      <Text variant="footnote" style={{ color: camera.ink, fontVariant: ["tabular-nums"] }}>
        {hint ? hint.text : "Point at the plant"}
      </Text>
    </View>
  );
}
