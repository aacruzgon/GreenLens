import type { ReactNode } from "react";
import { ActivityIndicator, View } from "react-native";

import { describeError } from "@/api";
import { ErrorState, useTheme } from "@/ui";

import { useSessionGate } from "../hooks/useSessionGate";
import { OnboardingScreen } from "../screens/OnboardingScreen";

/** SessionGate (docs/frontend 7): onboarding until the user has a garden, then the app. */
export function SessionGate({ children }: { children: ReactNode }) {
  const gate = useSessionGate();
  const t = useTheme();
  if (gate.state === "loading") {
    return (
      <View style={{ flex: 1, alignItems: "center", justifyContent: "center", backgroundColor: t.colors.background }}>
        <ActivityIndicator color={t.colors.accent} accessibilityLabel="Loading your garden" />
      </View>
    );
  }
  if (gate.state === "error") {
    const e = describeError(gate.error);
    return (
      <View style={{ flex: 1, justifyContent: "center", backgroundColor: t.colors.background }}>
        <ErrorState title={e.title} message={e.message} requestId={e.requestId} action={{ label: "Try again", onPress: gate.retry }} />
      </View>
    );
  }
  if (gate.state === "onboarding") return <OnboardingScreen />;
  return <>{children}</>;
}
