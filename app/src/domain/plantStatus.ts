// A plant's status at each of its scans (roadmap F7: the garden's date scrubber, Insights' status strip). Pure.
import type { TimelineEntryWithStatus } from "./proposed";
import type { PlantStatus } from "./types";

/**
 * The plant's status after `entry`'s scan: the proposed `plant_status` when the API sends it. Without it, a guess
 * from what the entry has: the latest scan takes the plant's current status; an earlier one with a primary
 * problem takes the current status when that is watch or sick, and watch otherwise; one without, healthy.
 */
export function statusOfEntry(entry: TimelineEntryWithStatus, current: PlantStatus, isLatest: boolean): PlantStatus {
  if (entry.plant_status) return entry.plant_status;
  if (isLatest) return current;
  if (!entry.primary) return "healthy";
  return current === "watch" || current === "sick" ? current : "watch";
}
