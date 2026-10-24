// The 3D garden's props from a garden map and each plant's measurements (docs/design/garden-3d.md 1; roadmap F5
// items 3–4). Pure TypeScript: the screen passes the result to the native GardenView, which tweens to it. A date
// change only changes heights, widths and statuses here, so the native side never rebuilds geometry for it.
import type { GardenMap, PlantStatus } from "@/domain/types";

export type GardenMode = "health" | "growth" | "frost";
export type SceneStatus = PlantStatus | "unknown";

/** `GardenViewBed`'s shape. */
export interface SceneBed {
  id: string;
  name?: string;
  polygon_m: number[][];
}

/** `GardenViewPlant`'s shape. */
export interface ScenePlant {
  id: string;
  name: string;
  x_m: number;
  y_m: number;
  canopy_radius_m?: number;
  height_m?: number;
  status: SceneStatus;
  tender?: boolean;
  bed_id?: string;
}

export interface PlantMeasurement {
  /** ISO date, `YYYY-MM-DD`. */
  date: string;
  height_cm?: number;
  width_cm?: number;
  status: PlantStatus;
}

/** A plant and its measurements over time, oldest first. */
export interface PlantRecord {
  id: string;
  name: string;
  /** Needs covering on a frost night. */
  tender: boolean;
  history: PlantMeasurement[];
}

export interface SceneDate {
  date: string;
  label: string;
  /** The forecast night, not a scan: Frost shows it. */
  forecast?: boolean;
}

/** The latest measurement on or before `date`; undefined before the first. */
export function measurementAt(record: PlantRecord, date: string): PlantMeasurement | undefined {
  let found: PlantMeasurement | undefined;
  for (const m of record.history) {
    if (m.date <= date && (!found || m.date >= found.date)) found = m;
  }
  return found;
}

/** The beds and each pinned plant's values on `date`. Pins without a record are left out. */
export function buildScene(map: Pick<GardenMap, "beds" | "pins">, records: PlantRecord[], date: string): { beds: SceneBed[]; plants: ScenePlant[] } {
  const byId = new Map(records.map((r) => [r.id, r]));
  const beds = map.beds.map((b) => ({ id: b.id, ...(b.name ? { name: b.name } : {}), polygon_m: b.polygon_m }));
  const plants: ScenePlant[] = [];
  for (const pin of map.pins) {
    const record = byId.get(pin.plant_id);
    if (!record) continue;
    const m = measurementAt(record, date);
    const canopy = m?.width_cm !== undefined ? m.width_cm / 200 : pin.canopy_radius_m;
    plants.push({
      id: record.id,
      name: record.name,
      x_m: pin.x_m,
      y_m: pin.y_m,
      ...(canopy !== undefined ? { canopy_radius_m: canopy } : {}),
      ...(m?.height_cm !== undefined ? { height_m: m.height_cm / 100 } : {}),
      status: m?.status ?? "unknown",
      tender: record.tender,
      ...(pin.bed_id ? { bed_id: pin.bed_id } : {}),
    });
  }
  return { beds, plants };
}

/** "2 beds, 6 plants, 2 need attention" (garden-3d 6: the summary line above the view). */
export function sceneSummary(scene: { beds: SceneBed[]; plants: ScenePlant[] }): string {
  const attention = scene.plants.filter((p) => p.status === "watch" || p.status === "sick").length;
  const n = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;
  const parts = [n(scene.beds.length, "bed", "beds"), n(scene.plants.length, "plant", "plants")];
  if (attention > 0) parts.push(`${attention} ${attention === 1 ? "needs" : "need"} attention`);
  return parts.join(", ");
}

/** "Tomato 1, sick, 62 cm tall" (garden-3d 1, accessibility). */
export function plantSpoken(plant: ScenePlant): string {
  const parts = [plant.name, plant.status === "unknown" ? "status unknown" : plant.status];
  if (plant.height_m !== undefined) parts.push(`${Math.round(plant.height_m * 100)} cm tall`);
  return parts.join(", ");
}

// --- The spike screen's state ------------------------------------------------

export interface SpikeState {
  mode: GardenMode;
  dateIndex: number;
  large: boolean;
  selectedId: string | null;
}

export type SpikeAction =
  | { type: "mode"; mode: GardenMode }
  | { type: "date"; index: number }
  | { type: "large"; on: boolean }
  | { type: "select"; id: string | null };

/**
 * The modes follow the date (design.md, the 3D garden): Health and Growth show the latest scan, Frost shows the
 * forecast night, and moving the date to the forecast chooses Frost.
 */
export function spikeReducer(dates: SceneDate[], state: SpikeState, action: SpikeAction): SpikeState {
  const forecast = dates.findIndex((d) => d.forecast);
  const latestScan = forecast > 0 ? forecast - 1 : dates.length - 1;
  switch (action.type) {
    case "mode": {
      if (action.mode === "frost") return { ...state, mode: "frost", dateIndex: forecast >= 0 ? forecast : state.dateIndex };
      const dateIndex = state.dateIndex === forecast ? latestScan : state.dateIndex;
      return { ...state, mode: action.mode, dateIndex };
    }
    case "date": {
      const index = Math.max(0, Math.min(dates.length - 1, action.index));
      if (index === forecast) return { ...state, dateIndex: index, mode: "frost" };
      return { ...state, dateIndex: index, mode: state.mode === "frost" ? "health" : state.mode };
    }
    case "large":
      return { ...state, large: action.on, selectedId: null };
    case "select":
      return { ...state, selectedId: action.id };
  }
}
