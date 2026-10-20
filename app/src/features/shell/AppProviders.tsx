import { QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { GestureHandlerRootView } from "react-native-gesture-handler";
import { SafeAreaProvider } from "react-native-safe-area-context";

import { liveTransport, queryClient, setBaseUrl, setTransport } from "@/api";
import { mockTransport } from "@/api/mock";
import type { Env } from "@/config/env";
import { SessionProvider } from "@/features/auth";
import { ThemeProvider } from "@/ui";

// The provider tree of docs/frontend/README.md §7. Called once, after the env check passed.
export function configureApi(env: Env) {
  if (env.apiMode === "mock") {
    setTransport(mockTransport);
  } else {
    setTransport(liveTransport);
    setBaseUrl(env.apiUrl);
  }
}

export function AppProviders({ env, children }: { env: Env; children: ReactNode }) {
  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <SafeAreaProvider>
        <SessionProvider publishableKey={env.clerkPublishableKey}>
          <QueryClientProvider client={queryClient}>
            <ThemeProvider>{children}</ThemeProvider>
          </QueryClientProvider>
        </SessionProvider>
      </SafeAreaProvider>
    </GestureHandlerRootView>
  );
}
