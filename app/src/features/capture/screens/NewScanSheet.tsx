import { useState } from "react";
import { ScrollView, View } from "react-native";
import { Image } from "expo-image";

import { imageSource } from "@/api";
import { Button, ListItem, Skeleton, StatusChip, Text, TextField, space, useTheme } from "@/ui";

import { useNewScanSheet } from "../hooks/useNewScanSheet";

// The new-scan sheet (roadmap F1 item 7): which plant, or a new one by name, then capture.
export function NewScanSheet() {
  const { colors } = useTheme();
  const view = useNewScanSheet();
  const [newName, setNewName] = useState("");

  return (
    <ScrollView
      style={{ backgroundColor: colors.background }}
      contentContainerStyle={{ padding: space[4], gap: space[4] }}
      keyboardShouldPersistTaps="handled"
    >
      <Text variant="section" accessibilityRole="header">
        Scan a plant
      </Text>
      {/* One green pill per screen: the chosen plant's scan when a task chose one, otherwise the new plant's. */}
      {view.chosen ? (
        <Button
          label={`Scan ${view.chosen.name}`}
          size="large"
          busy={view.starting === view.chosen.id}
          disabled={view.busy}
          onPress={() => view.chosen && view.scanPlant(view.chosen.id)}
        />
      ) : null}
      <Text variant="subheadline" color="textMuted">
        {view.chosen ? "Or another plant" : "Which plant?"}
      </Text>
      <View style={{ gap: space[1] }}>
        {!view.ready ? (
          <>
            <Skeleton height={56} />
            <Skeleton height={56} />
          </>
        ) : (
          view.plants.map((plant) => (
            <ListItem
              key={plant.id}
              title={plant.name}
              leading={
                <Image
                  source={imageSource(plant.latest_scan?.thumbnail_url, plant.latest_scan?.thumbnail_image_id)}
                  style={{ width: 40, height: 40, borderRadius: 12, backgroundColor: colors.surfaceMuted }}
                  accessibilityIgnoresInvertColors
                />
              }
              trailing={<StatusChip status={plant.status} />}
              onPress={view.busy ? undefined : () => view.scanPlant(plant.id)}
              accessibilityHint="Starts a scan of this plant"
            />
          ))
        )}
      </View>

      <Text variant="subheadline" color="textMuted">
        Or a new plant
      </Text>
      <TextField label="Its name" placeholder="Tomato 3" value={newName} onChangeText={setNewName} autoCapitalize="words" />
      {view.error ? (
        <Text variant="footnote" color="danger">
          {view.error}
        </Text>
      ) : null}
      <Button
        label="Start the scan"
        size="large"
        variant={view.chosen ? "secondary" : "primary"}
        busy={view.starting === "new"}
        disabled={!newName.trim() || view.busy}
        onPress={() => view.scanNewPlant(newName)}
      />
    </ScrollView>
  );
}
