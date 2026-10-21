// The shutter and its coverage dial (roadmap F2 item 5; design 5, after Object Capture's coverage ring;
// styles.css .dial-wrap, .shutter): a white shutter with one segment per needed shot around it, each turning
// white once its shot is stored. Neutral on the photo: white, never tinted (design 2).
import { Pressable, View } from "react-native";
import Svg, { Path } from "react-native-svg";

import { camera, hiddenSvg } from "@/ui";

const SIZE = 94;
const SHUTTER = 76;
const R = 47;
/** Degrees left open between segments. */
const GAP = 18;

function arc(from: number, to: number): string {
  // Angles in degrees, clockwise from the top, on a 100 × 100 view.
  const pt = (deg: number) => {
    const rad = ((deg - 90) * Math.PI) / 180;
    return `${(50 + R * Math.cos(rad)).toFixed(2)} ${(50 + R * Math.sin(rad)).toFixed(2)}`;
  };
  return `M${pt(from)} A${R} ${R} 0 ${to - from > 180 ? 1 : 0} 1 ${pt(to)}`;
}

export interface CoverageDialProps {
  /** One per needed shot, in order. */
  segments: { stored: boolean }[];
  /** "take" fills a free slot, "retake" replaces the mode's shot, "busy" waits for the one on its way. */
  shutter: "take" | "retake" | "busy";
  onShutter: () => void;
  /** "1 of 2 needed shots", read with the shutter. */
  coverageText: string;
}

export function CoverageDial({ segments, shutter, onShutter, coverageText }: CoverageDialProps) {
  const n = Math.max(segments.length, 1);
  const span = 360 / n;
  const disabled = shutter === "busy";
  return (
    <View style={{ width: SIZE, height: SIZE, alignItems: "center", justifyContent: "center" }}>
      <Svg width={SIZE} height={SIZE} viewBox="0 0 100 100" style={{ position: "absolute" }} {...hiddenSvg}>
        {segments.map((seg, i) => (
          <Path
            key={i}
            testID={`dial-segment-${i}`}
            d={arc(i * span + GAP / 2, (i + 1) * span - GAP / 2)}
            stroke={seg.stored ? camera.ink : camera.track}
            strokeWidth={4}
            strokeLinecap="round"
            fill="none"
          />
        ))}
      </Svg>
      <Pressable
        testID="shutter"
        accessibilityRole="button"
        accessibilityLabel={shutter === "retake" ? "Retake photo" : "Take photo"}
        accessibilityHint={coverageText}
        accessibilityState={{ disabled }}
        disabled={disabled}
        onPress={onShutter}
        style={{
          width: SHUTTER,
          height: SHUTTER,
          borderRadius: SHUTTER / 2,
          borderWidth: 4,
          borderColor: camera.ink,
          alignItems: "center",
          justifyContent: "center",
          opacity: disabled ? 0.5 : 1,
        }}
      >
        {({ pressed }) => (
          <View
            style={{ width: 62, height: 62, borderRadius: 31, backgroundColor: camera.ink, transform: [{ scale: pressed ? 0.88 : 1 }] }}
          />
        )}
      </Pressable>
    </View>
  );
}
