// The diagnosis's top (roadmap 10 item 2; design 3.1, after Apple Music): the photo fading into its tint, Back and
// In the garden as bar buttons over it, and the diagnosis centred over the faded edge in Urbanist at 40 pt with its
// basis below. Never a number: the words come from domain/diagnosis.ts (screens 6).
import type { ImageSource } from "expo-image";
import { View, useWindowDimensions } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { BAR_INK, BarButton, LineIcon, PhotoTint, Text, space } from "@/ui";

import type { Header } from "../model/result";

export interface DiagnosisHeroProps {
  photo?: { source: ImageSource | number | null; label: string };
  ground: string;
  header: Header;
  onBack: () => void;
  onInGarden: () => void;
}

/** The photo's height: 4:3.4 of the width (styles.css .scan-hero). */
export const heroHeightFor = (width: number) => Math.round((width * 3.4) / 4);

/** How far the name hangs below the photo's bottom edge (styles.css .profile-head: top calc(100% - 26px)). */
const OVERHANG = 26;

export function DiagnosisHero({ photo, ground, header, onBack, onInGarden }: DiagnosisHeroProps) {
  const { width } = useWindowDimensions();
  const insets = useSafeAreaInsets();
  const height = heroHeightFor(width);
  return (
    <View testID="diagnosis-hero">
      <PhotoTint source={photo?.source ?? null} accessibilityLabel={photo?.label ?? ""} ground={ground} height={height} />
      <View
        pointerEvents="box-none"
        style={{ position: "absolute", top: Math.max(insets.top - 2, 8), left: 14, right: 14, flexDirection: "row", justifyContent: "space-between" }}
      >
        <BarButton accessibilityLabel="Back" onPress={onBack} testID="result-back">
          <LineIcon name="back" color={BAR_INK} size={22} strokeWidth={2} />
        </BarButton>
        <BarButton accessibilityLabel="In the garden" onPress={onInGarden} testID="result-in-garden">
          <LineIcon name="map" color={BAR_INK} size={22} />
        </BarButton>
      </View>
      <View style={{ marginTop: -OVERHANG, paddingHorizontal: space[4], alignItems: "center", gap: space[1] }}>
        <Text variant="display" align="center" testID="diagnosis-headline">
          {header.headline}
        </Text>
        <Text variant="footnote" color="textMuted" align="center" testID="diagnosis-basis">
          {header.basis}
        </Text>
        {header.subline ? (
          <Text variant="subheadline" color="textMuted" align="center" style={{ marginTop: space[1] }}>
            {header.subline}
          </Text>
        ) : null}
      </View>
    </View>
  );
}
