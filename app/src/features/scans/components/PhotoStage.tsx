// The live analysis's photo (roadmap F3 items 2, 4 and 5; design 3.1): the photo just taken, held still and full
// screen, not a live feed, with the findings as hotspots on it; the scan's photos in a solid dark strip, the one
// being read ringed in white; and the shutter's place taken by a glowing ring while the analysis runs.
import { Image } from "expo-image";
import { useEffect, useMemo, useState } from "react";
import { Pressable, View, type LayoutChangeEvent } from "react-native";
import Animated, { Easing, cancelAnimation, useAnimatedStyle, useSharedValue, withRepeat, withTiming } from "react-native-reanimated";
import Svg, { Circle, Defs, LinearGradient, Rect, Stop } from "react-native-svg";

import { imageSource } from "@/api";
import { regionToView } from "@/domain/regions";
import type { ScanImage } from "@/domain/types";
import { Hotspot, Text, camera, hiddenSvg, motion, useReducedMotion, useSvgId } from "@/ui";

import { bandShift, labelSides, ovalOf, type Hotspot as HotspotModel } from "../model/hotspots";
import { ScanIcon } from "./ScanIcons";

const KIND_WORDS: Record<ScanImage["kind"], string> = {
  whole_plant: "Whole plant",
  leaf_closeup: "Close-up",
  leaf_underside: "Underside",
  soil_surface: "Soil",
  flower_fruit: "Flower or fruit",
  squeeze_test: "Squeeze test",
  jar_test: "Jar test",
};

export interface PhotoStageProps {
  image: ScanImage | undefined;
  hotspots: readonly HotspotModel[];
  /** Keys of hotspots that arrived live, so only they animate in. */
  animated: ReadonlySet<string>;
  /** "Tomato · Change" for the species check, while the species waits for the user. */
  species?: { name: string; onChange?: () => void };
  /** The band the cards leave clear, in the view's points: the photo moves so its hotspots sit in it. */
  band?: { top: number; bottom: number };
}

export function PhotoStage({ image, hotspots, animated, species, band }: PhotoStageProps) {
  const reduced = useReducedMotion();
  const [view, setView] = useState({ width: 0, height: 0 });
  const onLayout = (e: LayoutChangeEvent) => setView({ width: e.nativeEvent.layout.width, height: e.nativeEvent.layout.height });
  const shown = useMemo(() => {
    if (!image || view.width === 0) return [];
    const size = { width: image.width, height: image.height };
    return hotspots
      .filter((h) => h.imageId === image.id)
      .map((h) => ({ ...h, oval: ovalOf(regionToView(h.bbox, size, view, "cover")), text: h.size ? `${h.label} · ${h.size}` : h.label }));
  }, [hotspots, image, view]);
  const sides = useMemo(() => labelSides(shown), [shown]);

  // While the cards cover part of the photo (the question, "Not sure yet"), the photo slides so the spots
  // stay in view between them; the edge it uncovers is black like the screen, under the cards.
  const shift = bandShift(
    shown.map((h) => h.oval),
    band,
    view.height * 0.3,
  );
  const dy = useSharedValue(0);
  useEffect(() => {
    dy.value = reduced ? shift : withTiming(shift, { duration: motion.sheet.duration, easing: Easing.bezier(...motion.sheet.bezier) });
  }, [dy, reduced, shift]);
  const slide = useAnimatedStyle(() => ({ transform: [{ translateY: dy.value }] }));

  return (
    <View style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0 }} onLayout={onLayout} testID="analysis-photo">
      <Animated.View style={[{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0 }, slide]}>
        {image ? (
          <Image
            source={imageSource(image.url, image.id)}
            contentFit="cover"
            style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0 }}
            accessible
            accessibilityLabel={`${KIND_WORDS[image.kind]} photo`}
            transition={200}
          />
        ) : null}
        <Shade />
        {shown.map((h) => (
          <Hotspot
            key={h.key}
            testID={`hotspot-${h.kind}`}
            oval={h.oval}
            label={h.label}
            size={h.size}
            labelAt={sides.get(h.key)}
            animate={animated.has(h.key)}
          />
        ))}
      </Animated.View>
      {species ? <SpeciesChip name={species.name} onChange={species.onChange} /> : null}
    </View>
  );
}

/** The mockup's shade over the photo (.pwr::after), so the cards and controls read on a bright leaf. */
function Shade() {
  const id = useSvgId("shade");
  return (
    <Svg style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0 }} width="100%" height="100%" preserveAspectRatio="none" {...hiddenSvg} pointerEvents="none">
      <Defs>
        <LinearGradient id={id} x1="0" y1="0" x2="0" y2="1">
          <Stop offset="0" stopColor="#000" stopOpacity={0.42} />
          <Stop offset="0.32" stopColor="#000" stopOpacity={0.04} />
          <Stop offset="0.64" stopColor="#000" stopOpacity={0.04} />
          <Stop offset="1" stopColor="#000" stopOpacity={0.5} />
        </LinearGradient>
      </Defs>
      <Rect x={0} y={0} width="100%" height="100%" fill={`url(#${id})`} />
    </Svg>
  );
}

