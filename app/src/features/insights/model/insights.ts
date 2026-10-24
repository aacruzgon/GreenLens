// A plant's insights from its records (roadmap F7 item 3; design 8; analytics 2, P1 to P4, G3 and G4): the status
// at each check, spot cover after treatment with the steps done, height with its noise band, the weather before
// the diagnosis, and the nearest plant. Pure. The rules of analytics 2 and the wording of analytics 7 hold here:
// only medium bands and up enter a series, "lower" only when the ranges don't overlap, no extrapolation, no
// number on a diagnosis, and "after" or "also found on", never "because", "worked" or "spread from".
import { statusOfEntry } from "@/domain/plantStatus";
import { formatDay, formatMonthDay } from "@/domain/format";
import type { InsightTimelineEntry, RangePoint } from "@/domain/proposed";
import type { GardenMap, PlantStatus, Task, TimelineEntry } from "@/domain/types";

type ProblemRef = NonNullable<TimelineEntry["primary"]>;

export const DAY_MS = 86_400_000;
/** A trend needs 3 points spanning 14 days (analytics 2); P3 needs the 3 points only. */
export const MIN_POINTS = 3;
export const MIN_DAYS = 14;
/** A plant whose last check is older than this is "not checked recently" (analytics 2). */
export const STALE_DAYS = 14;
/** The nearest plant counts as nearby within this distance on the map. */
export const NEARBY_CM = 100;

export type StripStatus = PlantStatus | "unknown";

const STATUS_WORD: Record<StripStatus, string> = { healthy: "Healthy", watch: "Watch", sick: "Sick", dead: "Dead", unknown: "Not sure" };

const ts = (iso: string) => Date.parse(iso);
const lower = (name: string) => name.charAt(0).toLowerCase() + name.slice(1);

/** Entries with a result, oldest first: a scan still being analysed has no place on a chart yet. */
export function checks(entries: readonly InsightTimelineEntry[]): InsightTimelineEntry[] {
  return entries
    .filter((e) => e.status === "planned" || e.status === "diagnosed" || e.status === "failed")
    .sort((a, b) => a.created_at.localeCompare(b.created_at));
}

// ---- P1, health over time

export interface Cell {
  key: string;
  t: number;
  status: StripStatus;
  label: string;
}

/**
 * One cell per check (P1): the plant's status after it (`statusOfEntry`), grey "Not sure" for an uncertain
 * result with `low_evidence` and "Failed" for a failed scan.
 */
export function stripCells(entries: readonly InsightTimelineEntry[], current: PlantStatus, tz: string): Cell[] {
  const list = checks(entries);
  return list.map((e, i) => {
    const failed = e.status === "failed";
    const status: StripStatus = failed || (e.diagnosis_status === "uncertain" && e.low_evidence) ? "unknown" : statusOfEntry(e, current, i === list.length - 1);
    const word = failed ? "Failed" : STATUS_WORD[status];
    const problem = e.diagnosis_status === "confirmed" && e.primary ? `, ${lower(e.primary.name)}` : "";
    return { key: e.scan_id, t: ts(e.created_at), status, label: `${formatMonthDay(e.created_at, tz)}: ${word}${problem}` };
  });
}

/** "Healthy at 3 checks, then early blight on Oct 11." The strip's headline, from the records only. */
export function healthHeadline(entries: readonly InsightTimelineEntry[], cells: readonly Cell[], tz: string): string {
  const list = checks(entries);
  if (list.length === 0) return "Scan this plant to start its history.";
  if (list.length === 1) return "One check so far";
  const healthyRun = cells.findIndex((c) => c.status !== "healthy");
  if (healthyRun === -1) return `Healthy at all ${cells.length} checks`;
  const turn = list[healthyRun]!;
  const what = turn.diagnosis_status === "confirmed" && turn.primary ? lower(turn.primary.name) : STATUS_WORD[cells[healthyRun]!.status].toLowerCase();
  if (healthyRun === 0) return `${what.charAt(0).toUpperCase()}${what.slice(1)} on ${formatMonthDay(turn.created_at, tz)}`;
  return `Healthy at ${healthyRun === 1 ? "1 check" : `${healthyRun} checks`}, then ${what} on ${formatMonthDay(turn.created_at, tz)}`;
}

/** The strip in words for VoiceOver: each check's date and status. */
export function stripSummary(cells: readonly Cell[]): string {
  return cells.length ? `Status at each check: ${cells.map((c) => c.label).join("; ")}` : "No checks yet";
}

