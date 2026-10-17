import { View } from "react-native";
import { Image } from "expo-image";
import { Ionicons } from "@expo/vector-icons";

import { ago } from "@/domain/format";
import type { Plant } from "@/domain/types";
import { Card, PlantStatusChip, Text, useTheme } from "@/ui";

export function PlantCard({ plant, onPress }: { plant: Plant; onPress?: () => void }) {
  const t = useTheme();
  const thumb = plant.latest_scan?.thumbnail_url;
  const species = plant.species?.common_name ?? plant.custom_species_name;
  const last = plant.latest_scan ? `Scanned ${ago(plant.latest_scan.created_at)}` : "Not scanned yet";
  return (
    <Card padded={false} onPress={onPress} accessibilityLabel={`${plant.name}${species ? `, ${species}` : ""}. ${last}`} style={{ flex: 1, minWidth: 150 }}>
      {thumb ? (
        <Image source={{ uri: thumb, cacheKey: plant.latest_scan?.thumbnail_image_id }} style={{ width: "100%", aspectRatio: 4 / 3 }} contentFit="cover" />
      ) : (
        <View style={{ width: "100%", aspectRatio: 4 / 3, backgroundColor: t.colors.accentSoft, alignItems: "center", justifyContent: "center" }}>
          <Ionicons name="leaf-outline" size={40} color={t.colors.accent} />
        </View>
      )}
      <View style={{ padding: t.space.md, gap: t.space.xs }}>
        <Text variant="headline" numberOfLines={2}>
          {plant.name}
        </Text>
        {species ? (
          <Text variant="footnote" color="textMuted" numberOfLines={1}>
            {species}
            {plant.species_status === "suggested" ? " (to confirm)" : ""}
          </Text>
        ) : null}
        <PlantStatusChip status={plant.status} />
        <Text variant="caption" color="textMuted">
          {last}
        </Text>
      </View>
    </Card>
  );
}
