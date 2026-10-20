import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react-native";

import { setTransport } from "@/api";
import { IDS, mockStore, mockTransport, resetMock, scenario } from "@/api/mock";
import { haptics } from "@/platform";
import { ThemeProvider } from "@/ui";

import { ActiveGardenContext } from "../hooks/useActiveGarden";
import { GardenHomeScreen } from "../screens/GardenHomeScreen";

// The home over the mock transport with no delays (the mock's own timers stay off: delayScale 0).
async function renderHome() {
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
  await screen.findByText("Check Tomato 2's lower leaves");
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
