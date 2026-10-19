import { useSignIn } from "@clerk/expo";
import { Link, useRouter } from "expo-router";
import { useState } from "react";

import { envResult } from "@/config/env";
import { Banner, Button, Text, TextField } from "@/ui";

import { AuthShell } from "../components/AuthShell";
import { clerkErrorMessage } from "../model/clerkErrors";
import { useSession } from "../session";

export function SignInScreen() {
  const session = useSession();
  return session.mode === "demo" ? <DemoSignIn onContinue={session.signInDemo} /> : <ClerkSignIn />;
}

// Mock mode with no Clerk key: a local demo user (roadmap F0 item 6).
function DemoSignIn({ onContinue }: { onContinue: () => void }) {
  return (
    <AuthShell title="Plant health, shown its work" subtitle="This build runs on sample data, with no account.">
      <Button label="Continue with the demo" onPress={onContinue} size="large" />
    </AuthShell>
  );
}

function ClerkSignIn() {
  const { signIn, fetchStatus } = useSignIn();
  const router = useRouter();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const sampleData = envResult.ok && envResult.env.apiMode === "mock";

  async function submit() {
    setError(null);
    const { error: passwordError } = await signIn.password({ identifier: email.trim(), password });
    if (passwordError) return setError(clerkErrorMessage(passwordError));
    if (signIn.status === "needs_second_factor") return router.push({ pathname: "/verify", params: { mode: "mfa" } });
    if (signIn.status === "complete") {
      const { error: finalizeError } = await signIn.finalize();
      if (finalizeError) setError(clerkErrorMessage(finalizeError));
      return;
    }
    setError("This sign-in needs a step the app doesn't support yet.");
  }

  return (
    <AuthShell title="Sign in">
      {sampleData ? <Banner tone="watch" title="Sample data" message="This build shows sample data, not your garden." /> : null}
      <TextField
        label="Email"
        value={email}
        onChangeText={setEmail}
        autoCapitalize="none"
        autoComplete="email"
        keyboardType="email-address"
        textContentType="emailAddress"
      />
      <TextField
        label="Password"
        value={password}
        onChangeText={setPassword}
        secureTextEntry
        autoComplete="current-password"
        textContentType="password"
        error={error ?? undefined}
      />
      <Button
        label="Sign in"
        onPress={submit}
        busy={fetchStatus === "fetching"}
        disabled={!email.trim() || !password}
        size="large"
      />
      <Link href="/sign-up" asChild>
        <Text variant="body" color="textMuted" accessibilityRole="link">
          New here? Create an account
        </Text>
      </Link>
    </AuthShell>
  );
}
