import { useQuery } from "@tanstack/react-query";

import type { UpdateTaskRequest } from "@/domain/types";

import { call, client } from "../client";
import { qk } from "../queryKeys";

export const tasks = {
  list: (gardenId: string, query: { source?: "plan" | "forecast_alert" | "chat" | "user"; state?: string } = {}) =>
    call(client.GET("/tasks", { params: { query: { garden_id: gardenId, limit: 100, ...query } } })),
  update: (id: string, body: UpdateTaskRequest) => call(client.PATCH("/tasks/{id}", { params: { path: { id } }, body })),
};

export function useTasks(gardenId: string | undefined, filter: { source?: "forecast_alert"; state?: "open" } = {}) {
  return useQuery({
    queryKey: qk.tasks(gardenId ?? "", filter),
    queryFn: () => tasks.list(gardenId!, filter),
    enabled: !!gardenId,
  });
}
