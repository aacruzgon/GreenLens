// What the answer blocks show, computed from the records they carry (design 5, "Chat: answers you can interact
// with"). Every number here is a record's (a forecast day, a measurement, a map position), never the model's.
// Each block has a summary for VoiceOver, and the chart rules of analytics 2 hold: a trend needs 3 points over at
// least 14 days, only medium bands and up enter a series, no extrapolation, and weather is a pattern, never a cause.
import type { AnswerBlock, BlockPlant } from "@/domain/proposed";
import { formatLocalDate } from "@/domain/format";
import type { ForecastDay, GardenMap, MeasurementPoint, Pin } from "@/domain/types";

import { nameList } from "./conversations";

type Block<K extends AnswerBlock["kind"]> = Extract<AnswerBlock, { kind: K }>;

const DAY_MS = 86_400_000;

function weekday(date: string): string {
  const [y, m, d] = date.split("-").map(Number);
  return new Date(y!, m! - 1, d!).toLocaleDateString("en-CA", { weekday: "long" });
}

// ---- frost cover planner

export type FrostPlant = Block<"frost_planner">["plants"][number];

/** The plants the planner starts with chosen: the tender ones. */
export function defaultCover(block: Block<"frost_planner">): string[] {
  return block.plants.filter((p) => p.frost_tender).map((p) => p.plant_id);
}

export function frostNight(block: Block<"frost_planner">): ForecastDay | undefined {
  return block.nights[0];
}

/** "Cover 3 plants on Thursday evening: Tomato 1, Tomato 2 and Basil." The planner's live summary. */
export function frostSummary(block: Block<"frost_planner">, chosen: readonly string[]): string {
  const night = frostNight(block);
  const names = block.plants.filter((p) => chosen.includes(p.plant_id)).map((p) => p.name);
  const when = night ? `on ${weekday(night.date)} evening` : "before the frost";
  if (names.length === 0) return "No plants chosen to cover.";
  const count = names.length === 1 ? "1 plant" : `${names.length} plants`;
  const uncovered = block.plants.filter((p) => p.frost_tender && !chosen.includes(p.plant_id)).map((p) => p.name);
  const left = uncovered.length ? ` ${nameList(uncovered)} ${uncovered.length === 1 ? "stays" : "stay"} uncovered.` : "";
  return `Cover ${count} ${when}: ${nameList(names)}.${left}`;
}

/** The message the planner sends: the agent answers with a `create_task` card, so the task needs a confirm. */
export function coverRequest(block: Block<"frost_planner">, chosen: readonly string[]): string {
  const night = frostNight(block);
  const names = block.plants.filter((p) => chosen.includes(p.plant_id)).map((p) => p.name);
  return `Add a task to cover ${nameList(names)}${night ? ` on ${weekday(night.date)} evening` : ""}.`;
}

// ---- weather hours

/** "14 hours over 90% humidity" for a day, the leaf-wetness stand-in. */
export function humidHoursText(day: ForecastDay): string {
  const h = day.hours_rh_over_90;
  return `${h} ${h === 1 ? "hour" : "hours"} over 90% humidity`;
}

/** The day's bar, 0–1 of a full day's humid hours. */
export function humidShare(day: ForecastDay): number {
  return Math.max(0, Math.min(1, day.hours_rh_over_90 / 24));
}

export function dayLabel(date: string): string {
  return weekday(date).slice(0, 3);
}

// ---- measurement trend

export const TREND_MIN_POINTS = 3;
export const TREND_MIN_DAYS = 14;

/** The points a chart may draw: medium band and up (analytics 2). */
export function chartPoints(points: readonly MeasurementPoint[]): MeasurementPoint[] {
  return points.filter((p) => p.confidence_band !== "low");
}

/** A trend line and trend words need 3 points spanning 14 days; with fewer, the points only. */
export function hasTrend(points: readonly MeasurementPoint[]): boolean {
  const pts = chartPoints(points);
  if (pts.length < TREND_MIN_POINTS) return false;
  const span = new Date(pts.at(-1)!.created_at).getTime() - new Date(pts[0]!.created_at).getTime();
  return span >= TREND_MIN_DAYS * DAY_MS;
}

/** The noise band of height and width: ±2 cm or ±5%, whichever is larger (analytics 2). */
export function noise(value: number): number {
  return Math.max(2, value * 0.05);
}

export interface PlotPoint {
  x: number;
  y: number;
  lo: number;
  hi: number;
  point: MeasurementPoint;
}

/** Points in a `width` × `height` box, the value axis padded by the noise band; time on x. */
export function plot(points: readonly MeasurementPoint[], width: number, height: number): PlotPoint[] {
  const pts = chartPoints(points);
  if (pts.length === 0) return [];
  const ts = pts.map((p) => new Date(p.created_at).getTime());
  const t0 = Math.min(...ts);
  const t1 = Math.max(...ts);
  const lo = Math.min(...pts.map((p) => p.value - noise(p.value)));
  const hi = Math.max(...pts.map((p) => p.value + noise(p.value)));
  const sx = (t: number) => (t1 === t0 ? width / 2 : ((t - t0) / (t1 - t0)) * width);
  const sy = (v: number) => (hi === lo ? height / 2 : height - ((v - lo) / (hi - lo)) * height);
  return pts.map((p, i) => ({ x: sx(ts[i]!), y: sy(p.value), lo: sy(p.value - noise(p.value)), hi: sy(p.value + noise(p.value)), point: p }));
}

/** "Higher", "lower" or "about the same", first point against last, only when the bands don't overlap. */
export function trendWord(points: readonly MeasurementPoint[]): "higher" | "lower" | "about the same" | undefined {
  if (!hasTrend(points)) return undefined;
  const pts = chartPoints(points);
  const a = pts[0]!.value;
  const b = pts.at(-1)!.value;
  if (b - noise(b) > a + noise(a)) return "higher";
  if (b + noise(b) < a - noise(a)) return "lower";
  return "about the same";
}

