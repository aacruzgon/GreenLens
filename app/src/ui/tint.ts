// The diagnosis's photo tint (design 3.1, photo tint rule; roadmap 2): the photo's dominant hue at a fixed
// OKLCH lightness, L 0.93 in light and 0.24 in dark, with chroma at most 0.03. Holding L and C in that narrow
// band is what keeps F0's text and status colours readable on any photo (checked in tint.test.ts for every hue).
// D9 put the computation on the phone: platform/imagePixels samples the photo, and this file is the pure part.
import { colors, type ColorScheme } from "./tokens";

type Rgb = readonly [number, number, number];
type Lab = readonly [number, number, number];

const TINT_L: Record<ColorScheme, number> = { light: 0.93, dark: 0.24 };
const TINT_MAX_C = 0.03;
/** Pixels below this OKLab chroma are greys: they have no hue to give. */
const GREY_C = 0.02;
const BINS = 36;

function toLinear(c: number): number {
  const v = c / 255;
  return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
}

function fromLinear(v: number): number {
  const c = v <= 0.0031308 ? v * 12.92 : 1.055 * v ** (1 / 2.4) - 0.055;
  return c * 255;
}

/** sRGB (0–255) to OKLab (Björn Ottosson's matrices). */
export function rgbToOklab([r8, g8, b8]: Rgb): Lab {
  const r = toLinear(r8);
  const g = toLinear(g8);
  const b = toLinear(b8);
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return [
    0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  ];
}

/** OKLab to linear sRGB, unclamped, so a caller can tell when it falls outside the gamut. */
function oklabToLinear([L, a, b]: Lab): Rgb {
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
}

/** OKLCH (hue in degrees) to sRGB 0–255, lowering the chroma until it fits the gamut. */
export function oklchToRgb(L: number, C: number, hueDeg: number): Rgb {
  const h = (hueDeg * Math.PI) / 180;
  for (let c = C; c >= 0; c -= 0.001) {
    const lin = oklabToLinear([L, c * Math.cos(h), c * Math.sin(h)]);
    if (lin.every((v) => v >= -1e-4 && v <= 1 + 1e-4)) {
      return lin.map((v) => Math.round(fromLinear(Math.min(1, Math.max(0, v))))) as unknown as Rgb;
    }
  }
  const grey = Math.round(fromLinear(L ** 3));
  return [grey, grey, grey];
}

export function hexToRgb(hex: string): Rgb {
  const n = parseInt(hex.slice(1, 7), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

export function rgbToHex([r, g, b]: Rgb): string {
  return `#${[r, g, b].map((v) => Math.round(v).toString(16).padStart(2, "0")).join("").toUpperCase()}`;
}

/**
 * The dominant hue of RGBA pixels in degrees, or undefined for a grey photo. Each coloured pixel votes for its
 * 10° bin by its chroma; the winning bin and its two neighbours give the hue as a chroma-weighted mean, so a
 * green leaf with brown spots reads green, not the olive average of the two.
 */
export function dominantHue(rgba: Uint8Array | Uint8ClampedArray): number | undefined {
  const weight = new Float64Array(BINS);
  const sumA = new Float64Array(BINS);
  const sumB = new Float64Array(BINS);
  for (let i = 0; i + 3 < rgba.length; i += 4) {
    if ((rgba[i + 3] ?? 255) < 128) continue;
    const [, a, b] = rgbToOklab([rgba[i] ?? 0, rgba[i + 1] ?? 0, rgba[i + 2] ?? 0]);
    const c = Math.hypot(a, b);
    if (c < GREY_C) continue;
    const deg = ((Math.atan2(b, a) * 180) / Math.PI + 360) % 360;
    const bin = Math.min(BINS - 1, Math.floor(deg / (360 / BINS)));
    weight[bin] = (weight[bin] ?? 0) + c;
    sumA[bin] = (sumA[bin] ?? 0) + a;
    sumB[bin] = (sumB[bin] ?? 0) + b;
  }
  const around = (i: number) => [(i + BINS - 1) % BINS, i, (i + 1) % BINS];
  let best = -1;
  let bestWeight = 0;
  for (let i = 0; i < BINS; i++) {
    const w = around(i).reduce((sum, j) => sum + (weight[j] ?? 0), 0);
    if (w > bestWeight) {
      bestWeight = w;
      best = i;
    }
  }
  if (best < 0) return undefined;
  const [a, b] = around(best).reduce<[number, number]>(([x, y], j) => [x + (sumA[j] ?? 0), y + (sumB[j] ?? 0)], [0, 0]);
  return ((Math.atan2(b, a) * 180) / Math.PI + 360) % 360;
}

export interface PhotoTint {
  /** The ground the photo fades into, and the screen's background below it. */
  ground: string;
  /**
   * The lists' cards on the tint (styles.css --p-fill: text 7% and surface 45% mixed in sRGB, over the tint),
   * flattened to an opaque colour: solid, never glass (design 3.1).
   */
  card: string;
  /** Hairlines between rows on a card (text at 10% over the card). */
  separator: string;
}

function mix(over: Rgb, under: Rgb, alpha: number): Rgb {
  return [0, 1, 2].map((i) => (over[i] ?? 0) * alpha + (under[i] ?? 0) * (1 - alpha)) as unknown as Rgb;
}

/** The tint for a hue, or the wash when the photo has none (the mockups' sage: design 3.1). */
export function photoTint(hue: number | undefined, scheme: ColorScheme): PhotoTint {
  const roles = colors[scheme];
  const ground = hue === undefined ? hexToRgb(roles.wash) : oklchToRgb(TINT_L[scheme], TINT_MAX_C, hue);
  const text = hexToRgb(roles.text);
  const surface = hexToRgb(roles.surface);
  const fill = mix(text, surface, 7 / 52);
  const card = mix(fill, ground, 0.52);
  return {
    ground: rgbToHex(ground),
    card: rgbToHex(card),
    separator: rgbToHex(mix(text, card, 0.1)),
  };
}

function luminance(rgb: Rgb): number {
  const [r, g, b] = rgb.map(toLinear) as unknown as Rgb;
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** WCAG 2 contrast ratio between two hex colours. */
export function contrast(a: string, b: string): number {
  const la = luminance(hexToRgb(a));
  const lb = luminance(hexToRgb(b));
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}
