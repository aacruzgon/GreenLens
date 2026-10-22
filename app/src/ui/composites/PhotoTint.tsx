// The diagnosis's photo (design 3.1, after Apple Music's artist page): the scan photo fades into a ground tinted
// from it, and the screen below continues on that ground. A light scrim at the top keeps the bar buttons readable
// over a bright photo (styles.css .scan-hero). The tint itself is computed by the caller (ui/tint.ts).
import type { ReactNode } from "react";
import { View } from "react-native";
import type { ImageSource } from "expo-image";
import Svg, { Defs, LinearGradient, Rect, Stop } from "react-native-svg";

import { hiddenSvg, useSvgId } from "../svgId";
import { MeltPhoto } from "./MeltPhoto";

export interface PhotoTintProps {
  source: ImageSource | string | number | null;
  accessibilityLabel: string;
  /** The tint's ground colour. */
  ground: string;
  /** The photo's height (styles.css: 4:3.4 of the width). */
  height: number;
  /** Drawn over the faded edge: the diagnosis's name. */
  children?: ReactNode;
}

const SCRIM_HEIGHT = 140;

export function PhotoTint({ source, accessibilityLabel, ground, height, children }: PhotoTintProps) {
  const id = useSvgId("scrim");
  return (
    <View testID="photo-tint">
      <MeltPhoto source={source} accessibilityLabel={accessibilityLabel} groundColor={ground} melt="profile" height={height} />
      <Svg width="100%" height={SCRIM_HEIGHT} style={{ position: "absolute", top: 0, left: 0 }} pointerEvents="none" {...hiddenSvg}>
        <Defs>
          <LinearGradient id={id} x1="0" y1="0" x2="0" y2="1">
            <Stop offset={0} stopColor="#000000" stopOpacity={0.36} />
            <Stop offset={1} stopColor="#000000" stopOpacity={0} />
          </LinearGradient>
        </Defs>
        <Rect x="0" y="0" width="100%" height="100%" fill={`url(#${id})`} />
      </Svg>
      {children ? <View style={{ position: "absolute", left: 0, right: 0, bottom: 0 }}>{children}</View> : null}
    </View>
  );
}
