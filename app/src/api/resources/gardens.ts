import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { client, ok } from "../client";
import type { CreateGardenRequest, Garden, GardenMap, GardenMapInput, GardenWeather, Page, UpdateGardenRequest } from "@/domain/types";
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
  /** Owner: a new name or location; the location is geocoded again (contracts 9). */
  update: async (id: string, body: UpdateGardenRequest): Promise<Garden> =>
    ok(await client().PATCH("/gardens/{id}", { params: { path: { id } }, body })),
  /** Owner: `202`; the cascade runs in the background (contracts 9, deletion cascades). */
  remove: async (id: string): Promise<void> => {
    await client().DELETE("/gardens/{id}", { params: { path: { id } } });
  },
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

export function useUpdateGarden(gardenId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: UpdateGardenRequest) => gardens.update(gardenId, body),
    onSuccess: (garden) => {
      qc.setQueryData(qk.garden(garden.id), garden);
      // A new location means new weather (contracts 7, the weather cache is per location).
      void qc.invalidateQueries({ queryKey: qk.weather(garden.id) });
      return qc.invalidateQueries({ queryKey: qk.gardens() });
    },
  });
}

/** Deletes a garden. The caller moves off it first; the list refetches without it. */
export function useDeleteGarden() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (gardenId: string) => gardens.remove(gardenId),
    onSuccess: (_void, gardenId) => {
      qc.removeQueries({ queryKey: qk.garden(gardenId), exact: true });
      return qc.invalidateQueries({ queryKey: qk.gardens() });
    },
  });
}
