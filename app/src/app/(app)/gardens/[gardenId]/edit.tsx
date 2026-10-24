import { useLocalSearchParams } from "expo-router";

import { GardenEditScreen } from "@/features/gardens";

export default function GardenEdit() {
  const { gardenId } = useLocalSearchParams<{ gardenId: string }>();
  return <GardenEditScreen gardenId={gardenId} />;
}
