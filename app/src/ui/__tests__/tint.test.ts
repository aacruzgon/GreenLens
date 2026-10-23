import { colors, type ColorRole, type ColorScheme } from "../tokens";
import { contrast, dominantHue, hexToRgb, oklchToRgb, photoTint, rgbToOklab } from "../tint";

// The roles drawn as text or a status mark on the diagnosis's tint and on its cards.
const ON_TINT: ColorRole[] = ["text", "textMuted", "accent", "info", "danger", "healthy", "watch", "sick", "dead"];
const SCHEMES: ColorScheme[] = ["light", "dark"];

function pixels(...rgb: [number, number, number][]): Uint8Array {
  return new Uint8Array(rgb.flatMap(([r, g, b]) => [r, g, b, 255]));
}

describe("photo tint", () => {
  it("round-trips sRGB through OKLCH", () => {
    const [L, a, b] = rgbToOklab(hexToRgb("#E3EBDC"));
    const hue = (Math.atan2(b, a) * 180) / Math.PI;
    expect(oklchToRgb(L, Math.hypot(a, b), hue)).toEqual(hexToRgb("#E3EBDC"));
  });

  it("finds a green leaf's hue despite brown spots and grey sky", () => {
    const leaf: [number, number, number] = [70, 120, 50];
    const spot: [number, number, number] = [110, 80, 40];
    const sky: [number, number, number] = [200, 200, 200];
    const hue = dominantHue(pixels(leaf, leaf, leaf, leaf, spot, sky, sky));
    const [, a, b] = rgbToOklab(leaf);
    expect(hue).toBeCloseTo(((Math.atan2(b, a) * 180) / Math.PI + 360) % 360, 0);
  });

  it("has no hue for a grey photo, and falls back to the wash", () => {
    expect(dominantHue(pixels([128, 128, 128], [20, 20, 20]))).toBeUndefined();
    expect(photoTint(undefined, "light").ground).toBe(colors.light.wash);
    expect(photoTint(undefined, "dark").ground).toBe(colors.dark.wash);
  });

  it("holds the lightness at 0.93 in light and 0.24 in dark, chroma at most 0.03", () => {
    for (const hue of [0, 30, 140, 250]) {
      const [L1, a1, b1] = rgbToOklab(hexToRgb(photoTint(hue, "light").ground));
      const [L2, a2, b2] = rgbToOklab(hexToRgb(photoTint(hue, "dark").ground));
      expect(L1).toBeCloseTo(0.93, 2);
      expect(L2).toBeCloseTo(0.24, 2);
      expect(Math.hypot(a1, b1)).toBeLessThanOrEqual(0.032);
      expect(Math.hypot(a2, b2)).toBeLessThanOrEqual(0.032);
    }
  });

  // Design 3.1 checked the sage only; this checks every hue at 5° steps (roadmap 10 exit check: 4.5:1).
  it.each(SCHEMES)("keeps every text and status colour at 4.5:1 or more on any hue (%s)", (scheme) => {
    let lowest = Infinity;
    for (let hue = 0; hue < 360; hue += 5) {
      const tint = photoTint(hue, scheme);
      for (const role of ON_TINT) {
        lowest = Math.min(lowest, contrast(colors[scheme][role], tint.ground), contrast(colors[scheme][role], tint.card));
      }
    }
    expect(lowest).toBeGreaterThanOrEqual(4.5);
  });
});
