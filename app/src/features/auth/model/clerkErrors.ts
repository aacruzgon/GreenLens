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

// Sign-up refused because the email already belongs to an account, so the screen can offer a password reset.
// A taken phone number has the same code but names `phone_number`; its account's email is unknown, so it isn't one.
export function isEmailTaken(error: unknown): boolean {
  if (!error || typeof error !== "object") return false;
  const e = error as { code?: unknown; meta?: { paramName?: unknown }; errors?: unknown };
  if (Array.isArray(e.errors) && e.errors.length > 0) return isEmailTaken(e.errors[0]);
  return e.code === "form_identifier_exists" && (e.meta?.paramName ?? "email_address") === "email_address";
}
