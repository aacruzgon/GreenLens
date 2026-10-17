import { transport } from "./transport";

// docs/frontend 8, token flow. The api layer never imports Clerk: the auth feature registers these.
// A 401 is handled per request: refresh the token, send that request again once, and sign out only
// when the retry is rejected too. A request sent before a refresh reuses that refresh instead of
// starting another, so requests rejected together refresh once.

export type TokenProvider = (opts?: { skipCache?: boolean }) => Promise<string | null>;

let provider: TokenProvider | null = null;
let onSignedOut: (() => void) | null = null;
/** Counts refreshes; each request remembers the count it was sent at. */
let generation = 0;
let latest: Promise<string | null> | null = null;

export function setTokenProvider(fn: TokenProvider | null, signOut?: () => void) {
  provider = fn;
  onSignedOut = signOut ?? null;
  generation += 1;
  latest = null;
}

async function token(): Promise<string | null> {
  return provider ? provider() : null;
}

function refreshedToken(sentAt: number): Promise<string | null> {
  if (!provider) return Promise.resolve(null);
  if (sentAt !== generation && latest) return latest;
  generation += 1;
  latest = provider({ skipCache: true }).catch(() => null);
  return latest;
}

function withBearer(headers: Record<string, string>, t: string | null): Record<string, string> {
  return t ? { ...headers, Authorization: `Bearer ${t}` } : headers;
}

/**
 * Sends a request with the session token. `send` is called once, or twice when the first answer is
 * a 401: the second time with a freshly fetched token. A second 401 signs the user out.
 */
export async function sendAuthed(send: (headers: Record<string, string>) => Promise<Response>, headers: Record<string, string> = {}): Promise<Response> {
  const sentAt = generation;
  const first = await send(withBearer(headers, await token()));
  if (first.status !== 401 || !provider) return first;
  const fresh = await refreshedToken(sentAt);
  if (!fresh) {
    onSignedOut?.();
    return first;
  }
  const second = await send(withBearer(headers, fresh));
  if (second.status === 401) onSignedOut?.();
  return second;
}

/** The fetch openapi-fetch uses: a copy of the request for each attempt, since a body can be read once. */
export function authedFetch(request: Request): Promise<Response> {
  const copies = [request.clone(), request];
  return sendAuthed((headers) => {
    const r = copies.shift()!;
    for (const [k, v] of Object.entries(headers)) r.headers.set(k, v);
    return transport().fetch(r);
  });
}
