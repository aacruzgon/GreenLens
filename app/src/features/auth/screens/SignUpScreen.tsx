import { useSignUp } from "@clerk/expo";
import { useRouter } from "expo-router";
import { useState } from "react";

import { Button, TextField } from "@/ui";

import { AuthShell } from "../components/AuthShell";
import { clerkErrorMessage } from "../model/clerkErrors";

export function SignUpScreen() {
  const { signUp, fetchStatus } = useSignUp();
  const router = useRouter();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);

  async function submit() {
    setError(null);
    const { error: createError } = await signUp.password({ emailAddress: email.trim(), password });
    if (createError) return setError(clerkErrorMessage(createError));
    const { error: sendError } = await signUp.verifications.sendEmailCode();
    if (sendError) return setError(clerkErrorMessage(sendError));
    router.push({ pathname: "/verify", params: { mode: "signup" } });
  }

  return (
    <AuthShell title="Create an account" subtitle="We'll send a code to your email.">
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
        autoComplete="new-password"
        textContentType="newPassword"
        error={error ?? undefined}
      />
      <Button
        label="Continue"
        onPress={submit}
        busy={fetchStatus === "fetching"}
        disabled={!email.trim() || password.length < 8}
        size="large"
      />
    </AuthShell>
  );
}
