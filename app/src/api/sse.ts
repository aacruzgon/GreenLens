// docs/frontend 10: an SSE parser over a byte stream. Pure and chunk-boundary safe: handles id, event,
// multi-line data, ":" comments (the keepalive), \r\n and \r line endings, and events split across chunks.

export interface SseMessage {
  id?: string;
  event: string;
  data: string;
}

export function createSseParser() {
  let buffer = "";
  let id: string | undefined;
  let event = "";
  let data: string[] = [];
  let pendingCR = false;

  function line(l: string, out: SseMessage[]) {
    if (l === "") {
      if (data.length > 0) out.push({ id, event: event || "message", data: data.join("\n") });
      event = "";
      data = [];
      return;
    }
    if (l.startsWith(":")) return;
    const colon = l.indexOf(":");
    const field = colon === -1 ? l : l.slice(0, colon);
    let value = colon === -1 ? "" : l.slice(colon + 1);
    if (value.startsWith(" ")) value = value.slice(1);
    if (field === "data") data.push(value);
    else if (field === "event") event = value;
    else if (field === "id" && !value.includes("\0")) id = value;
  }

  return {
    /** Feed decoded text; returns the events completed by it. */
    push(text: string): SseMessage[] {
      const out: SseMessage[] = [];
      if (pendingCR && text.startsWith("\n")) text = text.slice(1);
      pendingCR = false;
      buffer += text;
      let start = 0;
      for (let i = 0; i < buffer.length; i++) {
        const c = buffer[i];
        if (c === "\n" || c === "\r") {
          line(buffer.slice(start, i), out);
          if (c === "\r") {
            if (i + 1 < buffer.length) {
              if (buffer[i + 1] === "\n") i++;
            } else {
              pendingCR = true;
            }
          }
          start = i + 1;
        }
      }
      buffer = buffer.slice(start);
      return out;
    },
  };
}

/** Reads a streaming response body into SSE messages until it ends, it goes quiet, or the signal aborts. */
export async function readSse(
  response: Response,
  onMessage: (m: SseMessage) => void,
  { signal, idleMs = 45_000 }: { signal?: AbortSignal; idleMs?: number } = {},
): Promise<"closed" | "idle"> {
  const body = response.body;
  if (!body) {
    // No stream support: parse the whole body at once.
    const parser = createSseParser();
    parser.push(await response.text()).forEach(onMessage);
    parser.push("\n\n").forEach(onMessage);
    return "closed";
  }
  const reader = body.getReader();
  const decoder = new TextDecoder();
  const parser = createSseParser();
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  let wentIdle = false;
  const armIdle = () => {
    clearTimeout(idleTimer);
    idleTimer = setTimeout(() => {
      wentIdle = true;
      void reader.cancel();
    }, idleMs);
  };
  const onAbort = () => void reader.cancel();
  signal?.addEventListener("abort", onAbort, { once: true });
  try {
    armIdle();
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      armIdle();
      parser.push(decoder.decode(value, { stream: true })).forEach(onMessage);
    }
    parser.push(decoder.decode()).forEach(onMessage);
  } finally {
    clearTimeout(idleTimer);
    signal?.removeEventListener("abort", onAbort);
  }
  return wentIdle ? "idle" : "closed";
}
