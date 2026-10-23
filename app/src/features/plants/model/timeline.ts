// A plant's timeline in words (roadmap 10 item 8; screens 3.5): each scan with what it found, worded by the rules of
// screens 6, and where a tap opens it. Pure.
import type { ScanFailureCode, SpeciesStatus, TimelineEntry } from "@/domain/types";

const FAILURE: Record<ScanFailureCode, string> = {
  no_plant_found: "no plant found",
  photos_unusable: "the photos couldn't be used",
  processing_error: "something went wrong",
  upstream_unavailable: "the analysis wasn't available",
};

export interface TimelineRow {
  scanId: string;
  title: string;
  /** Where a tap goes: the diagnosis once there is a plan, the scan while it runs, capture while it waits. */
  opens: "result" | "scan" | "capture";
}

export function timelineRow(e: TimelineEntry): TimelineRow {
  const base = { scanId: e.scan_id };
  switch (e.status) {
    case "pending":
      return { ...base, title: "Not submitted", opens: "capture" };
    case "failed":
      return { ...base, title: `Failed: ${e.failure_code ? FAILURE[e.failure_code] : "the analysis stopped"}`, opens: "scan" };
    case "planned":
    case "diagnosed": {
      const opens = e.plan_id ? "result" : "scan";
      if (e.diagnosis_status === "healthy") return { ...base, title: "Looks healthy", opens };
      if (e.diagnosis_status === "confirmed" && e.primary) return { ...base, title: e.primary.name, opens };
      return { ...base, title: "Not sure yet", opens };
    }
    default:
      return { ...base, title: "Analysing", opens: "scan" };
  }
}

const SPECIES_STATUS: Record<SpeciesStatus, string> = {
  confirmed: "confirmed",
  suggested: "suggested",
  unknown: "not known yet",
};

export function speciesStatusWord(s: SpeciesStatus): string {
  return SPECIES_STATUS[s];
}
