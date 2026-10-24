import { useLocalSearchParams } from "expo-router";

import { GardenScreen, type GardenMode } from "@/features/garden3d";

const MODES: readonly string[] = ["health", "growth", "frost"];

// The garden in 3D or on the map (roadmap F7 item 1): `plantId` opens on a plant, `focus` on the plants a chat
// answer is about (comma-separated), `mode` on a mode.
export default function GardenRoute() {
  const { plantId, focus, mode } = useLocalSearchParams<{ plantId?: string; focus?: string; mode?: string }>();
  return (
    <GardenScreen
      {...(plantId ? { plantId } : {})}
      {...(focus ? { focus: focus.split(",").filter(Boolean) } : {})}
      {...(mode && MODES.includes(mode) ? { mode: mode as GardenMode } : {})}
    />
  );
}
