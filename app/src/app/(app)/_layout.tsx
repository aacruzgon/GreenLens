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
      <Stack screenOptions={{ headerShown: false }} />
    </SessionGate>
  );
}
