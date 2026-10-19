import { View, type StyleProp, type ViewStyle } from "react-native";
import { Image, type ImageSource } from "expo-image";

import { Fade, type FadeStop } from "../primitives/Fade";

export interface MeltPhotoProps {
  source: ImageSource | number;
  /** The colour the photo dissolves into: the screen's background, or the card's surface. */
  ground: string;
  /** Where the photo starts to fade and where it is gone, from 0 at the top to 1 at the bottom. */
  from?: number;
  to?: number;
  /** Darkens the top edge so the status bar and bar buttons stay readable over the photo. */
  shadeTop?: boolean;
  style?: StyleProp<ViewStyle>;
  accessibilityLabel?: string;
}

/**
 * Melt (design 3.1): a photo that dissolves into the ground instead of sitting in a frame. The fade
 * eases through a middle stop so the edge never reads as a line.
 */
export function MeltPhoto({ source, ground, from = 0.38, to = 0.88, shadeTop, style, accessibilityLabel }: MeltPhotoProps) {
  const mid = from + (to - from) * 0.48;
  const stops: FadeStop[] = [
    { at: 0, color: ground, opacity: 0 },
    { at: from, color: ground, opacity: 0 },
    { at: mid, color: ground, opacity: 0.45 },
    { at: to, color: ground, opacity: 1 },
    { at: 1, color: ground, opacity: 1 },
  ];
  return (
    <View style={[{ overflow: "hidden" }, style]} accessible={!!accessibilityLabel} accessibilityRole={accessibilityLabel ? "image" : undefined} accessibilityLabel={accessibilityLabel}>
      <Image source={source} style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0 }} contentFit="cover" transition={250} />
      <Fade stops={stops} />
      {shadeTop ? (
        <Fade
          stops={[
            { at: 0, color: "#000000", opacity: 0.32 },
            { at: 0.22, color: "#000000", opacity: 0 },
          ]}
        />
      ) : null}
    </View>
  );
}
