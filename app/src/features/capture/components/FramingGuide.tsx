// The framing guide (roadmap F2 item 4; styles.css .guide): four white corners where the subject goes, sized for
// the chosen shot. The whole plant gets a tall frame; close shots a wider, squarer one.
import { View } from "react-native";

import { camera } from "@/ui";

import type { CaptureKind } from "../model/shotPlan";

/** The frame as fractions of the preview (mockup: the whole plant 24%–80% tall, close shots 32%–61%). */
const FRAMES: Record<CaptureKind, { top: string; bottom: string; side: string }> = {
  whole_plant: { top: "24%", bottom: "20%", side: "14%" },
  leaf_closeup: { top: "32%", bottom: "39%", side: "14%" },
  leaf_underside: { top: "32%", bottom: "39%", side: "14%" },
  soil_surface: { top: "36%", bottom: "30%", side: "10%" },
  flower_fruit: { top: "32%", bottom: "39%", side: "18%" },
};

const ARM = 30;
const LINE = 3;
const RADIUS = 14;

export function FramingGuide({ kind }: { kind: CaptureKind }) {
  const f = FRAMES[kind];
  const corner = (pos: object, radius: object, hide: object) => (
    <View style={{ position: "absolute", width: ARM, height: ARM, borderColor: camera.ink, borderWidth: LINE, ...hide, ...radius, ...pos }} />
  );
  return (
    <View
      pointerEvents="none"
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      testID="framing-guide"
      style={{ position: "absolute", top: f.top as `${number}%`, bottom: f.bottom as `${number}%`, left: f.side as `${number}%`, right: f.side as `${number}%` }}
    >
      {corner({ top: 0, left: 0 }, { borderTopLeftRadius: RADIUS }, { borderRightWidth: 0, borderBottomWidth: 0 })}
      {corner({ top: 0, right: 0 }, { borderTopRightRadius: RADIUS }, { borderLeftWidth: 0, borderBottomWidth: 0 })}
      {corner({ bottom: 0, left: 0 }, { borderBottomLeftRadius: RADIUS }, { borderRightWidth: 0, borderTopWidth: 0 })}
      {corner({ bottom: 0, right: 0 }, { borderBottomRightRadius: RADIUS }, { borderLeftWidth: 0, borderTopWidth: 0 })}
    </View>
  );
}
