import { useState } from "react";
import { View } from "react-native";
import { useRouter } from "expo-router";
import { useSignIn } from "@clerk/expo";

import { env } from "@/config/env";
import { Banner, Button, Text, TextField, useTheme } from "@/ui";

import { AuthShell } from "../components/AuthShell";
import { clerkMessage } from "../model/clerkErrors";
import { useDemoSignIn } from "../session";

export function SignInScreen() {
  return env.demoAuth ? <DemoSignIn /> : <ClerkSignIn />;
}

function ClerkSignIn() {
  const t = useTheme();
  const router = useRouter();
  const { signIn, fetchStatus } = useSignIn();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string>();

  async function submit() {
    setError(undefined);
    const { error: e } = await signIn.password({ emailAddress: email.trim(), password });
    if (e) return setError(clerkMessage(e));
    if (signIn.status === "complete") {
      const { error: f } = await signIn.finalize();
      if (f) setError(clerkMessage(f));
    } else if (signIn.status === "needs_second_factor") {
      router.push({ pathname: "/verify", params: { flow: "mfa" } });
    } else {
      setError("This account needs another step we don't support yet. Try signing in on the web.");
    }
  }

  return (
    <AuthShell title="Welcome back" subtitle="Sign in to check on your garden.">
      <View style={{ gap: t.space.lg }}>
        {env.apiMode === "mock" ? (
          <Banner tone="watch" title="Sample data" body="You're signing in with a real account, but this build shows sample data, not your garden." />
        ) : null}
        <TextField label="Email" value={email} onChangeText={setEmail} autoCapitalize="none" autoComplete="email" keyboardType="email-address" textContentType="emailAddress" />
        <TextField label="Password" value={password} onChangeText={setPassword} secureTextEntry autoComplete="current-password" textContentType="password" onSubmitEditing={submit} />
        {error ? <Banner tone="danger" title={error} /> : null}
        <Button label="Sign in" size="large" onPress={submit} loading={fetchStatus === "fetching"} disabled={!email || !password} />
        <View style={{ flexDirection: "row", justifyContent: "center", alignItems: "center" }}>
          <Text color="textMuted">New here?</Text>
          <Button label="Create an account" variant="plain" onPress={() => router.push("/sign-up")} />
        </View>
      </View>
    </AuthShell>
  );
}

function DemoSignIn() {
  const t = useTheme();
  const signIn = useDemoSignIn();
  const [name, setName] = useState("");
  return (
    <AuthShell title="PDM" subtitle="Photograph a plant. Find out what's wrong, and what to do about it.">
      <View style={{ gap: t.space.lg }}>
        <Banner
          tone="info"
          title="Demo mode"
          body="This build runs on sample data with no account. Photos you take stay on this device."
        />
        <TextField label="Your name" value={name} onChangeText={setName} autoComplete="name" placeholder="Optional" />
        <Button label="Try the demo garden" icon="leaf" size="large" onPress={() => signIn(true, name.trim())} />
      </View>
    </AuthShell>
  );
}
