import { useSignIn, useSignUp } from "@clerk/expo";
import { useLocalSearchParams, useRouter } from "expo-router";
import { useState } from "react";

import { Button, TextField } from "@/ui";

import { AuthShell } from "../components/AuthShell";
import { clerkErrorMessage } from "../model/clerkErrors";
import { TEST_PHONE_HINT } from "../model/phone";

// One 6-digit code field for every step (screens 3.10):
// - email: the sign-up email code, then the phone's;
// - phone: the sign-up phone code, which completes the account;
// - sms: two-step sign-in, a code by text;
// - trust: Device Trust, an email code that confirms a new device for an account without two-step sign-in.
export function VerifyScreen() {
  const { mode } = useLocalSearchParams<{ mode?: string }>();
  switch (mode) {
    case "phone":
      return <SignUpPhone />;
    case "sms":
      return <SignInSms />;
    case "trust":
      return <NewDevice />;
    default:
      return <SignUpEmail />;
  }
}

type Result = { error: unknown };

function CodeStep({
  title,
  subtitle,
  hint,
  busy,
  verify,
  resend,
}: {
  title: string;
  subtitle: string;
  hint?: string;
  busy: boolean;
  verify: (code: string) => Promise<string | null>;
  resend: () => Promise<Result>;
}) {
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);

  async function submit() {
    setError(null);
    setError(await verify(code.trim()));
  }

  return (
    <AuthShell title={title} subtitle={subtitle}>
      <TextField
        label="Code"
        hint={hint}
        value={code}
        onChangeText={setCode}
        autoComplete="one-time-code"
        textContentType="oneTimeCode"
        keyboardType="number-pad"
        error={error ?? undefined}
      />
      <Button label="Verify" onPress={submit} busy={busy} disabled={code.trim().length < 6} size="large" />
      <Button
        label="Send a new code"
        variant="plain"
        onPress={async () => {
          const { error: sendError } = await resend();
          setError(sendError ? clerkErrorMessage(sendError) : null);
        }}
      />
    </AuthShell>
  );
}

const UNSUPPORTED = "This step isn't supported in the app yet.";

function SignUpEmail() {
  const { signUp, fetchStatus } = useSignUp();
  const router = useRouter();
  return (
    <CodeStep
      title="Check your email"
      subtitle="Enter the 6-digit code we sent you."
      busy={fetchStatus === "fetching"}
      resend={() => signUp.verifications.sendEmailCode()}
      verify={async (code) => {
        const { error } = await signUp.verifications.verifyEmailCode({ code });
        if (error) return clerkErrorMessage(error);
        // The phone is required too; its code comes next.
        if (signUp.unverifiedFields.includes("phone_number")) {
          const { error: sendError } = await signUp.verifications.sendPhoneCode();
          if (sendError) return clerkErrorMessage(sendError);
          router.replace({ pathname: "/verify", params: { mode: "phone" } });
          return null;
        }
        if (signUp.status !== "complete") return UNSUPPORTED;
        const { error: finalizeError } = await signUp.finalize();
        return finalizeError ? clerkErrorMessage(finalizeError) : null;
      }}
    />
  );
}

function SignUpPhone() {
  const { signUp, fetchStatus } = useSignUp();
  return (
    <CodeStep
      title="Check your phone"
      subtitle="Enter the 6-digit code we texted you."
      hint={__DEV__ ? TEST_PHONE_HINT : undefined}
      busy={fetchStatus === "fetching"}
      resend={() => signUp.verifications.sendPhoneCode()}
      verify={async (code) => {
        const { error } = await signUp.verifications.verifyPhoneCode({ code });
        if (error) return clerkErrorMessage(error);
        if (signUp.status !== "complete") return UNSUPPORTED;
        const { error: finalizeError } = await signUp.finalize();
        return finalizeError ? clerkErrorMessage(finalizeError) : null;
      }}
    />
  );
}

function SignInSms() {
  const { signIn, fetchStatus } = useSignIn();
  return (
    <CodeStep
      title="Two-step sign-in"
      subtitle="Enter the 6-digit code we texted to your phone."
      hint={__DEV__ ? TEST_PHONE_HINT : undefined}
      busy={fetchStatus === "fetching"}
      resend={() => signIn.mfa.sendPhoneCode()}
      verify={async (code) => {
        const { error } = await signIn.mfa.verifyPhoneCode({ code });
        if (error) return clerkErrorMessage(error);
        if (signIn.status !== "complete") return UNSUPPORTED;
        const { error: finalizeError } = await signIn.finalize();
        return finalizeError ? clerkErrorMessage(finalizeError) : null;
      }}
    />
  );
}

// Device Trust (Clerk's custom flow): the code sent to the account's email confirms this device.
function NewDevice() {
  const { signIn, fetchStatus } = useSignIn();
  return (
    <CodeStep
      title="Confirm this device"
      subtitle="We sent a 6-digit code to your email."
      busy={fetchStatus === "fetching"}
      resend={() => signIn.mfa.sendEmailCode()}
      verify={async (code) => {
        const { error } = await signIn.mfa.verifyEmailCode({ code });
        if (error) return clerkErrorMessage(error);
        if (signIn.status !== "complete") return UNSUPPORTED;
        const { error: finalizeError } = await signIn.finalize();
        return finalizeError ? clerkErrorMessage(finalizeError) : null;
      }}
    />
  );
}
