import { useInfiniteQuery, useQueryClient } from "@tanstack/react-query";

import { client, ok } from "../client";
import type { CreateMemoryRequest, Memory, Page } from "@/domain/types";
import { useIdempotentMutation, type IdempotencyKey } from "../idempotency";
import { nextCursor } from "../pages";
import { qk } from "../queryKeys";

export const memories = {
  list: async (gardenId: string, plantId?: string, cursor?: string): Promise<Page<Memory>> =>
    ok(await client().GET("/memories", { params: { query: { garden_id: gardenId, plant_id: plantId, cursor } } })),
  create: async (body: CreateMemoryRequest, key: IdempotencyKey): Promise<Memory> =>
    ok(await client().POST("/memories", { body, params: { header: { "Idempotency-Key": key } } })),
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
