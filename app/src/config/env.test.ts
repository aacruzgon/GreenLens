import { parseEnv } from "./env";

const KEY = "pk_test_Y2xlcmsuZXhhbXBsZS5jb20k";

describe("parseEnv", () => {
  it("requires API_MODE", () => {
    expect(() => parseEnv({})).toThrow(/EXPO_PUBLIC_API_MODE must be live or mock/);
  });

  it("uses the demo sign-in in mock mode with an empty key", () => {
    expect(parseEnv({ EXPO_PUBLIC_API_MODE: "mock", EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY: "" }).demoAuth).toBe(true);
  });

  it("refuses the .env.example placeholder and malformed keys", () => {
    expect(() => parseEnv({ EXPO_PUBLIC_API_MODE: "mock", EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY: "pk_test_replace_me" })).toThrow(/placeholder/);
    expect(() => parseEnv({ EXPO_PUBLIC_API_MODE: "mock", EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY: "sk_test_abc" })).toThrow(/publishable key/);
  });

  it("requires a URL and a key in live mode, and never uses the demo sign-in there", () => {
    expect(() => parseEnv({ EXPO_PUBLIC_API_MODE: "live" })).toThrow(/EXPO_PUBLIC_API_URL|publishable/);
    expect(() => parseEnv({ EXPO_PUBLIC_API_MODE: "live", EXPO_PUBLIC_API_URL: "https://api.example.org/", EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY: KEY })).toThrow(/slash/);
    const e = parseEnv({ EXPO_PUBLIC_API_MODE: "live", EXPO_PUBLIC_API_URL: "https://api.example.org", EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY: KEY });
    expect(e).toEqual({ apiMode: "live", apiUrl: "https://api.example.org", clerkPublishableKey: KEY, demoAuth: false });
  });
});
