// A garden's Farm connectors, pushed from its settings (EON E10.1; screens 3.9).
import { useLocalSearchParams } from "expo-router";

import { ConnectorsScreen } from "@/features/connectors";

export default function GardenConnectors() {
  const { gardenId } = useLocalSearchParams<{ gardenId: string }>();
  return <ConnectorsScreen gardenId={gardenId} />;
}
