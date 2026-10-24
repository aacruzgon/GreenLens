// Clerk errors arrive as ClerkError objects or as a list on a ClerkAPIResponseError; show the most specific text.
export function clerkErrorMessage(error: unknown): string {
  if (error && typeof error === "object") {
    const e = error as { longMessage?: unknown; message?: unknown; errors?: unknown };
    if (Array.isArray(e.errors) && e.errors.length > 0) return clerkErrorMessage(e.errors[0]);
    if (typeof e.longMessage === "string" && e.longMessage) return e.longMessage;
    if (typeof e.message === "string" && e.message) return e.message;
  }
  return "Something went wrong. Try again.";
}

// The first error of an API response, or the error itself.
function firstError(error: unknown): { code?: unknown; meta?: { paramName?: unknown } } | undefined {
  if (!error || typeof error !== "object") return undefined;
  const e = error as { errors?: unknown };
  if (Array.isArray(e.errors) && e.errors.length > 0) return firstError(e.errors[0]);
  return error as { code?: unknown; meta?: { paramName?: unknown } };
}

// Clerk's machine-stable code, such as `form_identifier_exists`.
export function clerkErrorCode(error: unknown): string | undefined {
  const code = firstError(error)?.code;
  return typeof code === "string" ? code : undefined;
}

// Sign-up refused because the email already belongs to an account, so the screen can offer a password reset.
// A taken phone number has the same code but names `phone_number`; its account's email is unknown, so it isn't one.
export function isEmailTaken(error: unknown): boolean {
  const paramName = firstError(error)?.meta?.paramName ?? "email_address";
  return clerkErrorCode(error) === "form_identifier_exists" && paramName === "email_address";
}

// A sign-in method the Clerk instance doesn't have turned on: an error on the `strategy` parameter, such as
// "oauth_apple does not match one of the allowed values for parameter strategy" (seen on the development instance).
export function isStrategyOff(error: unknown): boolean {
  const first = firstError(error);
  if (first?.meta?.paramName === "strategy") return true;
  const message = clerkErrorMessage(error);
  return /allowed values for parameter strategy/i.test(message);
}
