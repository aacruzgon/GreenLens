import { useInfiniteQuery, useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";

import { client, ok } from "../client";
import type { ConfirmSpeciesRequest, Page, Plant, TimelineEntry } from "@/domain/types";
import { fetchAllPages, MAX_PAGE_SIZE, nextCursor } from "../pages";
import { STALE } from "../queryClient";
import { qk } from "../queryKeys";

type PlantPage = Page<Plant>;
type TimelinePage = Page<TimelineEntry>;

export const plants = {
  list: async (gardenId: string, cursor?: string, limit?: number): Promise<PlantPage> =>
    ok(await client().GET("/plants", { params: { query: { garden_id: gardenId, cursor, limit } } })),
  /** Every plant of a garden, for the map and its "Which plant?" picker (README §9, lists). */
  listAll: (gardenId: string): Promise<Plant[]> =>
    fetchAllPages((cursor) => plants.list(gardenId, cursor, MAX_PAGE_SIZE)),
  get: async (id: string): Promise<Plant> => ok(await client().GET("/plants/{id}", { params: { path: { id } } })),
  timeline: async (id: string, cursor?: string): Promise<TimelinePage> =>
    ok(await client().GET("/plants/{id}/timeline", { params: { path: { id }, query: { cursor } } })),
  /** Confirms or changes the species; the plant's latest scan may rerun (flows 7). Idempotent on the server. */
  confirmSpecies: async (id: string, body: ConfirmSpeciesRequest): Promise<Plant> =>
    ok(await client().POST("/plants/{id}/species", { params: { path: { id } }, body })),
};

/** Every plant of a garden, all pages loaded: for pickers, which must show everything (screens 4). An empty ID waits. */
export function useAllPlants(gardenId: string) {
  return useQuery({
    queryKey: [...qk.plants(gardenId), "all"] as const,
    queryFn: () => plants.listAll(gardenId),
    staleTime: STALE.signedUrls,
    enabled: gardenId !== "",
  });
}

export function usePlants(gardenId: string) {
  return useInfiniteQuery({
    queryKey: qk.plants(gardenId),
    queryFn: ({ pageParam }) => plants.list(gardenId, pageParam),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: nextCursor,
    staleTime: STALE.signedUrls,
  });
}

/** An empty ID waits: a screen that learns the plant from another record (a scan) passes "" until then. */
export function usePlant(id: string) {
  return useQuery({ queryKey: qk.plant(id), queryFn: () => plants.get(id), staleTime: STALE.signedUrls, enabled: id !== "" });
}

export function useTimeline(plantId: string) {
  return useInfiniteQuery({
    queryKey: qk.timeline(plantId),
    queryFn: ({ pageParam }) => plants.timeline(plantId, pageParam),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: nextCursor,
    staleTime: STALE.signedUrls,
  });
}

/**
 * Every plant's whole timeline, all pages loaded: the garden's date scrubber needs each plant at each scan
 * (roadmap F7). Its own key under the timeline's, since `useTimeline`'s cache holds pages.
 */
export function useTimelines(plantIds: string[]) {
  return useQueries({
    queries: plantIds.map((id) => ({
      queryKey: [...qk.timeline(id), "all"] as const,
      queryFn: () => fetchAllPages((cursor) => plants.timeline(id, cursor)),
      staleTime: STALE.signedUrls,
    })),
    combine: (results) => ({
      byPlant: Object.fromEntries(plantIds.map((id, i) => [id, results[i]?.data])) as Record<string, TimelineEntry[] | undefined>,
      isPending: results.some((r) => r.isPending),
      isError: results.some((r) => r.isError),
    }),
  });
}

/** README 10 invalidation for `species_confirmed`: the plant and its garden's list. */
export function useConfirmSpecies(plantId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: ConfirmSpeciesRequest) => plants.confirmSpecies(plantId, body),
    onSuccess: (plant) => {
      qc.setQueryData(qk.plant(plant.id), plant);
      // The latest scan follows the species, with a rerun when it changed (flows 7): its screen picks that up.
      const scan = plant.latest_scan?.scan_id;
      return Promise.all([
        qc.invalidateQueries({ queryKey: qk.plants(plant.garden_id) }),
        ...(scan ? [qc.invalidateQueries({ queryKey: qk.scan(scan) })] : []),
      ]);
    },
  });
}
