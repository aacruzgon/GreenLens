// The garden screen's model (roadmap F7 items 1–2; design 3.1, the 3D garden; screens 3.8): the scrubber's dates
// from the plants' scans and the forecast's frost night, each plant's record over those dates, the top card's
// words for each mode, the selected plant's card, and the screen's state. Pure TypeScript.
import { formatLocalDate, inZone } from "@/domain/format";
import { statusOfEntry } from "@/domain/plantStatus";
import type { PlantWithFrost, TimelineEntryWithStatus } from "@/domain/proposed";
import type { ForecastDay, GardenMap, LocalDate, PlantStatus } from "@/domain/types";
import { format } from "date-fns";

import { measurementAt, type GardenMode, type PlantMeasurement, type PlantRecord, type SceneDate, type ScenePlant } from "./gardenScene";

export type GardenViewKind = "3d" | "map";

/** A scan date's local day, `YYYY-MM-DD`, in the garden's time zone. */
export function localDay(iso: string, timeZone: string): LocalDate {
  return format(inZone(iso, timeZone), "yyyy-MM-dd");
}

/** The record's measurement on the day: a plant scanned twice in a day keeps the later scan. */
function byDay(entries: TimelineEntryWithStatus[], current: PlantStatus, timeZone: string): PlantMeasurement[] {
  // The timeline is newest first; the first entry is the latest scan.
  const done = entries.filter((e) => e.status !== "failed" && e.status !== "pending");
  const out = new Map<string, PlantMeasurement>();
  done.forEach((e, i) => {
    const date = localDay(e.created_at, timeZone);
    if (out.has(date)) return;
    const m = e.measurements;
    out.set(date, {
      date,
      status: statusOfEntry(e, current, i === 0),
      ...(m?.plant_height_cm !== undefined ? { height_cm: m.plant_height_cm } : {}),
      ...(m?.canopy_width_cm !== undefined ? { width_cm: m.canopy_width_cm } : {}),
    });
  });
  return [...out.values()].reverse();
}

/** A plant's record for the scene: its status and sizes at each scan, oldest first, and whether frost harms it. */
export interface GardenPlantRecord extends PlantRecord {
  /** The species' `frost`; undefined when not known (the Frost card counts only the plants known to be tender). */
  frost?: "tender" | "hardy";
  /** The primary problem at each scan that had one, by local day. */
  problems: Record<string, string>;
}

export function plantRecords(
  plants: PlantWithFrost[],
  timelines: Record<string, TimelineEntryWithStatus[] | undefined>,
  timeZone: string,
): GardenPlantRecord[] {
  return plants.map((p) => {
    const entries = timelines[p.id] ?? [];
    const problems: Record<string, string> = {};
    for (const e of [...entries].reverse()) {
      if (e.primary) problems[localDay(e.created_at, timeZone)] = e.primary.name;
    }
    const frost = p.species?.frost;
    return { id: p.id, name: p.name, tender: frost === "tender", ...(frost ? { frost } : {}), history: byDay(entries, p.status, timeZone), problems };
  });
}

/**
 * The scrubber's dates: every day a plant was scanned, oldest first, then one step past today, the forecast's
 * first frost night (design 3.1: dragging to it chooses Frost). Without a frost in the forecast there is no step.
 */
export function gardenDates(records: GardenPlantRecord[], forecast: readonly ForecastDay[], today: LocalDate): SceneDate[] {
  const days = new Set<string>();
  for (const r of records) for (const m of r.history) days.add(m.date);
  const dates: SceneDate[] = [...days].sort().map((date) => ({ date, label: formatLocalDate(date) }));
  const frost = forecast.find((d) => d.frost_risk && d.date >= today);
  // A frost tonight shares today's date with today's scan; it is still its own step, after the scan.
  if (frost) dates.push({ date: frost.date, label: frost.date === today ? "Tonight" : weekdayShort(frost.date), forecast: true });
  return dates;
}

function weekdayShort(date: LocalDate): string {
  const [y = 1970, m = 1, d = 1] = date.split("-").map(Number);
  return format(new Date(y, m - 1, d), "EEE");
}

