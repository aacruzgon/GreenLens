// Bearer tokens and the 401 rule (docs/frontend/README.md §8, token flow).
// The api layer never imports Clerk: AuthBridge registers Clerk's getToken here.

export type TokenProvider = (opts?: { skipCache?: boolean }) => Promise<string | null>;

let provider: TokenProvider | null = null;
let onSignedOut: (() => void) | null = null;
let signedOutSent = false;

// Each completed refresh bumps `generation`. A request remembers the generation its token came
// from, so a 401 for a token older than a finished refresh reuses that refresh's token.
let generation = 0;
let refreshed: string | null = null;
let inFlight: Promise<string | null> | null = null;

// Bumped whenever the provider changes. A refresh started under an older session must not touch
// the state of a newer one when it settles.
let session = 0;

export function setTokenProvider(fn: TokenProvider): void {
  session += 1;
  provider = fn;
  inFlight = null;
  refreshed = null;
  generation = 0;
  signedOutSent = false;
}

export function clearTokenProvider(): void {
  session += 1;
  provider = null;
  inFlight = null;
  refreshed = null;
  generation = 0;
}

export function setOnSignedOut(cb: (() => void) | null): void {
  onSignedOut = cb;
}

function signOut(): void {
  if (signedOutSent) return;
  signedOutSent = true;
  onSignedOut?.();
}

async function currentToken(): Promise<{ token: string | null; gen: number }> {
  // A request that starts during a refresh waits for it rather than sending the stale token.
  if (inFlight) await inFlight.catch(() => null);
  const gen = generation;
  const token = provider ? await provider() : null;
  return { token, gen };
}

/** One refresh for a burst of 401s: joins the one in flight, or reuses one that finished after `gen`. */
function refreshToken(gen: number): Promise<string | null> {
  if (inFlight) return inFlight;
  if (generation > gen) return Promise.resolve(refreshed);
  const p = provider;
  if (!p) return Promise.resolve(null);
  const startedIn = session;
  const refresh: Promise<string | null> = p({ skipCache: true })
    .catch(() => null)
    .then((token) => {
      // Signed out or in again meanwhile: the new session's state is not ours to change.
      if (startedIn !== session) return null;
      refreshed = token;
      generation += 1;
      inFlight = null;
      return token;
    });
  inFlight = refresh;
  return refresh;
}

function withToken(request: Request, token: string | null): Request {
  if (token) request.headers.set("Authorization", `Bearer ${token}`);
  else request.headers.delete("Authorization");
  return request;
}

/**
 * Sends `request` with the bearer token. On a 401, refreshes the token once (skipCache) and
 * retries once; a second 401, or no token after the refresh, signs the user out.
 * Every call goes through here: JSON requests, multipart uploads and the scan stream.
 */
export async function authedFetch(request: Request, send: (r: Request) => Promise<Response>): Promise<Response> {
  // The body can be read only once, so keep a copy for the retry before the first send.
  const retry = request.clone();
  const { token, gen } = await currentToken();
  const first = await send(withToken(request, token));
  if (first.status !== 401) return first;

  const fresh = await refreshToken(gen);
  if (!fresh) {
    signOut();
    return first;
  }
  const second = await send(withToken(retry, fresh));
  if (second.status === 401) signOut();
  return second;
}

/** Test hook: forget every refresh and callback. */
export function resetAuthForTests(): void {
  provider = null;
  onSignedOut = null;
  signedOutSent = false;
  session += 1;
  generation = 0;
  refreshed = null;
  inFlight = null;
}
