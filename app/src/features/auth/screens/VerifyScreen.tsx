import { useSignIn, useSignUp } from "@clerk/expo";
import { useLocalSearchParams } from "expo-router";
import { useState } from "react";

import { Button, TextField } from "@/ui";

import { AuthShell } from "../components/AuthShell";
import { clerkErrorMessage } from "../model/clerkErrors";

// One code field for the sign-up email code or a new device's email code (screens 3.10).
export function VerifyScreen() {
  const { mode } = useLocalSearchParams<{ mode?: string }>();
  return mode === "trust" ? <NewDevice /> : <EmailCode />;
}

// Device Trust (Clerk's custom flow): the code sent to the account's email confirms this device.
function NewDevice() {
  const { signIn, fetchStatus } = useSignIn();
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);

  async function submit() {
    setError(null);
    const { error: verifyError } = await signIn.mfa.verifyEmailCode({ code: code.trim() });
    if (verifyError) return setError(clerkErrorMessage(verifyError));
    if (signIn.status !== "complete") return setError("This sign-in needs a step the app doesn't support yet.");
    const { error: finalizeError } = await signIn.finalize();
    if (finalizeError) setError(clerkErrorMessage(finalizeError));
  }

  return (
    <AuthShell title="Confirm this device" subtitle="We sent a 6-digit code to your email.">
      <CodeField value={code} onChange={setCode} error={error} />
      <Button label="Verify" onPress={submit} busy={fetchStatus === "fetching"} disabled={code.trim().length < 6} size="large" />
      <Button label="Send a new code" variant="plain" onPress={() => void signIn.mfa.sendEmailCode()} />
    </AuthShell>
  );
}

function EmailCode() {
  const { signUp, fetchStatus } = useSignUp();
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);

  async function submit() {
    setError(null);
    const { error: verifyError } = await signUp.verifications.verifyEmailCode({ code: code.trim() });
    if (verifyError) return setError(clerkErrorMessage(verifyError));
    if (signUp.status !== "complete") return setError("This sign-up needs a step the app doesn't support yet.");
    const { error: finalizeError } = await signUp.finalize();
    if (finalizeError) setError(clerkErrorMessage(finalizeError));
  }

  return (
    <AuthShell title="Check your email" subtitle="Enter the 6-digit code we sent you.">
      <CodeField value={code} onChange={setCode} error={error} />
      <Button label="Verify" onPress={submit} busy={fetchStatus === "fetching"} disabled={code.trim().length < 6} size="large" />
      <Button label="Send a new code" variant="plain" onPress={() => void signUp.verifications.sendEmailCode()} />
    </AuthShell>
  );
}

function CodeField({ value, onChange, error }: { value: string; onChange: (v: string) => void; error: string | null }) {
  return (
    <TextField
      label="Code"
      value={value}
      onChangeText={onChange}
      autoCapitalize="none"
      autoComplete="one-time-code"
      textContentType="oneTimeCode"
      keyboardType="number-pad"
      error={error ?? undefined}
    />
  );
}
