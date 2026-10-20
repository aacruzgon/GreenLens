// The framing guide (roadmap F2 item 4; styles.css .guide): four white corners where the subject goes, sized for
// the chosen shot. The whole plant gets a tall frame; close shots a wider, squarer one.
import { useState } from "react";
import { View } from "react-native";

import { camera } from "@/ui";

import type { CaptureKind } from "../model/shotPlan";

/** The frame as fractions of the preview (mockup: the whole plant 24%–80% tall, close shots 32%–61%). */
const FRAMES: Record<CaptureKind, { top: number; bottom: number; side: number }> = {
  whole_plant: { top: 0.24, bottom: 0.2, side: 0.14 },
  leaf_closeup: { top: 0.32, bottom: 0.39, side: 0.14 },
  leaf_underside: { top: 0.32, bottom: 0.39, side: 0.14 },
  soil_surface: { top: 0.36, bottom: 0.3, side: 0.1 },
  flower_fruit: { top: 0.32, bottom: 0.39, side: 0.18 },
};

const ARM = 30;
const LINE = 3;
const RADIUS = 14;
/** Space kept between the drop card and the frame's top corners. */
const GAP = 12;

/** `clearTop`: where the drop card ends. A card grown by thumbnails would cover the frame, so the frame starts below it. */
export function FramingGuide({ kind, clearTop = 0 }: { kind: CaptureKind; clearTop?: number }) {
  const [height, setHeight] = useState(0);
  const f = FRAMES[kind];
  const bottom = height * f.bottom;
  // Below the card, but always tall enough for its four corners.
  const top = Math.min(Math.max(height * f.top, clearTop ? clearTop + GAP : 0), height - bottom - 2 * ARM);
  const corner = (pos: object, radius: object, hide: object) => (
    <View style={{ position: "absolute", width: ARM, height: ARM, borderColor: camera.ink, borderWidth: LINE, ...hide, ...radius, ...pos }} />
  );
  return (
    <View
      pointerEvents="none"
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      testID="framing-guide"
      onLayout={(e) => setHeight(e.nativeEvent.layout.height)}
      style={{ position: "absolute", top: 0, bottom: 0, left: 0, right: 0 }}
    >
      {height ? (
        <View style={{ position: "absolute", top, bottom, left: `${f.side * 100}%`, right: `${f.side * 100}%` }}>
          {corner({ top: 0, left: 0 }, { borderTopLeftRadius: RADIUS }, { borderRightWidth: 0, borderBottomWidth: 0 })}
          {corner({ top: 0, right: 0 }, { borderTopRightRadius: RADIUS }, { borderLeftWidth: 0, borderBottomWidth: 0 })}
          {corner({ bottom: 0, left: 0 }, { borderBottomLeftRadius: RADIUS }, { borderRightWidth: 0, borderTopWidth: 0 })}
          {corner({ bottom: 0, right: 0 }, { borderBottomRightRadius: RADIUS }, { borderLeftWidth: 0, borderTopWidth: 0 })}
        </View>
      ) : null}
    </View>
  );
}
