import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react-native";
import { SafeAreaProvider } from "react-native-safe-area-context";

import { makeQueryClient, setBaseUrl, setTransport } from "@/api";
import { mockDb, mockTransport } from "@/api/mock/transport";

import { GardenHomeScreen } from "./GardenHomeScreen";

jest.mock("@/features/auth", () => ({ onSignOut: () => () => {} }));

beforeAll(() => {
  mockDb.pace = 0;
  setTransport(mockTransport);
  setBaseUrl("http://mock.pdm");
});

const metrics = { frame: { x: 0, y: 0, width: 390, height: 844 }, insets: { top: 47, left: 0, right: 0, bottom: 34 } };
const queryClient = makeQueryClient();
afterAll(() => queryClient.clear());

it("renders the sample garden's plants, weather and today's tasks", async () => {
  await render(
    <SafeAreaProvider initialMetrics={metrics}>
      <QueryClientProvider client={queryClient}>
        <GardenHomeScreen />
      </QueryClientProvider>
    </SafeAreaProvider>,
  );
  expect(await screen.findByText("Tomato by the fence")).toBeOnTheScreen();
  expect(await screen.findByText("14°C")).toBeOnTheScreen();
  expect(await screen.findByText("Check the lower leaves for new spots")).toBeOnTheScreen();
  expect(screen.getByLabelText("Status: Needs care")).toBeOnTheScreen();
});
