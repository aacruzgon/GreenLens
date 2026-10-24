// Farm connectors (docs/connectors/README.md 5 and 11; contracts 9, farm connectors): the garden's list, the
// connect calls, the source choice, Sync now and disconnect. Every write is an owner's.
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { client, ok } from "../client";
import { isApiError, shouldRetry } from "../errors";
import type {
  AvailableSources,
  CompleteConnectorRequest,
  Connector,
  ConnectorList,
  CreateConnectorRequest,
  CreateConnectorResponse,
  SetSourcesRequest,
} from "@/domain/types";
import { useIdempotentMutation, type IdempotencyKey } from "../idempotency";
import { qk } from "../queryKeys";

export const connectors = {
  list: async (gardenId: string): Promise<ConnectorList> =>
    ok(await client().GET("/gardens/{id}/connectors", { params: { path: { id: gardenId } } })),
  /** `201` with the pending connector and the consent URL to open in an auth session. */
  create: async (gardenId: string, body: CreateConnectorRequest, key: IdempotencyKey): Promise<CreateConnectorResponse> =>
    ok(
      await client().POST("/gardens/{id}/connectors", {
        params: { path: { id: gardenId }, header: { "Idempotency-Key": key } },
        body,
      }),
    ),
  /** The code from the redirect, with the verifier that never left this device. */
  complete: async (id: string, body: CompleteConnectorRequest, key: IdempotencyKey): Promise<Connector> =>
    ok(await client().POST("/connectors/{id}/complete", { params: { path: { id }, header: { "Idempotency-Key": key } }, body })),
  availableSources: async (id: string): Promise<AvailableSources> =>
    ok(await client().GET("/connectors/{id}/available-sources", { params: { path: { id } } })),
  setSources: async (id: string, body: SetSourcesRequest): Promise<Connector> =>
    ok(await client().PUT("/connectors/{id}/sources", { params: { path: { id } }, body })),
  /** `202`; `429 rate_limited` within 5 minutes of the last accepted request. */
  sync: async (id: string, key: IdempotencyKey): Promise<void> => {
    await client().POST("/connectors/{id}/sync", { params: { path: { id }, header: { "Idempotency-Key": key } } });
  },
  /** `202`: the connector turns `deleting`, then its token is revoked and its records deleted. */
  delete: async (id: string): Promise<void> => {
    await client().DELETE("/connectors/{id}", { params: { path: { id } } });
  },
};

/** The garden's farm connectors. Chat reads them too, to tell an `era` record from a vendor's (EON E10.3). */
export function useConnectors(gardenId: string) {
  return useQuery({
    queryKey: qk.connectors(gardenId),
    queryFn: () => connectors.list(gardenId),
    enabled: gardenId !== "",
  });
}

/** What the owner may choose from: the channels the bot is in, or the Drive's folders. Asked when the picker opens. */
export function useAvailableSources(connectorId: string | undefined) {
  return useQuery({
    queryKey: qk.connectorSources(connectorId ?? ""),
    queryFn: () => connectors.availableSources(connectorId!),
    enabled: connectorId !== undefined,
    staleTime: 0,
  });
}

export function useCreateConnector(gardenId: string) {
  const qc = useQueryClient();
  return useIdempotentMutation({
    mutationFn: (body: CreateConnectorRequest, key) => connectors.create(gardenId, body, key),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.connectors(gardenId) }),
  });
}

export function useCompleteConnector() {
  const qc = useQueryClient();
  return useIdempotentMutation({
    mutationFn: ({ id, body }: { id: string; body: CompleteConnectorRequest }, key) => connectors.complete(id, body, key),
    onSuccess: (c) => qc.invalidateQueries({ queryKey: qk.connectors(c.garden_id) }),
  });
}

export function useSetConnectorSources() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: SetSourcesRequest }) => connectors.setSources(id, body),
    onSuccess: (c) => qc.invalidateQueries({ queryKey: qk.connectors(c.garden_id) }),
  });
}

export function useSyncConnector() {
  const qc = useQueryClient();
  return useIdempotentMutation({
    mutationFn: (c: Pick<Connector, "id" | "garden_id">, key) => connectors.sync(c.id, key),
    // A 429 means one was accepted in the last 5 minutes: retrying in 2 s won't change that (decision 12).
    retry: (n, e) => !(isApiError(e) && e.code === "rate_limited") && shouldRetry(n, e),
    onSuccess: (_d, { vars }) => qc.invalidateQueries({ queryKey: qk.connectors(vars.garden_id) }),
  });
}

export function useDeleteConnector() {
  const qc = useQueryClient();
  return useMutation({
    // A 404 means it's already gone (a retry after a lost response, or the cleanup took a pending one).
    mutationFn: (c: Pick<Connector, "id" | "garden_id">) =>
      connectors.delete(c.id).catch((error: unknown) => {
        if (!(isApiError(error) && error.status === 404)) throw error;
      }),
    onSuccess: (_d, c) => qc.invalidateQueries({ queryKey: qk.connectors(c.garden_id) }),
  });
}
