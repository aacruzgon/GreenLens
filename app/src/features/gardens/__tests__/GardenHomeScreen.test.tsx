import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react-native";

import { setTransport } from "@/api";
import { IDS, mockStore, mockTransport, resetMock, scenario } from "@/api/mock";
import { haptics } from "@/platform";
import { ThemeProvider } from "@/ui";

import { ActiveGardenContext } from "../hooks/useActiveGarden";
import { GardenHomeScreen } from "../screens/GardenHomeScreen";

// The home over the mock transport with no delays (the mock's own timers stay off: delayScale 0).
async function renderHome(firstTask: string | null = "Check Tomato 2's lower leaves") {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity } } });
  await render(
    <QueryClientProvider client={qc}>
      <ThemeProvider scheme="light" reducedMotion>
        <ActiveGardenContext.Provider value={IDS.garden}>
          <GardenHomeScreen />
        </ActiveGardenContext.Provider>
      </ThemeProvider>
    </QueryClientProvider>,
  );
  await screen.findByRole("header", { name: "Back garden" });
  if (firstTask) await screen.findByText(firstTask);
}

/** Whether the status-bar strip is shown; it never takes a touch either way. */
function scrimShown(): boolean {
  expect(screen.getByTestId("status-bar-scrim", { includeHiddenElements: true }).props.pointerEvents).toBe("none");
  return !screen.getByTestId("status-bar-scrim-fade", { includeHiddenElements: true }).props.accessibilityElementsHidden;
}

function scrollTo(y: number) {
  return fireEvent.scroll(screen.getByTestId("garden-home-scroll"), { nativeEvent: { contentOffset: { x: 0, y } } });
}


beforeEach(() => {
  resetMock();
  scenario.set({ delayScale: 0 });
  setTransport(mockTransport);
});

describe("GardenHomeScreen", () => {
  it("lists the plants sick first, the frost alert and today's check of Tomato 2", async () => {
    await renderHome();
    const order = screen.getAllByTestId(/^plant-card-/).map((el) => el.props.testID);
    expect(order.slice(0, 2)).toEqual([`plant-card-${IDS.tomato1}`, `plant-card-${IDS.pepper}`]);
    expect(order).toHaveLength(6);
    expect(screen.getByText("Frost on Thursday: cover tender plants")).toBeTruthy();
    expect(await screen.findByText(/^Updated \d\d:\d\d$/)).toBeTruthy();
    expect(screen.getAllByTestId("weather-day-frost")).toHaveLength(1);
  });

  it("checks a task with a success haptic, and Done removes the frost alert", async () => {
    const success = jest.spyOn(haptics, "success");
    await renderHome();
    await fireEvent.press(screen.getByRole("checkbox", { name: "Mark done: Check Tomato 2's lower leaves" }));
    expect(success).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("checkbox", { name: "Check Tomato 2's lower leaves, done" })).toBeTruthy();
    await waitFor(() => expect(mockStore.tasks.find((t) => t.id === "task_t2_check_b")?.done_at).toBeTruthy());

    await fireEvent.press(screen.getByRole("button", { name: "Done: Frost on Thursday: cover tender plants" }));
    expect(screen.queryByText("Frost on Thursday: cover tender plants")).toBeNull();
  });

  it("sorts every plant, past the first page, and finds the alert past the first page of tasks", async () => {
    // 24 plants named before every fixture plant push the sick Tomato 1 off GET /plants' first page of 20, and
    // 24 overdue tasks push the frost alert off the first page of open tasks.
    const basil = [...mockStore.plants.values()].find((p) => p.id === IDS.basil)!;
    const chore = mockStore.tasks.find((t) => t.id === "task_t2_check_b")!;
    for (let i = 1; i <= 24; i++) {
      const n = String(i).padStart(2, "0");
      mockStore.plants.set(`plant_extra_${n}`, { ...basil, id: `plant_extra_${n}`, name: `Aster ${n}` });
      mockStore.tasks.push({ ...chore, id: `task_extra_${n}`, title: `Weed bed ${n}`, source: "user", repeat: undefined, due: "2026-01-01" });
    }
    await renderHome("Weed bed 01");
    await waitFor(() => expect(screen.getAllByTestId(/^plant-card-/)).toHaveLength(30));
    expect(screen.getAllByTestId(/^plant-card-/)[0]!.props.testID).toBe(`plant-card-${IDS.tomato1}`);
    expect(screen.getByText("Frost on Thursday: cover tender plants")).toBeTruthy();
  });

  it("says when a check fails, unchecks the row and offers to try again", async () => {
    const warning = jest.spyOn(haptics, "warning");
    await renderHome();
    scenario.set({ callFault: "invalid_state" });
    await fireEvent.press(screen.getByRole("checkbox", { name: "Mark done: Check Tomato 2's lower leaves" }));
    expect(await screen.findByText(`Couldn't mark "Check Tomato 2's lower leaves" done`)).toBeTruthy();
    expect(warning).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("checkbox", { name: "Mark done: Check Tomato 2's lower leaves" })).toBeTruthy();

    await fireEvent.press(screen.getByRole("button", { name: "Try again" }));
    expect(screen.queryByText(`Couldn't mark "Check Tomato 2's lower leaves" done`)).toBeNull();
    await waitFor(() => expect(mockStore.tasks.find((t) => t.id === "task_t2_check_b")?.done_at).toBeTruthy());
  });

  it("shows the status-bar strip only once the hero has scrolled away", async () => {
    await renderHome();
    expect(scrimShown()).toBe(false);
    await scrollTo(400);
    expect(scrimShown()).toBe(true);
    await scrollTo(0);
    expect(scrimShown()).toBe(false);
  });

  it("keeps the strip off a garden with no photo until it scrolls, so it never covers +", async () => {
    for (const [id, p] of mockStore.plants) if (p.garden_id === IDS.garden) mockStore.plants.delete(id);
    await renderHome(null);
    await screen.findByRole("button", { name: "Add a plant" });
    expect(scrimShown()).toBe(false);
    await scrollTo(400);
    expect(scrimShown()).toBe(true);
  });

  it("shows a viewer no check boxes and no Done", async () => {
    scenario.set({ role: "viewer" });
    await renderHome();
    expect(screen.queryByRole("checkbox")).toBeNull();
    expect(screen.queryByText("Done")).toBeNull();
    expect(screen.queryByRole("button", { name: "Add a plant" })).toBeNull();
  });

  it("shows editors + for a new plant", async () => {
    await renderHome();
    expect(screen.getByRole("button", { name: "Add a plant" })).toBeTruthy();
  });
});
