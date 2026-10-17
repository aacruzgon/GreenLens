import { QueryClient } from "@tanstack/react-query";

import { retryDelayMs, shouldRetry } from "./errors";

export function makeQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: {
        retry: shouldRetry,
        retryDelay: retryDelayMs,
        // Under the 15-minute life of signed image URLs, so records refetch before their URLs expire.
        staleTime: 60_000,
      },
      mutations: { retry: shouldRetry, retryDelay: retryDelayMs },
    },
  });
}

/** Records that carry signed image URLs (docs/frontend 9, signed image URLs). */
export const SIGNED_STALE_MS = 10 * 60_000;
