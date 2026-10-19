import { useInfiniteQuery, useMutation, useQueryClient } from "@tanstack/react-query";

import { client, ok } from "../client";
import type { components } from "../generated/schema";
import { nextCursor } from "../pages";
import { qk, type TaskFilter } from "../queryKeys";

type S = components["schemas"];
export type Task = S["Task"];
export type TaskPage = S["Page_Task"];
export type UpdateTaskRequest = S["UpdateTaskRequest"];

export const tasks = {
  list: async (gardenId: string, filter: TaskFilter = {}, cursor?: string): Promise<TaskPage> =>
    ok(await client().GET("/tasks", { params: { query: { garden_id: gardenId, ...filter, cursor } } })),
  update: async (id: string, body: UpdateTaskRequest): Promise<Task> =>
    ok(await client().PATCH("/tasks/{id}", { params: { path: { id } }, body })),
};

export function useTasks(gardenId: string, filter?: TaskFilter) {
  return useInfiniteQuery({
    queryKey: qk.tasks(gardenId, filter),
    queryFn: ({ pageParam }) => tasks.list(gardenId, filter, pageParam),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: nextCursor,
  });
}

export function useUpdateTask() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: UpdateTaskRequest }) => tasks.update(id, body),
    // Completing a repeating task creates the next one, so the whole garden's lists refetch.
    onSuccess: (task) => qc.invalidateQueries({ queryKey: qk.tasks(task.garden_id) }),
  });
}
