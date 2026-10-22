// F4's exit checks in Jest (roadmap 10), over the mock with no delays: Tomato 2's diagnosis after Yes reads "Early
// blight" with no number, the measured indicator and the contradicted one, the five-step plan with its mini player;
// checking a step completes its task, and step by step checks it everywhere; Not sure leaves the provisional plan.
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react-native";

import { newKey, scans, setTransport } from "@/api";
import { IDS, mockStore, mockTransport, resetMock, scenario } from "@/api/mock";
import type { Answer } from "@/domain/types";
import { ThemeProvider } from "@/ui";

import { ScanResultScreen } from "../screens/ScanResultScreen";
import { StepModeScreen } from "../screens/StepModeScreen";

jest.mock("expo-crypto", () => ({ randomUUID: () => jest.requireActual<{ randomUUID: () => string }>("crypto").randomUUID() }));
const mockRouter = { canGoBack: jest.fn(() => true), back: jest.fn(), replace: jest.fn(), push: jest.fn(), dismissAll: jest.fn(), dismissTo: jest.fn() };
let mockParams: Record<string, string> = {};
jest.mock("expo-router", () => ({
  useLocalSearchParams: () => mockParams,
  useRouter: () => mockRouter,
  useFocusEffect: () => undefined,
}));
jest.mock("@/features/capture", () => ({
  ...jest.requireActual<object>("@/features/capture"),
  useStartCapture: () => ({ start: jest.fn(), isPending: false, error: null }),
}));
// The photo's pixels come from the native image manipulator; here, one leaf-green pixel.
jest.mock("@/platform", () => ({
  ...jest.requireActual<object>("@/platform"),
  samplePixels: jest.fn(async () => ({ width: 1, height: 1, data: new Uint8Array([70, 120, 50, 255]) })),
}));

jest.setTimeout(30_000);
const RUN = { timeout: 15_000 };

beforeEach(() => {
  resetMock();
  scenario.set({ delayScale: 0, keepaliveMs: 50 });
  setTransport(mockTransport);
  jest.clearAllMocks();
});
// As in ScanScreen.test: let the mock's stream wind down before the next test resets the mock.
afterEach(() => new Promise((r) => setTimeout(r, 150)));

async function until<T>(get: () => Promise<T>, ok: (v: T) => boolean): Promise<T> {
  for (let i = 0; i < 500; i++) {
    const v = await get();
    if (ok(v)) return v;
    await new Promise((r) => setTimeout(r, 10));
  }
  throw new Error("the mock run didn't get there");
}

/** Tomato 2's live scan, with depth on the close-up, answered and run to its plan. */
async function planned(answer: Answer): Promise<string> {
  const scan = await scans.create({ garden_id: IDS.garden, plant_id: IDS.tomato2 }, newKey());
  const jpeg = () => new Blob(["x"], { type: "image/jpeg" });
  await scans.uploadImage(scan.id, { kind: "whole_plant", file: jpeg(), depth: new Blob(["d"], { type: "image/png" }) }, newKey());
  await scans.uploadImage(scan.id, { kind: "leaf_closeup", file: jpeg(), depth: new Blob(["d"], { type: "image/png" }) }, newKey());
  await scans.submit(scan.id);
  const asking = await until(() => scans.get(scan.id), (d) => d.open_question?.status === "open");
  await scans.answer(scan.id, { question_id: asking.open_question!.id, option_id: answer }, newKey());
  await until(() => scans.get(scan.id), (d) => d.status === "planned" && d.plan !== undefined);
  return scan.id;
}

function client() {
  return new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
}

async function renderResult(scanId: string, qc = client()) {
  mockParams = { scanId };
  await render(
    <QueryClientProvider client={qc}>
      <ThemeProvider scheme="light" reducedMotion>
        <ScanResultScreen />
      </ThemeProvider>
    </QueryClientProvider>,
  );
  await screen.findByTestId("scan-result", {}, RUN);
  return qc;
}

function planTasks(scanId: string) {
  const plan = mockStore.scan(scanId)?.plan;
  return mockStore.tasks.filter((t) => t.plan_id === plan?.id);
}

