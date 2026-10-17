/** Exponential backoff: 1, 2, 4 … capped at maxS seconds (docs/frontend 10, connection rules). */
export function backoffSeconds(attempt: number, maxS = 30): number {
  return Math.min(maxS, 2 ** Math.max(0, attempt));
}

export function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) return reject(signal.reason);
    const timer = setTimeout(resolve, ms);
    signal?.addEventListener(
      "abort",
      () => {
        clearTimeout(timer);
        reject(signal.reason);
      },
      { once: true },
    );
  });
}
