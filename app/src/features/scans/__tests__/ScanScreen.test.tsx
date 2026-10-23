// F3's exit check in Jest (roadmap 9), over the mock with no delays: Tomato 2's run from Analyse to the pill, with
// the hotspot on the photographed leaf, the ladybug larva's chip, "Not sure yet", Yes to the rings question and
// the pill; Not sure leaves a provisional plan; a failed run offers New scan; a species change reruns the scan.
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react-native";

import { newKey, qk, scans, setTransport } from "@/api";
import { IDS, mockStore, mockTransport, resetMock, scenario } from "@/api/mock";
import { ThemeProvider } from "@/ui";

import { SpeciesSheet } from "@/features/plants";

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

async function submitted(plantId: string, nextScan?: "no_plant_found"): Promise<string> {
  if (nextScan) scenario.set({ nextScan });
  const scan = await scans.create({ garden_id: IDS.garden, plant_id: plantId }, newKey());
  await scans.uploadImage(scan.id, { kind: "whole_plant", file: new Blob(["x"], { type: "image/jpeg" }) }, newKey());
  await scans.uploadImage(scan.id, { kind: "leaf_closeup", file: new Blob(["x"], { type: "image/jpeg" }) }, newKey());
  await scans.submit(scan.id);
  return scan.id;
}

let lastClient: QueryClient | undefined;

/** The scan screen; with `sheet`, the species sheet beside it, as when "Change" opens it over the screen. */
async function renderScan(scanId: string, sheet?: { plantId: string }) {
  mockParams = { scanId, ...sheet };
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  lastClient = qc;
  await render(
    <QueryClientProvider client={qc}>
      <ThemeProvider scheme="dark" reducedMotion>
        <ScanScreen />
        {sheet ? <SpeciesSheet /> : null}
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
    // The soil checks are a native menu on the More button (iOS).
    expect(screen.getByTestId("soil-menu")).toBeTruthy();

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

  it("reruns the scan when the species changes while the question is open", async () => {
    const scanId = await submitted(IDS.tomato2);
    await renderScan(scanId, { plantId: IDS.tomato2 });
    await screen.findByTestId("question-card", undefined, RUN);

    // Tomato is already the plant's species: offered as Yes, not again in the list.
    expect(await screen.findByTestId("species-yes", undefined, RUN)).toBeTruthy();
    await screen.findByText("Ocimum basilicum", undefined, RUN);
    expect(screen.queryByText("Solanum lycopersicum")).toBeNull();
    await fireEvent.press(await screen.findByText("Basil", undefined, RUN));
    await waitFor(() => expect(mockRouter.back).toHaveBeenCalled(), RUN);
    expect(screen.queryByTestId("species-problem")).toBeNull();
    expect(mockStore.plants.get(IDS.tomato2)?.species?.id).toBe("basil");

    // The open question is superseded and the rerun ends in a plan for the new species.
    const events = mockStore.scan(scanId)!.events;
    expect(events.some((e) => e.type === "run_started" && e.reason === "species_changed")).toBe(true);
    expect(events.some((e) => e.type === "question_resolved" && e.by === "superseded")).toBe(true);
    expect(await screen.findByTestId("see-result", undefined, RUN)).toBeTruthy();
    expect(screen.queryByTestId("question-card")).toBeNull();
    expect(screen.getByText("Plan ready")).toBeTruthy();
  });

  it("drops \"You answered\" when a species change reruns the scan", async () => {
    const scanId = await submitted(IDS.tomato2);
    await renderScan(scanId, { plantId: IDS.tomato2 });
    await fireEvent.press(await screen.findByRole("button", { name: "Not sure" }, RUN));
    expect(await screen.findByTestId("last-answer", undefined, RUN)).toBeTruthy();
    await screen.findByTestId("see-result", undefined, RUN);

    await fireEvent.press(await screen.findByText("Basil", undefined, RUN));
    await waitFor(() => expect(screen.getByText("Plan updated")).toBeTruthy(), RUN);
    expect(screen.queryByTestId("last-answer")).toBeNull();
  });

  it("shows a scan that can't be loaded with Try again", async () => {
    const scanId = await submitted(IDS.tomato2);
    scenario.set({ callFault: "rate_limited" });
    mockParams = { scanId };
    await render(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })}>
        <ThemeProvider scheme="dark" reducedMotion>
          <ScanScreen />
        </ThemeProvider>
      </QueryClientProvider>,
    );
    expect(await screen.findByTestId("scan-load-error", undefined, RUN)).toBeTruthy();
    await fireEvent.press(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByTestId("analysis-ring", undefined, RUN)).toBeTruthy();
    expect(screen.queryByTestId("scan-load-error")).toBeNull();
  });

  it("shows a soil check's texture when it finishes, and the plan it replaces", async () => {
    const scanId = await submitted(IDS.tomato2);
    await renderScan(scanId);
    await fireEvent.press(await screen.findByRole("button", { name: "Yes, rings" }, RUN));
    await screen.findByTestId("see-result", undefined, RUN);
    expect(screen.queryByTestId("soil-result")).toBeNull();

    // As capture's Send does: the photo, then the scan's record invalidated.
    await scans.uploadImage(scanId, { kind: "squeeze_test", file: new Blob(["x"], { type: "image/jpeg" }) }, newKey());
    await lastClient!.invalidateQueries({ queryKey: qk.scan(scanId) });
    expect(await screen.findByText("Your soil: loamy", undefined, RUN)).toBeTruthy();
    // The server's squeeze result (holds_ball), in words.
    expect(screen.getByText("It holds a ball.")).toBeTruthy();
    expect(screen.queryByText(/holds_ball/)).toBeNull();
    expect(screen.queryByTestId("checking-soil")).toBeNull();
    await waitFor(() => expect(screen.getByText("Plan updated")).toBeTruthy(), RUN);
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
