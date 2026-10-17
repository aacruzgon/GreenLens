import { useState, type ReactNode } from "react";
import { ScrollView } from "react-native";
import { QueryClientProvider } from "@tanstack/react-query";
import { GestureHandlerRootView } from "react-native-gesture-handler";
import { SafeAreaProvider } from "react-native-safe-area-context";

import { makeQueryClient, setBaseUrl, setTransport } from "@/api";
import { liveTransport } from "@/api/liveTransport";
import { mockTransport } from "@/api/mock/transport";
import { env, envProblem } from "@/config/env";
import { SessionProvider } from "@/features/auth";
import { Text, ThemeProvider, useTheme } from "@/ui";

// docs/frontend 7, provider tree. The transport is chosen once, before anything fetches.
setTransport(env.apiMode === "mock" ? mockTransport : liveTransport);
setBaseUrl(env.apiUrl);

export function AppProviders({ children }: { children: ReactNode }) {
  const [queryClient] = useState(makeQueryClient);
  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <SafeAreaProvider>
        <QueryClientProvider client={queryClient}>
          <ThemeProvider>{envProblem ? <ConfigProblem message={envProblem} /> : <SessionProvider>{children}</SessionProvider>}</ThemeProvider>
        </QueryClientProvider>
      </SafeAreaProvider>
    </GestureHandlerRootView>
  );
}

/** A build with missing or wrong EXPO_PUBLIC_* values says so, instead of failing blank (docs/frontend 17). */
function ConfigProblem({ message }: { message: string }) {
  const t = useTheme();
  return (
    <ScrollView style={{ flex: 1, backgroundColor: t.colors.background }} contentContainerStyle={{ padding: t.space.xl, paddingTop: t.space.xxxl, gap: t.space.md }}>
      <Text variant="title2" accessibilityRole="header">
        {"This build isn't configured"}
      </Text>
      <Text selectable>{message}</Text>
    </ScrollView>
  );
}
