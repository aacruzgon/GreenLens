import { z } from "zod";

// docs/frontend 17. Validated once at start-up so a missing value fails loudly, not mid-flow.
const schema = z.object({
  EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY: z.string().optional().default(""),
  EXPO_PUBLIC_API_URL: z
    .string()
    .url()
    .refine((v) => !v.endsWith("/"), "No trailing slash")
    .optional()
    .default("http://localhost:8080"),
  EXPO_PUBLIC_API_MODE: z.enum(["live", "mock"]).optional().default("mock"),
});

// Expo inlines EXPO_PUBLIC_* only when each is read by its full name.
const parsed = schema.parse({
  EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY: process.env.EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY || undefined,
  EXPO_PUBLIC_API_URL: process.env.EXPO_PUBLIC_API_URL || undefined,
  EXPO_PUBLIC_API_MODE: process.env.EXPO_PUBLIC_API_MODE || undefined,
});

export const env = {
  clerkPublishableKey: parsed.EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY,
  apiUrl: parsed.EXPO_PUBLIC_API_URL,
  apiMode: parsed.EXPO_PUBLIC_API_MODE,
  /** Without a Clerk key the app signs in a local demo user; only allowed in mock mode. */
  get demoAuth() {
    return this.apiMode === "mock" && this.clerkPublishableKey === "";
  },
} as const;
