// The garden screen over the mock's demo garden (roadmap F7 items 1–2): with no GardenView under Jest it opens
// on the map, as in Expo Go; the plant pill grows into its card and shrinks back; the scrubber's last step is the
// frost night and chooses Frost; the shutter row scans, asks about and opens Insights for the selected plant.
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react-native";

import { setTransport } from "@/api";
import { formatLocalDate, localToday } from "@/domain/format";
import { IDS, mockStore, mockTransport, resetMock, scenario } from "@/api/mock";
import { ThemeProvider } from "@/ui";

import { GardenScreen } from "../screens/GardenScreen";

const mockRouter = { push: jest.fn(), back: jest.fn(), replace: jest.fn(), canGoBack: () => true };
jest.mock("expo-router", () => ({ useRouter: () => mockRouter, useLocalSearchParams: () => ({}), useFocusEffect: () => undefined }));
const mockStart = jest.fn(async () => undefined);
jest.mock("@/features/capture", () => ({
  ...jest.requireActual<object>("@/features/capture"),
  useStartCapture: () => ({ start: mockStart, isPending: false, error: null }),
}));
const mockAsk = jest.fn(async () => undefined);
// The session gate's active garden: the mock's demo garden.
jest.mock("@/features/gardens", () => ({ useActiveGardenId: () => "garden_back" }));
jest.mock("@/features/chat", () => ({ useAskAbout: () => ({ ask: mockAsk, opening: false, problem: undefined }) }));

beforeEach(() => {
  resetMock();
  scenario.set({ delayScale: 0 });
  setTransport(mockTransport);
  jest.clearAllMocks();
});

async function renderGarden(props: Parameters<typeof GardenScreen>[0] = {}) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  await render(
    <QueryClientProvider client={qc}>
      <ThemeProvider scheme="light" reducedMotion>
        <GardenScreen {...props} />
      </ThemeProvider>
    </QueryClientProvider>,
  );
  await screen.findByTestId("garden-screen");
  // Jest has no layout: give the map a phone's size so it draws its pins.
  await fireEvent(screen.getByTestId("garden-map"), "layout", { nativeEvent: { layout: { x: 0, y: 0, width: 402, height: 874 } } });
}

