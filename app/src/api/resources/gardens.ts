import { useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query";

import { client, ok } from "../client";
import type { components } from "../generated/schema";
import { useIdempotentMutation, type IdempotencyKey } from "../idempotency";
import { nextCursor } from "../pages";
import { STALE } from "../queryClient";
import { qk } from "../queryKeys";

type S = components["schemas"];
export type Garden = S["Garden"];
export type GardenPage = S["Page_Garden"];
export type CreateGardenRequest = S["CreateGardenRequest"];
export type GardenWeather = S["GardenWeather"];

export const gardens = {
  list: async (cursor?: string): Promise<GardenPage> =>
    ok(await client().GET("/gardens", { params: { query: { cursor } } })),
  get: async (id: string): Promise<Garden> => ok(await client().GET("/gardens/{id}", { params: { path: { id } } })),
  create: async (body: CreateGardenRequest, key: IdempotencyKey): Promise<Garden> =>
    ok(await client().POST("/gardens", { body, params: { header: { "Idempotency-Key": key } } })),
  weather: async (id: string): Promise<GardenWeather> =>
    ok(await client().GET("/gardens/{id}/weather", { params: { path: { id } } })),
};

export function useGardens() {
  return useInfiniteQuery({
    queryKey: qk.gardens(),
    queryFn: ({ pageParam }) => gardens.list(pageParam),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: nextCursor,
  });
}

export function useGarden(id: string) {
  return useQuery({ queryKey: qk.garden(id), queryFn: () => gardens.get(id) });
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
