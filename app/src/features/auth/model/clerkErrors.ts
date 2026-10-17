// Clerk's errors as one sentence for a form (docs/frontend 9: errors say what to do next).
export function clerkMessage(error: unknown): string | undefined {
  if (!error) return undefined;
  const e = error as { longMessage?: string; message?: string; errors?: { longMessage?: string; message?: string }[] };
  return e.errors?.[0]?.longMessage ?? e.errors?.[0]?.message ?? e.longMessage ?? e.message ?? "That didn't work. Try again.";
}