describe("the garden screen", () => {
  it("opens on the map where the 3D view can't draw, with a pin per plant and the sick plant selected", async () => {
    await renderGarden();
    expect(screen.getByTestId("garden-map")).toBeTruthy();
    // No 3D / Map switch where there is no 3D.
    expect(screen.queryByTestId("garden-view")).toBeNull();
    for (const id of [IDS.tomato1, IDS.tomato2, IDS.pepper, IDS.basil, IDS.lettuce, IDS.kale]) {
      expect(screen.getByTestId(`map-pin-${id}`)).toBeTruthy();
    }
    expect(within(screen.getByTestId("plant-pill")).getByText("Tomato 1")).toBeTruthy();
    expect(screen.getByTestId("garden-top-card")).toHaveTextContent(/Back garden · 6 plants/);
  });

  it("opens on the latest scan when the timelines arrive after the plants", async () => {
    // On the simulator the timelines came later, and the garden opened on the forecast night's index (Sep 27).
    setTransport({
      fetch: async (input, init) => {
        const url = typeof input === "string" ? input : input.url;
        if (url.includes("/timeline")) await new Promise((r) => setTimeout(r, 30));
        return mockTransport.fetch(input, init);
      },
    });
    await renderGarden();
    await waitFor(() => expect(screen.getByTestId("garden-top-card")).toHaveTextContent(`1 sick, 1 to watch, 4 healthy on ${formatLocalDate(localToday("America/Toronto"))}.`, { exact: false }));
  });

  it("grows the pill into the plant's card and shrinks it back", async () => {
    await renderGarden({ plantId: IDS.tomato1 });
    await fireEvent.press(screen.getByTestId("plant-pill"));
    expect(await screen.findByText("40 cm to Tomato 2")).toBeTruthy();
    expect(screen.getByText("62 cm")).toBeTruthy();
    await fireEvent.press(screen.getByRole("button", { name: "Shrink the plant's card" }));
    expect(screen.queryByText("40 cm to Tomato 2")).toBeNull();
  });

  it("chooses Frost at the scrubber's last step, and covers the tender plants", async () => {
    await renderGarden({ plantId: IDS.tomato1 });
    await fireEvent.press(screen.getByTestId("plant-pill"));
    const ticks = screen.getAllByTestId(/^scrubber-tick-/);
    await fireEvent.press(ticks.at(-1)!);
    await waitFor(() => expect(screen.getByTestId("garden-top-card")).toHaveTextContent(/Frost on .+ night/));
    // Tomatoes, the pepper and the basil are tender in the mock; kale is hardy and lettuce isn't known.
    expect(screen.getByTestId("garden-top-card")).toHaveTextContent(/Cover the 4 tender plants/);
    expect(screen.getByRole("button", { name: /Tomato 1.*needs a frost cover/ })).toBeTruthy();
    // The cover's ring has its word in the legend, in Frost only.
    expect(screen.getByTestId("legend-cover", { includeHiddenElements: true })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Kale.*needs a frost cover/ })).toBeNull();
    // A step back leaves Frost.
    await fireEvent.press(ticks.at(-2)!);
    await waitFor(() => expect(screen.getByTestId("garden-top-card")).toHaveTextContent(/Back garden/));
  });

  it("makes each pin its own target, the size of the pin, with its label a separate one", async () => {
    // Before: a 100 pt box per pin, so Tomato 2's covered Tomato 1's pin, 13 pt away (review of #205).
    await renderGarden();
    const pin = screen.getByTestId(`map-pin-${IDS.tomato1}`);
    expect(pin.props.style).toMatchObject({ width: 24, height: 24 });
    await fireEvent.press(screen.getByTestId(`map-label-${IDS.kale}`, { includeHiddenElements: true }));
    expect(within(screen.getByTestId("plant-pill")).getByText("Kale")).toBeTruthy();
  });

  it("selects another plant from its pin, and the shutter row acts on it", async () => {
    await renderGarden();
    await fireEvent.press(screen.getByTestId(`map-pin-${IDS.kale}`));
    expect(within(screen.getByTestId("plant-pill")).getByText("Kale")).toBeTruthy();
    await fireEvent.press(screen.getByRole("button", { name: "Scan Kale" }));
    expect(mockStart).toHaveBeenCalledWith({ plantId: IDS.kale }, { push: true });
    await fireEvent.press(screen.getByRole("button", { name: "Ask about Kale" }));
    expect(mockAsk).toHaveBeenCalledWith({ type: "plant", id: IDS.kale });
    await fireEvent.press(screen.getByRole("button", { name: "Insights for Kale" }));
    expect(mockRouter.push).toHaveBeenCalledWith({ pathname: "/plants/[plantId]/insights", params: { plantId: IDS.kale } });
  });

  it("selects the plant that needs attention even when its pin isn't first and the timelines come late", async () => {
    // Before the timelines, every pin is "unknown", and the first pin won (review of #205: Kale moved first).
    const map = mockStore.maps.get(IDS.garden)!;
    const kale = map.pins.find((p) => p.plant_id === IDS.kale)!;
    mockStore.maps.set(IDS.garden, { ...map, pins: [kale, ...map.pins.filter((p) => p !== kale)] });
    setTransport({
      fetch: async (input, init) => {
        const url = typeof input === "string" ? input : input.url;
        if (url.includes("/timeline")) await new Promise((r) => setTimeout(r, 30));
        return mockTransport.fetch(input, init);
      },
    });
    await renderGarden();
    await waitFor(() => expect(within(screen.getByTestId("plant-pill")).getByText("Tomato 1")).toBeTruthy());
  });

  it("hides the shutter from viewers", async () => {
    scenario.set({ role: "viewer" });
    await renderGarden();
    expect(screen.queryByTestId("garden-scan")).toBeNull();
    expect(screen.getByTestId("garden-ask")).toBeTruthy();
  });

  it("explains a garden with no map", async () => {
    mockStore.maps.set(IDS.garden, null);
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    await render(
      <QueryClientProvider client={qc}>
        <ThemeProvider scheme="light" reducedMotion>
          <GardenScreen />
        </ThemeProvider>
      </QueryClientProvider>,
    );
    expect(await screen.findByText("Map your garden from an iPhone or iPad with LiDAR.")).toBeTruthy();
  });
});
