import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react-native";
import type { ReactNode } from "react";

import { setTransport } from "@/api";
import { mockTransport, resetMock, scenario } from "@/api/mock";
import { SessionProvider } from "@/features/auth";
import { GardenHomeScreen, SessionGate } from "@/features/gardens";
import { ThemeProvider } from "@/ui";

function Providers({ children }: { children: ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity } } });
  return (
    <SessionProvider publishableKey="" onSignedOut={() => {}}>
      <QueryClientProvider client={qc}>
        <ThemeProvider>{children}</ThemeProvider>
      </QueryClientProvider>
    </SessionProvider>
  );
}

beforeEach(() => {
  resetMock();
  scenario.set({ delayScale: 0 });
  setTransport(mockTransport);
});

// The gate must hand the home its garden on the first frame after /me and /gardens load.
it("opens the garden home on the demo garden", async () => {
  await render(
    <Providers>
      <SessionGate>
        <GardenHomeScreen />
      </SessionGate>
    </Providers>,
  );
  expect(await screen.findByRole("header", { name: "Back garden" })).toBeTruthy();
  expect(await screen.findByText("Tomato 1")).toBeTruthy();
});
