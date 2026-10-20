import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { client, ok } from "../client";
import type { Me, UpdateMeRequest } from "@/domain/types";
import { qk } from "../queryKeys";

export const me = {
  get: async (): Promise<Me> => ok(await client().GET("/me")),
  update: async (body: UpdateMeRequest): Promise<Me> => ok(await client().PATCH("/me", { body })),
};

export function useMe() {
  return useQuery({ queryKey: qk.me(), queryFn: me.get });
}

export function useUpdateMe() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: me.update,
    onSuccess: (data) => qc.setQueryData(qk.me(), data),
  });
}
