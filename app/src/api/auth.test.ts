import { authedFetch, resetAuthForTests, setOnSignedOut, setTokenProvider } from "./auth";
import { client, ok, setTransport } from "./client";
import { ApiError } from "./errors";

const UNAUTH = {
  error: { code: "unauthenticated", message: "Sign in again.", retryable: false, request_id: "req_1" },
};

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });
}

/** A deferred promise, to hold responses until the test releases them. */
function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => (resolve = r));
  return { promise, resolve };
}

/** A token provider: "old" from the cache, "new-N" for each forced refresh. */
function tokens() {
  let refreshes = 0;
  const fn = jest.fn(async (opts?: { skipCache?: boolean }) => {
    if (opts?.skipCache) refreshes += 1;
    return refreshes === 0 ? "old" : `new-${refreshes}`;
  });
  return { fn, refreshCount: () => fn.mock.calls.filter(([o]) => o?.skipCache).length };
}

const ME = { id: "u1", display_name: "Ada" };

beforeEach(() => {
  resetAuthForTests();
});

describe("auth", () => {
  it("refreshes once for three parallel 401s and retries all three", async () => {
    const t = tokens();
    setTokenProvider(t.fn);
    const signedOut = jest.fn();
    setOnSignedOut(signedOut);

    const firstWave = [deferred<void>(), deferred<void>(), deferred<void>()];
    let oldCalls = 0;
    setTransport({
      fetch: async (input) => {
        const req = input as Request;
        if (req.headers.get("Authorization") === "Bearer old") {
          await firstWave[oldCalls++]!.promise;
          return json(401, UNAUTH);
        }
        return json(200, ME);
      },
    });

    const calls = [0, 1, 2].map(() => client().GET("/me").then(ok));
    // Let all three reach the transport, then release their 401s together.
    await new Promise((r) => setTimeout(r, 0));
    expect(oldCalls).toBe(3);
    firstWave.forEach((d) => d.resolve());

    await expect(Promise.all(calls)).resolves.toEqual([ME, ME, ME]);
    expect(t.refreshCount()).toBe(1);
    expect(signedOut).not.toHaveBeenCalled();
  });

  it("shares a refresh that finished before the other 401s arrived", async () => {
    const t = tokens();
    setTokenProvider(t.fn);
    const late = deferred<void>();
    let oldCalls = 0;
    const send = jest.fn(async (req: Request) => {
      if (req.headers.get("Authorization") === "Bearer old") {
        oldCalls += 1;
        if (oldCalls === 2) await late.promise; // the second 401 arrives after the first refresh is done
        return json(401, UNAUTH);
      }
      return json(200, ME);
    });

    const a = authedFetch(new Request("http://localhost/me"), send);
    const b = authedFetch(new Request("http://localhost/me"), send);
    const ra = await a;
    expect(ra.status).toBe(200);
    expect(t.refreshCount()).toBe(1);

    late.resolve();
    const rb = await b;
    expect(rb.status).toBe(200);
    expect(t.refreshCount()).toBe(1);
    expect(send.mock.calls.at(-1)![0].headers.get("Authorization")).toBe("Bearer new-1");
  });

  it("signs out when the retry gets a 401 too", async () => {
    setTokenProvider(tokens().fn);
    const signedOut = jest.fn();
    setOnSignedOut(signedOut);
    setTransport({ fetch: async () => json(401, UNAUTH) });

    const err = await client().GET("/me").catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect((err as ApiError).code).toBe("unauthenticated");
    expect(signedOut).toHaveBeenCalledTimes(1);
  });

  it("resends the same body on the retry", async () => {
    setTokenProvider(tokens().fn);
    const bodies: string[] = [];
    const send = async (req: Request) => {
      bodies.push(await req.text());
      return bodies.length === 1 ? json(401, UNAUTH) : json(200, ME);
    };
    const res = await authedFetch(new Request("http://localhost/me", { method: "PATCH", body: '{"display_name":"Ada"}' }), send);
    expect(res.status).toBe(200);
    expect(bodies).toEqual(['{"display_name":"Ada"}', '{"display_name":"Ada"}']);
  });
});
