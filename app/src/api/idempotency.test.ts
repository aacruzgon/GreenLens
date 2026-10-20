import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react-native";
import { createElement, type ReactNode } from "react";

import { newKey, useIdempotentMutation, type IdempotencyKey } from "./idempotency";

let mockN = 0;
jest.mock("@/lib/uuid", () => ({ uuid: () => `key-${++mockN}` }));

function wrapper() {
  const qc = new QueryClient({ defaultOptions: { mutations: { retry: 2, retryDelay: 0, gcTime: Infinity } } });
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client: qc }, children);
  };
}

beforeEach(() => {
  mockN = 0;
});

describe("idempotency keys", () => {
  it("newKey makes a fresh key each time", () => {
    expect(newKey()).not.toBe(newKey());
  });

  it("reuses one key and one body for every retry of an action", async () => {
    const seen: [string, IdempotencyKey][] = [];
    let failures = 2;
    const { result } = await renderHook(
      () =>
        useIdempotentMutation({
          mutationFn: async (body: string, key) => {
            seen.push([body, key]);
            if (failures-- > 0) throw new TypeError("Network request failed");
            return "ok";
          },
        }),
      { wrapper: wrapper() },
    );

    await act(async () => {
      result.current.mutate("answer");
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(seen).toEqual([
      ["answer", "key-1"],
      ["answer", "key-1"],
      ["answer", "key-1"],
    ]);
  });

  it("makes a new key for the next action, and uses a key the caller holds", async () => {
    const keys: IdempotencyKey[] = [];
    const { result } = await renderHook(
      () => useIdempotentMutation({ mutationFn: async (_: string, key) => void keys.push(key) }),
      { wrapper: wrapper() },
    );
    await act(async () => {
      await result.current.mutateAsync("a");
      await result.current.mutateAsync("a");
      await result.current.mutateAsync("a", "draft-key" as IdempotencyKey);
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(keys).toEqual(["key-1", "key-2", "draft-key"]);
  });
});
