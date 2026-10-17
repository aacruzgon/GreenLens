import { useQuery } from "@tanstack/react-query";

import type { UpdateMeRequest } from "@/domain/types";

import { call, client } from "../client";
import { qk } from "../queryKeys";

export const me = {
  get: () => call(client.GET("/me")),
  update: (body: UpdateMeRequest) => call(client.PATCH("/me", { body })),
};

export function useMe(enabled = true) {
  return useQuery({ queryKey: qk.me(), queryFn: me.get, enabled });
}
