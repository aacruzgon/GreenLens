// The plants, sideways (styles.css .plants under data-plants="carousel"): cards that snap to their leading
// edge and run to the screen's edges, sorted by health with the sick first (roadmap F1 item 4).
import { ScrollView, View } from "react-native";

import { Skeleton, gutter, radii, space } from "@/ui";

import type { PlantCardView } from "../model/views";
import { PLANT_CARD_HEIGHT, PLANT_CARD_WIDTH, PlantCard } from "./PlantCard";

export interface PlantCarouselProps {
  plants: PlantCardView[];
  // Opening a plant joins with plant detail in F4; until then the cards are not pressable.
}

const GAP = space[3];

export function PlantCarousel({ plants }: PlantCarouselProps) {
  return (
    <ScrollView
      horizontal
      showsHorizontalScrollIndicator={false}
      snapToInterval={PLANT_CARD_WIDTH + GAP}
      decelerationRate="fast"
      // The cards' shadows need room: the strip runs into the gutters and pads below.
      style={{ marginHorizontal: -gutter, overflow: "visible" }}
      contentContainerStyle={{ paddingHorizontal: gutter, paddingTop: 2, paddingBottom: space[3], gap: GAP }}
      accessibilityRole="list"
      testID="plant-carousel"
    >
      {plants.map((p) => (
        <PlantCard
          key={p.id}
          testID={`plant-card-${p.id}`}
          name={p.name}
          status={p.status}
          photo={p.photo}
          note={p.note}
        />
      ))}
    </ScrollView>
  );
}

export function PlantCarouselSkeleton() {
  return (
    <View style={{ flexDirection: "row", gap: GAP, overflow: "hidden", marginRight: -gutter }} testID="plants-loading">
      {[0, 1, 2].map((i) => (
        <Skeleton key={i} width={PLANT_CARD_WIDTH} height={PLANT_CARD_HEIGHT} radius={radii.card} />
      ))}
    </View>
  );
}