/** "Sep 20 to Oct 17", the checks' first and last dates. */
export function rangeText(entries: readonly InsightTimelineEntry[], tz: string): string | undefined {
  const list = checks(entries);
  if (!list.length) return undefined;
  const a = formatMonthDay(list[0]!.created_at, tz);
  const b = formatMonthDay(list.at(-1)!.created_at, tz);
  return a === b ? a : `${a} to ${b}`;
}

/** The last diagnosis is older than 14 days (analytics 2: stale data says so). */
export function notCheckedRecently(entries: readonly InsightTimelineEntry[], now: Date = new Date()): boolean {
  const last = checks(entries).filter((e) => e.diagnosis_status).at(-1);
  return Boolean(last) && now.getTime() - ts(last!.created_at) > STALE_DAYS * DAY_MS;
}

// ---- series

/** Medium band and up, oldest first (analytics 2). */
export function series(points: readonly RangePoint[]): RangePoint[] {
  return points.filter((p) => p.confidence_band !== "low").sort((a, b) => a.created_at.localeCompare(b.created_at));
}

const lo = (p: RangePoint) => p.min ?? p.value;
const hi = (p: RangePoint) => p.max ?? p.value;

/** "14, 9 and 5" */
function andList(items: readonly string[]): string {
  return items.length <= 1 ? (items[0] ?? "") : `${items.slice(0, -1).join(", ")} and ${items.at(-1)}`;
}

// ---- P3, spots after treatment

export interface SpotsReading {
  points: RangePoint[];
  /** "14% → 5%", with at least 2 points. */
  kpi?: string;
  /** The checks in the series. */
  checks: number;
  headline: string;
  /** P3's rule: 3 points and the last range clear of the first. */
  direction?: "lower" | "higher" | "about the same";
  /** The line joins the points only with a direction other than "about the same". */
  line: boolean;
  summary: string;
  caption: string;
}

/** Affected area over the checks (P3). `undefined` with no close-up measurements. */
export function spotsReading(points: readonly RangePoint[], tz: string): SpotsReading | undefined {
  const pts = series(points);
  if (pts.length === 0) return undefined;
  const pct = (v: number) => `${Math.round(v)}%`;
  const first = pts[0]!;
  const last = pts.at(-1)!;
  let direction: SpotsReading["direction"];
  if (pts.length >= MIN_POINTS) direction = hi(last) < lo(first) ? "lower" : lo(last) > hi(first) ? "higher" : "about the same";
  const falling = pts.every((p, i) => i === 0 || p.value < pts[i - 1]!.value);
  const since = formatMonthDay(first.created_at, tz);
  const headline =
    pts.length === 1
      ? `Measured once: spots cover about ${pct(first.value)} of the measured leaves`
      : direction === "lower"
        ? falling
          ? `Lower at each check since ${since}`
          : "Lower at the last check than at the first"
        : direction === "higher"
          ? "Higher at the last check than at the first"
          : direction === "about the same"
            ? "About the same at each check"
            : `Measured at ${pts.length} checks: too few to compare`;
  const ranged = pts.filter((p) => p.min !== undefined && p.max !== undefined);
  const ns = new Set(pts.map((p) => p.n).filter((n): n is number => n !== undefined));
  const leaves = ns.size === 1 ? ` across ${[...ns][0]} ${[...ns][0] === 1 ? "leaf" : "leaves"}` : "";
  const summary =
    `Spot cover: ${andList(pts.map((p) => `${Math.round(p.value)} percent on ${formatMonthDay(p.created_at, tz)}`))}` +
    (ranged.length ? `, with ranges ${andList(ranged.map((p) => `${Math.round(lo(p))} to ${Math.round(hi(p))}`))}` : "");
  return {
    points: pts,
    kpi: pts.length >= 2 ? `${pct(first.value)} → ${pct(last.value)}` : undefined,
    checks: pts.length,
    headline,
    direction,
    line: direction === "lower" || direction === "higher",
    summary,
    caption: `On the leaves in the close-ups, which may differ between scans.${ranged.length ? ` Bars show the range${leaves}.` : ""}`,
  };
}

// ---- P4, the steps done

export interface Lane {
  key: string;
  title: string;
  times: number[];
  dates: string;
}