function weekdayLong(date: LocalDate): string {
  const [y = 1970, m = 1, d = 1] = date.split("-").map(Number);
  return format(new Date(y, m - 1, d), "EEEE");
}

// --- The screen's state --------------------------------------------------------

export interface GardenState {
  view: GardenViewKind;
  mode: GardenMode;
  dateIndex: number;
  selectedId: string | null;
  /** The selected plant's pill grown into its card. */
  expanded: boolean;
  /** The dates, timelines and forecast are all in, and the date is chosen (`init`). */
  initialised: boolean;
}

export type GardenAction =
  | { type: "mode"; mode: GardenMode }
  | { type: "date"; index: number }
  | { type: "select"; id: string | null }
  | { type: "view"; view: GardenViewKind }
  | { type: "expand"; on: boolean }
  /** The dates arrived: open on the latest scan, or on the forecast night in Frost. */
  | { type: "init"; mode?: GardenMode };

/** The latest scan's index: the date before the forecast step, or the last. */
export function latestScanIndex(dates: SceneDate[]): number {
  const forecast = dates.findIndex((d) => d.forecast);
  return forecast > 0 ? forecast - 1 : Math.max(0, dates.length - 1);
}

export function initialGardenState(dates: SceneDate[], opts: { view: GardenViewKind; selectedId?: string | null; mode?: GardenMode }): GardenState {
  const state: GardenState = { view: opts.view, mode: "health", dateIndex: latestScanIndex(dates), selectedId: opts.selectedId ?? null, expanded: false, initialised: false };
  return opts.mode ? gardenReducer(dates, state, { type: "mode", mode: opts.mode }) : state;
}

/**
 * The modes follow the date (design 3.1): Health and Growth show the latest scan, Frost the forecast night, and
 * moving the date to the forecast chooses Frost; moving it off the forecast leaves Frost for Health.
 */
export function gardenReducer(dates: SceneDate[], state: GardenState, action: GardenAction): GardenState {
  const forecast = dates.findIndex((d) => d.forecast);
  switch (action.type) {
    case "mode": {
      if (action.mode === "frost") return { ...state, mode: "frost", dateIndex: forecast >= 0 ? forecast : state.dateIndex };
      const dateIndex = forecast >= 0 && state.dateIndex === forecast ? latestScanIndex(dates) : state.dateIndex;
      return { ...state, mode: action.mode, dateIndex };
    }
    case "date": {
      const index = Math.max(0, Math.min(dates.length - 1, Math.round(action.index)));
      if (index === state.dateIndex) return state;
      if (index === forecast) return { ...state, dateIndex: index, mode: "frost" };
      return { ...state, dateIndex: index, mode: state.mode === "frost" ? "health" : state.mode };
    }
    case "select":
      return { ...state, selectedId: action.id, expanded: action.id ? state.expanded && action.id === state.selectedId : false };
    case "view":
      return { ...state, view: action.view };
    case "expand":
      return { ...state, expanded: action.on && state.selectedId !== null };
    case "init":
      return { ...gardenReducer(dates, { ...state, mode: "health", dateIndex: latestScanIndex(dates) }, { type: "mode", mode: action.mode ?? "health" }), initialised: true };
  }
}

/** The plant the garden opens on: the one asked for, else the first that needs attention, else the first. */
export function defaultSelection(plants: ScenePlant[], asked?: string): string | null {
  if (asked && plants.some((p) => p.id === asked)) return asked;
  const rank: Record<string, number> = { sick: 0, watch: 1, dead: 2, healthy: 3, unknown: 4 };
  const sorted = [...plants].sort((a, b) => (rank[a.status] ?? 9) - (rank[b.status] ?? 9));
  return sorted[0]?.id ?? null;
}

// --- Words ---------------------------------------------------------------------

export interface TopCard {
  title: string;
  text: string;
}

