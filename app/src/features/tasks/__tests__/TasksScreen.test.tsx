// The Tasks tab over the mock's demo garden (roadmap 10 item 9): grouped by due date, Done from the check box and
// as a VoiceOver action, the alert inbox, and read-only for a viewer.
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react-native";

import { setTransport } from "@/api";
import { IDS, mockStore, mockTransport, resetMock, scenario } from "@/api/mock";
import { ThemeProvider } from "@/ui";

import { TasksScreen } from "../screens/TasksScreen";

const mockRouter = { push: jest.fn(), back: jest.fn() };
jest.mock("expo-router", () => ({ useRouter: () => mockRouter }));

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

  it("is read-only for a viewer", async () => {
    scenario.set({ role: "viewer" });
    await renderTab();
    expect(screen.queryByTestId("task-check")).toBeNull();
    expect(screen.queryByTestId("task-add")).toBeNull();
  });
});
