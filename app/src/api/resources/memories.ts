import { useInfiniteQuery, useMutation, useQueryClient } from "@tanstack/react-query";

import { client, ok } from "../client";
import { isApiError } from "../errors";
import type { CreateMemoryRequest, Memory, Page } from "@/domain/types";
import { useIdempotentMutation, type IdempotencyKey } from "../idempotency";
import { nextCursor } from "../pages";
import { qk } from "../queryKeys";

export const memories = {
  list: async (gardenId: string, plantId?: string, cursor?: string): Promise<Page<Memory>> =>
    ok(await client().GET("/memories", { params: { query: { garden_id: gardenId, plant_id: plantId, cursor } } })),
  create: async (body: CreateMemoryRequest, key: IdempotencyKey): Promise<Memory> =>
    ok(await client().POST("/memories", { body, params: { header: { "Idempotency-Key": key } } })),
  /** A chat memory made shared: `source` becomes `user` (contracts 8). Its author, an editor or owner. */
  share: async (id: string): Promise<Memory> => ok(await client().POST("/memories/{id}/share", { params: { path: { id } } })),
  /** The Undo of `memory_saved`, and an owner's removal of a shared memory. */
  delete: async (id: string): Promise<void> => {
    await client().DELETE("/memories/{id}", { params: { path: { id } } });
  },
};

/** A garden's memories, or one plant's: shared ones plus the caller's own (contracts 8). */
export function useMemories(gardenId: string, plantId?: string) {
  return useInfiniteQuery({
    queryKey: qk.memories(gardenId, plantId),
    queryFn: ({ pageParam }) => memories.list(gardenId, plantId, pageParam),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: nextCursor,
  });
}

export function useCreateMemory() {
  const qc = useQueryClient();
  return useIdempotentMutation({
    mutationFn: (body: CreateMemoryRequest, key) => memories.create(body, key),
    onSuccess: (memory) => qc.invalidateQueries({ queryKey: qk.memories(memory.garden_id) }),
  });
}

export function useShareMemory() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => memories.share(id),
    onSuccess: (memory) => qc.invalidateQueries({ queryKey: qk.memories(memory.garden_id) }),
  });
}

export function useDeleteMemory() {
  const qc = useQueryClient();
  return useMutation({
    // A 404 means it's already gone (a retry after a lost response): what was asked for.
    mutationFn: (memory: Pick<Memory, "id" | "garden_id">) =>
      memories.delete(memory.id).catch((error: unknown) => {
        if (!(isApiError(error) && error.status === 404)) throw error;
      }),
    onSuccess: (_data, memory) => qc.invalidateQueries({ queryKey: qk.memories(memory.garden_id) }),
  });
}
