import createClient from "openapi-fetch";

import type { paths } from "./generated/schema";
import { authedFetch } from "./auth";
import { ApiError, networkError, toApiError } from "./errors";

// docs/frontend 9, request pipeline: auth (with the 401 refresh) → idempotency per call → errors → transport.

let baseUrl = "http://localhost:8080";

export function setBaseUrl(url: string) {
  baseUrl = url;
  client = build();
}

export function apiUrl(path: string) {
  return `${baseUrl}${path}`;
}

function build() {
  return createClient<paths>({ baseUrl, fetch: authedFetch });
}

export let client = build();

/** Unwraps an openapi-fetch result: data on success, an ApiError otherwise. */
export async function call<T>(p: Promise<{ data?: T; error?: unknown; response: Response }>): Promise<T> {
  let result: { data?: T; error?: unknown; response: Response };
  try {
    result = await p;
  } catch (e) {
    if (e instanceof ApiError) throw e;
    throw networkError();
  }
  return check(result.response, result.data, result.error);
}

export function check<T>(response: Response, data: T | undefined, error: unknown): T {
  if (response.ok) return data as T;
  throw toApiError(response.status, error);
}

export function idempotent(key: string) {
  return { "Idempotency-Key": key };
}