/** "Oct 14, 17", or "Oct 30, Nov 2" across a month. */
export function datesText(isos: readonly string[], tz: string): string {
  let month = "";
  return isos
    .map((iso) => {
      const [m, d] = formatMonthDay(iso, tz).split(" ");
      const out = m === month ? d! : `${m} ${d}`;
      month = m!;
      return out;
    })
    .join(", ");
}

/**
 * One lane per plan step of this plant, with each time it was done (P4), in the order first done. Dismissed
 * tasks are left out: the system dismissed a superseded step, not a person.
 */
export function stepLanes(tasks: readonly Task[], plantId: string, tz: string): Lane[] {
  const done = tasks
    .filter((t) => t.plant_id === plantId && t.plan_id && t.done_at && !t.dismissed_at)
    .sort((a, b) => a.done_at!.localeCompare(b.done_at!));
  const lanes = new Map<string, { title: string; isos: string[] }>();
  for (const t of done) {
    const key = t.plan_step_id ?? t.id;
    const lane = lanes.get(key) ?? { title: t.title, isos: [] };
    lane.isos.push(t.done_at!);
    lanes.set(key, lane);
  }
  return [...lanes].map(([key, l]) => ({ key, title: l.title, times: l.isos.map(ts), dates: datesText(l.isos, tz) }));
}

/** P4's sentence under the lanes and the spots: "after", never "worked". */
export function afterSteps(lanes: readonly Lane[], spots: SpotsReading | undefined): string | undefined {
  if (!lanes.length) return undefined;
  if (!spots || spots.checks < 2) return "Scan again to see how it's doing.";
  if (spots.direction === "lower") return "After the steps were done, spots covered less of the measured leaves at the next checks.";
  if (spots.direction === "higher") return "After the steps were done, spots covered more of the measured leaves at the next checks.";
  return undefined;
}

// ---- P2, height with its noise band

/** The provisional noise floor: ±2 cm or ±5%, whichever is larger (analytics 2; vision evaluation). */
export function noiseFloor(value: number): number {
  return Math.max(2, value * 0.05);
}

export interface HeightReading {
  points: RangePoint[];
  band: { lo: number; hi: number }[];
  trend: boolean;
  headline: string;
  /** True when a point has no measured noise and the band is the proposal's floor. */
  provisional: boolean;
  summary: string;
  caption: string;
}

export function heightReading(points: readonly RangePoint[], tz: string): HeightReading | undefined {
  const pts = series(points);
  if (pts.length === 0) return undefined;
  const noise = (p: RangePoint) => p.noise_cm ?? noiseFloor(p.value);
  const provisional = pts.some((p) => p.noise_cm === undefined);
  const times = pts.map((p) => ts(p.created_at));
  const trend = pts.length >= MIN_POINTS && times.at(-1)! - times[0]! >= MIN_DAYS * DAY_MS;
  const first = pts[0]!;
  const last = pts.at(-1)!;
  const now = `About ${Math.round(last.value)} cm`;
  let headline: string;
  if (!trend) headline = `${now}. ${pts.length === 1 ? "Measured once" : `${pts.length} checks so far`}: too few for a rate`;
  else {
    // A rate only when the change is larger than the noise at both ends.
    const clear = Math.abs(last.value - first.value) > noise(first) + noise(last);
    const weeks = (times.at(-1)! - times[0]!) / (7 * DAY_MS);
    const rate = Math.round((last.value - first.value) / weeks);
    headline = !clear || rate === 0 ? `${now}. No clear change yet` : rate > 0 ? `${now}, growing about ${rate} cm a week` : `${now}, about ${-rate} cm a week shorter`;
  }
  const cm = pts.map((p) => `${Math.round(p.value)}`);
  return {
    points: pts,
    band: pts.map((p) => ({ lo: p.value - noise(p), hi: p.value + noise(p) })),
    trend,
    headline,
    provisional,
    summary: `Height: ${andList(cm)} centimetres, from ${formatMonthDay(first.created_at, tz)} to ${formatMonthDay(last.created_at, tz)}`,
    caption: provisional
      ? "From whole-plant LiDAR scans. The band is the measurement noise, ±2 cm or 5%: provisional, until repeat scans measure it."
      : "From whole-plant LiDAR scans. The band is the measured noise of repeat scans.",
  };
}

// ---- G4, weather before the diagnosis

export interface HumidBar {
  key: string;
  date: string;
  hours: number;
  /** The diagnosis' own scan. */
  highlight: boolean;
}

export interface WeatherReading {
  bars: HumidBar[];
  headline: string;
  summary: string;
  caption: string;
}

