import { useState } from "react";
import { View } from "react-native";
import { useRouter } from "expo-router";
import { useSignUp } from "@clerk/expo";

import { Banner, Button, Text, TextField, useTheme } from "@/ui";

import { AuthShell } from "../components/AuthShell";
import { clerkMessage } from "../model/clerkErrors";

export function SignUpScreen() {
  const t = useTheme();
  const router = useRouter();
  const { signUp, fetchStatus } = useSignUp();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string>();

  async function submit() {
    setError(undefined);
    const { error: e } = await signUp.password({ emailAddress: email.trim(), password });
    if (e) return setError(clerkMessage(e));
    const { error: s } = await signUp.verifications.sendEmailCode();
    if (s) return setError(clerkMessage(s));
    router.push({ pathname: "/verify", params: { flow: "signup" } });
  }

  return (
    <AuthShell title="Create an account" subtitle="We'll email you a code to confirm it's you.">
      <View style={{ gap: t.space.lg }}>
        <TextField label="Email" value={email} onChangeText={setEmail} autoCapitalize="none" autoComplete="email" keyboardType="email-address" textContentType="emailAddress" />
        <TextField
          label="Password"
          value={password}
          onChangeText={setPassword}
          secureTextEntry
          autoComplete="new-password"
          textContentType="newPassword"
          hint="At least 8 characters."
        />
        {error ? <Banner tone="danger" title={error} /> : null}
        <Button label="Continue" size="large" onPress={submit} loading={fetchStatus === "fetching"} disabled={!email || password.length < 8} />
        <View style={{ flexDirection: "row", justifyContent: "center", alignItems: "center" }}>
          <Text color="textMuted">Have an account?</Text>
          <Button label="Sign in" variant="plain" onPress={() => router.back()} />
        </View>
      </View>
    </AuthShell>
  );
}
