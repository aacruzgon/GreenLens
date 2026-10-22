import { useInfiniteQuery, useMutation, useQueryClient } from "@tanstack/react-query";

import { client, ok } from "../client";
import type { CreateTaskRequest, Page, Task, UpdateTaskRequest } from "@/domain/types";
import { useIdempotentMutation, type IdempotencyKey } from "../idempotency";
import { nextCursor } from "../pages";
import { qk, type TaskFilter } from "../queryKeys";

type TaskPage = Page<Task>;

export const tasks = {
  list: async (gardenId: string, filter: TaskFilter = {}, cursor?: string): Promise<TaskPage> =>
    ok(await client().GET("/tasks", { params: { query: { garden_id: gardenId, ...filter, cursor } } })),
  create: async (body: CreateTaskRequest, key: IdempotencyKey): Promise<Task> =>
    ok(await client().POST("/tasks", { body, params: { header: { "Idempotency-Key": key } } })),
  update: async (id: string, body: UpdateTaskRequest): Promise<Task> =>
    ok(await client().PATCH("/tasks/{id}", { params: { path: { id } }, body })),
};

export function useTasks(gardenId: string, filter?: TaskFilter) {
  return useInfiniteQuery({
    queryKey: qk.tasks(gardenId, filter),
    queryFn: ({ pageParam }) => tasks.list(gardenId, filter, pageParam),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: nextCursor,
    enabled: gardenId !== "",
  });
}

/** A user task; with `scan_id`, the jar test's "photograph the jar later" (contracts 9). */
export function useCreateTask() {
  const qc = useQueryClient();
  return useIdempotentMutation({
    mutationFn: (body: CreateTaskRequest, key) => tasks.create(body, key),
    onSuccess: (task) => qc.invalidateQueries({ queryKey: qk.tasks(task.garden_id) }),
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
