import { useSignUp } from "@clerk/expo";
import { useRouter } from "expo-router";
import { useState } from "react";

import { Button, TextField } from "@/ui";

import { AuthShell } from "../components/AuthShell";
import { clerkErrorMessage } from "../model/clerkErrors";
import { TEST_PHONE_HINT, toE164 } from "../model/phone";

export function SignUpScreen() {
  const { signUp, fetchStatus } = useSignUp();
  const router = useRouter();
  const [email, setEmail] = useState("");
  const [phone, setPhone] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);

  async function submit() {
    setError(null);
    const phoneNumber = toE164(phone);
    if (!phoneNumber) return setError("Enter a phone number with its area code.");
    const { error: createError } = await signUp.password({ emailAddress: email.trim(), phoneNumber, password });
    if (createError) return setError(clerkErrorMessage(createError));
    const { error: sendError } = await signUp.verifications.sendEmailCode();
    if (sendError) return setError(clerkErrorMessage(sendError));
    router.push({ pathname: "/verify", params: { mode: "email" } });
  }

  return (
    <AuthShell title="Create an account" subtitle="We'll send a code to your email, then one to your phone. Signing in asks for a code by text.">
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
        label="Phone number"
        hint={__DEV__ ? TEST_PHONE_HINT : undefined}
        value={phone}
        onChangeText={setPhone}
        autoComplete="tel"
        keyboardType="phone-pad"
        textContentType="telephoneNumber"
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
        disabled={!email.trim() || !phone.trim() || !password}
        size="large"
      />
    </AuthShell>
  );
}
