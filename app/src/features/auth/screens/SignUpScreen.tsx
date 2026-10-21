import { useSignUp } from "@clerk/expo";
import { Link, useRouter } from "expo-router";
import { useState } from "react";

import { Banner, Button, Text, TextField } from "@/ui";

import { AuthShell } from "../components/AuthShell";
import { clerkErrorMessage, isEmailTaken } from "../model/clerkErrors";
import { TEST_PHONE_HINT, toE164 } from "../model/phone";

export function SignUpScreen() {
  const { signUp, fetchStatus } = useSignUp();
  const router = useRouter();
  const [email, setEmail] = useState("");
  const [phone, setPhone] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [emailTaken, setEmailTaken] = useState(false);

  async function submit() {
    setError(null);
    setEmailTaken(false);
    const phoneNumber = toE164(phone);
    if (!phoneNumber) return setError("Enter a phone number with its area code.");
    const { error: createError } = await signUp.password({ emailAddress: email.trim(), phoneNumber, password });
    if (isEmailTaken(createError)) return setEmailTaken(true);
    if (createError) return setError(clerkErrorMessage(createError));
    const { error: sendError } = await signUp.verifications.sendEmailCode();
    if (sendError) return setError(clerkErrorMessage(sendError));
    router.push({ pathname: "/verify", params: { mode: "email" } });
  }

  return (
    <AuthShell title="Create an account" subtitle="We'll send a code to your email, then one to your phone. Signing in asks for a code by text.">
      {emailTaken ? (
        <Banner
          title="You already have an account"
          message="This email is already used by an account. Sign in, or reset its password if you've forgotten it."
          action={{
            label: "Reset password",
            onPress: () => router.push({ pathname: "/reset-password", params: { email: email.trim() } }),
          }}
        />
      ) : null}
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
      <Link dismissTo href="/sign-in" asChild>
        <Text variant="body" color="textMuted" accessibilityRole="link">
          Already have an account? Sign in
        </Text>
      </Link>
    </AuthShell>
  );
}
