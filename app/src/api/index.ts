// The api layer's public surface for features (docs/frontend 5).
export { setTransport, type Transport, type UploadFile, type UploadPart } from "./transport";
export { setBaseUrl } from "./client";
export { setTokenProvider, type TokenProvider } from "./auth";
export { ApiError, describeError, isApiError } from "./errors";
export { readSse, createSseParser, type SseMessage } from "./sse";
export { qk } from "./queryKeys";
export { makeQueryClient } from "./queryClient";
export { newKey, useIdempotentMutation } from "./idempotency";
export { me, useMe } from "./resources/me";
export { gardens, useGarden, useGardens, useWeather } from "./resources/gardens";
export { plants, usePlant, usePlants, useTimeline } from "./resources/plants";
export { scans, useScan } from "./resources/scans";
export { tasks, useTasks } from "./resources/tasks";
