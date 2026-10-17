// docs/frontend 8, token flow. The api layer never imports Clerk: AuthBridge registers these.

export type TokenProvider = (opts?: { skipCache?: boolean }) => Promise<string | null>;

let provider: TokenProvider | null = null;
let onSignedOut: (() => void) | null = null;
let refreshNext = false;

export function setTokenProvider(fn: TokenProvider | null, signOut?: () => void) {
  provider = fn;
  onSignedOut = signOut ?? null;
  refreshNext = false;
}

export async function getToken(): Promise<string | null> {
  if (!provider) return null;
  const skipCache = refreshNext;
  refreshNext = false;
  return provider(skipCache ? { skipCache } : undefined);
}

let unauthenticatedStreak = 0;

/** A 401: the first one refreshes the token for the retry; a second in a row signs out. */
export function noteUnauthenticated() {
  unauthenticatedStreak += 1;
  if (unauthenticatedStreak >= 2) {
    unauthenticatedStreak = 0;
    onSignedOut?.();
  } else {
    refreshNext = true;
  }
}

export function noteAuthenticated() {
  unauthenticatedStreak = 0;
}