/** "Tomato · Change": the species check as a chip on the photo (design 3.1). */
function SpeciesChip({ name, onChange }: { name: string; onChange?: () => void }) {
  return (
    <View style={{ position: "absolute", left: 0, right: 0, bottom: "30%", alignItems: "center" }} pointerEvents="box-none">
      <Pressable
        testID="species-chip"
        accessibilityRole={onChange ? "button" : "text"}
        accessibilityLabel={`Species: ${name}`}
        accessibilityHint={onChange ? "Confirms or changes the species" : undefined}
        disabled={!onChange}
        onPress={onChange}
        style={{ flexDirection: "row", alignItems: "center", gap: 6, minHeight: 36, paddingHorizontal: 12, borderRadius: 999, backgroundColor: camera.pill }}
      >
        <ScanIcon name="leaf" color={camera.ink} size={16} />
        <Text variant="subheadline" style={{ color: camera.ink, fontWeight: "600" }}>
          {name}
        </Text>
        {onChange ? (
          <Text variant="subheadline" style={{ color: camera.ink, fontWeight: "600", marginLeft: 4, textDecorationLine: "underline" }}>
            Change
          </Text>
        ) : null}
      </Pressable>
    </View>
  );
}

/** The scan's photos as thumbnails in a solid dark pill; the one shown is ringed in white (.pstrip). */
export function PhotoStrip({ images, selectedId, onSelect }: { images: readonly ScanImage[]; selectedId?: string; onSelect: (id: string) => void }) {
  if (images.length < 2) return null;
  return (
    <View
      accessibilityRole="tablist"
      style={{ flexDirection: "row", alignSelf: "center", gap: 6, padding: 6, borderRadius: 999, backgroundColor: camera.pill, borderWidth: 1, borderColor: camera.hairline }}
    >
      {images.map((img) => {
        const on = img.id === selectedId;
        return (
          <Pressable
            key={img.id}
            testID={`strip-${img.kind}`}
            accessibilityRole="tab"
            accessibilityLabel={KIND_WORDS[img.kind]}
            accessibilityState={{ selected: on }}
            onPress={() => onSelect(img.id)}
            hitSlop={4}
            style={{ width: 46, height: 34, borderRadius: 17, borderWidth: on ? 2 : 0, borderColor: camera.ink, overflow: "hidden" }}
          >
            <Image source={imageSource(img.url, img.id)} contentFit="cover" style={{ flex: 1 }} />
          </Pressable>
        );
      })}
    </View>
  );
}

/**
 * The shutter as a glowing ring while the analysis runs (styles.css .cam-ring): a white arc turning around a leaf.
 * Under reduced motion the arc holds still. It is status, not a control.
 */
export function AnalysisRing() {
  const reduced = useReducedMotion();
  const turn = useSharedValue(0);
  const id = useSvgId("ring");
  useEffect(() => {
    if (reduced) {
      cancelAnimation(turn);
      turn.value = 0;
      return;
    }
    turn.value = withRepeat(withTiming(1, { duration: 2400, easing: Easing.linear }), -1);
    return () => cancelAnimation(turn);
  }, [reduced, turn]);
  const spin = useAnimatedStyle(() => ({ transform: [{ rotate: `${turn.value * 360}deg` }] }));

  return (
    <View
      testID="analysis-ring"
      accessible
      accessibilityRole="progressbar"
      accessibilityLabel="Analysing"
      style={{ width: 72, height: 72, borderRadius: 36, alignItems: "center", justifyContent: "center", backgroundColor: "rgba(28, 28, 30, 0.5)", boxShadow: "0 0 28px rgba(255, 255, 255, 0.45)" }}
    >
      <Animated.View style={[{ position: "absolute", width: 78, height: 78 }, spin]}>
        <Svg width={78} height={78} viewBox="0 0 78 78" {...hiddenSvg}>
          <Defs>
            <LinearGradient id={id} x1="0" y1="0" x2="1" y2="1">
              <Stop offset="0" stopColor={camera.ink} stopOpacity={1} />
              <Stop offset="0.5" stopColor={camera.ink} stopOpacity={0} />
              <Stop offset="1" stopColor={camera.ink} stopOpacity={1} />
            </LinearGradient>
          </Defs>
          <Circle cx={39} cy={39} r={37.5} stroke={`url(#${id})`} strokeWidth={3} fill="none" />
        </Svg>
      </Animated.View>
      <ScanIcon name="leaf" color={camera.ink} size={28} />
    </View>
  );
}
