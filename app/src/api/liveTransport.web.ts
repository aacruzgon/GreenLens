import type { Transport } from "./transport";

// The web build: the browser's own fetch, which streams response bodies (README §9).
export const liveTransport: Transport = {
  fetch: (input, init) => window.fetch(input, init),
};
