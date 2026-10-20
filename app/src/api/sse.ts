// The SSE wire format (WHATWG HTML, server-sent events), as docs/frontend/README.md §10 draws it:
// bytes → TextDecoder (stream mode) → lines → fields → a message at each blank line. Pure: no I/O.

export interface SseMessage {
  /** The last `id:` seen, which persists across messages as the spec says; undefined before any. */
  id: string | undefined;
  /** `event:`, "message" when absent. */
  event: string;
  /** The `data:` lines joined with "\n". */
  data: string;
}

export class SseParser {
  private readonly decoder = new TextDecoder("utf-8");
  private buffer = "";
  // A chunk that ends in "\r" may be the first half of "\r\n".
  private pendingCr = false;
  private lastId: string | undefined;
  private event = "";
  private data: string[] = [];
  private hasData = false;

  /** Feeds a chunk; returns the messages it completed. A chunk may end anywhere, mid-character too. */
  push(chunk: Uint8Array | string): SseMessage[] {
    const text = typeof chunk === "string" ? chunk : this.decoder.decode(chunk, { stream: true });
    return this.consume(text);
  }

  /** Ends the stream. A message without its closing blank line is dropped, as the spec says. */
  end(): SseMessage[] {
    const out = this.consume(this.decoder.decode());
    this.buffer = "";
    this.pendingCr = false;
    this.resetMessage();
    return out;
  }

  private consume(text: string): SseMessage[] {
    const out: SseMessage[] = [];
    let s = text;
    if (this.pendingCr && s.length > 0) {
      this.pendingCr = false;
      if (s.startsWith("\n")) s = s.slice(1);
    }
    this.buffer += s;
    let start = 0;
    for (let i = 0; i < this.buffer.length; i++) {
      const c = this.buffer[i];
      if (c !== "\n" && c !== "\r") continue;
      const line = this.buffer.slice(start, i);
      if (c === "\r") {
        if (i + 1 === this.buffer.length) this.pendingCr = true;
        else if (this.buffer[i + 1] === "\n") i++;
      }
      start = i + 1;
      const msg = this.line(line);
      if (msg) out.push(msg);
    }
    this.buffer = this.buffer.slice(start);
    return out;
  }

  private line(line: string): SseMessage | undefined {
    if (line === "") return this.dispatch();
    if (line.startsWith(":")) return undefined; // a comment: the 15-second keepalive (contracts §4)
    const colon = line.indexOf(":");
    const field = colon === -1 ? line : line.slice(0, colon);
    let value = colon === -1 ? "" : line.slice(colon + 1);
    if (value.startsWith(" ")) value = value.slice(1);
    switch (field) {
      case "data":
        this.data.push(value);
        this.hasData = true;
        break;
      case "event":
        this.event = value;
        break;
      case "id":
        if (!value.includes("\0")) this.lastId = value;
        break;
      default:
        break; // "retry" and unknown fields are ignored: reconnects follow README §10
    }
    return undefined;
  }

  private dispatch(): SseMessage | undefined {
    const msg = this.hasData ? { id: this.lastId, event: this.event || "message", data: this.data.join("\n") } : undefined;
    this.resetMessage();
    return msg;
  }

  private resetMessage(): void {
    this.event = "";
    this.data = [];
    this.hasData = false;
  }
}

export interface ReadSseOptions {
  /** Aborting closes the connection and ends the iteration. */
  signal?: AbortSignal;
  /**
   * Called for every chunk of bytes that arrives, keepalive comments included, so the caller can
   * tell a waiting stream from a dead one (README §10: no bytes for 45 s → reconnect).
   */
  onActivity?: () => void;
}

/** Reads a byte stream into SSE messages. Stops when the stream ends or `signal` aborts. */
export async function* readSse(body: ReadableStream<Uint8Array>, opts: ReadSseOptions = {}): AsyncGenerator<SseMessage> {
  const { signal, onActivity } = opts;
  const reader = body.getReader();
  const parser = new SseParser();
  const onAbort = () => {
    reader.cancel().catch(() => undefined);
  };
  signal?.addEventListener("abort", onAbort);
  let finished = false;
  try {
    while (!signal?.aborted) {
      const { done, value } = await reader.read();
      if (signal?.aborted) return;
      if (done) {
        finished = true;
        yield* parser.end();
        return;
      }
      onActivity?.();
      yield* parser.push(value);
    }
  } finally {
    signal?.removeEventListener("abort", onAbort);
    // The consumer stopped early (break, abort): close the connection.
    if (!finished) await reader.cancel().catch(() => undefined);
  }
}
