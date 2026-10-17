import { useState } from "react";
import { View } from "react-native";
import { useLocalSearchParams } from "expo-router";
import { useSignIn, useSignUp } from "@clerk/expo";

import { Banner, Button, TextField, useTheme } from "@/ui";

import { AuthShell } from "../components/AuthShell";
import { clerkMessage } from "../model/clerkErrors";

/** The email code after sign-up, or the second factor (TOTP or a backup code) at sign-in. */
export function VerifyScreen() {
  const { flow } = useLocalSearchParams<{ flow?: string }>();
  return flow === "mfa" ? <SecondFactor /> : <EmailCode />;
}

function EmailCode() {
  const t = useTheme();
  const { signUp, fetchStatus } = useSignUp();
  const [code, setCode] = useState("");
  const [error, setError] = useState<string>();

  async function submit() {
    setError(undefined);
    const { error: e } = await signUp.verifications.verifyEmailCode({ code: code.trim() });
    if (e) return setError(clerkMessage(e));
    if (signUp.status === "complete") {
      const { error: f } = await signUp.finalize();
      if (f) setError(clerkMessage(f));
    } else {
      setError("Your account needs more details. Finish signing up on the web.");
    }
  }

  return (
    <AuthShell title="Check your email" subtitle="Enter the 6-digit code we sent you.">
      <View style={{ gap: t.space.lg }}>
        <TextField label="Code" value={code} onChangeText={setCode} keyboardType="number-pad" autoComplete="one-time-code" textContentType="oneTimeCode" maxLength={6} />
        {error ? <Banner tone="danger" title={error} /> : null}
        <Button label="Verify" size="large" onPress={submit} loading={fetchStatus === "fetching"} disabled={code.trim().length < 6} />
        <Button label="Send a new code" variant="plain" onPress={() => void signUp.verifications.sendEmailCode()} />
      </View>
    </AuthShell>
  );
}

function SecondFactor() {
  const t = useTheme();
  const { signIn, fetchStatus } = useSignIn();
  const [code, setCode] = useState("");
  const [backup, setBackup] = useState(false);
  const [error, setError] = useState<string>();

  async function submit() {
    setError(undefined);
    const { error: e } = backup ? await signIn.mfa.verifyBackupCode({ code: code.trim() }) : await signIn.mfa.verifyTOTP({ code: code.trim() });
    if (e) return setError(clerkMessage(e));
    if (signIn.status === "complete") {
      const { error: f } = await signIn.finalize();
      if (f) setError(clerkMessage(f));
    }
  }

  return (
    <AuthShell
      title="Two-step check"
      subtitle={backup ? "Enter one of your backup codes." : "Enter the code from your authenticator app."}
    >
      <View style={{ gap: t.space.lg }}>
        <TextField
          label={backup ? "Backup code" : "Code"}
          value={code}
          onChangeText={setCode}
          autoCapitalize="none"
          keyboardType={backup ? "default" : "number-pad"}
          autoComplete="one-time-code"
          textContentType="oneTimeCode"
        />
        {error ? <Banner tone="danger" title={error} /> : null}
        <Button label="Verify" size="large" onPress={submit} loading={fetchStatus === "fetching"} disabled={!code.trim()} />
        <Button label={backup ? "Use my authenticator app" : "Use a backup code"} variant="plain" onPress={() => setBackup(!backup)} />
      </View>
    </AuthShell>
  );
}
