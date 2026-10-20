import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { View } from "react-native";

import { useMe } from "@/api";
import { envResult } from "@/config/env";
import { useSession } from "@/features/auth";
import { useActiveGardenStore } from "@/features/gardens";
import { Button, ListItem, Screen, Section, Text, space } from "@/ui";

import { DemoControls } from "../components/DemoControls";

// Settings (screens 3.9), F0's part: the account and sign-out; the gardens, members and notifications come in F7.
export function SettingsScreen() {
  const me = useMe();
  const session = useSession();
  const queryClient = useQueryClient();
  const setGardenId = useActiveGardenStore((s) => s.setGardenId);
  const [signingOut, setSigningOut] = useState(false);
  const mock = envResult.ok && envResult.env.apiMode === "mock";

  // Sign-out (README §8): clear the cache and client state, then end the session.
  async function signOut() {
    setSigningOut(true);
    queryClient.clear();
    setGardenId(null);
    await session.signOut();
    setSigningOut(false);
  }

  return (
    <Screen>
      <View style={{ gap: space[6], paddingTop: space[4] }}>
        <Text variant="largeTitle" emphasis accessibilityRole="header">
          Settings
        </Text>
        <Section title="Account">
          <ListItem title={me.data?.display_name || "Your account"} subtitle={session.mode === "demo" ? "Demo account" : undefined} />
        </Section>
        {mock ? <DemoControls /> : null}
        <Button label="Sign out" variant="secondary" onPress={signOut} busy={signingOut} />
        {mock ? (
          <Text variant="footnote" color="textMuted" align="center">
            Sample data. Nothing here reaches a server.
          </Text>
        ) : null}
      </View>
    </Screen>
  );
}
