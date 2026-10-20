import { View } from "react-native";

import { flattenPages, useGarden, usePlants } from "@/api";
import { EmptyState, ErrorState, ListItem, Screen, Section, Skeleton, StatusChip, Text, space } from "@/ui";

import { useActiveGardenId } from "../hooks/useActiveGarden";

// F0's garden home: the garden's name on the wash and its plants. F1 builds the full home of screens 3.1.
export function GardenHomeScreen() {
  const gardenId = useActiveGardenId();
  const garden = useGarden(gardenId);
  const plants = usePlants(gardenId);
  const list = flattenPages(plants.data);

  return (
    <Screen>
      <View style={{ paddingTop: space[8], gap: space[1] }}>
        {garden.data ? (
          <Text variant="hero" accessibilityRole="header">
            {garden.data.name}
          </Text>
        ) : (
          <Skeleton height={52} width="70%" />
        )}
      </View>

      {plants.error ? (
        <ErrorState onRetry={() => void plants.refetch()} />
      ) : !plants.data ? (
        <View style={{ marginTop: space[6], gap: space[3] }}>
          <Skeleton height={56} />
          <Skeleton height={56} />
        </View>
      ) : list.length === 0 ? (
        <EmptyState title="Add your first plant by scanning it" />
      ) : (
        <Section title="Plants" style={{ marginTop: space[6] }}>
          {list.map((plant) => (
            <ListItem key={plant.id} title={plant.name} trailing={<StatusChip status={plant.status} />} />
          ))}
        </Section>
      )}
    </Screen>
  );
}
