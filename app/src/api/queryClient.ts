import { QueryClient } from "@tanstack/react-query";

import { retryDelay, shouldRetry } from "./errors";

/** Stale times (README §9). Signed image URLs expire after 15 minutes (contracts §1). */
export const STALE = {
  /** Records with signed URLs: scans, plants, timeline, species. Under the 15-minute expiry. */
  signedUrls: 10 * 60_000,
  weather: 30 * 60_000,
} as const;

/** The app's query client: README §9's retry policy for every query and mutation. */
export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: shouldRetry, retryDelay },
      // Every create call carries an Idempotency-Key and the others are idempotent by contract,
      // so a retry is safe; a retry always sends the same key and bytes (contracts §1).
      mutations: { retry: shouldRetry, retryDelay },
    },
  });
}

export const queryClient = createQueryClient();
