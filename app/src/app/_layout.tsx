import { Stack } from "expo-router";
import * as SplashScreen from "expo-splash-screen";
import { useEffect } from "react";

import { envResult } from "@/config/env";
import { useSession } from "@/features/auth";
import { AppProviders, ConfigErrorScreen, configureApi } from "@/features/shell";
import { useDisplayFontLoaded } from "@/ui";

// The provider tree and the auth guards of docs/frontend/README.md §7.
void SplashScreen.preventAutoHideAsync();

if (envResult.ok) configureApi(envResult.env);

export default function RootLayout() {
  if (!envResult.ok) {
    void SplashScreen.hideAsync();
    return <ConfigErrorScreen problems={envResult.problems} />;
  }
  return (
    <AppProviders env={envResult.env}>
      <RootStack />
    </AppProviders>
  );
}

function RootStack() {
  const session = useSession();
  const fontsReady = useDisplayFontLoaded();
  // The splash stays until the session is known, so the first frame is the right side of the guard.
  // A font that fails to load leaves the system font in place, so it never holds the splash.
  const ready = session.isLoaded;

  useEffect(() => {
    if (ready) void SplashScreen.hideAsync();
  }, [ready, fontsReady]);

  if (!ready) return null;
  return (
    <Stack screenOptions={{ headerShown: false }}>
      <Stack.Protected guard={!session.isSignedIn}>
        <Stack.Screen name="(auth)" />
      </Stack.Protected>
      <Stack.Protected guard={session.isSignedIn}>
        <Stack.Screen name="(app)" />
      </Stack.Protected>
      {/* Outside both guards: an invite link works signed in or out (flows 3). */}
      <Stack.Screen name="invite/[code]" />
    </Stack>
  );
}
