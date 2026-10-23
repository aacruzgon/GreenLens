import { Redirect, Stack } from "expo-router";

import { useSession } from "@/features/auth";
import { useDraftSweep } from "@/features/capture";
import { SessionGate } from "@/features/gardens";

export default function AppLayout() {
  const session = useSession();
  // Capture drafts whose scan is gone or no longer pending, once per launch.
  useDraftSweep(session.isSignedIn);
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
        <Stack.Screen name="capture/[scanId]/index" options={{ presentation: "fullScreenModal" }} />
        {/* A question's photo and the guided soil checks: the capture view cut to one slot (screens 3.2). */}
        <Stack.Screen name="capture/[scanId]/question/[questionId]" options={{ presentation: "fullScreenModal" }} />
        <Stack.Screen name="capture/[scanId]/guided/[kind]" options={{ presentation: "fullScreenModal" }} />
        {/* Analyse replaces capture with the scan, which stays full screen and dark like the camera (design 3.1).
            One screen per scan: `dismissTo` a scan goes back to that scan's screen, never another scan's. */}
        <Stack.Screen name="scans/[scanId]/index" options={{ presentation: "fullScreenModal" }} getId={({ params }) => params?.scanId as string | undefined} />
        {/* "See the diagnosis and plan" pushes the result over the analysis; a plant's timeline opens it too. */}
        <Stack.Screen name="scans/[scanId]/result" />
        {/* The plan step by step covers the screen, with Close at the top (design 5, after Crouton). */}
        <Stack.Screen name="scans/[scanId]/steps" options={{ presentation: "fullScreenModal" }} />
        {/* "Was this right?", for reviewers and up (screens 3.11). */}
        <Stack.Screen
          name="scans/[scanId]/feedback"
          options={{ presentation: "formSheet", sheetAllowedDetents: [0.6, 1], sheetGrabberVisible: true }}
        />
        <Stack.Screen name="plants/[plantId]/index" />
        {/* A new task, or an edit of the user's own (screens 3.7). */}
        <Stack.Screen
          name="tasks/edit"
          options={{ presentation: "formSheet", sheetAllowedDetents: [0.75, 1], sheetGrabberVisible: true }}
        />
        {/* The species check's "Change": the top candidates and a search, as a sheet (flows 7, screens 3.11). */}
        <Stack.Screen
          name="plants/[plantId]/species"
          options={{ presentation: "formSheet", sheetAllowedDetents: [0.6, 1], sheetGrabberVisible: true }}
        />
      </Stack>
    </SessionGate>
  );
}
