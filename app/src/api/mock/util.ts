// Small helpers shared by the mock transport, store, fixtures and runs.

import { TZDate } from "@date-fns/tz";

import type { components } from "../generated/schema";

export type S = components["schemas"];

/** Relative URLs are resolved against this host; the mock never touches the network. */
export const MOCK_ORIGIN = "https://mock.pdm.local";

/** The mock gardens' `time_zone`: their calendar, which the fixtures' dates follow. */
export const MOCK_TIME_ZONE = "America/Toronto";

const JSON_HEADERS = { "content-type": "application/json" } as const;

export function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: JSON_HEADERS });
}

export function emptyResponse(status = 204): Response {
  return new Response(null, { status });
}

const DEFAULT_MESSAGE: Record<S["ErrorCode"], string> = {
  invalid_request: "The request is malformed.",
  unauthenticated: "Sign in again.",
  forbidden: "Your role in this garden can't do that.",
  invalid_signature: "This link has expired.",
  not_found: "Not found.",
  invalid_state: "That isn't possible in the current state.",
  request_in_progress: "The same request is still running.",
  lease_lost: "The job was handed to another worker.",
  upload_too_large: "That image is too large.",
  unsupported_media_type: "Use a JPEG, PNG, WebP or HEIC photo.",
  image_quality: "That photo is too blurry. Try again.",
  missing_required_shot: "Add a photo of the whole plant first.",
  too_many_images: "That kind of photo is already at its limit.",
  idempotency_conflict: "That request key was already used with different content.",
  rate_limited: "Too many requests. Wait a moment.",
  budget_exceeded: "Today's limit is used up.",
  internal: "Something went wrong on our side.",
  processing_unavailable: "The photo could not be processed. Try again.",
  upstream_unavailable: "A service we depend on is down.",
};

const RETRYABLE: ReadonlySet<S["ErrorCode"]> = new Set([
  "request_in_progress",
  "rate_limited",
  "internal",
  "processing_unavailable",
  "upstream_unavailable",
]);

let requestCounter = 0;

/** An ErrorBody (contracts 2). `details` is passed through untyped: callers match the code's row. */
export function errorBody(
  code: S["ErrorCode"],
  details?: Record<string, unknown>,
  message?: string,
): S["ErrorBody"] {
  requestCounter += 1;
  const error: Record<string, unknown> = {
    code,
    message: message ?? DEFAULT_MESSAGE[code],
    retryable: RETRYABLE.has(code),
    request_id: `mock-req-${requestCounter}`,
  };
  if (details) error.details = details;
  if (code === "rate_limited" || code === "request_in_progress") error.retry_after_s = 2;
  return { error } as unknown as S["ErrorBody"];
}

const STATUS_OF: Record<S["ErrorCode"], number> = {
  invalid_request: 400,
  unauthenticated: 401,
  forbidden: 403,
  invalid_signature: 403,
  not_found: 404,
  invalid_state: 409,
  request_in_progress: 409,
  lease_lost: 409,
  upload_too_large: 413,
  unsupported_media_type: 415,
  image_quality: 422,
  missing_required_shot: 422,
  too_many_images: 422,
  idempotency_conflict: 422,
  rate_limited: 429,
  budget_exceeded: 429,
  internal: 500,
  processing_unavailable: 503,
  upstream_unavailable: 503,
};

export function errorResponse(
  code: S["ErrorCode"],
  details?: Record<string, unknown>,
  message?: string,
): Response {
  return jsonResponse(STATUS_OF[code], errorBody(code, details, message));
}

/** A Timestamp in whole seconds (contracts 1), `offsetMs` from now. */
export function nowTs(offsetMs = 0): string {
  return isoSeconds(new Date(Date.now() + offsetMs));
}

export function isoSeconds(d: Date): string {
  return d.toISOString().replace(/\.\d{3}Z$/, "Z");
}

export function bandFor(confidence: number): S["ConfidenceBand"] {
  if (confidence >= 0.8) return "high";
  if (confidence >= 0.5) return "medium";
  return "low";
}

/** `{ items, next_cursor }` for `?cursor=&limit=` (default 20, max 100). The cursor is the next offset. */
export function paginate<T>(
  items: readonly T[],
  query: URLSearchParams,
): { items: T[]; next_cursor?: string } {
  const rawLimit = Number(query.get("limit") ?? 20);
  const limit = Number.isFinite(rawLimit) ? Math.min(100, Math.max(1, Math.floor(rawLimit))) : 20;
  const offset = Math.max(0, Number(query.get("cursor") ?? 0) || 0);
  const page = items.slice(offset, offset + limit);
  const next = offset + limit;
  return next < items.length ? { items: page, next_cursor: String(next) } : { items: page };
}

const WEEKDAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/**
 * Dates relative to today in the gardens' calendar (`MOCK_TIME_ZONE`), so the demo always looks current (analytics
 * 6) and a task due today is due today wherever the device is.
 */
export class DateKit {
  private readonly y: number;
  private readonly m: number;
  private readonly d: number;

  constructor(
    readonly now: Date = new Date(),
    readonly timeZone: string = MOCK_TIME_ZONE,
  ) {
    const today = new TZDate(now.getTime(), timeZone);
    this.y = today.getFullYear();
    this.m = today.getMonth();
    this.d = today.getDate();
  }

  private at(daysAgo: number, hour = 0, minute = 0): TZDate {
    return new TZDate(this.y, this.m, this.d - daysAgo, hour, minute, this.timeZone);
  }

  /**
   * A UTC timestamp for the garden's day `daysAgo` days back (negative: ahead), at hour:minute there. Today's
   * fixtures sit in the morning, none after noon; before noon their times are squeezed, in order, into the part of
   * today already past, so none is later than now and a scan taken now is the plant's latest.
   */
  ts(daysAgo: number, hour = 9, minute = 0): string {
    const t = this.at(daysAgo, hour, minute).getTime();
    if (daysAgo !== 0) return isoSeconds(new Date(t));
    const midnight = this.at(0).getTime();
    const squeeze = Math.min(1, (this.now.getTime() - midnight) / (this.at(0, 12).getTime() - midnight));
    return isoSeconds(new Date(midnight + (t - midnight) * squeeze));
  }

  /** A `YYYY-MM-DD` date in the garden's calendar. */
  date(daysAgo: number): string {
    const day = this.at(daysAgo);
    const mm = String(day.getMonth() + 1).padStart(2, "0");
    const dd = String(day.getDate()).padStart(2, "0");
    return `${day.getFullYear()}-${mm}-${dd}`;
  }

  /** 0 = Sunday ... 4 = Thursday. */
  weekday(daysAgo: number): number {
    return this.at(daysAgo).getDay();
  }

  /** "Thursday". */
  weekdayName(daysAgo: number): string {
    return WEEKDAYS[this.weekday(daysAgo)]!;
  }
}

/** Plain `Record` lookup that throws, so a typo in a fixture fails loudly. */
export function must<T>(value: T | undefined | null, what: string): T {
  if (value === undefined || value === null) throw new Error(`mock: missing ${what}`);
  return value;
}
