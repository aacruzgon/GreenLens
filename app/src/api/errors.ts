import type { ErrorCode } from "@/domain/types";

// docs/frontend 9, error handling: one place maps every code of contracts 2 to its behaviour.

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: ErrorCode,
    message: string,
    readonly retryable: boolean,
    readonly requestId?: string,
    readonly retryAfterS?: number,
    readonly details?: Record<string, unknown>,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

/** Anything that isn't an ErrorBody (a proxy's 502, a dropped connection) is internal and retryable. */
export function toApiError(status: number, body: unknown): ApiError {
  const e = (body as { error?: Record<string, unknown> } | null)?.error;
  if (e && typeof e.code === "string" && typeof e.message === "string") {
    return new ApiError(
      status,
      e.code as ErrorCode,
      e.message,
      Boolean(e.retryable),
      typeof e.request_id === "string" ? e.request_id : undefined,
      typeof e.retry_after_s === "number" ? e.retry_after_s : undefined,
      (e.details as Record<string, unknown> | undefined) ?? undefined,
    );
  }
  return new ApiError(status, "internal", "Something went wrong on our side. Try again.", true);
}

export function networkError(): ApiError {
  return new ApiError(0, "upstream_unavailable", "You're offline or the server can't be reached.", true);
}

export function isApiError(e: unknown): e is ApiError {
  return e instanceof ApiError;
}

const RETRY_LIMIT = 3;

/** TanStack retry policy: 409 in progress, 429 rate limited and retryable 5xx, at most 3 times; never budget_exceeded. */
export function shouldRetry(failureCount: number, error: unknown): boolean {
  if (failureCount >= RETRY_LIMIT) return false;
  if (!isApiError(error)) return false;
  if (error.code === "unauthenticated") return failureCount < 1;
  if (error.code === "budget_exceeded") return false;
  if (error.code === "request_in_progress" || error.code === "rate_limited") return true;
  if (error.status === 0) return true;
  return error.status >= 500 && error.retryable;
}

export function retryDelayMs(failureCount: number, error: unknown): number {
  if (isApiError(error) && error.retryAfterS !== undefined) return error.retryAfterS * 1000;
  return Math.min(30_000, 1000 * 2 ** failureCount);
}

/** What a screen says for an error: a title, what to do, and whether to show the request ID. */
export function describeError(e: unknown): { title: string; message: string; requestId?: string } {
  if (!isApiError(e)) return { title: "Something went wrong", message: "Try again in a moment." };
  switch (e.code) {
    case "not_found":
      return { title: "We can't find this", message: "It may have been deleted, or you're no longer a member of its garden." };
    case "forbidden":
      return { title: "You can't do that here", message: "Ask a garden owner to change your role." };
    case "budget_exceeded":
      return { title: "Daily limit reached", message: e.message };
    case "rate_limited":
      return { title: "Slow down a little", message: e.message };
    case "internal":
      return { title: "Something went wrong", message: e.message, requestId: e.requestId };
    default:
      return { title: e.status === 0 ? "You're offline" : "That didn't work", message: e.message, requestId: e.requestId };
  }
}
