// The home's hero (styles.css .h-hero under data-melt): a plant photo melting into the wash, the garden's
// name at 54 pt in Urbanist over its faded edge, the place and day above it and a line on the weather below.
// A light shade at the top keeps the light status bar readable over the photo; nothing tinted sits on it.
import { useId } from "react";
import { Pressable, View } from "react-native";
import type { ImageSource } from "expo-image";
import Svg, { Defs, LinearGradient, Rect, Stop } from "react-native-svg";

import { MeltPhoto, Skeleton, Text, hiddenSvg, space, useTheme } from "@/ui";

import { Icon } from "./Icons";

export interface GardenHeroProps {
  /** Undefined while the garden loads. */
  name?: string;
  /** "Toronto · Monday". */
  eyebrow?: string;
  /** "14 °C and light rain, 92% humidity. Frost possible Thursday night." */
  weatherLine?: string;
  /** Opens the garden switcher; set only with two or more gardens, and draws a ▾ after the name. */
  onSwitchGarden?: () => void;
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
      {...hiddenSvg}
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

type HeroTextProps = Pick<GardenHeroProps, "name" | "eyebrow" | "weatherLine" | "note" | "onSwitchGarden">;

function Title({ name, onSwitchGarden }: { name: string; onSwitchGarden?: () => void }) {
  const { colors } = useTheme();
  const title = (
    <Text variant="hero" accessibilityRole="header" numberOfLines={2} style={{ flexShrink: 1 }}>
      {name}
    </Text>
  );
  if (!onSwitchGarden) return <View style={{ marginBottom: 10 }}>{title}</View>;
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={`${name}. Switch garden`}
      onPress={onSwitchGarden}
      testID="garden-switcher"
      style={({ pressed }) => ({ flexDirection: "row", alignItems: "center", gap: 6, marginBottom: 10, opacity: pressed ? 0.6 : 1 })}
    >
      {title}
      <Icon name="chevron" color={colors.textMuted} size={28} strokeWidth={2.2} />
    </Pressable>
  );
}

// .hero-text under data-melt: the eyebrow at 15 pt, the name 4 pt below it with 10 pt under, then the line.
function HeroText({ name, eyebrow, weatherLine, note, onSwitchGarden }: HeroTextProps) {
  return (
    <View style={{ paddingHorizontal: space[5], paddingBottom: 6 }}>
      {eyebrow ? (
        <Text variant="subheadline" color="textMuted" style={{ letterSpacing: 0.3, marginBottom: space[1] }}>
          {eyebrow}
        </Text>
      ) : null}
      {name ? (
        <Title name={name} onSwitchGarden={onSwitchGarden} />
      ) : (
        <Skeleton height={52} width="70%" style={{ marginTop: space[1], marginBottom: space[2] }} />
      )}
      {weatherLine ? (
        <Text variant="subheadline" color="textMuted" style={{ maxWidth: 320 }}>
          {weatherLine}
        </Text>
      ) : null}
      {note ? (
        <Text variant="footnote" color="textMuted" style={{ marginTop: space[1] }}>
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
