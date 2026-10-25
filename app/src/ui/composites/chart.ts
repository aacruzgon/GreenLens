// What the chart composites share (analytics 2, shared rules; styles.css .chart): the time axis, the value
// ticks, which date labels fit, and the trend rule. Pure, so the charts line up when they share a domain.

export const DAY_MS = 86_400_000;

/** A trend line needs 3 points spanning at least 14 days (analytics 2). */
export const TREND_MIN_POINTS = 3;
export const TREND_MIN_DAYS = 14;

/** Space for the value labels on the left, and the margin on the right (styles.css .chart, x 36 to 307). */
export const CHART_PAD = { left: 36, right: 4 } as const;
/** How far the first and last marks sit inside the plot, so a range bar isn't cut at the edge. */
export const CHART_INSET = 10;

export function isTrend(times: readonly number[]): boolean {
  if (times.length < TREND_MIN_POINTS) return false;
  return Math.max(...times) - Math.min(...times) >= TREND_MIN_DAYS * DAY_MS;
}

/** The time domain of some series: the earliest and latest of all their times. */
export function timeDomain(...series: readonly (readonly number[])[]): [number, number] {
  const all = series.flat();
  return all.length ? [Math.min(...all), Math.max(...all)] : [0, 0];
}

/**
 * Maps a time to x in a chart `width` wide, between the left pad and the right edge, inset at both ends. A
 * domain of one instant puts it in the middle.
 */
export function timeScale(domain: readonly [number, number], width: number, left: number = CHART_PAD.left): (t: number) => number {
  const [t0, t1] = domain;
  const a = left + CHART_INSET;
  const b = width - CHART_PAD.right - CHART_INSET;
  return (t) => (t1 === t0 ? (a + b) / 2 : a + ((t - t0) / (t1 - t0)) * (b - a));
}

/**
 * About `count` ticks over `lo`–`hi` on a familiar step (1, 2 or 5 × 10ⁿ: HIG Charts, Axes), covering both ends.
 * `lo` equal to `hi` still gets a span.
 */
export function niceTicks(lo: number, hi: number, count = 3): number[] {
  if (hi <= lo) hi = lo + 1;
  const raw = (hi - lo) / Math.max(1, count);
  const mag = 10 ** Math.floor(Math.log10(raw));
  const step = ([1, 2, 5, 10].find((m) => m * mag >= raw) ?? 10) * mag;
  const first = Math.floor(lo / step);
  const last = Math.ceil(hi / step - 1e-9);
  const out: number[] = [];
  for (let i = first; i <= last; i++) out.push(Math.round(i * step * 1e6) / 1e6);
  return out;
}

/** The indexes of the labels at `xs` that fit `minGap` apart: the first and last always, then left to right. */
export function fittingLabels(xs: readonly number[], minGap = 44): number[] {
  if (xs.length <= 1) return xs.map((_, i) => i);
  const last = xs.length - 1;
  const keep = [0];
  for (let i = 1; i < last; i++) {
    if (xs[i]! - xs[keep.at(-1)!]! >= minGap && xs[last]! - xs[i]! >= minGap) keep.push(i);
  }
  keep.push(last);
  return keep;
}

/** A date label under `x`: centred, or ended at the chart's right edge when centring would cut it (about 22 pt each side). */
export function dateAnchor(x: number, width: number): { x: number; textAnchor: "middle" | "end" } {
  return x + 22 > width ? { x: width - 1, textAnchor: "end" } : { x, textAnchor: "middle" };
}
