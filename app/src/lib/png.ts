// A small PNG decoder for the photo tint's sample (a few dozen pixels a side): 8-bit truecolour with or without
// alpha, which is what expo-image-manipulator writes. Pure JS (fflate's inflate), so it runs in Expo Go and on the web.
import { unzlibSync } from "fflate";

export interface Pixels {
  width: number;
  height: number;
  /** RGBA, 4 bytes a pixel, row by row. */
  data: Uint8Array;
}

const SIGNATURE = [137, 80, 78, 71, 13, 10, 26, 10];

function u32(b: Uint8Array, at: number): number {
  return (((b[at] ?? 0) << 24) | ((b[at + 1] ?? 0) << 16) | ((b[at + 2] ?? 0) << 8) | (b[at + 3] ?? 0)) >>> 0;
}

function paeth(a: number, b: number, c: number): number {
  const p = a + b - c;
  const pa = Math.abs(p - a);
  const pb = Math.abs(p - b);
  const pc = Math.abs(p - c);
  if (pa <= pb && pa <= pc) return a;
  return pb <= pc ? b : c;
}

export function decodePng(bytes: Uint8Array): Pixels {
  if (!SIGNATURE.every((v, i) => bytes[i] === v)) throw new Error("not a PNG");
  let width = 0;
  let height = 0;
  let channels = 0;
  const idat: Uint8Array[] = [];
  for (let at = 8; at + 8 <= bytes.length; ) {
    const length = u32(bytes, at);
    const type = String.fromCharCode(...bytes.subarray(at + 4, at + 8));
    const body = bytes.subarray(at + 8, at + 8 + length);
    if (type === "IHDR") {
      width = u32(body, 0);
      height = u32(body, 4);
      const depth = body[8];
      const colour = body[9];
      if (depth !== 8 || (colour !== 2 && colour !== 6) || body[12] !== 0) {
        throw new Error(`unsupported PNG: depth ${depth}, colour type ${colour}, interlace ${body[12]}`);
      }
      channels = colour === 6 ? 4 : 3;
    } else if (type === "IDAT") {
      idat.push(body);
    } else if (type === "IEND") {
      break;
    }
    at += 12 + length;
  }
  if (!width || !height || idat.length === 0) throw new Error("PNG without image data");

  const joined = new Uint8Array(idat.reduce((n, c) => n + c.length, 0));
  idat.reduce((offset, c) => (joined.set(c, offset), offset + c.length), 0);
  const raw = unzlibSync(joined);

  const stride = width * channels;
  const rows = new Uint8Array(height * stride);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const src = y * (stride + 1) + 1;
    const dst = y * stride;
    for (let x = 0; x < stride; x++) {
      const v = raw[src + x] ?? 0;
      const left = x >= channels ? (rows[dst + x - channels] ?? 0) : 0;
      const up = y > 0 ? (rows[dst - stride + x] ?? 0) : 0;
      const upLeft = y > 0 && x >= channels ? (rows[dst - stride + x - channels] ?? 0) : 0;
      let out: number;
      switch (filter) {
        case 0: out = v; break;
        case 1: out = v + left; break;
        case 2: out = v + up; break;
        case 3: out = v + ((left + up) >> 1); break;
        case 4: out = v + paeth(left, up, upLeft); break;
        default: throw new Error(`bad PNG filter ${filter}`);
      }
      rows[dst + x] = out & 255;
    }
  }

  if (channels === 4) return { width, height, data: rows };
  const data = new Uint8Array(width * height * 4);
  for (let i = 0, j = 0; i < rows.length; i += 3, j += 4) {
    data[j] = rows[i] ?? 0;
    data[j + 1] = rows[i + 1] ?? 0;
    data[j + 2] = rows[i + 2] ?? 0;
    data[j + 3] = 255;
  }
  return { width, height, data };
}

/** Base64 (as expo-image-manipulator returns it) to bytes. */
export function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}
