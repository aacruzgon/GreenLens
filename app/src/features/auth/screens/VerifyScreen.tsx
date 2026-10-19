import { useSignIn, useSignUp } from "@clerk/expo";
import { useLocalSearchParams } from "expo-router";
import { useState } from "react";

import { Button, TextField } from "@/ui";

import { AuthShell } from "../components/AuthShell";
import { clerkErrorMessage } from "../model/clerkErrors";

// One code field for the sign-up email code or the second factor (screens 3.10).
export function VerifyScreen() {
  const { mode } = useLocalSearchParams<{ mode?: string }>();
  return mode === "mfa" ? <SecondFactor /> : <EmailCode />;
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

function SecondFactor() {
  const { signIn, fetchStatus } = useSignIn();
  const [code, setCode] = useState("");
  const [backup, setBackup] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit() {
    setError(null);
    const verify = backup ? signIn.mfa.verifyBackupCode : signIn.mfa.verifyTOTP;
    const { error: verifyError } = await verify({ code: code.trim() });
    if (verifyError) return setError(clerkErrorMessage(verifyError));
    const { error: finalizeError } = await signIn.finalize();
    if (finalizeError) setError(clerkErrorMessage(finalizeError));
  }

  return (
    <AuthShell
      title="Two-step sign-in"
      subtitle={backup ? "Enter one of your backup codes." : "Enter the code from your authenticator app."}
    >
      <CodeField value={code} onChange={setCode} error={error} backup={backup} />
      <Button label="Verify" onPress={submit} busy={fetchStatus === "fetching"} disabled={!code.trim()} size="large" />
      <Button
        label={backup ? "Use your authenticator app" : "Use a backup code"}
        variant="plain"
        onPress={() => {
          setBackup(!backup);
          setCode("");
        }}
      />
    </AuthShell>
  );
}

function CodeField({ value, onChange, error, backup }: { value: string; onChange: (v: string) => void; error: string | null; backup?: boolean }) {
  return (
    <TextField
      label={backup ? "Backup code" : "Code"}
      value={value}
      onChangeText={onChange}
      autoCapitalize="none"
      autoComplete="one-time-code"
      textContentType="oneTimeCode"
      keyboardType={backup ? "default" : "number-pad"}
      error={error ?? undefined}
    />
  );
}
