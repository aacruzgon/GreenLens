import { useQuery } from "@tanstack/react-query";

import type { CreatePlantRequest } from "@/domain/types";

import { call, client, idempotent } from "../client";
import { qk } from "../queryKeys";
import { SIGNED_STALE_MS } from "../queryClient";

export const plants = {
  list: (gardenId: string) => call(client.GET("/plants", { params: { query: { garden_id: gardenId, limit: 100 } } })),
  get: (id: string) => call(client.GET("/plants/{id}", { params: { path: { id } } })),
  create: (body: CreatePlantRequest, key: string) => call(client.POST("/plants", { body, params: { header: idempotent(key) } })),
  timeline: (id: string) => call(client.GET("/plants/{id}/timeline", { params: { path: { id } } })),
};

export function usePlants(gardenId: string | undefined) {
  return useQuery({
    queryKey: qk.plants(gardenId ?? ""),
    queryFn: () => plants.list(gardenId!),
    enabled: !!gardenId,
    staleTime: SIGNED_STALE_MS,
  });
}

export function usePlant(id: string | undefined) {
  return useQuery({ queryKey: qk.plant(id ?? ""), queryFn: () => plants.get(id!), enabled: !!id, staleTime: SIGNED_STALE_MS });
}

export function useTimeline(plantId: string | undefined) {
  return useQuery({
    queryKey: qk.timeline(plantId ?? ""),
    queryFn: () => plants.timeline(plantId!),
    enabled: !!plantId,
    staleTime: SIGNED_STALE_MS,
  });
}
