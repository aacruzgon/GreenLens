import { Stack } from "expo-router";

import { SessionGate } from "@/features/gardens";

export default function AppLayout() {
  return (
    <SessionGate>
      <Stack screenOptions={{ headerShown: false }} />
    </SessionGate>
  );
}
