// What a finished Google or Apple sign-in means for the screen (README 8). Pure.
export type SsoStrategy = "oauth_google" | "oauth_apple";

interface SsoResult {
  createdSessionId: string | null;
  authSessionResult: { type: string } | null;
  signUp?: { status?: string | null; missingFields?: readonly string[] } | null;
}

/**
 * Null when the user is signed in or closed the browser; otherwise the words to show. A new account that Clerk
 * can't complete from the provider alone (this app's sign-up asks for a phone) is `missing_requirements`.
 */
export function ssoOutcome(result: SsoResult): string | null {
  if (result.createdSessionId) return null;
  const type = result.authSessionResult?.type;
  if (type === "cancel" || type === "dismiss") return null;
  if (result.signUp?.status === "missing_requirements") {
    const phone = result.signUp.missingFields?.includes("phone_number");
    return phone
      ? "A new account needs a phone number for sign-in codes. Create the account with your email first, then sign in."
      : "This account needs more details than Google or Apple sent. Create it with your email instead.";
  }
  return "The sign-in didn't finish. Try again.";
}
