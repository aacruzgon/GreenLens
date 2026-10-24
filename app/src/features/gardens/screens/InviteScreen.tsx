import { Redirect, useRouter } from "expo-router";
import { useEffect } from "react";
import { ActivityIndicator, View } from "react-native";

import { useSession } from "@/features/auth";
import { Button, Screen, Text, space } from "@/ui";

import { useAcceptInviteScreen } from "../hooks/useAcceptInviteScreen";
import { usePendingInviteStore } from "../hooks/pendingInviteStore";
import { parseInviteCode } from "../model/members";

// invite/[code], outside the auth guards (screens 1; flows 3). Signed out, the code is kept and the user signs in
// first; the signed-in layout opens it again. Signed in, the invite is accepted and the garden opens.
export function InviteScreen({ code: raw }: { code: string }) {
  const session = useSession();
  const setPending = usePendingInviteStore((s) => s.setCode);
  const code = parseInviteCode(raw);

  useEffect(() => {
    if (!session.isSignedIn && code) setPending(code);
  }, [session.isSignedIn, code, setPending]);

  if (!code) return <InviteProblem message="This invite link is incomplete. Ask for a new one." />;
  if (!session.isSignedIn) return <Redirect href="/sign-in" />;
  return <Accepting code={code} />;
}

function Accepting({ code }: { code: string }) {
  const router = useRouter();
  const { state, retry } = useAcceptInviteScreen(code);

  useEffect(() => {
    // dismissTo pops back to the garden home when it is under this screen, or replaces this screen with it.
    if (state.kind === "joined") router.dismissTo("/");
  }, [state, router]);

  if (state.kind === "failed") return <InviteProblem message={state.message} retry={state.canRetry ? retry : undefined} />;
  return (
    <Screen>
      <View style={{ paddingTop: space[16], alignItems: "center", gap: space[4] }} accessibilityLiveRegion="polite">
        <ActivityIndicator />
        <Text variant="body" color="textMuted">
          {state.kind === "joined" ? `Opening ${state.garden.name}` : "Joining the garden"}
        </Text>
      </View>
    </Screen>
  );
}

function InviteProblem({ message, retry }: { message: string; retry?: () => void }) {
  const router = useRouter();
  return (
    <Screen>
      <View style={{ paddingTop: space[16], gap: space[4] }}>
        <Text variant="title2" emphasis accessibilityRole="header">
          {"Couldn't join"}
        </Text>
        <Text variant="body" color="textMuted" testID="invite-problem">
          {message}
        </Text>
        {retry ? <Button label="Try again" variant="secondary" onPress={retry} /> : null}
        <Button label="Go to your garden" variant="plain" onPress={() => router.dismissTo("/")} />
      </View>
    </Screen>
  );
}
