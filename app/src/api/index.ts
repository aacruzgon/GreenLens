// The api layer's public surface (docs/frontend/README.md §9, §10).

export type { Transport } from "./transport";
export { setTransport, getTransport, setBaseUrl, getBaseUrl, client, apiFetch, DEFAULT_BASE_URL } from "./client";
export { liveTransport } from "./liveTransport";
export { imageSource } from "./images";
export { useActionKey } from "./idempotency";
export { setTokenProvider, clearTokenProvider, setOnSignedOut, type TokenProvider } from "./auth";
export {
  ApiError,
  isApiError,
  toApiError,
  apiErrorFromResponse,
  shouldRetry,
  retryDelay,
  type ErrorCode,
  type ErrorBody,
} from "./errors";
export {
  newKey,
  useIdempotentMutation,
  type IdempotencyKey,
  type Keyed,
  type IdempotentMutationOptions,
  type IdempotentMutationResult,
} from "./idempotency";
export { SseParser, readSse, type SseMessage, type ReadSseOptions } from "./sse";
export { openScanStream, decodeScanEvent, type ScanStreamOptions } from "./scanStream";
export { fetchAllPages, flattenPages, nextCursor, MAX_PAGE_SIZE, type Page } from "./pages";
export { qk, type TaskFilter, type ConversationAnchor } from "./queryKeys";
export { createQueryClient, queryClient, STALE } from "./queryClient";

export * from "./resources/me";
export * from "./resources/gardens";
export * from "./resources/plants";
export * from "./resources/tasks";
export * from "./resources/scans";
export * from "./resources/species";
export * from "./resources/memories";
