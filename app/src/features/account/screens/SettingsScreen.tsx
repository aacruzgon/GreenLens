import { useState } from "react";
import { View } from "react-native";

import { useMe } from "@/api";
import { envResult } from "@/config/env";
import { useSession } from "@/features/auth";
import { Banner, Button, ListItem, Screen, Section, Text, space } from "@/ui";

import { DemoControls } from "../components/DemoControls";

// Settings (screens 3.9), F0's part: the account and sign-out; the gardens, members and notifications come in F7.
export function SettingsScreen() {
  const me = useMe();
  const session = useSession();
  const [signingOut, setSigningOut] = useState(false);
  const mock = envResult.ok && envResult.env.apiMode === "mock";

  // The session layer clears the cache and client state on every sign-out (features/shell).
  async function signOut() {
    setSigningOut(true);
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
          {session.secondFactorProblem ? (
            <Banner
              tone="watch"
              title="Sign-in codes by text aren't on yet"
              message={`Signing in asks only for your password until this works. ${session.secondFactorProblem.message}`}
              action={{ label: "Try again", onPress: session.secondFactorProblem.retry }}
            />
          ) : null}
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
