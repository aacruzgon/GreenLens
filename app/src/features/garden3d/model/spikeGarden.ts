// The spike's gardens (roadmap F5 item 3): the mock garden's map from src/api/mock/fixtures.ts (2 beds, 6 plants,
// the pepper in a pot), its plants over four dates, and a generated 10 beds × 4 plants for the performance check.
import type { GardenMap, PlantStatus } from "@/domain/types";

import type { PlantMeasurement, PlantRecord, SceneDate } from "./gardenScene";

/** Three scans and the forecast night (Thursday). */
export const SPIKE_DATES: SceneDate[] = [
  { date: "2026-09-20", label: "Sep 20" },
  { date: "2026-10-04", label: "Oct 4" },
  { date: "2026-10-17", label: "Oct 17" },
  { date: "2026-10-29", label: "Thu, Oct 29", forecast: true },
];

type SpikeMap = Pick<GardenMap, "beds" | "pins">;

/** As in fixtures.ts `buildSeed().map`. */
export const MOCK_MAP: SpikeMap = {
  beds: [
    { id: "bed_east", name: "East bed", polygon_m: [[4, 1], [6, 1], [6, 5], [4, 5]], area_m2: 8 },
    { id: "bed_south", name: "South bed", polygon_m: [[0.5, 5.5], [4, 5.5], [4, 7], [0.5, 7]], area_m2: 5.25 },
  ],
  pins: [
    { plant_id: "plant_tomato_1", x_m: 4.8, y_m: 2.0, canopy_radius_m: 0.25, bed_id: "bed_east" },
    { plant_id: "plant_tomato_2", x_m: 5.2, y_m: 2.0, canopy_radius_m: 0.25, bed_id: "bed_east" },
    { plant_id: "plant_pepper", x_m: 2.8, y_m: 3.4, canopy_radius_m: 0.2 },
    { plant_id: "plant_basil", x_m: 5.0, y_m: 3.6, canopy_radius_m: 0.15, bed_id: "bed_east" },
    { plant_id: "plant_lettuce", x_m: 1.2, y_m: 6.2, canopy_radius_m: 0.15, bed_id: "bed_south" },
    { plant_id: "plant_kale", x_m: 2.6, y_m: 6.2, canopy_radius_m: 0.2, bed_id: "bed_south" },
  ],
};

const SCANS = SPIKE_DATES.filter((d) => !d.forecast).map((d) => d.date);

function history(heights: number[], widths: number[], statuses: PlantStatus[]): PlantMeasurement[] {
  return SCANS.map((date, i) => ({ date, height_cm: heights[i], width_cm: widths[i], status: statuses[i] }));
}

/** Heights and widths from fixtures.ts (its 1st, 3rd and 5th scans); Tomato 1's blight and the pepper's aphids. */
export const MOCK_PLANTS: PlantRecord[] = [
  { id: "plant_tomato_1", name: "Tomato 1", tender: true, history: history([32, 50, 62], [30, 40, 48], ["healthy", "healthy", "sick"]) },
  { id: "plant_tomato_2", name: "Tomato 2", tender: true, history: history([28, 43, 56], [26, 35, 44], ["healthy", "healthy", "healthy"]) },
  { id: "plant_pepper", name: "Pepper", tender: true, history: history([18, 30, 38], [15, 23, 29], ["healthy", "watch", "watch"]) },
  { id: "plant_basil", name: "Basil", tender: true, history: history([12, 20, 26], [14, 22, 28], ["healthy", "healthy", "healthy"]) },
  { id: "plant_lettuce", name: "Lettuce", tender: false, history: history([8, 14, 19], [12, 20, 25], ["healthy", "healthy", "healthy"]) },
  { id: "plant_kale", name: "Kale", tender: false, history: history([15, 26, 34], [18, 30, 38], ["healthy", "healthy", "healthy"]) },
];

const KINDS = [
  { name: "Tomato", tender: true, h: [30, 48, 60], w: [28, 38, 46] },
  { name: "Pepper", tender: true, h: [18, 28, 36], w: [16, 22, 28] },
  { name: "Basil", tender: true, h: [12, 19, 25], w: [14, 21, 27] },
  { name: "Lettuce", tender: false, h: [8, 13, 18], w: [12, 19, 24] },
  { name: "Kale", tender: false, h: [15, 25, 33], w: [18, 28, 36] },
] as const;

const LAST_STATUS: PlantStatus[] = ["healthy", "healthy", "watch", "healthy", "sick", "healthy", "healthy", "dead"];

/** 10 beds of 1.2 × 2.4 m in two rows of five, 4 plants each: the performance check's garden (garden-3d 3). */
export function largeGarden(bedCount = 10, perBed = 4): { map: SpikeMap; plants: PlantRecord[] } {
  const beds: SpikeMap["beds"] = [];
  const pins: SpikeMap["pins"] = [];
  const plants: PlantRecord[] = [];
  const counts = new Map<string, number>();
  for (let b = 0; b < bedCount; b++) {
    const x0 = (b % 5) * 1.8;
    const y0 = Math.floor(b / 5) * 3.2;
    const id = `bed_${b + 1}`;
    beds.push({ id, name: `Bed ${b + 1}`, polygon_m: [[x0, y0], [x0 + 1.2, y0], [x0 + 1.2, y0 + 2.4], [x0, y0 + 2.4]], area_m2: 2.88 });
    for (let p = 0; p < perBed; p++) {
      const n = b * perBed + p;
      const kind = KINDS[n % KINDS.length];
      const count = (counts.get(kind.name) ?? 0) + 1;
      counts.set(kind.name, count);
      const plantId = `plant_${n + 1}`;
      pins.push({ plant_id: plantId, x_m: x0 + 0.6, y_m: y0 + 0.3 + p * (1.8 / Math.max(1, perBed - 1)), canopy_radius_m: 0.2, bed_id: id });
      const last = LAST_STATUS[n % LAST_STATUS.length];
      plants.push({
        id: plantId,
        name: `${kind.name} ${count}`,
        tender: kind.tender,
        history: history([...kind.h], [...kind.w], ["healthy", last === "dead" ? "sick" : "healthy", last]),
      });
    }
  }
  return { map: { beds, pins }, plants };
}
