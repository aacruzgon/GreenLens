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
