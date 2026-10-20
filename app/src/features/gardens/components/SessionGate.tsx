import type { ReactNode } from "react";
import { View } from "react-native";

import { ErrorState, Screen, Skeleton, space } from "@/ui";

import { ActiveGardenContext } from "../hooks/useActiveGarden";
import { useSessionGate } from "../hooks/useSessionGate";
import { OnboardingScreen } from "../screens/OnboardingScreen";

export function SessionGate({ children }: { children: ReactNode }) {
  const gate = useSessionGate();
  switch (gate.kind) {
    case "loading":
      return (
        <Screen>
          <View style={{ paddingTop: space[12], gap: space[4] }}>
            <Skeleton height={52} width="70%" />
            <Skeleton height={120} />
          </View>
        </Screen>
      );
    case "error":
      return (
        <Screen>
          <ErrorState onRetry={gate.retry} style={{ marginTop: space[12] }} />
        </Screen>
      );
    case "onboarding":
      return <OnboardingScreen />;
    case "ready":
      return <ActiveGardenContext.Provider value={gate.gardenId}>{children}</ActiveGardenContext.Provider>;
  }
}
