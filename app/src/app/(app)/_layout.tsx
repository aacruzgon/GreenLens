import { Stack } from "expo-router";

import { SessionGate } from "@/features/gardens";
import { useTheme } from "@/ui";

export default function AppLayout() {
  const t = useTheme();
  return (
    <SessionGate>
      <Stack
        screenOptions={{
          headerTintColor: t.colors.accent,
          headerStyle: { backgroundColor: t.colors.background },
          headerTitleStyle: { color: t.colors.text },
          contentStyle: { backgroundColor: t.colors.background },
        }}
      >
        <Stack.Screen name="(tabs)" options={{ headerShown: false }} />
      </Stack>
    </SessionGate>
  );
}
