import { fetch as expoFetch } from "expo/fetch";

import type { Transport } from "./transport";

// Native builds: expo/fetch, whose response body streams, which the scan stream needs (README §9, §10).
// It accepts a Request object, including whatwg-fetch's, and returns a standard Response.
export const liveTransport: Transport = {
  fetch: (input, init) => expoFetch(input, init ?? undefined) as Promise<Response>,
};