/** The first check of the latest run of confirmed diagnoses of one problem: when it was diagnosed. */
export function diagnosisEntry(entries: readonly InsightTimelineEntry[]): InsightTimelineEntry | undefined {
  const list = checks(entries).filter((e) => e.status !== "failed");
  const last = list.at(-1);
  if (last?.diagnosis_status !== "confirmed" || !last.primary) return undefined;
  let i = list.length - 1;
  while (i > 0 && list[i - 1]!.diagnosis_status === "confirmed" && list[i - 1]!.primary?.problem_id === last.primary.problem_id) i--;
  return list[i];
}

/**
 * Hours over 90% humidity in the week before each check up to the diagnosis (G4), from the proposed `weather_7d`.
 * A pattern, never a cause: the only cause named is the knowledge base's, when the problem's `weather_fit` is
 * favourable. `undefined` without a current diagnosis or its weather.
 */
export function weatherReading(entries: readonly InsightTimelineEntry[], tz: string, maxBars = 5): WeatherReading | undefined {
  const diag = diagnosisEntry(entries);
  if (!diag?.weather_7d || !diag.primary) return undefined;
  const upTo = checks(entries).filter((e) => e.weather_7d && e.created_at <= diag.created_at).slice(-maxBars);
  const bars = upTo.map((e) => ({ key: e.scan_id, date: formatMonthDay(e.created_at, tz), hours: e.weather_7d!.hours_rh_over_90, highlight: e.scan_id === diag.scan_id }));
  const date = formatMonthDay(diag.created_at, tz);
  const w = diag.weather_7d;
  const headline =
    w.longest_humid_run_days >= 2
      ? `${w.longest_humid_run_days} humid days in a row before ${date}`
      : `${w.hours_rh_over_90} ${w.hours_rh_over_90 === 1 ? "hour" : "hours"} over 90% humidity in the week before ${date}`;
  const lead = diag.primary.weather_fit === "favourable" ? `Humid stretches favour ${lower(diag.primary.name)}; this` : "This";
  return {
    bars,
    headline,
    summary: `Hours over 90 percent humidity in the week before each scan: ${andList(bars.map((b) => `${b.hours} before ${b.date}`))}`,
    caption: `Hours over 90% humidity in the week before each scan. ${lead} shows a pattern, not a cause.`,
  };
}

// ---- G3, the nearest plant

/** The nearest other pin within `maxCm`, its distance to the nearest 5 cm (design 3.1, the map). */
export function nearestPlant(map: GardenMap | null | undefined, plantId: string, maxCm = NEARBY_CM): { plantId: string; cm: number } | undefined {
  const me = map?.pins.find((p) => p.plant_id === plantId);
  if (!map || !me) return undefined;
  let best: { plantId: string; cm: number } | undefined;
  for (const p of map.pins) {
    if (p.plant_id === plantId) continue;
    const cm = Math.round((Math.hypot(p.x_m - me.x_m, p.y_m - me.y_m) * 100) / 5) * 5;
    if (cm <= maxCm && (!best || cm < best.cm)) best = { plantId: p.plant_id, cm };
  }
  return best;
}

/**
 * What the neighbour's checks found, beside this plant's problem: "Also found on it today: early blight. Healthy
 * at its check on Oct 13." Never "spread from" (analytics 7).
 */
export function nearbyLine(neighbour: readonly InsightTimelineEntry[], problem: ProblemRef | undefined, tz: string, now: Date = new Date()): string {
  const list = checks(neighbour).filter((e) => e.status !== "failed");
  const last = list.at(-1);
  if (!last) return "Not checked yet";
  const when = (iso: string) => {
    const d = formatDay(iso, tz, now);
    return d === "Today" || d === "Yesterday" ? d.toLowerCase() : `on ${d}`;
  };
  const healthyBefore = list.slice(0, -1).filter((e) => e.diagnosis_status === "healthy").at(-1);
  if (last.diagnosis_status === "confirmed" && last.primary) {
    const same = problem && last.primary.problem_id === problem.problem_id;
    const found = same ? `Also found on it ${when(last.created_at)}: ${lower(last.primary.name)}.` : `${last.primary.name} at its check ${when(last.created_at)}.`;
    return healthyBefore ? `${found} Healthy at its check on ${formatMonthDay(healthyBefore.created_at, tz)}` : found;
  }
  if (last.diagnosis_status === "healthy") return `Healthy at its check ${when(last.created_at)}`;
  return `Not sure at its check ${when(last.created_at)}`;
}
