import { useInfiniteQuery, useQuery } from "@tanstack/react-query";

import { client, ok } from "../client";
import type { Page, Plant, TimelineEntry } from "@/domain/types";
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
};

export function usePlants(gardenId: string) {
  return useInfiniteQuery({
    queryKey: qk.plants(gardenId),
    queryFn: ({ pageParam }) => plants.list(gardenId, pageParam),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: nextCursor,
    staleTime: STALE.signedUrls,
  });
}

export function usePlant(id: string) {
  return useQuery({ queryKey: qk.plant(id), queryFn: () => plants.get(id), staleTime: STALE.signedUrls });
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
