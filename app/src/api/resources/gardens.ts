import { useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query";

import { client, ok } from "../client";
import type { CreateGardenRequest, Garden, GardenMap, GardenMapInput, GardenWeather, Page } from "@/domain/types";
import { useIdempotentMutation, type IdempotencyKey } from "../idempotency";
import { nextCursor } from "../pages";
import { STALE } from "../queryClient";
import { qk } from "../queryKeys";

type GardenPage = Page<Garden>;

export const gardens = {
  list: async (cursor?: string): Promise<GardenPage> =>
    ok(await client().GET("/gardens", { params: { query: { cursor } } })),
  get: async (id: string): Promise<Garden> => ok(await client().GET("/gardens/{id}", { params: { path: { id } } })),
  create: async (body: CreateGardenRequest, key: IdempotencyKey): Promise<Garden> =>
    ok(await client().POST("/gardens", { body, params: { header: { "Idempotency-Key": key } } })),
  weather: async (id: string): Promise<GardenWeather> =>
    ok(await client().GET("/gardens/{id}/weather", { params: { path: { id } } })),
  map: async (id: string): Promise<GardenMap | null> =>
    (await ok(await client().GET("/gardens/{id}/map", { params: { path: { id } } }))).map,
  /** A whole new version; `409 invalid_state` with `current_map_version` when another member mapped since. */
  putMap: async (id: string, body: GardenMapInput, key: IdempotencyKey): Promise<GardenMap> =>
    ok(await client().PUT("/gardens/{id}/map", { params: { path: { id }, header: { "Idempotency-Key": key } }, body })),
};

/** The garden's current map, or null when it has none (contracts 9). Any member. */
export function useGardenMap(gardenId: string) {
  return useQuery({ queryKey: qk.map(gardenId), queryFn: () => gardens.map(gardenId), enabled: gardenId !== "" });
}

/** Uploads a mapping session's result. The map and the plants' positions change with it. */
export function usePutGardenMap(gardenId: string) {
  const qc = useQueryClient();
  return useIdempotentMutation({
    mutationFn: (body: GardenMapInput, key: IdempotencyKey) => gardens.putMap(gardenId, body, key),
    onSuccess: (map) => {
      qc.setQueryData(qk.map(gardenId), map);
      void qc.invalidateQueries({ queryKey: ["plants"] });
    },
  });
}

export function useGardens() {
  return useInfiniteQuery({
    queryKey: qk.gardens(),
    queryFn: ({ pageParam }) => gardens.list(pageParam),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: nextCursor,
  });
}

export function useGarden(id: string) {
  return useQuery({ queryKey: qk.garden(id), queryFn: () => gardens.get(id), enabled: id !== "" });
}

export function useGardenWeather(gardenId: string) {
  return useQuery({
    queryKey: qk.weather(gardenId),
    queryFn: () => gardens.weather(gardenId),
    staleTime: STALE.weather,
    refetchOnWindowFocus: true,
  });
}

export function useCreateGarden() {
  const qc = useQueryClient();
  return useIdempotentMutation({
    mutationFn: (body: CreateGardenRequest, key) => gardens.create(body, key),
    onSuccess: (garden) => {
      qc.setQueryData(qk.garden(garden.id), garden);
      return qc.invalidateQueries({ queryKey: qk.gardens() });
    },
  });
}
