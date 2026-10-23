import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react-native";
import type { ReactNode } from "react";

import { setTransport } from "@/api";
import { IDS, mockStore, mockTransport, resetMock, scenario } from "@/api/mock";

import { useMappingSession } from "../hooks/useMappingSession";

// The AR view and the world maps are the platform's; the API is the mock's, with every PUT's key recorded.
jest.mock("@/platform/mapping", () => ({
  GardenMappingView: null,
  mapping: {
    saveWorldMap: jest.fn(async () => undefined),
    savedWorldMapVersion: jest.fn(() => null),
    relocalise: jest.fn(async () => "failed"),
    remove: jest.fn(),
    removeAll: jest.fn(),
  },
}));
jest.mock("expo-crypto", () => ({ randomUUID: () => jest.requireActual<{ randomUUID: () => string }>("crypto").randomUUID() }));
jest.mock("expo-router", () => ({ useRouter: () => ({ back: jest.fn() }) }));

let puts: { key: string | null; body: { base_version: number; pins: { plant_id?: string }[] } }[] = [];

beforeEach(() => {
  resetMock();
  scenario.set({ delayScale: 0 });
  puts = [];
  setTransport({
    fetch: async (input, init) => {
      const request = new Request(input, init);
      if (request.method === "PUT" && request.url.endsWith("/map")) {
        puts.push({ key: request.headers.get("Idempotency-Key"), body: JSON.parse(await request.clone().text()) });
      }
      return mockTransport.fetch(request);
    },
  });
});

function renderSession() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
  return renderHook(() => useMappingSession(IDS.garden, { current: null }), { wrapper });
}

/** Pins each plant, one metre apart. */
async function pin(result: Awaited<ReturnType<typeof renderSession>>["result"], plantIds: string[]) {
  for (const [i, plantId] of plantIds.entries()) {
    await act(async () => result.current.actions.choosePlant({ plantId, name: plantId }));
    await act(async () => result.current.actions.dispatch({ type: "pin", at: [i, 0] }));
  }
}

const currentVersion = () => mockStore.maps.get(IDS.garden)!.version;

describe("the mapping session's upload (contracts 9, garden map)", () => {
  it("offers Replace after a 409, and Replace sends again from the newer version", async () => {
    const { result } = await renderSession();
    await waitFor(() => expect(result.current.phase.type).toBe("mapping"));
    const started = currentVersion();
    await pin(result, [IDS.tomato1]);
    // Another member uploads meanwhile.
    mockStore.maps.set(IDS.garden, { ...mockStore.maps.get(IDS.garden)!, version: started + 1 });

    await act(async () => result.current.actions.save());
    await waitFor(() => expect(result.current.phase).toEqual({ type: "stale", current: started + 1 }));

    await act(async () => result.current.actions.replace(started + 1));
    await waitFor(() => expect(result.current.phase).toEqual({ type: "saved", version: started + 2, dropped: 0 }));
    expect(puts.map((p) => p.body.base_version)).toEqual([started, started + 1]);
    expect(puts[1]!.key).not.toBe(puts[0]!.key);
  });

  it("maps again from their version after a 409, without sending", async () => {
    const { result } = await renderSession();
    await waitFor(() => expect(result.current.phase.type).toBe("mapping"));
    const started = currentVersion();
    await pin(result, [IDS.tomato1]);
    mockStore.maps.set(IDS.garden, { ...mockStore.maps.get(IDS.garden)!, version: started + 1 });
    await act(async () => result.current.actions.save());
    await waitFor(() => expect(result.current.phase.type).toBe("stale"));

    await act(async () => result.current.actions.mapAgainFromTheirs(started + 1));
    expect(result.current.phase.type).toBe("mapping");
    expect(result.current.state.pins).toHaveLength(0);
    await pin(result, [IDS.basil]);
    await act(async () => result.current.actions.save());
    await waitFor(() => expect(result.current.phase.type).toBe("saved"));
    expect(puts.map((p) => p.body.base_version)).toEqual([started, started + 1]);
  });

  it("drops the pins of plants deleted while mapping and resends from the same base under a new key", async () => {
    const { result } = await renderSession();
    await waitFor(() => expect(result.current.phase.type).toBe("mapping"));
    const started = currentVersion();
    await pin(result, [IDS.tomato1, IDS.basil]);
    mockStore.plants.delete(IDS.basil);

    await act(async () => result.current.actions.save());
    await waitFor(() => expect(result.current.phase).toEqual({ type: "saved", version: started + 1, dropped: 1 }));
    expect(puts).toHaveLength(2);
    expect(puts[1]!.body.base_version).toBe(started);
    expect(puts[1]!.body.pins.map((p) => p.plant_id)).toEqual([IDS.tomato1]);
    expect(puts[1]!.key).not.toBe(puts[0]!.key);
  });

  it("closes the open bed first when the user asks, and saves only once something is mapped", async () => {
    const { result } = await renderSession();
    await waitFor(() => expect(result.current.phase.type).toBe("mapping"));
    for (const at of [[0, 0], [2, 0], [2, 1]] as [number, number][]) await act(async () => result.current.actions.dispatch({ type: "corner", at }));
    expect(result.current.canSave).toBe(false);
    await pin(result, [IDS.tomato1]);
    expect(result.current.canSave).toBe(true);

    await act(async () => result.current.actions.save("Herbs"));
    await waitFor(() => expect(result.current.phase.type).toBe("saved"));
    expect(mockStore.maps.get(IDS.garden)!.beds.map((b) => b.name)).toEqual(["Herbs"]);
  });

  it("shows a load failure instead of spinning", async () => {
    mockStore.gardens.delete(IDS.garden);
    const { result } = await renderSession();
    await waitFor(() => expect(result.current.phase.type).toBe("failed"));
  });
});
