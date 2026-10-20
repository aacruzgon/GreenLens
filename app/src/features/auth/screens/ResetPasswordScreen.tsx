import { useSignIn } from "@clerk/expo";
import { useLocalSearchParams, useRouter } from "expo-router";
import { useState } from "react";

import { Button, TextField } from "@/ui";

import { AuthShell } from "../components/AuthShell";
import { clerkErrorMessage } from "../model/clerkErrors";
import { continueSignIn } from "../signInSteps";

// Forgot password (Clerk's forgot-password custom flow): a code to the account's email, then a new password.
// Resetting signs out every other session, and the code by text is still asked for after it.
export function ResetPasswordScreen() {
  const params = useLocalSearchParams<{ email?: string }>();
  const { signIn, fetchStatus } = useSignIn();
  const router = useRouter();
  const [email, setEmail] = useState(params.email ?? "");
  const [sent, setSent] = useState(false);
  const [code, setCode] = useState("");
  const [password, setPassword] = useState("");
  const [emailError, setEmailError] = useState<string | null>(null);
  const [codeError, setCodeError] = useState<string | null>(null);
  const [passwordError, setPasswordError] = useState<string | null>(null);
  const busy = fetchStatus === "fetching";

  async function sendCode() {
    setEmailError(null);
    const { error: createError } = await signIn.create({ identifier: email.trim() });
    if (createError) return setEmailError(clerkErrorMessage(createError));
    const { error: sendError } = await signIn.resetPasswordEmailCode.sendCode();
    if (sendError) return setEmailError(clerkErrorMessage(sendError));
    setSent(true);
  }

  async function reset() {
    setCodeError(null);
    setPasswordError(null);
    // A refused password leaves the code verified, so a second try only sends the new password.
    if (signIn.status !== "needs_new_password") {
      const { error: verifyError } = await signIn.resetPasswordEmailCode.verifyCode({ code: code.trim() });
      if (verifyError) return setCodeError(clerkErrorMessage(verifyError));
    }
    const { error: submitError } = await signIn.resetPasswordEmailCode.submitPassword({
      password,
      signOutOfOtherSessions: true,
    });
    if (submitError) return setPasswordError(clerkErrorMessage(submitError));
    setPasswordError(await continueSignIn(signIn, router, true));
  }

  if (!sent) {
    return (
      <AuthShell title="Reset your password" subtitle="We'll send a code to the email on your account.">
        <TextField
          label="Email"
          value={email}
          onChangeText={setEmail}
          autoCapitalize="none"
          autoComplete="email"
          keyboardType="email-address"
          textContentType="emailAddress"
          error={emailError ?? undefined}
        />
        <Button label="Send code" onPress={sendCode} busy={busy} disabled={!email.trim()} size="large" />
      </AuthShell>
    );
  }

  return (
    <AuthShell title="Reset your password" subtitle={`Enter the 6-digit code we sent to ${email.trim()}, and a new password.`}>
      <TextField
        label="Code"
        value={code}
        onChangeText={setCode}
        autoComplete="one-time-code"
        textContentType="oneTimeCode"
        keyboardType="number-pad"
        error={codeError ?? undefined}
      />
      <TextField
        label="New password"
        value={password}
        onChangeText={setPassword}
        secureTextEntry
        autoComplete="new-password"
        textContentType="newPassword"
        error={passwordError ?? undefined}
      />
      <Button
        label="Reset password"
        onPress={reset}
        busy={busy}
        disabled={code.trim().length < 6 || !password}
        size="large"
      />
      <Button
        label="Send a new code"
        variant="plain"
        onPress={async () => {
          const { error: sendError } = await signIn.resetPasswordEmailCode.sendCode();
          setCodeError(sendError ? clerkErrorMessage(sendError) : null);
        }}
      />
    </AuthShell>
  );
}
