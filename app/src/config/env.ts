import { z } from "zod";

// docs/frontend 17. Read once at start-up. API_MODE has no default, and live mode needs a URL and a
// Clerk key, so a build made without its variables stops with a message instead of shipping fixtures.

const clerkKey = z
  .string({ error: "EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY is required in live mode" })
  .regex(/^pk_(test|live)_[A-Za-z0-9]+$/, "EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY must be a Clerk publishable key (pk_test_… or pk_live_…), or empty in mock mode")
  .refine((k) => !k.includes("replace_me"), "EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY is still the placeholder from .env.example");

const apiUrl = z
  .string({ error: "EXPO_PUBLIC_API_URL is required in live mode" })
  .url("EXPO_PUBLIC_API_URL must be a URL")
  .refine((v) => !v.endsWith("/"), "EXPO_PUBLIC_API_URL must not end with a slash");

const schema = z.discriminatedUnion("EXPO_PUBLIC_API_MODE", [
  z.object({
    EXPO_PUBLIC_API_MODE: z.literal("live"),
    EXPO_PUBLIC_API_URL: apiUrl,
    EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY: clerkKey,
  }),
  z.object({
    EXPO_PUBLIC_API_MODE: z.literal("mock"),
    EXPO_PUBLIC_API_URL: apiUrl.optional(),
    EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY: clerkKey.optional(),
  }),
]);

export type Env = {
  apiMode: "live" | "mock";
  apiUrl: string;
  clerkPublishableKey: string;
  /** Mock mode without a Clerk key signs in a local demo user. Live mode never does. */
  demoAuth: boolean;
};

/** Pure, so it can be tested: raw values in, a checked config or a readable error out. */
export function parseEnv(raw: Record<string, string | undefined>): Env {
  const blank = (v: string | undefined) => (v && v.trim() !== "" ? v.trim() : undefined);
  const result = schema.safeParse({
    EXPO_PUBLIC_API_MODE: blank(raw.EXPO_PUBLIC_API_MODE),
    EXPO_PUBLIC_API_URL: blank(raw.EXPO_PUBLIC_API_URL),
    EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY: blank(raw.EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY),
  });
  if (!result.success) {
    const problems = result.error.issues.map((i) =>
      i.path[0] === "EXPO_PUBLIC_API_MODE" ? "EXPO_PUBLIC_API_MODE must be live or mock" : i.message,
    );
    throw new Error(`The app's configuration is incomplete (app/.env, docs/frontend 17):\n- ${problems.join("\n- ")}`);
  }
  const d = result.data;
  return {
    apiMode: d.EXPO_PUBLIC_API_MODE,
    apiUrl: d.EXPO_PUBLIC_API_URL ?? "http://mock.pdm",
    clerkPublishableKey: d.EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY ?? "",
    demoAuth: d.EXPO_PUBLIC_API_MODE === "mock" && !d.EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY,
  };
}

function load(): { env: Env; problem: string | null } {
  try {
    // Expo inlines EXPO_PUBLIC_* only when each is read by its full name.
    const parsed = parseEnv({
      EXPO_PUBLIC_API_MODE: process.env.EXPO_PUBLIC_API_MODE,
      EXPO_PUBLIC_API_URL: process.env.EXPO_PUBLIC_API_URL,
      EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY: process.env.EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY,
    });
    return { env: parsed, problem: null };
  } catch (e) {
    // The app shows this instead of starting (features/shell), so a bad build is never a blank page.
    return { env: { apiMode: "mock", apiUrl: "http://mock.pdm", clerkPublishableKey: "", demoAuth: false }, problem: (e as Error).message };
  }
}

const loaded = load();
export const env: Env = loaded.env;
export const envProblem: string | null = loaded.problem;
