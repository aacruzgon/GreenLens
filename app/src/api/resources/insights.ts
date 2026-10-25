// What Insights reads (roadmap F7 item 3; design 8): a plant's whole timeline and its measurement series, typed
// with the app's proposed fields (domain/proposed.ts). The live API sends the contract's shapes, which are the
// same records with those fields absent; it takes only the growth quantities, so a leaf quantity it refuses
// (`400 invalid_request`) reads as no points, and Insights shows its empty state.
import { useQuery } from "@tanstack/react-query";

import type { InsightTimelineEntry, RangePoint } from "@/domain/proposed";
import type { GrowthQuantity, MeasureQuantity } from "@/domain/types";

import { client, ok } from "../client";
import { isApiError } from "../errors";
import { fetchAllPages, MAX_PAGE_SIZE, type Page } from "../pages";
import { STALE } from "../queryClient";
import { qk } from "../queryKeys";

export const insights = {
  timelinePage: async (id: string, cursor?: string): Promise<Page<InsightTimelineEntry>> =>
    ok(await client().GET("/plants/{id}/timeline", { params: { path: { id }, query: { cursor, limit: MAX_PAGE_SIZE } } })),
  /** Newest first, every page. */
  timeline: (id: string): Promise<InsightTimelineEntry[]> => fetchAllPages((cursor) => insights.timelinePage(id, cursor)),
  measurementsPage: async (id: string, quantity: MeasureQuantity, cursor?: string): Promise<Page<RangePoint>> =>
    // The contract types `quantity` as the growth pair; the proposal widens it (proposed.ts, `RangePoint`).
    ok(await client().GET("/plants/{id}/measurements", { params: { path: { id }, query: { quantity: quantity as GrowthQuantity, cursor, limit: MAX_PAGE_SIZE } } })),
  /** Oldest first, every page. A quantity the API doesn't take yet is no points. */
  measurements: async (id: string, quantity: MeasureQuantity): Promise<RangePoint[]> => {
    try {
      return await fetchAllPages((cursor) => insights.measurementsPage(id, quantity, cursor));
    } catch (error) {
      if (isApiError(error) && error.code === "invalid_request") return [];
      throw error;
    }
  },
};

/** A plant's whole timeline, for Insights' strip and weather. Under qk.timeline, so a new scan refetches it. */
export function useInsightTimeline(plantId: string) {
  return useQuery({
    queryKey: [...qk.timeline(plantId), "all"] as const,
    queryFn: () => insights.timeline(plantId),
    staleTime: STALE.signedUrls,
    enabled: plantId !== "",
  });
}

/** A plant's series of one quantity, with ranges where the API sends them. */
export function usePlantMeasurements(plantId: string, quantity: MeasureQuantity) {
  return useQuery({
    queryKey: [...qk.measurements(plantId), quantity] as const,
    queryFn: () => insights.measurements(plantId, quantity),
    staleTime: STALE.signedUrls,
    enabled: plantId !== "",
  });
}
