// The Tasks tab over the mock's demo garden (roadmap 10 item 9): grouped by due date, Done from the check box and
// as a VoiceOver action, the alert inbox, and read-only for a viewer.
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react-native";

import { setTransport } from "@/api";
import { IDS, mockStore, mockTransport, resetMock, scenario } from "@/api/mock";
import { ThemeProvider } from "@/ui";

import { TasksScreen } from "../screens/TasksScreen";

const mockRouter = { push: jest.fn(), back: jest.fn() };
// Focus runs on mount; `mockBlur` runs the cleanup a blur would (the tab stays mounted).
let mockBlur: (() => void) | undefined;
jest.mock("expo-router", () => ({
  useRouter: () => mockRouter,
  useFocusEffect: (effect: () => (() => void) | void) =>
    (jest.requireActual("react") as typeof import("react")).useEffect(() => {
      const cleanup = effect();
      mockBlur = cleanup ?? undefined;
      return cleanup;
    }, [effect]),
}));

beforeEach(() => {
  resetMock();
  scenario.set({ delayScale: 0 });
  setTransport(mockTransport);
  jest.clearAllMocks();
});

async function renderTab() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  await render(
    <QueryClientProvider client={qc}>
      <ThemeProvider scheme="light" reducedMotion>
        <TasksScreen gardenId={IDS.garden} />
      </ThemeProvider>
    </QueryClientProvider>,
  );
  await screen.findByText("Today");
}

const chip = (label: string) => screen.getAllByRole("button", { name: label }).find((b) => b.props.accessibilityState?.selected !== undefined)!;

async function checkFirst() {
  const first = openTasks().find((t) => t.due && t.source !== "forecast_alert")!;
  await fireEvent.press(within(await screen.findByTestId(`task-${first.id}`)).getByTestId("task-check"));
  await waitFor(() => expect(mockStore.tasks.find((t) => t.id === first.id)?.done_at).toBeTruthy());
  return first;
}

const openTasks = () => mockStore.tasks.filter((t) => t.garden_id === IDS.garden && !t.done_at && !t.dismissed_at);

describe("the Tasks tab", () => {
  it("lists the open tasks grouped by due date, and checks one done", async () => {
    await renderTab();
    const first = openTasks().find((t) => t.due && t.source !== "forecast_alert")!;
    const row = await screen.findByTestId(`task-${first.id}`);
    await fireEvent.press(within(row).getByTestId("task-check"));
    await waitFor(() => expect(mockStore.tasks.find((t) => t.id === first.id)?.done_at).toBeTruthy());
  });

  it("offers Done and Dismiss to VoiceOver on a plan task, and dismisses it", async () => {
    await renderTab();
    const plan = openTasks().find((t) => t.source === "plan")!;
    const row = await screen.findByTestId(`task-${plan.id}`);
    expect(row.props.accessibilityActions.map((a: { name: string }) => a.name)).toEqual(["done", "dismiss"]);
    await fireEvent(row, "accessibilityAction", { nativeEvent: { actionName: "dismiss" } });
    await waitFor(() => expect(mockStore.tasks.find((t) => t.id === plan.id)?.dismissed_at).toBeTruthy());
  });

  it("shows the alert inbox", async () => {
    await renderTab();
    await fireEvent.press(screen.getByText("Alerts"));
    const alert = mockStore.tasks.find((t) => t.source === "forecast_alert" && !t.done_at)!;
    expect(await screen.findByText(alert.title)).toBeTruthy();
  });

  it("keeps a checked task in Open until the view changes", async () => {
    await renderTab();
    const first = await checkFirst();
    expect(screen.getByTestId(`task-${first.id}`)).toBeTruthy();
    await fireEvent.press(chip("Done"));
    await fireEvent.press(chip("Open"));
    await waitFor(() => expect(screen.queryByTestId(`task-${first.id}`)).toBeNull());
  });

  it("lets a checked task leave Open when the tab loses focus", async () => {
    await renderTab();
    const first = await checkFirst();
    expect(screen.getByTestId(`task-${first.id}`)).toBeTruthy();
    await act(async () => mockBlur?.());
    await waitFor(() => expect(screen.queryByTestId(`task-${first.id}`)).toBeNull());
  });

  it("says so when the garden doesn't load, rather than spinning", async () => {
    setTransport({
      fetch: (input, init) => {
        const url = typeof input === "string" ? input : input.url;
        if (/\/gardens\/[^/?]+(\?|$)/.test(url)) return Promise.resolve(new Response(JSON.stringify({ error: { code: "internal", message: "The garden didn't load.", retryable: false } }), { status: 500, headers: { "content-type": "application/json" } }));
        return mockTransport.fetch(input, init);
      },
    });
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
    await render(
      <QueryClientProvider client={qc}>
        <ThemeProvider scheme="light" reducedMotion>
          <TasksScreen gardenId={IDS.garden} />
        </ThemeProvider>
      </QueryClientProvider>,
    );
    expect(await screen.findByText("The garden didn't load.")).toBeTruthy();
  });

  it("is read-only for a viewer", async () => {
    scenario.set({ role: "viewer" });
    await renderTab();
    expect(screen.queryByTestId("task-check")).toBeNull();
    expect(screen.queryByTestId("task-add")).toBeNull();
  });
});
