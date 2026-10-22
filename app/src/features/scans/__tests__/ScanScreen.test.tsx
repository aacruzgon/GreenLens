// F3's exit check in Jest (roadmap 9), over the mock with no delays: Tomato 2's run from Analyse to the pill, with
// the hotspot on the photographed leaf, the ladybug larva's chip, "Not sure yet", Yes to the rings question and
// the pill; Not sure leaves a provisional plan; a failed run offers New scan.
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, within } from "@testing-library/react-native";

import { newKey, scans, setTransport } from "@/api";
import { IDS, mockTransport, resetMock, scenario } from "@/api/mock";
import { ThemeProvider } from "@/ui";

import { ScanScreen } from "../screens/ScanScreen";

jest.mock("expo-crypto", () => ({ randomUUID: () => jest.requireActual<{ randomUUID: () => string }>("crypto").randomUUID() }));
const mockRouter = { canGoBack: jest.fn(() => true), back: jest.fn(), replace: jest.fn(), push: jest.fn(), dismissAll: jest.fn(), dismissTo: jest.fn() };
let mockParams: Record<string, string> = {};
jest.mock("expo-router", () => ({
  useLocalSearchParams: () => mockParams,
  useRouter: () => mockRouter,
  useFocusEffect: () => undefined,
}));
// New scan opens capture through the garden's session; here it only has to be called.
const mockStart = jest.fn();
jest.mock("@/features/capture", () => ({
  ...jest.requireActual<object>("@/features/capture"),
  useStartCapture: () => ({ start: mockStart, isPending: false, error: null }),
}));

// A scripted run re-renders the screen for each of its ~70 events; with the whole suite running in parallel, that
// takes longer than Testing Library's and Jest's defaults.
jest.setTimeout(30_000);
const RUN = { timeout: 15_000 };

beforeEach(() => {
  resetMock();
  scenario.set({ delayScale: 0, keepaliveMs: 50 });
  setTransport(mockTransport);
  jest.clearAllMocks();
});

// A test can end with the stream open (waiting for the answer). After Testing Library unmounts the screen, let the
// mock's stream wind down before the next test resets the mock: resetting under it fails the next test with
// "ReadableStream is already closed" (seen in Jest only; where it is thrown isn't found yet).
afterEach(() => new Promise((r) => setTimeout(r, 150)));

async function submitted(plantId: string, nextScan?: "no_plant_found"): Promise<string> {
  if (nextScan) scenario.set({ nextScan });
  const scan = await scans.create({ garden_id: IDS.garden, plant_id: plantId }, newKey());
  await scans.uploadImage(scan.id, { kind: "whole_plant", file: new Blob(["x"], { type: "image/jpeg" }) }, newKey());
  await scans.uploadImage(scan.id, { kind: "leaf_closeup", file: new Blob(["x"], { type: "image/jpeg" }) }, newKey());
  await scans.submit(scan.id);
  return scan.id;
}

async function renderScan(scanId: string) {
  mockParams = { scanId };
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  await render(
    <QueryClientProvider client={qc}>
      <ThemeProvider scheme="dark" reducedMotion>
        <ScanScreen />
      </ThemeProvider>
    </QueryClientProvider>,
  );
  // The photo's frame, so the hotspots have somewhere to go.
  await fireEvent(await screen.findByTestId("analysis-photo", undefined, RUN), "layout", { nativeEvent: { layout: { x: 0, y: 0, width: 390, height: 844 } } });
}

describe("ScanScreen", () => {
  it("runs Tomato 2's scan to the question, takes Yes, and offers the diagnosis and plan", async () => {
    const scanId = await submitted(IDS.tomato2);
    await renderScan(scanId);

    expect(await screen.findByText("Not sure yet: Early blight or Septoria leaf spot", undefined, RUN)).toBeTruthy();
    expect(screen.getByText("Do the spots on the lower leaves have target-like rings, dark circles inside circles?")).toBeTruthy();
    // The findings on the photographed leaf: spots, never the problem's name, and the beneficial larva.
    await fireEvent.press(screen.getByRole("tab", { name: "Close-up" }));
    const photo = within(screen.getByTestId("analysis-photo"));
    expect(photo.getByLabelText("Leaf spots")).toBeTruthy();
    expect(photo.getByLabelText("Ladybug larva")).toBeTruthy();
    expect(photo.queryByLabelText(/blight|septoria/i)).toBeNull();
    expect(screen.getByTestId("analysis-ring")).toBeTruthy();
    expect(screen.getByText("Waiting for your answer")).toBeTruthy();

    // The rings indicator has picture answers.
    await fireEvent.press(screen.getByRole("button", { name: "Yes, rings" }));
    expect(await screen.findByTestId("see-result", undefined, RUN)).toBeTruthy();
    expect(screen.getByText("Plan ready")).toBeTruthy();
    expect(screen.queryByTestId("question-card")).toBeNull();
    expect(screen.queryByTestId("analysis-ring")).toBeNull();
    expect(screen.queryByTestId("uncertain-result")).toBeNull();

    await fireEvent.press(screen.getByTestId("see-result"));
    expect(mockRouter.push).toHaveBeenCalledWith({ pathname: "/scans/[scanId]/result", params: { scanId } });
  });

  it("leaves a provisional plan after Not sure", async () => {
    const scanId = await submitted(IDS.tomato2);
    await renderScan(scanId);
    await fireEvent.press(await screen.findByRole("button", { name: "Not sure" }, RUN));
    expect(await screen.findByTestId("see-result", undefined, RUN)).toBeTruthy();
    expect(screen.getByText("Not sure yet: Early blight or Septoria leaf spot")).toBeTruthy();
    expect(screen.getByTestId("uncertain-result")).toBeTruthy();
  });

  it("opens the run log from the stage card", async () => {
    const scanId = await submitted(IDS.tomato2);
    await renderScan(scanId);
    await screen.findByTestId("question-card", undefined, RUN);
    await fireEvent.press(screen.getByRole("button", { expanded: false }));
    expect(screen.getByTestId("phase-plant-done")).toBeTruthy();
    expect(screen.getByTestId("phase-signs-running")).toBeTruthy();
    expect(screen.getByTestId("phase-weigh-waiting")).toBeTruthy();
  });

  it("hides the answers from a viewer", async () => {
    const scanId = await submitted(IDS.tomato2);
    scenario.set({ role: "viewer" });
    await renderScan(scanId);
    expect(await screen.findByText("An editor or the garden's owner can answer this.", undefined, RUN)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Yes, rings" })).toBeNull();
  });

  it("shows a failed run with New scan", async () => {
    const scanId = await submitted(IDS.lettuce, "no_plant_found");
    await renderScan(scanId);
    expect(await screen.findByTestId("failure-card", undefined, RUN)).toBeTruthy();
    expect(screen.getByText("We could not find a plant in your photos. Take a new photo with the plant in the middle.")).toBeTruthy();
    expect(screen.getByText("Stopped")).toBeTruthy();
    await fireEvent.press(screen.getByRole("button", { name: "New scan" }));
    expect(mockStart).toHaveBeenCalledWith({ plantId: IDS.lettuce });
  });
});
