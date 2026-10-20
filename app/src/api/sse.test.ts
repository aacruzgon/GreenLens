import { readSse, SseParser, type SseMessage } from "./sse";

// A sample of contracts §4: two events, a keepalive between them, a two-line data field,
// CRLF and lone-CR line endings, and a multi-byte character (é, 🌱) to split mid-character.
const SAMPLE =
  "id: 17\r\n" +
  "event: finding\r\n" +
  'data: {"v":1,"seq":17,"type":"finding","note":"tâche 🌱"}\r\n' +
  "\r\n" +
  ": keepalive\n" +
  "\n" +
  "id: 18\r" +
  "event: diagnosis\r" +
  'data: {"v":1,\n' +
  'data: "seq":18}\n' +
  "\n";

const EXPECTED: SseMessage[] = [
  { id: "17", event: "finding", data: '{"v":1,"seq":17,"type":"finding","note":"tâche 🌱"}' },
  { id: "18", event: "diagnosis", data: '{"v":1,\n"seq":18}' },
];

const bytes = new TextEncoder().encode(SAMPLE);

function parseChunks(chunks: Uint8Array[]): SseMessage[] {
  const p = new SseParser();
  const out = chunks.flatMap((c) => p.push(c));
  return [...out, ...p.end()];
}

describe("SseParser", () => {
  it("parses the sample in one chunk", () => {
    expect(parseChunks([bytes])).toEqual(EXPECTED);
  });

  it("parses the sample split at every byte boundary", () => {
    for (let i = 0; i <= bytes.length; i++) {
      expect(parseChunks([bytes.slice(0, i), bytes.slice(i)])).toEqual(EXPECTED);
    }
  });

  it("parses the sample split at every pair of byte boundaries", () => {
    for (let i = 0; i <= bytes.length; i++) {
      for (let j = i; j <= bytes.length; j++) {
        expect(parseChunks([bytes.slice(0, i), bytes.slice(i, j), bytes.slice(j)])).toEqual(EXPECTED);
      }
    }
  });

  it("parses the sample one byte at a time", () => {
    expect(parseChunks(Array.from(bytes, (b) => Uint8Array.of(b)))).toEqual(EXPECTED);
  });

  it("ignores comments, unknown fields and a message without data", () => {
    const p = new SseParser();
    expect(p.push(": keepalive\n\nretry: 1000\nfoo: bar\nevent: x\n\n")).toEqual([]);
  });

  it("drops a message cut off before its blank line", () => {
    const p = new SseParser();
    expect(p.push("id: 1\ndata: half")).toEqual([]);
    expect(p.end()).toEqual([]);
  });

  it("keeps the last id across messages and defaults the event name", () => {
    const p = new SseParser();
    expect(p.push("id: 5\ndata: a\n\ndata: b\n\n")).toEqual([
      { id: "5", event: "message", data: "a" },
      { id: "5", event: "message", data: "b" },
    ]);
  });

  it("takes a value with no space after the colon, and a field with no colon", () => {
    const p = new SseParser();
    expect(p.push("data:x\ndata\n\n")).toEqual([{ id: undefined, event: "message", data: "x\n" }]);
  });
});

describe("readSse", () => {
  function streamOf(chunks: Uint8Array[]): ReadableStream<Uint8Array> {
    let i = 0;
    return {
      getReader: () => ({
        read: async () => (i < chunks.length ? { done: false, value: chunks[i++] } : { done: true, value: undefined }),
        cancel: async () => undefined,
        releaseLock: () => undefined,
      }),
    } as unknown as ReadableStream<Uint8Array>;
  }

  it("yields the messages of a chunked body", async () => {
    const out: SseMessage[] = [];
    for await (const m of readSse(streamOf([bytes.slice(0, 7), bytes.slice(7, 60), bytes.slice(60)]))) out.push(m);
    expect(out).toEqual(EXPECTED);
  });
});

describe("readSse onActivity", () => {
  it("fires for keepalive-only chunks and yields no event", async () => {
    const enc = new TextEncoder();
    const body = new ReadableStream<Uint8Array>({
      start(c) {
        c.enqueue(enc.encode(": keepalive\n\n"));
        c.enqueue(enc.encode(": keepalive\n\n"));
        c.close();
      },
    });
    const onActivity = jest.fn();
    const out: SseMessage[] = [];
    for await (const m of readSse(body, { onActivity })) out.push(m);
    expect(out).toEqual([]);
    expect(onActivity).toHaveBeenCalledTimes(2);
  });
});
