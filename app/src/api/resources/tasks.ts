import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { client, ok } from "../client";
import { isApiError } from "../errors";
import type { CreateTaskRequest, Page, Task, UpdateTaskRequest } from "@/domain/types";
import { useIdempotentMutation, type IdempotencyKey } from "../idempotency";
import { fetchAllPages, MAX_PAGE_SIZE, nextCursor } from "../pages";
import { qk, type TaskFilter } from "../queryKeys";

type TaskPage = Page<Task>;

export const tasks = {
  list: async (gardenId: string, filter: TaskFilter = {}, cursor?: string, limit?: number): Promise<TaskPage> =>
    ok(await client().GET("/tasks", { params: { query: { garden_id: gardenId, ...filter, cursor, limit } } })),
  /** Every matching task, all pages loaded: for a lookup whose "none" must be final. */
  listAll: (gardenId: string, filter: TaskFilter = {}): Promise<Task[]> =>
    fetchAllPages((cursor) => tasks.list(gardenId, filter, cursor, MAX_PAGE_SIZE)),
  create: async (body: CreateTaskRequest, key: IdempotencyKey): Promise<Task> =>
    ok(await client().POST("/tasks", { body, params: { header: { "Idempotency-Key": key } } })),
  update: async (id: string, body: UpdateTaskRequest): Promise<Task> =>
    ok(await client().PATCH("/tasks/{id}", { params: { path: { id } }, body })),
  /** `user` and `chat` tasks only; the others are dismissed (`409 invalid_state`; contracts 9). */
  delete: async (id: string): Promise<void> => {
    await client().DELETE("/tasks/{id}", { params: { path: { id } } });
  },
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

/** Every matching task of a garden, all pages loaded. Under qk.tasks, so a task change refetches it. */
export function useAllTasks(gardenId: string, filter?: TaskFilter) {
  return useQuery({
    queryKey: [...qk.tasks(gardenId, filter), "all"] as const,
    queryFn: () => tasks.listAll(gardenId, filter),
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

export function useDeleteTask() {
  const qc = useQueryClient();
  return useMutation({
    // A 404 means it's already gone (a retry after a lost response): what was asked for.
    mutationFn: (task: Pick<Task, "id" | "garden_id">) =>
      tasks.delete(task.id).catch((error: unknown) => {
        if (!(isApiError(error) && error.status === 404)) throw error;
      }),
    onSuccess: (_data, task) => qc.invalidateQueries({ queryKey: qk.tasks(task.garden_id) }),
  });
}
