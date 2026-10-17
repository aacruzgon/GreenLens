import { createSseParser } from "./sse";

const stream = "id: 1\nevent: status_changed\ndata: {\"a\":1}\n\n: keepalive\n\nid: 2\r\nevent: finding\r\ndata: line1\r\ndata: line2\r\n\r\n";

function parseInChunks(text: string, size: number) {
  const p = createSseParser();
  const out = [];
  for (let i = 0; i < text.length; i += size) out.push(...p.push(text.slice(i, i + size)));
  return out;
}

describe("createSseParser", () => {
  const expected = [
    { id: "1", event: "status_changed", data: '{"a":1}' },
    { id: "2", event: "finding", data: "line1\nline2" },
  ];

  it("parses whole input, skipping the keepalive comment", () => {
    expect(parseInChunks(stream, stream.length)).toEqual(expected);
  });

  it("gives the same events at every chunk boundary", () => {
    for (let size = 1; size <= stream.length; size++) {
      expect(parseInChunks(stream, size)).toEqual(expected);
    }
  });

  it("handles a lone \\r line ending split from its \\n", () => {
    expect(parseInChunks("data: x\r", 100).length).toBe(0);
    const p = createSseParser();
    expect([...p.push("data: x\r"), ...p.push("\n\r\n")]).toEqual([{ id: undefined, event: "message", data: "x" }]);
  });
});
