// Exponential backoff: 1, 2, 4 … capped at 30 s (docs/frontend/README.md §9 retries, §10 reconnects).
export interface BackoffOptions {
  baseMs?: number;
  maxMs?: number;
}

/** The wait before retry number `attempt` (0 for the first retry), in milliseconds. */
export function backoffMs(attempt: number, { baseMs = 1_000, maxMs = 30_000 }: BackoffOptions = {}): number {
  const n = Math.max(0, Math.floor(attempt));
  return Math.min(maxMs, baseMs * 2 ** Math.min(n, 30));
}
