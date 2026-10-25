import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { client, ok } from "../client";
import type { AccountDeletion, Me, UpdateMeRequest } from "@/domain/types";
import { qk } from "../queryKeys";

export const me = {
  get: async (): Promise<Me> => ok(await client().GET("/me")),
  update: async (body: UpdateMeRequest): Promise<Me> => ok(await client().PATCH("/me", { body })),
  /** `202`: what happens to each garden; the cascade and the Clerk deletion run in the background (contracts 9). */
  remove: async (): Promise<AccountDeletion> => ok(await client().DELETE("/me")),
};

export function useMe() {
  return useQuery({ queryKey: qk.me(), queryFn: me.get });
}

export function useUpdateMe() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: me.update,
    onSuccess: (data, body) => {
      qc.setQueryData(qk.me(), data);
      // Withdrawn consent rounds every garden the user owns to about 10 km (contracts 9, account).
      if (body.location_consent === false) void qc.invalidateQueries({ queryKey: qk.gardens() });
    },
  });
}

/** DELETE /me. Nothing is invalidated: the caller signs out, which clears the cache (flows 13). */
export function useDeleteMe() {
  return useMutation({ mutationFn: me.remove });
}
