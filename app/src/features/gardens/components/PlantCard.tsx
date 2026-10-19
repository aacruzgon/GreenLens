import { View } from "react-native";
import { Ionicons } from "@expo/vector-icons";

import { ago } from "@/domain/format";
import type { Plant } from "@/domain/types";
import { Card, MeltPhoto, PlantStatusChip, Text, useTheme } from "@/ui";

/** A plant in the home carousel (design 3.1): its photo melts into the card, with the name in ink below. */
export function PlantCard({ plant, onPress }: { plant: Plant; onPress?: () => void }) {
  const t = useTheme();
  const thumb = plant.latest_scan?.thumbnail_url;
  const species = plant.species?.common_name ?? plant.custom_species_name;
  const last = plant.latest_scan ? `Scanned ${ago(plant.latest_scan.created_at)}` : "Not scanned yet";
  return (
    <Card
      padded={false}
      onPress={onPress}
      accessibilityLabel={`${plant.name}${species ? `, ${species}` : ""}${plant.species_status === "suggested" ? " (to confirm)" : ""}. ${last}`}
      style={{ width: 170, height: 232 }}
    >
      {thumb ? (
        <MeltPhoto
          source={{ uri: thumb, cacheKey: plant.latest_scan?.thumbnail_image_id }}
          ground={t.colors.surface}
          from={0.36}
          to={0.74}
          style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0 }}
        />
      ) : (
        <View style={{ position: "absolute", top: 0, left: 0, right: 0, height: 150, backgroundColor: t.colors.wash, alignItems: "center", justifyContent: "center" }}>
          <Ionicons name="leaf-outline" size={40} color={t.colors.textMuted} />
        </View>
      )}
      <View style={{ flex: 1, justifyContent: "flex-end", padding: 14, gap: t.space.sm }}>
        <Text variant="displaySmall" numberOfLines={2}>
          {plant.name}
        </Text>
        <PlantStatusChip status={plant.status} />
      </View>
    </Card>
  );
}
