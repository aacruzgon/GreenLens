import { QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react-native";
import { SafeAreaProvider } from "react-native-safe-area-context";

import { makeQueryClient, setBaseUrl, setTransport } from "@/api";
import { mockDb, mockTransport } from "@/api/mock/transport";

import { SettingsScreen } from "./SettingsScreen";

jest.mock("@/features/auth", () => ({ useSession: () => ({ email: "demo@pdm.garden", signOut: jest.fn() }) }));
jest.mock("@/features/gardens", () => ({ useActiveGarden: () => ({ garden: undefined }) }));

const metrics = { frame: { x: 0, y: 0, width: 390, height: 844 }, insets: { top: 0, left: 0, right: 0, bottom: 0 } };
const queryClient = makeQueryClient();
afterAll(() => queryClient.clear());

it("shows the chosen demo outcome as selected, and the next scan uses it", async () => {
  mockDb.pace = 0;
  setTransport(mockTransport);
  setBaseUrl("http://mock.pdm");
  await render(
    <SafeAreaProvider initialMetrics={metrics}>
      <QueryClientProvider client={queryClient}>
        <SettingsScreen />
      </QueryClientProvider>
    </SafeAreaProvider>,
  );
  expect(screen.getByRole("button", { name: "Early blight" })).toBeSelected();
  await fireEvent.press(screen.getByRole("button", { name: "Healthy plant" }));
  expect(screen.getByRole("button", { name: "Healthy plant" })).toBeSelected();
  expect(screen.getByRole("button", { name: "Early blight" })).not.toBeSelected();
  expect(mockDb.scenario).toBe("healthy");
  await waitFor(() => expect(queryClient.isFetching()).toBe(0));
});
