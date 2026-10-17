import { useQuery } from "@tanstack/react-query";

import type { CreateGardenRequest } from "@/domain/types";

import { call, client, idempotent } from "../client";
import { qk } from "../queryKeys";

export const gardens = {
  list: () => call(client.GET("/gardens", { params: { query: { limit: 100 } } })),
  get: (id: string) => call(client.GET("/gardens/{id}", { params: { path: { id } } })),
  create: (body: CreateGardenRequest, key: string) =>
    call(client.POST("/gardens", { body, params: { header: idempotent(key) } })),
  weather: (id: string) => call(client.GET("/gardens/{id}/weather", { params: { path: { id } } })),
};

export function useGardens(enabled = true) {
  return useQuery({ queryKey: qk.gardens(), queryFn: gardens.list, enabled });
}

export function useGarden(id: string | undefined) {
  return useQuery({ queryKey: qk.garden(id ?? ""), queryFn: () => gardens.get(id!), enabled: !!id });
}

export function useWeather(gardenId: string | undefined) {
  return useQuery({
    queryKey: qk.weather(gardenId ?? ""),
    queryFn: () => gardens.weather(gardenId!),
    enabled: !!gardenId,
    staleTime: 30 * 60_000,
  });
}
