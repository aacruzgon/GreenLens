import { backoffMs } from "@/lib/backoff";

import type { components } from "./generated/schema";

export type ErrorCode = components["schemas"]["ErrorCode"];
export type ErrorBody = components["schemas"]["ErrorBody"];

const ERROR_CODES: readonly ErrorCode[] = [
  "invalid_request", "unauthenticated", "forbidden", "invalid_signature",
  "not_found", "invalid_state", "request_in_progress", "lease_lost",
  "upload_too_large", "unsupported_media_type", "image_quality",
  "missing_required_shot", "too_many_images", "idempotency_conflict",
  "rate_limited", "budget_exceeded", "internal",
  "processing_unavailable", "upstream_unavailable",
];

/** Every non-2xx response, as the app sees it (contracts §2). */
export class ApiError extends Error {
  readonly code: ErrorCode;
  readonly retryable: boolean;
  readonly retryAfterS: number | undefined;
  readonly details: Record<string, unknown> | undefined;
  readonly requestId: string | undefined;
  readonly status: number;

  constructor(init: {
    code: ErrorCode;
    message: string;
    retryable: boolean;
    status: number;
    retryAfterS?: number;
    details?: Record<string, unknown>;
    requestId?: string;
  }) {
    super(init.message);
    this.name = "ApiError";
    this.code = init.code;
    this.retryable = init.retryable;
    this.retryAfterS = init.retryAfterS;
    this.details = init.details;
    this.requestId = init.requestId;
    this.status = init.status;
  }
}

export function isApiError(e: unknown): e is ApiError {
  return e instanceof ApiError;
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

/**
 * Builds an ApiError from a status and a parsed (or raw text) body. A body that isn't an
 * ErrorBody, such as a proxy's 502 page, becomes `internal` with `retryable: true` (README §9).
 */
export function toApiError(status: number, body: unknown): ApiError {
  const err = isRecord(body) && isRecord(body.error) ? body.error : undefined;
  if (
    err &&
    typeof err.code === "string" &&
    (ERROR_CODES as readonly string[]).includes(err.code) &&
    typeof err.message === "string" &&
    typeof err.retryable === "boolean"
  ) {
    return new ApiError({
      code: err.code as ErrorCode,
      message: err.message,
      retryable: err.retryable,
      status,
      retryAfterS: typeof err.retry_after_s === "number" ? err.retry_after_s : undefined,
      details: isRecord(err.details) ? err.details : undefined,
      requestId: typeof err.request_id === "string" ? err.request_id : undefined,
    });
  }
  return new ApiError({
    code: "internal",
    message: `Unexpected response from the server (HTTP ${status}).`,
    retryable: true,
    status,
  });
}

/** Reads a non-2xx Response into an ApiError. */
export async function apiErrorFromResponse(response: Response): Promise<ApiError> {
  let body: unknown;
  try {
    const text = await response.text();
    body = text ? JSON.parse(text) : undefined;
  } catch {
    body = undefined;
  }
  return toApiError(response.status, body);
}

const MAX_RETRIES = 3;

function isAbort(e: unknown): boolean {
  return isRecord(e) && e.name === "AbortError";
}

/**
 * TanStack Query's `retry` (README §9, retries). `failureCount` starts at 0.
 * Retries 409 request_in_progress and 429 rate_limited, retryable 5xx, and network errors
 * (every create call is idempotent, so the same key and bytes are safe to resend), at most 3 times.
 * Never budget_exceeded or any other 4xx.
 */
export function shouldRetry(failureCount: number, error: unknown): boolean {
  if (failureCount >= MAX_RETRIES) return false;
  if (!(error instanceof ApiError)) return !isAbort(error);
  if (error.code === "request_in_progress" || error.code === "rate_limited") return true;
  return error.status >= 500 && error.retryable;
}

/** TanStack Query's `retryDelay`: `retry_after_s` when sent, otherwise backoff from 1 s. */
export function retryDelay(failureCount: number, error: unknown): number {
  if (error instanceof ApiError && error.retryAfterS !== undefined) return error.retryAfterS * 1_000;
  return backoffMs(failureCount);
}
