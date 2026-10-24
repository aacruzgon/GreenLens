// The 3D scene is remounted when the garden screen comes back into view (review of #205: after a second garden
// screen closed, the first one's GardenView stayed blank until remounted).
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen } from "@testing-library/react-native";
import { useEffect } from "react";
import { View } from "react-native";

import { setTransport } from "@/api";
import { mockTransport, resetMock, scenario } from "@/api/mock";
import { ThemeProvider } from "@/ui";

import { GardenScreen } from "../screens/GardenScreen";

let mockMounts = 0;
function MockGardenView() {
  useEffect(() => {
    mockMounts += 1;
  }, []);
  return <View testID="mock-garden-view" />;
}
jest.mock("@/platform", () => ({ ...jest.requireActual<object>("@/platform"), GardenView: MockGardenView, capabilities: () => ({ depth: false, mapping: false }) }));

let mockFocus: (() => (() => void) | void) | undefined;
jest.mock("expo-router", () => ({
  useRouter: () => ({ push: jest.fn(), back: jest.fn(), replace: jest.fn(), canGoBack: () => true }),
  useLocalSearchParams: () => ({}),
  useFocusEffect: (effect: () => (() => void) | void) => {
    mockFocus = effect;
    (jest.requireActual("react") as typeof import("react")).useEffect(() => effect(), [effect]);
  },
}));
jest.mock("@/features/gardens", () => ({ useActiveGardenId: () => "garden_back" }));
jest.mock("@/features/chat", () => ({ useAskAbout: () => ({ ask: jest.fn(), opening: false, problem: undefined }) }));

beforeEach(() => {
  resetMock();
  scenario.set({ delayScale: 0 });
  setTransport(mockTransport);
  mockMounts = 0;
});

it("draws a new scene when the screen is focused again after a blur", async () => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity } } });
  await render(
    <QueryClientProvider client={qc}>
      <ThemeProvider scheme="light" reducedMotion>
        <GardenScreen />
      </ThemeProvider>
    </QueryClientProvider>,
  );
  await screen.findByTestId("mock-garden-view");
  expect(mockMounts).toBe(1);
  // A screen pushed over it, then closed: the blur's cleanup, then focus again.
  const cleanup = mockFocus!();
  await act(async () => {
    if (typeof cleanup === "function") cleanup();
    mockFocus!();
  });
  expect(mockMounts).toBe(2);
});
