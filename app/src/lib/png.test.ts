import { zlibSync } from "fflate";

import { base64ToBytes, decodePng } from "./png";

// Builds a PNG by hand: the decoder doesn't check CRCs, so they are left at zero.
function chunk(type: string, body: Uint8Array): number[] {
  const n = body.length;
  return [(n >>> 24) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255, ...[...type].map((c) => c.charCodeAt(0)), ...body, 0, 0, 0, 0];
}

function png(width: number, height: number, colour: 2 | 6, scanlines: number[][]): Uint8Array {
  const ihdr = new Uint8Array([0, 0, 0, width, 0, 0, 0, height, 8, colour, 0, 0, 0]);
  const raw = new Uint8Array(scanlines.flat());
  return new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10, ...chunk("IHDR", ihdr), ...chunk("IDAT", zlibSync(raw)), ...chunk("IEND", new Uint8Array())]);
}

describe("decodePng", () => {
  it("reads RGB with the Sub, Up and Paeth filters into RGBA", () => {
    const bytes = png(2, 3, 2, [
      [1, 10, 20, 30, 5, 5, 5], // Sub: second pixel is (15, 25, 35)
      [2, 1, 1, 1, 1, 1, 1], // Up: (11, 21, 31), (16, 26, 36)
      [4, 1, 1, 1, 1, 1, 1], // Paeth
    ]);
    const { width, height, data } = decodePng(bytes);
    expect([width, height]).toEqual([2, 3]);
    expect([...data.subarray(0, 8)]).toEqual([10, 20, 30, 255, 15, 25, 35, 255]);
    expect([...data.subarray(8, 16)]).toEqual([11, 21, 31, 255, 16, 26, 36, 255]);
    expect([...data.subarray(16, 20)]).toEqual([12, 22, 32, 255]);
  });

  it("keeps the alpha of RGBA, with the Average filter", () => {
    const { data } = decodePng(png(1, 2, 6, [[0, 100, 50, 0, 200], [3, 10, 10, 10, 10]]));
    expect([...data]).toEqual([100, 50, 0, 200, 60, 35, 10, 110]);
  });

  it("refuses what isn't a PNG", () => {
    expect(() => decodePng(new Uint8Array([1, 2, 3]))).toThrow("not a PNG");
  });

  it("decodes base64", () => {
    expect([...base64ToBytes("AQID")]).toEqual([1, 2, 3]);
  });
});