const count = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** "2 sick, 1 to watch, 3 healthy" (the mockup's Health card), leaving out the statuses no plant has. */
export function statusMix(plants: ScenePlant[]): string {
  const n = (s: string) => plants.filter((p) => p.status === s).length;
  const parts = [
    n("sick") ? `${n("sick")} sick` : "",
    n("watch") ? `${n("watch")} to watch` : "",
    n("healthy") ? `${n("healthy")} healthy` : "",
    n("dead") ? `${n("dead")} dead` : "",
    n("unknown") ? `${n("unknown")} not scanned yet` : "",
  ].filter(Boolean);
  return parts.join(", ");
}

/**
 * The card from the top: what the mode shows (design 3.1). Health counts the statuses on the date, Growth names
 * the tallest measured plant, Frost the night's low and the tender plants to cover.
 */
export function topCard(args: {
  mode: GardenMode;
  gardenName: string;
  plants: ScenePlant[];
  records: GardenPlantRecord[];
  date: SceneDate;
  forecast: readonly ForecastDay[];
  mappedOn?: string;
}): TopCard {
  const { mode, plants, date } = args;
  if (mode === "frost") {
    const night = date.forecast ? args.forecast.find((d) => d.date === date.date) : undefined;
    if (!night) return { title: "No frost in the forecast", text: "Frost shows the next frost night when one is forecast." };
    const tender = plants.filter((p) => p.tender);
    const day = weekdayLong(night.date);
    const low = `Low of ${formatTemp(night.t_min_c)}.`;
    if (tender.length === 0) return { title: `Frost on ${day} night`, text: `${low} None of the mapped plants is known to be tender.` };
    return { title: `Frost on ${day} night`, text: `${low} Cover the ${count(tender.length, "tender plant", "tender plants")} by ${day} evening.` };
  }
  if (mode === "growth") {
    const measured = plants.filter((p) => p.height_m !== undefined);
    if (measured.length === 0) return { title: `Heights on ${date.label}`, text: "No plant was measured by then. A whole-plant scan with depth adds a height." };
    const tallest = measured.reduce((a, b) => ((b.height_m ?? 0) > (a.height_m ?? 0) ? b : a));
    const missing = plants.length - measured.length;
    const rest = missing > 0 ? ` ${count(missing, "plant isn't", "plants aren't")} measured yet.` : "";
    return {
      title: `Heights, measured ${date.label}`,
      text: `From each scan's depth. ${tallest.name} is the tallest at ${Math.round((tallest.height_m ?? 0) * 100)} cm.${rest}`,
    };
  }
  const mix = statusMix(plants);
  const mapped = args.mappedOn ? ` Mapped ${args.mappedOn}.` : "";
  return { title: `${args.gardenName} · ${count(plants.length, "plant", "plants")}`, text: `${mix ? `${mix} on ${date.label}.` : "No plants on the map yet."}${mapped}` };
}

/** "−1 °C", with a true minus sign. */
export function formatTemp(c: number): string {
  const n = Math.round(c);
  return `${n < 0 ? "−" : ""}${Math.abs(n)} °C`;
}

// --- The map's geometry --------------------------------------------------------

/** The distance between two pins, to the nearest 5 cm (design 3.1: pins are tapped in AR). */
export function distanceCm(a: { x_m: number; y_m: number }, b: { x_m: number; y_m: number }): number {
  const cm = Math.hypot(a.x_m - b.x_m, a.y_m - b.y_m) * 100;
  return Math.max(5, Math.round(cm / 5) * 5);
}

/** The nearest other plant on the map, and its distance. */
export function nearestPlant(plants: ScenePlant[], id: string): { plant: ScenePlant; cm: number } | undefined {
  const me = plants.find((p) => p.id === id);
  if (!me) return undefined;
  let best: { plant: ScenePlant; cm: number } | undefined;
  for (const p of plants) {
    if (p.id === id) continue;
    const cm = distanceCm(me, p);
    if (!best || cm < best.cm) best = { plant: p, cm };
  }
  return best;
}

/** "40 cm" or "1.2 m". */
export function formatDistance(cm: number): string {
  return cm < 100 ? `${cm} cm` : `${(cm / 100).toFixed(cm % 100 === 0 ? 0 : 1)} m`;
}

export interface MapFrame {
  /** Points per metre. */
  scale: number;
  /** The map's corner at the frame's top left, metres. */
  ox: number;
  oy: number;
  width: number;
  height: number;
}

/** Fits the beds and pins into a `width` × `height` box with a margin, keeping metres square. */
export function fitWholeMap(map: Pick<GardenMap, "beds" | "pins">, width: number, height: number, margin = 28): MapFrame {
  const xs: number[] = [];
  const ys: number[] = [];
  for (const b of map.beds) {
    for (const [x = 0, y = 0] of b.polygon_m) {
      xs.push(x);
      ys.push(y);
    }
  }
  for (const p of map.pins) {
    const r = p.canopy_radius_m ?? 0;
    xs.push(p.x_m - r, p.x_m + r);
    ys.push(p.y_m - r, p.y_m + r);
  }
  if (xs.length === 0) return { scale: 40, ox: 0, oy: 0, width, height };
  const minX = Math.min(...xs);
  const maxX = Math.max(...xs);
  const minY = Math.min(...ys);
  const maxY = Math.max(...ys);
  const w = Math.max(maxX - minX, 1);
  const h = Math.max(maxY - minY, 1);
  const scale = Math.min((width - margin * 2) / w, (height - margin * 2) / h);
  // Centred in the box.
  const ox = minX - (width / scale - w) / 2;
  const oy = minY - (height / scale - h) / 2;
  return { scale, ox, oy, width, height };
}

/** The timeline's steps for the plant's card: each scrubber date with the plant's status then, or none before its first scan. */
export function plantSteps(record: GardenPlantRecord | undefined, dates: SceneDate[]): { date: SceneDate; status?: PlantStatus }[] {
  return dates.map((d) => {
    if (!record) return { date: d };
    if (d.forecast) return { date: d };
    const m = measurementAt(record, d.date);
    return m ? { date: d, status: m.status } : { date: d };
  });
}

/** The selected plant's note on the date: its primary problem then, or "No problems found". */
export function plantNote(record: GardenPlantRecord | undefined, date: SceneDate): string | undefined {
  if (!record) return undefined;
  // The forecast night shows the latest scan before it.
  const m = measurementAt(record, date.date);
  if (!m) return "Not scanned by then";
  const problem = record.problems[m.date];
  if (problem) return problem;
  return m.status === "healthy" ? "No problems found" : undefined;
}

/** A label's box on the map, in points. */
export interface LabelBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

const overlaps = (a: LabelBox, b: LabelBox) => a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;

/**
 * Where each pin's label goes: below the pin, or above it when the box below would cover a label already placed or
 * another pin (Tomato 1 and 2 are 40 cm apart). Pins are placed left to right; one that fits neither way keeps
 * below. `width` is the label's width; `pin` the pin's diameter and `gap` the space between pin and label.
 */
export function labelsAbove(
  pins: { id: string; x: number; y: number; width: number }[],
  size: { pin: number; label: number; gap: number },
): Set<string> {
  const sorted = [...pins].sort((a, b) => a.x - b.x);
  const placed: LabelBox[] = [];
  const pinBoxes = sorted.map((p) => ({ id: p.id, box: { x: p.x - size.pin / 2, y: p.y - size.pin / 2, width: size.pin, height: size.pin } }));
  const above = new Set<string>();
  for (const p of sorted) {
    const box = (up: boolean): LabelBox => ({
      x: p.x - p.width / 2,
      y: up ? p.y - size.pin / 2 - size.gap - size.label : p.y + size.pin / 2 + size.gap,
      width: p.width,
      height: size.label,
    });
    const clear = (b: LabelBox) => !placed.some((q) => overlaps(b, q)) && !pinBoxes.some((q) => q.id !== p.id && overlaps(b, q.box));
    const up = !clear(box(false)) && clear(box(true));
    if (up) above.add(p.id);
    placed.push(box(up));
  }
  return above;
}

/** A label chip's width in points: about 6.4 pt a character at caption 2 semibold, plus its padding. */
export function labelWidth(text: string): number {
  return Math.ceil(text.length * 6.4) + 16;
}
