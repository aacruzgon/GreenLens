import { setTokenProvider } from "./auth";
import { call, client, setBaseUrl } from "./client";
import { setTransport, type Transport } from "./transport";

// README 16, API tests: token refresh on 401, with requests in parallel.

function fakeApi(validToken: () => string) {
  const seen: (string | null)[] = [];
  const t: Transport = {
    async fetch(input) {
      const req = input as Request;
      const auth = req.headers.get("Authorization");
      seen.push(auth);
      await new Promise((r) => setTimeout(r, 5));
      if (auth !== `Bearer ${validToken()}`) {
        return new Response(JSON.stringify({ error: { code: "unauthenticated", message: "Sign in again.", retryable: false, request_id: "r" } }), { status: 401 });
      }
      return new Response(JSON.stringify({ id: "U1", display_name: "Mark", location_consent: false, created_at: "2026-10-17T00:00:00Z" }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    },
    upload: () => Promise.reject(new Error("unused")),
  };
  setTransport(t);
  setBaseUrl("http://test.pdm");
  return seen;
}

describe("401 handling", () => {
  it("refreshes once for parallel requests, retries each once, and keeps the user signed in", async () => {
    const seen = fakeApi(() => "fresh");
    // Like Clerk's cache: a skipCache fetch replaces the cached token.
    let cached = "stale";
    const provider = jest.fn(async (opts?: { skipCache?: boolean }) => {
      if (opts?.skipCache) {
        await new Promise((r) => setTimeout(r, 20));
        cached = "fresh";
      }
      return cached;
    });
    const signOut = jest.fn();
    setTokenProvider(provider, signOut);

    const results = await Promise.all([call(client.GET("/me")), call(client.GET("/me")), call(client.GET("/me"))]);

    expect(results.map((r) => r.id)).toEqual(["U1", "U1", "U1"]);
    expect(provider.mock.calls.filter(([o]) => o?.skipCache)).toHaveLength(1);
    expect(seen.filter((h) => h === "Bearer stale")).toHaveLength(3);
    expect(seen.filter((h) => h === "Bearer fresh")).toHaveLength(3);
    expect(signOut).not.toHaveBeenCalled();
  });

  it("still refreshes once when the refresh finishes before the other 401s arrive", async () => {
    fakeApi(() => "fresh");
    let cached = "stale";
    const provider = jest.fn(async (opts?: { skipCache?: boolean }) => {
      if (opts?.skipCache) cached = "fresh";
      return cached;
    });
    setTokenProvider(provider, jest.fn());

    await Promise.all([call(client.GET("/me")), call(client.GET("/me"))]);
    expect(provider.mock.calls.filter(([o]) => o?.skipCache)).toHaveLength(1);
  });

  it("signs out when the retry with a fresh token is rejected too", async () => {
    fakeApi(() => "never");
    const signOut = jest.fn();
    setTokenProvider(async () => "stale", signOut);

    await expect(call(client.GET("/me"))).rejects.toMatchObject({ code: "unauthenticated" });
    expect(signOut).toHaveBeenCalledTimes(1);
  });
});
