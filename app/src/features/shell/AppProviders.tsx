import { useState, type ReactNode } from "react";
import { QueryClientProvider } from "@tanstack/react-query";
import { GestureHandlerRootView } from "react-native-gesture-handler";
import { SafeAreaProvider } from "react-native-safe-area-context";

import { makeQueryClient, setBaseUrl, setTransport } from "@/api";
import { liveTransport } from "@/api/liveTransport";
import { mockTransport } from "@/api/mock/transport";
import { env } from "@/config/env";
import { SessionProvider } from "@/features/auth";
import { ThemeProvider } from "@/ui";

// docs/frontend 7, provider tree. The transport is chosen once, before anything fetches.
setTransport(env.apiMode === "mock" ? mockTransport : liveTransport);
setBaseUrl(env.apiMode === "mock" ? "http://mock.pdm" : env.apiUrl);

export function AppProviders({ children }: { children: ReactNode }) {
  const [queryClient] = useState(makeQueryClient);
  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <SafeAreaProvider>
        <QueryClientProvider client={queryClient}>
          <ThemeProvider>
            <SessionProvider>{children}</SessionProvider>
          </ThemeProvider>
        </QueryClientProvider>
      </SafeAreaProvider>
    </GestureHandlerRootView>
  );
}
