import { useRouter } from "expo-router";

import { useActionKey, useCreateScan } from "@/api";
import type { ImageKind } from "@/domain/types";
import { useActiveGardenId } from "@/features/gardens";

/** `photoKind`: the shot chat's `rescan_requested` asked for (flows 9); capture opens on that mode. */
export type ScanTarget = { plantId: string; photoKind?: ImageKind } | { newPlantName: string };

// Starts a scan of a plant, or of a new one by name, then opens capture (flows 4). Other features
// (chat's rescan_requested, a task's Scan) start scans through this too (README 5).
export function useStartCapture() {
  const gardenId = useActiveGardenId();
  const createScan = useCreateScan();
  const router = useRouter();
  const keyFor = useActionKey();

  /** `push` keeps the screen it starts from (chat's rescan) under capture; the new-scan sheet replaces itself. */
  async function start(target: ScanTarget, opts: { push?: boolean } = {}) {
    const body =
      "plantId" in target
        ? { garden_id: gardenId, plant_id: target.plantId }
        : { garden_id: gardenId, new_plant: { name: target.newPlantName.trim() } };
    const scan = await createScan.mutateAsync(body, keyFor(body));
    const kind = "plantId" in target ? target.photoKind : undefined;
    const href = { pathname: "/capture/[scanId]", params: { scanId: scan.id, ...(kind ? { kind } : {}) } } as const;
    if (opts.push) router.push(href);
    else router.replace(href);
  }

  return { start, isPending: createScan.isPending, error: createScan.error };
}