describe("the diagnosis and plan", () => {
  it("reads Tomato 2's early blight from the evidence, with no number, and the five-step plan", async () => {
    const scanId = await planned("yes");
    await renderResult(scanId);

    const headline = screen.getByTestId("diagnosis-headline");
    expect(headline).toHaveTextContent("Early blight");
    expect(screen.getByTestId("diagnosis-basis")).toHaveTextContent("Tomato 2 · from 2 photos, your answer and the weather");
    // No number for the diagnosis: not in its name, not as a percentage anywhere near it (screens 6).
    expect(within(screen.getByTestId("diagnosis-hero")).queryByText(/\d+ ?%|0\.\d/)).toBeNull();

    expect(screen.getByText("Crowded: 40 cm from Tomato 1")).toBeTruthy();
    expect(screen.getByText("Leave it: it eats aphids")).toBeTruthy();

    const evidence = within(screen.getByTestId("evidence-list"));
    expect(evidence.getByText("measured")).toBeTruthy();
    expect(evidence.getByText("8–12 mm spots")).toBeTruthy();
    expect(evidence.getByText("Yellow halo around the spots")).toBeTruthy();
    expect(evidence.getByText("Not seen")).toBeTruthy();

    await waitFor(() => expect(screen.getByTestId("mini-player")).toBeTruthy(), RUN);
    expect(screen.getByText("Up next · step 1 of 5")).toBeTruthy();
    expect(within(screen.getByTestId("mini-player")).getByText("Remove the infected leaves")).toBeTruthy();
    expect(planTasks(scanId)).toHaveLength(5);
  });

  it("completes a step's task when its box is checked, and moves the mini player on", async () => {
    const scanId = await planned("yes");
    await renderResult(scanId);
    const first = await screen.findByTestId("plan-step-remove_infected_leaves", {}, RUN);
    await waitFor(() => expect(within(first).getByTestId("task-check")).toBeTruthy(), RUN);
    await fireEvent.press(within(first).getByTestId("task-check"));

    await waitFor(() => expect(planTasks(scanId).find((t) => t.plan_step_id === "remove_infected_leaves")?.done_at).toBeTruthy(), RUN);
    await waitFor(() => expect(screen.getByText("Up next · step 2 of 5")).toBeTruthy(), RUN);
  });

  it("checks a step in step-by-step mode, and the plan reads it done", async () => {
    const scanId = await planned("yes");
    const qc = await renderResult(scanId);
    await fireEvent.press(await screen.findByTestId("mini-player-play", {}, RUN));
    expect(mockRouter.push).toHaveBeenCalledWith({ pathname: "/scans/[scanId]/steps", params: { scanId, start: "0" } });
    await screen.unmount();

    mockParams = { scanId, start: "0" };
    await render(
      <QueryClientProvider client={qc}>
        <ThemeProvider scheme="light" reducedMotion>
          <StepModeScreen />
        </ThemeProvider>
      </QueryClientProvider>,
    );
    expect(await screen.findByText("Remove the infected leaves", {}, RUN)).toBeTruthy();
    await waitFor(() => expect(screen.getByTestId("step-done")).toHaveTextContent("Done"), RUN);
    await fireEvent.press(screen.getByTestId("step-done"));
    expect(await screen.findByText("Mulch around the base")).toBeTruthy();
    await waitFor(() => expect(planTasks(scanId).find((t) => t.plan_step_id === "remove_infected_leaves")?.done_at).toBeTruthy(), RUN);
    await screen.unmount();

    await renderResult(scanId, qc);
    await waitFor(() => expect(screen.getByText("Up next · step 2 of 5")).toBeTruthy(), RUN);
  });

  it("names the repeating check's dates in step-by-step mode", async () => {
    const scanId = await planned("yes");
    const qc = client();
    mockParams = { scanId, start: "4" };
    await render(
      <QueryClientProvider client={qc}>
        <ThemeProvider scheme="light" reducedMotion>
          <StepModeScreen />
        </ThemeProvider>
      </QueryClientProvider>,
    );
    expect(await screen.findByText("Check Tomato 2 for new spots", {}, RUN)).toBeTruthy();
    const dates = within(await screen.findByTestId("step-dates", {}, RUN));
    expect(dates.getByText(/^Every 3 days, starting /)).toBeTruthy();
  });

  it("words a Not sure answer's plan as provisional (screens 6)", async () => {
    const scanId = await planned("unsure");
    await renderResult(scanId);
    expect(screen.getByTestId("diagnosis-headline")).toHaveTextContent("Not sure yet: Early blight or Septoria leaf spot");
    expect(screen.getByText("Provisional plan")).toBeTruthy();
    expect(screen.getByText("We're not sure yet. These steps are safe for every likely problem.")).toBeTruthy();
  });

  it("hides the boxes and feedback from a viewer", async () => {
    const scanId = await planned("yes");
    scenario.set({ role: "viewer" });
    await renderResult(scanId);
    await screen.findByTestId("plan-step-remove_infected_leaves", {}, RUN);
    expect(screen.queryByTestId("task-check")).toBeNull();
    expect(screen.queryByTestId("result-feedback")).toBeNull();
  });
});
