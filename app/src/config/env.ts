import { z } from "zod";

// The EXPO_PUBLIC_* values, checked once at start-up (docs/frontend/README.md §17).
// Mock mode runs with no backend; live mode needs the API's URL and a real Clerk key.

const clerkKey = z.string().regex(/^pk_(test|live)_[A-Za-z0-9]+/, "must be a Clerk publishable key (pk_test_… or pk_live_…)");

const schema = z.discriminatedUnion("apiMode", [
  z.object({
    apiMode: z.literal("mock"),
    apiUrl: z.string().optional(),
    clerkPublishableKey: z.union([clerkKey, z.literal("")]),
  }),
  z.object({
    apiMode: z.literal("live"),
    apiUrl: z.url().refine((u) => !u.endsWith("/"), "must not end with a slash"),
    clerkPublishableKey: clerkKey,
  }),
]);

export type Env = z.infer<typeof schema>;

export type EnvResult = { ok: true; env: Env } | { ok: false; problems: string[] };

export function parseEnv(raw: Record<string, string | undefined>): EnvResult {
  const result = schema.safeParse({
    apiMode: raw.EXPO_PUBLIC_API_MODE,
    apiUrl: raw.EXPO_PUBLIC_API_URL || undefined,
    clerkPublishableKey: raw.EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY ?? "",
  });
  if (result.success) return { ok: true, env: result.data };
  return {
    ok: false,
    problems: result.error.issues.map((issue) => `${issue.path.join(".") || "EXPO_PUBLIC_API_MODE"}: ${issue.message}`),
  };
}

// Metro inlines each EXPO_PUBLIC_* value only when it is read by its full name.
export const envResult = parseEnv({
  EXPO_PUBLIC_API_MODE: process.env.EXPO_PUBLIC_API_MODE,
  EXPO_PUBLIC_API_URL: process.env.EXPO_PUBLIC_API_URL,
  EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY: process.env.EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY,
});
