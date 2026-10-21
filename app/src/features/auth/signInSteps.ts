import type { useSignIn } from "@clerk/expo";
import type { useRouter } from "expo-router";

import { clerkErrorMessage } from "./model/clerkErrors";

type SignIn = ReturnType<typeof useSignIn>["signIn"];
type Router = ReturnType<typeof useRouter>;

// After the password is accepted, at sign-in or after a reset: finish, or open the step Clerk asks for next.
// Returns the error to show, or null. `replace` drops the screen that led here, so Back doesn't return to it.
export async function continueSignIn(signIn: SignIn, router: Router, replace = false): Promise<string | null> {
  const go = replace ? router.replace : router.push;
  // Two-step sign-in: a code by text to the phone reserved for it.
  if (signIn.status === "needs_second_factor") {
    const { error } = await signIn.mfa.sendPhoneCode();
    if (error) return clerkErrorMessage(error);
    go({ pathname: "/verify", params: { mode: "sms" } });
    return null;
  }
  // Device Trust: a password sign-in on a new device, without a second factor, confirms it with an email code.
  if (signIn.status === "needs_client_trust") {
    const { error } = await signIn.mfa.sendEmailCode();
    if (error) return clerkErrorMessage(error);
    go({ pathname: "/verify", params: { mode: "trust" } });
    return null;
  }
  if (signIn.status === "complete") {
    const { error } = await signIn.finalize();
    return error ? clerkErrorMessage(error) : null;
  }
  return "This sign-in needs a step the app doesn't support yet.";
}
