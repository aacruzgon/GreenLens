import { useGardens } from "@/api";
import type { Garden } from "@/domain/types";

import { pickActiveGarden } from "../model/activeGarden";
import { useActiveGardenStore } from "./activeGardenStore";

export function useActiveGarden(): { garden?: Garden; isLoading: boolean } {
  const gardens = useGardens();
  const stored = useActiveGardenStore((s) => s.gardenId);
  return { garden: pickActiveGarden(gardens.data?.items ?? [], stored), isLoading: gardens.isLoading };
}
