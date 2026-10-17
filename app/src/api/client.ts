import createClient, { type Middleware } from "openapi-fetch";

import type { paths } from "./generated/schema";
import { getToken, noteAuthenticated, noteUnauthenticated } from "./auth";
import { ApiError, networkError, toApiError } from "./errors";
import { transport } from "./transport";

// docs/frontend 9, request pipeline: auth → (idempotency per call) → errors → transport.

let baseUrl = "http://localhost:8080";

export function setBaseUrl(url: string) {
  baseUrl = url;
  client = build();
}

export function apiUrl(path: string) {
  return `${baseUrl}${path}`;
}

const auth: Middleware = {
  async onRequest({ request }) {
    const token = await getToken();
    if (token) request.headers.set("Authorization", `Bearer ${token}`);
    return request;
  },
};

function build() {
  const c = createClient<paths>({ baseUrl, fetch: (req) => transport().fetch(req) });
  c.use(auth);
  return c;
}

export let client = build();

export async function authHeaders(extra: Record<string, string> = {}): Promise<Record<string, string>> {
  const token = await getToken();
  return token ? { ...extra, Authorization: `Bearer ${token}` } : extra;
}

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
  if (response.ok) {
    noteAuthenticated();
    return data as T;
  }
  if (response.status === 401) noteUnauthenticated();
  throw toApiError(response.status, error);
}

export function idempotent(key: string) {
  return { "Idempotency-Key": key };
}
