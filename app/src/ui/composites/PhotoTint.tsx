// The diagnosis's photo (design 3.1, after Apple Music's artist page): the scan photo fades into a ground tinted
// from it, and the screen below continues on that ground. A dark scrim at the top keeps the bar buttons readable
// over a bright photo (styles.css .scan-hero). The tint itself is computed by the caller (ui/tint.ts).
import type { ReactNode } from "react";
import { View } from "react-native";
import type { ImageSource } from "expo-image";

import { MeltPhoto } from "./MeltPhoto";
import { TopScrim } from "./TopEdge";

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

export function PhotoTint({ source, accessibilityLabel, ground, height, children }: PhotoTintProps) {
  return (
    <View testID="photo-tint">
      <MeltPhoto source={source} accessibilityLabel={accessibilityLabel} groundColor={ground} melt="profile" height={height} />
      <TopScrim />
      {children ? <View style={{ position: "absolute", left: 0, right: 0, bottom: 0 }}>{children}</View> : null}
    </View>
  );
}