// ---- garden map snippet

export interface MapFrame {
  /** Metres to points: x right, y down, as the map's own coordinates. */
  scale: number;
  ox: number;
  oy: number;
  width: number;
  height: number;
}

/**
 * Fits the map into `width`, keeping its aspect. With `focus`, the frame is the snippet around those plants (at
 * least `span` metres a side), so two plants 40 cm apart don't draw as one pin: fitting the whole garden did,
 * and hid their canopy rings, on the simulator. Without it, every bed and pin, with a margin in metres.
 */
export function fitMap(map: GardenMap, width: number, focus: readonly string[] = [], span = 1.6, margin = 0.4): MapFrame {
  const pins = map.pins.filter((p) => focus.includes(p.plant_id));
  let minX: number, maxX: number, minY: number, maxY: number;
  if (pins.length) {
    const xs = pins.map((p) => p.x_m);
    const ys = pins.map((p) => p.y_m);
    const cx = (Math.min(...xs) + Math.max(...xs)) / 2;
    const cy = (Math.min(...ys) + Math.max(...ys)) / 2;
    const half = Math.max(span, Math.max(...xs) - Math.min(...xs) + 2 * margin, Math.max(...ys) - Math.min(...ys) + 2 * margin) / 2;
    [minX, maxX, minY, maxY] = [cx - half, cx + half, cy - half * 0.6, cy + half * 0.6];
  } else {
    const xs = [...map.beds.flatMap((b) => b.polygon_m.map((p) => p[0]!)), ...map.pins.map((p) => p.x_m)];
    const ys = [...map.beds.flatMap((b) => b.polygon_m.map((p) => p[1]!)), ...map.pins.map((p) => p.y_m)];
    [minX, maxX, minY, maxY] = [Math.min(...xs) - margin, Math.max(...xs) + margin, Math.min(...ys) - margin, Math.max(...ys) + margin];
  }
  const scale = width / Math.max(maxX - minX, 0.5);
  return { scale, ox: minX, oy: minY, width, height: Math.max(maxY - minY, 0.5) * scale };
}

/** Whether a point in metres falls inside the frame (pins outside a snippet aren't drawn). */
export function inFrame(frame: MapFrame, x: number, y: number): boolean {
  const px = (x - frame.ox) * frame.scale;
  const py = (y - frame.oy) * frame.scale;
  return px >= 0 && px <= frame.width && py >= 0 && py <= frame.height;
}

/** Pins are tapped in AR, so distances are given to the nearest 5 cm (design 3.1, the map). */
export function pinDistanceCm(a: Pin, b: Pin): number {
  return Math.round((Math.hypot(a.x_m - b.x_m, a.y_m - b.y_m) * 100) / 5) * 5;
}

/** "Tomato 1 and Tomato 2 are 40 cm apart." for the first two focus plants. */
export function focusDistance(block: Block<"garden_map">): string | undefined {
  const [a, b] = block.focus_plant_ids.map((id) => block.map.pins.find((p) => p.plant_id === id));
  if (!a || !b) return undefined;
  const name = (id: string) => block.plants.find((p) => p.plant_id === id)?.name ?? "A plant";
  return `${name(a.plant_id)} and ${name(b.plant_id)} are ${pinDistanceCm(a, b)} cm apart.`;
}

export function plantLine(p: BlockPlant): string {
  const status = { healthy: "healthy", watch: "to watch", sick: "sick", dead: "dead" }[p.status];
  return p.height_cm !== undefined ? `${p.name}, ${status}, about ${Math.round(p.height_cm)} cm tall` : `${p.name}, ${status}`;
}

// ---- VoiceOver summaries

const QUANTITY_WORD: Record<string, string> = { plant_height_cm: "height", canopy_width_cm: "width" };

/** A block in words, for screen readers. The text answer makes sense without the block (design 5). */
export function blockSummary(block: AnswerBlock): string {
  switch (block.kind) {
    case "garden_map":
      return [`Garden map with ${block.map.beds.length} beds and ${block.map.pins.length} plants.`, focusDistance(block)].filter(Boolean).join(" ");
    case "weather_hours":
      return `Humid hours, ${block.days.length} days: ${block.days.map((d) => `${weekday(d.date)} ${d.hours_rh_over_90}`).join(", ")}.`;
    case "frost_planner": {
      const night = frostNight(block);
      const tender = block.plants.filter((p) => p.frost_tender).map((p) => p.name);
      return `Frost cover planner. ${night ? `Frost on ${weekday(night.date)} night, low of ${night.t_min_c} °C.` : ""} Tender plants: ${nameList(tender) || "none"}.`.replace(/\s+/g, " ").trim();
    }
    case "measurement_trend": {
      const pts = chartPoints(block.points);
      const word = trendWord(block.points);
      const q = QUANTITY_WORD[block.quantity] ?? "size";
      if (pts.length === 0) return `${block.plant_name}'s ${q}: no measurements yet.`;
      const range = `${Math.round(pts[0]!.value)} to ${Math.round(pts.at(-1)!.value)} cm over ${pts.length} scans, from ${formatLocalDate(pts[0]!.created_at.slice(0, 10))}`;
      return `${block.plant_name}'s ${q}: ${range}.${word ? ` The latest is ${word} than the first.` : ""}`;
    }
    case "candidates":
      return `${block.disease.name} compared with ${nameList(block.lookalikes.map((l) => l.name))}. ${block.lookalikes.map((l) => `${l.name}: ${l.how_to_tell}`).join(" ")}`;
  }
}
