import { client } from "./client";
import type { ScanEvent, ScanEventType } from "@/domain/types";

import { readSse } from "./sse";


const KNOWN_TYPES: ReadonlySet<string> = new Set<ScanEventType>([
  "run_started", "status_changed", "stage_started", "stage_completed", "finding",
  "finding_retracted", "weather_loaded", "candidates_selected", "species_suggested",
  "species_confirmed", "question", "question_resolved", "guided_check_done", "diagnosis",
  "plan_ready", "plan_superseded", "failed",
]);

export interface ScanStreamOptions {
  /** The last applied `seq`. Omit it to replay the scan's whole history (contracts §4, resume). */
  lastEventId?: number;
  /** Aborting closes the connection and ends the iteration. */
  signal?: AbortSignal;
  /** Fired for every chunk of bytes, keepalives too (README §10: 45 s of silence → reconnect). */
  onActivity?: () => void;
}

/** One decoded `data:` line, or undefined for a line the app must ignore. */
export function decodeScanEvent(data: string): ScanEvent | undefined {
  let parsed: unknown;
  try {
    parsed = JSON.parse(data);
  } catch {
    return undefined;
  }
  if (typeof parsed !== "object" || parsed === null) return undefined;
  const e = parsed as { v?: unknown; seq?: unknown; type?: unknown };
  // v must be 1 (contracts §1, versioning); unknown event types are ignored.
  if (e.v !== 1 || typeof e.seq !== "number" || typeof e.type !== "string" || !KNOWN_TYPES.has(e.type)) {
    return undefined;
  }
  return parsed as ScanEvent;
}

/**
 * Opens `GET /scans/{id}/events` and yields its events in order. Errors before the first byte
 * (401 after the refresh, 404) throw an ApiError. The iteration ends when the server closes the
 * stream or `signal` aborts; reconnecting is the caller's job (README §10, connection rules).
 */
export async function* openScanStream(scanId: string, opts: ScanStreamOptions = {}): AsyncGenerator<ScanEvent> {
  const { data } = await client().GET("/scans/{id}/events", {
    params: {
      path: { id: scanId },
      header: opts.lastEventId === undefined ? undefined : { "Last-Event-ID": opts.lastEventId },
    },
    headers: { Accept: "text/event-stream" },
    parseAs: "stream",
    signal: opts.signal,
  });
  const body = data as ReadableStream<Uint8Array> | null | undefined;
  if (!body) return;
  for await (const msg of readSse(body, { signal: opts.signal, onActivity: opts.onActivity })) {
    const event = decodeScanEvent(msg.data);
    if (event) yield event;
  }
}
