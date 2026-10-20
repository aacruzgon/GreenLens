import { useInfiniteQuery, useQuery } from "@tanstack/react-query";

import { client, ok } from "../client";
import type { components } from "../generated/schema";
import { nextCursor } from "../pages";
import { STALE } from "../queryClient";
import { qk } from "../queryKeys";

type S = components["schemas"];
export type SpeciesSummary = S["SpeciesSummary"];
export type SpeciesPage = S["Page_SpeciesSummary"];

export const species = {
  list: async (q?: string, cursor?: string): Promise<SpeciesPage> =>
    ok(await client().GET("/species", { params: { query: { q: q || undefined, cursor } } })),
  get: async (id: string): Promise<SpeciesSummary> =>
    ok(await client().GET("/species/{id}", { params: { path: { id } } })),
};

// The knowledge base changes only with a migration; the stale time is for the signed photo URLs.
export function useSpecies(q?: string) {
  return useInfiniteQuery({
    queryKey: qk.species(q),
    queryFn: ({ pageParam }) => species.list(q, pageParam),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: nextCursor,
    staleTime: STALE.signedUrls,
  });
}

export function useSpeciesItem(id: string) {
  return useQuery({ queryKey: qk.speciesItem(id), queryFn: () => species.get(id), staleTime: STALE.signedUrls });
}
