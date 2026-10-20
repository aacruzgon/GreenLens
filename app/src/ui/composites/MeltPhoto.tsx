// A photo that dissolves into its ground at the bottom edge instead of sitting in a frame
// (design.md 3.1, the melt). An SVG gradient in the ground colour lies over the lower part of the photo;
// its stops mirror the mockup's masks (styles.css: .h-hero .ph, .pcard .thumb, the profile .pwr > .ph).
import type { ReactNode } from "react";
import { View, type DimensionValue, type StyleProp, type ViewStyle } from "react-native";
import { Image, type ImageSource } from "expo-image";
import Svg, { Defs, LinearGradient, Rect, Stop } from "react-native-svg";

import { useSvgId } from "../svgId";
import { useTheme } from "../theme";
import { motion, type ColorRole } from "../tokens";

/** Where the photo still shows (opacity of the photo) down its height: [offset, photo opacity]. */
const MELTS = {
  /** The garden home's hero: #000 38%, .55 at 62%, transparent 88%. */
  hero: [
    [0.38, 1],
    [0.62, 0.55],
    [0.88, 0],
  ],
  /** Plant cards: #000 40%, transparent 82%. */
  card: [
    [0.4, 1],
    [0.82, 0],
  ],
  /** The diagnosis, into its tint: #000 40%, .6 at 62%, .2 at 82%, transparent 96%. */
  profile: [
    [0.4, 1],
    [0.62, 0.6],
    [0.82, 0.2],
    [0.96, 0],
  ],
} as const satisfies Record<string, readonly (readonly [number, number])[]>;

export type MeltKind = keyof typeof MELTS;

export interface MeltPhotoProps {
  source: ImageSource | string | number | null;
  /** Required: what the photo shows. Pass "" for a decorative photo. */
  accessibilityLabel: string;
  /** The colour it melts into, as a role (default wash). */
  ground?: ColorRole;
  /** A computed ground, such as the diagnosis's photo tint; wins over `ground`. */
  groundColor?: string;
  melt?: MeltKind;
  height?: DimensionValue;
  aspectRatio?: number;
  /** Corner radius at the top; 0 for full-bleed heroes, radii.photo inside cards. */
  radius?: number;
  blurhash?: string;
  /** Drawn over the melted edge, such as the hero's name. */
  children?: ReactNode;
  style?: StyleProp<ViewStyle>;
}

export function MeltPhoto({
  source,
  accessibilityLabel,
  ground = "wash",
  groundColor,
  melt = "hero",
  height,
  aspectRatio,
  radius = 0,
  blurhash,
  children,
  style,
}: MeltPhotoProps) {
  const { colors, reducedMotion } = useTheme();
  const id = useSvgId("melt");
  const color = groundColor ?? colors[ground];
  const decorative = accessibilityLabel === "";
  return (
    <View
      style={[
        { width: "100%", height, aspectRatio, backgroundColor: color, overflow: "hidden", borderTopLeftRadius: radius, borderTopRightRadius: radius },
        style,
      ]}
    >
      <Image
        source={source}
        placeholder={blurhash ? { blurhash } : undefined}
        contentFit="cover"
        transition={reducedMotion ? 0 : motion.fade.duration}
        accessible={!decorative}
        accessibilityLabel={decorative ? undefined : accessibilityLabel}
        style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0 }}
      />
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
            {[
              <Stop key="start" offset={0} stopColor={color} stopOpacity={0} />,
              ...MELTS[melt].map(([offset, photo]) => (
                <Stop key={offset} offset={offset} stopColor={color} stopOpacity={1 - photo} />
              )),
              <Stop key="end" offset={1} stopColor={color} stopOpacity={1} />,
            ]}
          </LinearGradient>
        </Defs>
        <Rect x="0" y="0" width="100%" height="100%" fill={`url(#${id})`} />
      </Svg>
      {children ? <View style={{ position: "absolute", left: 0, right: 0, bottom: 0 }}>{children}</View> : null}
    </View>
  );
}
