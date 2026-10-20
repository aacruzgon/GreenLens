import { useLocalSearchParams, useRouter } from "expo-router";
import { useEffect, useState } from "react";

import { isApiError, useAllPlants, useGarden } from "@/api";
import { can } from "@/domain/permissions";
import { useActiveGardenId } from "@/features/gardens";

import { useStartCapture, type ScanTarget } from "./useStartCapture";

// The new-scan sheet's view model (README 5): every plant of the garden, the plant a task chose,
// and starting a scan. Scanning is for editors and owners; anyone else is sent back (screens 4).
export function useNewScanSheet() {
  const gardenId = useActiveGardenId();
  const garden = useGarden(gardenId);
  const plants = useAllPlants(gardenId);
  const { plantId } = useLocalSearchParams<{ plantId?: string }>();
  const { start, isPending, error } = useStartCapture();
  const router = useRouter();
  const [starting, setStarting] = useState<string | null>(null);

  const allowed = garden.data ? can(garden.data.my_role, "scan.create") : undefined;
  useEffect(() => {
    if (allowed === false) router.back();
  }, [allowed, router]);

  async function begin(key: string, target: ScanTarget) {
    setStarting(key);
    await start(target).catch(() => {});
    setStarting(null);
  }

  return {
    ready: allowed === true && plants.data !== undefined,
    plants: plants.data ?? [],
    chosen: plantId ? plants.data?.find((p) => p.id === plantId) : undefined,
    starting,
    busy: isPending,
    error: error ? (isApiError(error) ? error.message : "The scan couldn't start. Try again.") : null,
    scanPlant: (id: string) => void begin(id, { plantId: id }),
    scanNewPlant: (name: string) => void begin("new", { newPlantName: name }),
  };
}
