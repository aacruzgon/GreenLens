import { useGarden } from "@/api";
import { can } from "@/domain/permissions";
import { useActiveGardenId } from "@/features/gardens";

/** Scanning is for editors and owners; for viewers the scan action is hidden, not disabled (screens 4). */
export function useCanScan(): boolean {
  const garden = useGarden(useActiveGardenId());
  return garden.data ? can(garden.data.my_role, "scan.create") : false;
}
