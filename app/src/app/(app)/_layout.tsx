import { Redirect, Stack } from "expo-router";

import { useSession } from "@/features/auth";
import { SessionGate } from "@/features/gardens";

export default function AppLayout() {
  const session = useSession();
  // Stack.Protected in the root layout already keeps (app) out when signed out; this makes the rule
  // hold on a cold start too, where both groups answer "/" and the first match could win.
  if (!session.isSignedIn) return <Redirect href="/sign-in" />;
  return (
    <SessionGate>
      <Stack screenOptions={{ headerShown: false }}>
        <Stack.Screen name="(tabs)" />
        <Stack.Screen
          name="scan/new"
          options={{ presentation: "formSheet", sheetAllowedDetents: [0.6, 1], sheetGrabberVisible: true }}
        />
        <Stack.Screen
          name="notes/new"
          options={{ presentation: "formSheet", sheetAllowedDetents: [0.5], sheetGrabberVisible: true }}
        />
        {/* Capture is a camera: it covers the screen, like the Camera app (design 3.1). */}
        <Stack.Screen name="capture/[scanId]" options={{ presentation: "fullScreenModal" }} />
        {/* Analyse replaces capture with the scan, which stays full screen and dark like the camera (design 3.1). */}
        <Stack.Screen name="scans/[scanId]" options={{ presentation: "fullScreenModal" }} />
      </Stack>
    </SessionGate>
  );
}
