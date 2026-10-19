import createClient, { type Middleware } from "openapi-fetch";

import { authedFetch } from "./auth";
import { apiErrorFromResponse } from "./errors";
import type { paths } from "./generated/schema";
import type { Transport } from "./transport";

// The request pipeline of docs/frontend/README.md §9:
//   openapi-fetch client → auth (Bearer, one refresh on 401) → Idempotency-Key (per call,
//   from the resource function's header param) → error middleware (non-2xx → ApiError) → Transport.

/** Mock mode has no backend; the mock transport matches on the path, so any origin works. */
export const DEFAULT_BASE_URL = "http://localhost";

let transport: Transport | null = null;

/** The shell sets the live or mock transport once at start-up (README §7, §15). */
export function setTransport(t: Transport): void {
  transport = t;
}

export function getTransport(): Transport {
  if (!transport) throw new Error("api: no transport set; call setTransport() at start-up");
  return transport;
}

/** fetch for openapi-fetch and the scan stream: the token, the 401 rule, then the transport. */
export function apiFetch(request: Request): Promise<Response> {
  return authedFetch(request, (r) => getTransport().fetch(r));
}

const errorMiddleware: Middleware = {
  async onResponse({ response }) {
    if (!response.ok) throw await apiErrorFromResponse(response);
    return undefined;
  },
};

function makeClient(baseUrl: string) {
  const c = createClient<paths>({ baseUrl, fetch: apiFetch });
  c.use(errorMiddleware);
  return c;
}

export type ApiClient = ReturnType<typeof makeClient>;

let current: ApiClient = makeClient(DEFAULT_BASE_URL);
let currentBaseUrl = DEFAULT_BASE_URL;

/** The API's origin, from EXPO_PUBLIC_API_URL in live mode (README §17). */
export function setBaseUrl(url: string): void {
  currentBaseUrl = url.replace(/\/+$/, "");
  current = makeClient(currentBaseUrl);
}

export function getBaseUrl(): string {
  return currentBaseUrl;
}

/** The typed client. Errors arrive as thrown ApiErrors, so `data` is set whenever a call resolves. */
export function client(): ApiClient {
  return current;
}

/** The `data` of a resolved call. The error middleware has already thrown for every non-2xx. */
export function ok<T>(result: { data?: T }): T {
  return result.data as T;
}
