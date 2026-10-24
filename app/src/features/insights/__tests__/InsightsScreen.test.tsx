// F7's Insights exit check in Jest (roadmap 13): the screen draws the demo's two moments from the mock fixture
// (analytics 6): Tomato 1's recovery, spot cover 14% → 9% → 5% under the steps done, and Tomato 2, 40 cm away,
// with early blight also found on it after the live scan, never "spread from".
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react-native";

import { setTransport } from "@/api";
import { IDS, mockStore, mockTransport, resetMock, scenario } from "@/api/mock";
import { ThemeProvider } from "@/ui";

import { InsightsScreen } from "../screens/InsightsScreen";

const mockRouter = { back: jest.fn(), push: jest.fn(), replace: jest.fn(), canGoBack: () => true, dismissAll: jest.fn() };
let mockParams: Record<string, string> = {};
jest.mock("expo-router", () => ({
  useLocalSearchParams: () => mockParams,
  useRouter: () => mockRouter,
}));

jest.setTimeout(20_000);

beforeEach(() => {
  resetMock();
  scenario.set({ delayScale: 0 });
  setTransport(mockTransport);
  jest.clearAllMocks();
});

async function open(plantId: string) {
  mockParams = { plantId };
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity } } });
  await render(
    <QueryClientProvider client={qc}>
      <ThemeProvider scheme="light" reducedMotion>
        <InsightsScreen />
      </ThemeProvider>
    </QueryClientProvider>,
  );
  await screen.findByTestId("insights-health");
}

/** Charts draw at their measured width: lays one out as the phone would. */
async function layOut(testID: string, width = 320) {
  await fireEvent(screen.getByTestId(testID), "layout", { nativeEvent: { layout: { x: 0, y: 0, width, height: 46 } } });
}

/** Tomato 2's live scan (analytics 6): early blight confirmed on it today, as Tomato 1's latest scan found. */
function liveScanOfTomato2() {
  const t1 = mockStore.scansOf(IDS.tomato1).at(-1)!;
  const copy = JSON.parse(JSON.stringify(t1)) as typeof t1;
  copy.scan.id = "scan_tomato2_live";
  copy.scan.plant_id = IDS.tomato2;
  mockStore.scans.set(copy.scan.id, copy);
  mockStore.plants.get(IDS.tomato2)!.status = "sick";
}

describe("Insights (design 8)", () => {
  it("draws Tomato 1's recovery: its strip, spot cover under the steps, height and the humid week", async () => {
    await open(IDS.tomato1);
    expect(screen.getByText(/^Insights · \w+ \d+ to \w+ \d+$/)).toBeTruthy();
    expect(screen.getByText(/^Healthy at 3 checks, then early blight on /)).toBeTruthy();
    await layOut("insights-strip");
    expect(screen.getAllByLabelText(/^\w+ \d+: Healthy$/)).toHaveLength(3);
    expect(screen.getAllByLabelText(/^\w+ \d+: Sick, early blight$/)).toHaveLength(3);

    expect(await screen.findByText("About 14% → 5%")).toBeTruthy();
    expect(screen.getByText(/^Lower at each check since /)).toBeTruthy();
    expect(screen.getByLabelText(/^Spot cover: 14 percent on .+, 9 percent on .+ and 5 percent on .+, with ranges 9 to 21, 6 to 12 and 3 to 7$/)).toBeTruthy();
    expect(await screen.findByText("Remove the infected leaves")).toBeTruthy();
    expect(screen.getByText("Mulch around the base")).toBeTruthy();
    expect(screen.getByText(/^After the steps were done/)).toBeTruthy();

    expect(await screen.findByText("About 62 cm, growing about 8 cm a week")).toBeTruthy();
    expect(screen.getByText(/provisional/)).toBeTruthy();
    expect(screen.getByText(/^3 humid days in a row before /)).toBeTruthy();
    expect(screen.getByText(/shows a pattern, not a cause/)).toBeTruthy();

    expect(await screen.findByText("Tomato 2, 40 cm away")).toBeTruthy();
    expect(screen.queryByText(/spread/i)).toBeNull();
  });

  it("names early blight as also found on Tomato 2 after the live scan", async () => {
    liveScanOfTomato2();
    await open(IDS.tomato1);
    expect(await screen.findByText(/^Also found on it today: early blight\. Healthy at its check on /)).toBeTruthy();
    expect(screen.queryByText(/spread/i)).toBeNull();
    await fireEvent.press(screen.getByTestId("insights-nearby"));
    expect(mockRouter.push).toHaveBeenCalledWith({ pathname: "/plants/[plantId]", params: { plantId: IDS.tomato2 } });
  });

  it("opens a check's result from the strip", async () => {
    await open(IDS.tomato1);
    const first = mockStore.scansOf(IDS.tomato1)[0]!.scan.id;
    await layOut("insights-strip");
    await fireEvent.press(screen.getByTestId(`strip-cell-${first}`));
    expect(mockRouter.push).toHaveBeenCalledWith({ pathname: "/scans/[scanId]/result", params: { scanId: first } });
  });

  it("leaves out spots and weather for a plant that has had no problem", async () => {
    await open(IDS.basil);
    expect(screen.getByText(/^Healthy at all \d checks$/)).toBeTruthy();
    expect(await screen.findByTestId("insights-height")).toBeTruthy();
    expect(screen.queryByTestId("insights-spots")).toBeNull();
    expect(screen.queryByTestId("insights-weather")).toBeNull();
  });
});
