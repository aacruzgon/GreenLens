// The home's hero (styles.css .h-hero under data-melt): a plant photo melting into the wash, the garden's
// name at 54 pt in Urbanist over its faded edge, the place and day above it and a line on the weather below.
// A light shade at the top keeps the light status bar readable over the photo; nothing tinted sits on it.
import { useId } from "react";
import { View } from "react-native";
import type { ImageSource } from "expo-image";
import Svg, { Defs, LinearGradient, Rect, Stop } from "react-native-svg";

import { MeltPhoto, Skeleton, Text, space } from "@/ui";

export interface GardenHeroProps {
  /** Undefined while the garden loads. */
  name?: string;
  /** "Toronto · Monday". */
  eyebrow?: string;
  /** "11 °C, 84% humidity. Frost possible Thursday." */
  weatherLine?: string;
  /** "Soil data loading" while the soil profile is pending (screens 3.1). */
  note?: string;
  /** null: no photo, the name sits on the wash. undefined: still loading, the hero keeps its height. */
  photo: ImageSource | number | null | undefined;
  /** What the photo shows ("Basil"). */
  photoLabel: string;
  /** The colour the photo melts into: the wash as it stands at the hero's bottom edge. */
  ground: string;
  /** The hero's height with a photo (styles.css: 470). */
  height: number;
  /** The status bar's height: text starts below it when there is no photo. */
  topInset: number;
}

function TopShade() {
  const id = `shade${useId().replace(/[^a-zA-Z0-9]/g, "")}`;
  return (
    <Svg
      width="100%"
      height="100%"
      style={{ position: "absolute", top: 0, left: 0 }}
      pointerEvents="none"
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
    >
      <Defs>
        <LinearGradient id={id} x1="0" y1="0" x2="0" y2="1">
          <Stop offset={0} stopColor="#000000" stopOpacity={0.32} />
          <Stop offset={0.22} stopColor="#000000" stopOpacity={0} />
        </LinearGradient>
      </Defs>
      <Rect x="0" y="0" width="100%" height="100%" fill={`url(#${id})`} />
    </Svg>
  );
}

function HeroText({ name, eyebrow, weatherLine, note }: Pick<GardenHeroProps, "name" | "eyebrow" | "weatherLine" | "note">) {
  return (
    <View style={{ paddingHorizontal: space[5], paddingBottom: 6, gap: space[1] }}>
      {eyebrow ? (
        <Text variant="subheadline" color="textMuted">
          {eyebrow}
        </Text>
      ) : null}
      {name ? (
        <Text variant="hero" accessibilityRole="header" numberOfLines={2} style={{ marginTop: space[1], marginBottom: 6 }}>
          {name}
        </Text>
      ) : (
        <Skeleton height={52} width="70%" style={{ marginVertical: space[2] }} />
      )}
      {weatherLine ? (
        <Text variant="subheadline" color="textMuted" style={{ maxWidth: 320 }}>
          {weatherLine}
        </Text>
      ) : null}
      {note ? (
        <Text variant="footnote" color="textMuted">
          {note}
        </Text>
      ) : null}
    </View>
  );
}

export function GardenHero({ photo, photoLabel, ground, height, topInset, ...text }: GardenHeroProps) {
  if (photo === null) {
    return (
      <View style={{ paddingTop: topInset + space[10] + space[4] }} testID="garden-hero">
        <HeroText {...text} />
      </View>
    );
  }
  return (
    <View testID="garden-hero">
      <MeltPhoto source={photo ?? null} accessibilityLabel={photo ? photoLabel : ""} groundColor={ground} melt="hero" height={height}>
        <HeroText {...text} />
      </MeltPhoto>
      <View style={{ position: "absolute", top: 0, left: 0, right: 0, height }} pointerEvents="none">
        <TopShade />
      </View>
    </View>
  );
}
