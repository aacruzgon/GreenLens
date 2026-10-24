import { QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { GestureHandlerRootView } from "react-native-gesture-handler";
import { KeyboardProvider } from "react-native-keyboard-controller";
import { SafeAreaProvider } from "react-native-safe-area-context";

import { liveTransport, queryClient, setBaseUrl, setTransport } from "@/api";
import { mockAuthSession, mockTransport } from "@/api/mock";
import type { Env } from "@/config/env";
import { SessionProvider } from "@/features/auth";
import { useDraftStore } from "@/features/capture";
import { clearPendingInvite, useActiveGardenStore } from "@/features/gardens";
import { authSession, mapping } from "@/platform";
import { ThemeProvider } from "@/ui";

// The provider tree of docs/frontend/README.md §7. Called once, after the env check passed.
export function configureApi(env: Env) {
  if (env.apiMode === "mock") {
    setTransport(mockTransport);
    // The connect flow's consent page answers at once, with no vendor (docs/connectors/README.md 5).
    authSession.setFake(mockAuthSession);
  } else {
    setTransport(liveTransport);
    setBaseUrl(env.apiUrl);
  }
}

// Sign-out (README 8): nothing of the last account stays for the next one on this phone.
export function clearSignedOutState() {
  queryClient.clear();
  useActiveGardenStore.getState().setGardenId(null);
  // An invite kept while signed out is for the account that signs in next, not one after a sign-out (flows 3).
  clearPendingInvite();
  // Capture drafts and their photos (README 12).
  useDraftStore.getState().clear();
  // Garden world maps from mapping sessions (architecture 6.8).
  mapping.removeAll();
}

export function AppProviders({ env, children }: { env: Env; children: ReactNode }) {
  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <SafeAreaProvider>
        {/* The chat composer rides the keyboard (README 4, react-native-keyboard-controller). */}
        <KeyboardProvider>
          <SessionProvider publishableKey={env.clerkPublishableKey} onSignedOut={clearSignedOutState}>
            <QueryClientProvider client={queryClient}>
              <ThemeProvider>{children}</ThemeProvider>
            </QueryClientProvider>
          </SessionProvider>
        </KeyboardProvider>
      </SafeAreaProvider>
    </GestureHandlerRootView>
  );
}
