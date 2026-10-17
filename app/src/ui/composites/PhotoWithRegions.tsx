import { useState } from "react";
import { Pressable, View, type LayoutChangeEvent } from "react-native";
import { Image } from "expo-image";
import Svg, { Rect } from "react-native-svg";

import type { BBox } from "@/domain/types";
import { toViewRect } from "@/domain/regions";

import { useTheme } from "../theme";

export interface PhotoRegion {
  id: string;
  bbox: BBox;
  tone: "sick" | "watch" | "healthy" | "info";
  label: string;
}

/**
 * A photo with its finding boxes drawn on top (contracts 1, regions). The image keeps its aspect ratio,
 * and boxes are converted from normalised coordinates to the view's size.
 */
export function PhotoWithRegions({
  uri,
  cacheKey,
  width,
  height,
  regions = [],
  highlighted,
  onRegionPress,
  accessibilityLabel,
  onError,
}: {
  uri: string;
  cacheKey?: string;
  width: number;
  height: number;
  regions?: PhotoRegion[];
  highlighted?: string;
  onRegionPress?: (id: string) => void;
  accessibilityLabel: string;
  onError?: () => void;
}) {
  const t = useTheme();
  const [viewWidth, setViewWidth] = useState(0);
  const viewHeight = viewWidth * (height / Math.max(1, width));
  const onLayout = (e: LayoutChangeEvent) => setViewWidth(e.nativeEvent.layout.width);

  return (
    <View onLayout={onLayout} style={{ width: "100%", borderRadius: t.radius.lg, overflow: "hidden", backgroundColor: t.colors.surfaceMuted }}>
      <Image
        source={{ uri, cacheKey }}
        style={{ width: "100%", height: viewHeight || 240 }}
        contentFit="cover"
        transition={200}
        accessibilityLabel={accessibilityLabel}
        onError={onError}
      />
      {viewWidth > 0 && regions.length > 0 ? (
        <Svg width={viewWidth} height={viewHeight} style={{ position: "absolute", top: 0, left: 0 }} pointerEvents="none">
          {regions.map((r) => {
            const rect = toViewRect(r.bbox, viewWidth, viewHeight);
            const active = highlighted === r.id;
            return (
              <Rect
                key={r.id}
                {...rect}
                rx={6}
                stroke={t.colors[r.tone]}
                strokeWidth={active ? 4 : 2.5}
                fill={active ? t.colors.overlay : "transparent"}
                fillOpacity={active ? 0.15 : 0}
              />
            );
          })}
        </Svg>
      ) : null}
      {onRegionPress && viewWidth > 0
        ? regions.map((r) => {
            const rect = toViewRect(r.bbox, viewWidth, viewHeight);
            return (
              <Pressable
                key={`hit-${r.id}`}
                accessibilityRole="button"
                accessibilityLabel={r.label}
                onPress={() => onRegionPress(r.id)}
                style={{ position: "absolute", left: rect.x, top: rect.y, width: Math.max(44, rect.width), height: Math.max(44, rect.height) }}
              />
            );
          })
        : null}
    </View>
  );
}
