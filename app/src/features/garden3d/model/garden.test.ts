import type { TimelineEntryWithStatus } from "@/domain/proposed";
import { statusOfEntry } from "@/domain/plantStatus";

import {
  defaultSelection,
  distanceCm,
  formatDistance,
  formatTemp,
  gardenDates,
  gardenReducer,
  initialGardenState,
  labelsAbove,
  nearestPlant,
  plantRecords,
  statusMix,
  topCard,
  type GardenState,
} from "./garden";
import type { ScenePlant } from "./gardenScene";

const TZ = "America/Toronto";
const entry = (date: string, extra: Partial<TimelineEntryWithStatus> = {}): TimelineEntryWithStatus => ({
  scan_id: `scan_${date}`,
  created_at: `${date}T14:40:00Z`,
  status: "planned",
  ...extra,
});

const plant = (id: string, status: ScenePlant["status"], extra: Partial<ScenePlant> = {}): ScenePlant => ({ id, name: id, x_m: 0, y_m: 0, status, ...extra });

describe("statusOfEntry", () => {
  it("takes plant_status when sent, and guesses from the primary problem otherwise", () => {
    expect(statusOfEntry(entry("2026-10-11", { plant_status: "sick" }), "healthy", false)).toBe("sick");
    expect(statusOfEntry(entry("2026-10-11"), "sick", true)).toBe("sick");
    expect(statusOfEntry(entry("2026-10-04"), "sick", false)).toBe("healthy");
    expect(statusOfEntry(entry("2026-10-04", { primary: { problem_id: "aphids", name: "Aphids" } }), "healthy", false)).toBe("watch");
    expect(statusOfEntry(entry("2026-10-04", { primary: { problem_id: "aphids", name: "Aphids" } }), "sick", false)).toBe("sick");
  });
});

describe("plantRecords and gardenDates", () => {
  const plants = [
    { id: "t1", garden_id: "g", name: "Tomato 1", species_status: "confirmed", species_candidates: [], status: "sick", created_at: "2026-09-01T00:00:00Z", species: { id: "tomato", scientific_name: "S", common_name: "Tomato", frost: "tender" } },
    { id: "k", garden_id: "g", name: "Kale", species_status: "confirmed", species_candidates: [], status: "healthy", created_at: "2026-09-01T00:00:00Z" },
  ] as const;
  // Newest first, as the API sends a timeline.
  const timelines = {
    t1: [
      entry("2026-10-11", { plant_status: "sick", primary: { problem_id: "tomato_early_blight", name: "Early blight" }, measurements: { plant_height_cm: 58 } }),
      entry("2026-10-04", { plant_status: "healthy", measurements: { plant_height_cm: 50, canopy_width_cm: 40 } }),
      entry("2026-10-03", { status: "failed" }),
    ],
    k: [entry("2026-09-20", { plant_status: "healthy" })],
  };
  // The proposed `frost` on the species, as the mock sends it.
  const records = plantRecords(plants as never, timelines, TZ);

  it("keeps each scan's status and sizes oldest first, skipping failed scans", () => {
    const t1 = records.find((r) => r.id === "t1")!;
    expect(t1.history.map((m) => [m.date, m.status, m.height_cm])).toEqual([
      ["2026-10-04", "healthy", 50],
      ["2026-10-11", "sick", 58],
    ]);
    expect(t1.tender).toBe(true);
    expect(t1.problems["2026-10-11"]).toBe("Early blight");
    expect(records.find((r) => r.id === "k")!.frost).toBeUndefined();
  });

  it("runs from the first scan to the forecast's frost night, a step per scan date", () => {
    const forecast = [
      { date: "2026-10-28", t_min_c: 4, t_max_c: 12, frost_risk: false, mean_rh_pct: 70, precip_mm: 0, hours_rh_over_90: 0 },
      { date: "2026-10-29", t_min_c: -1, t_max_c: 6, frost_risk: true, mean_rh_pct: 70, precip_mm: 0, hours_rh_over_90: 0 },
    ];
    const dates = gardenDates(records, forecast, "2026-10-23");
    expect(dates.map((d) => d.label)).toEqual(["Sep 20", "Oct 4", "Oct 11", "Thu"]);
    expect(dates.at(-1)!.forecast).toBe(true);
    expect(gardenDates(records, [], "2026-10-23").some((d) => d.forecast)).toBe(false);
  });
});

describe("gardenReducer", () => {
  const dates = [
    { date: "2026-09-20", label: "Sep 20" },
    { date: "2026-10-17", label: "Oct 17" },
    { date: "2026-10-29", label: "Thu", forecast: true },
  ];
  const start: GardenState = initialGardenState(dates, { view: "3d" });

  it("opens on the latest scan in Health", () => {
    expect(start).toMatchObject({ mode: "health", dateIndex: 1, view: "3d" });
  });

  it("chooses Frost when the date reaches the forecast, and leaves it for Health when the date moves off", () => {
    const frost = gardenReducer(dates, start, { type: "date", index: 2 });
    expect(frost).toMatchObject({ mode: "frost", dateIndex: 2 });
    expect(gardenReducer(dates, frost, { type: "date", index: 0 })).toMatchObject({ mode: "health", dateIndex: 0 });
  });

  it("jumps to the forecast on Frost, and back to the latest scan on Growth", () => {
    const frost = gardenReducer(dates, start, { type: "mode", mode: "frost" });
    expect(frost.dateIndex).toBe(2);
    expect(gardenReducer(dates, frost, { type: "mode", mode: "growth" })).toMatchObject({ mode: "growth", dateIndex: 1 });
  });

  it("shrinks the card when the selection changes, and can't grow one with nothing selected", () => {
    const open = gardenReducer(dates, { ...start, selectedId: "a" }, { type: "expand", on: true });
    expect(open.expanded).toBe(true);
    expect(gardenReducer(dates, open, { type: "select", id: "b" }).expanded).toBe(false);
    expect(gardenReducer(dates, start, { type: "expand", on: true }).expanded).toBe(false);
  });

  it("opens Frost on the forecast night when asked", () => {
    expect(initialGardenState(dates, { view: "map", mode: "frost" })).toMatchObject({ mode: "frost", dateIndex: 2, view: "map" });
  });
});

describe("words and distances", () => {
  const plants = [
    plant("Tomato 1", "sick", { x_m: 4.8, y_m: 2, tender: true, height_m: 0.62 }),
    plant("Tomato 2", "healthy", { x_m: 5.2, y_m: 2, tender: true, height_m: 0.7 }),
    plant("Kale", "healthy", { x_m: 2.6, y_m: 6.2 }),
  ];

  it("rounds distances to the nearest 5 cm and finds the nearest plant", () => {
    expect(distanceCm({ x_m: 0, y_m: 0 }, { x_m: 0.42, y_m: 0 })).toBe(40);
    expect(nearestPlant(plants, "Tomato 1")).toEqual({ plant: plants[1], cm: 40 });
    expect(formatDistance(40)).toBe("40 cm");
    expect(formatDistance(150)).toBe("1.5 m");
  });

  it("says what each mode shows", () => {
    const base = { gardenName: "Back garden", plants, records: [], forecast: [{ date: "2026-10-29", t_min_c: -1, t_max_c: 6, frost_risk: true, mean_rh_pct: 70, precip_mm: 0, hours_rh_over_90: 0 }] };
    expect(statusMix(plants)).toBe("1 sick, 2 healthy");
    expect(topCard({ ...base, mode: "health", date: { date: "2026-10-17", label: "Oct 17" }, mappedOn: "Sep 23" })).toEqual({
      title: "Back garden · 3 plants",
      text: "1 sick, 2 healthy on Oct 17. Mapped Sep 23.",
    });
    expect(topCard({ ...base, mode: "growth", date: { date: "2026-10-17", label: "Oct 17" } }).text).toBe(
      "From each scan's depth. Tomato 2 is the tallest at 70 cm. 1 plant isn't measured yet.",
    );
    expect(topCard({ ...base, mode: "frost", date: { date: "2026-10-29", label: "Thu", forecast: true } })).toEqual({
      title: "Frost on Thursday night",
      text: "Low of −1 °C. Cover the 2 tender plants by Thursday evening.",
    });
    expect(formatTemp(3.4)).toBe("3 °C");
  });

  it("opens on the plant asked for, else the first that needs attention", () => {
    expect(defaultSelection(plants, "Kale")).toBe("Kale");
    expect(defaultSelection(plants, "missing")).toBe("Tomato 1");
    expect(defaultSelection([])).toBeNull();
  });
});

describe("labelsAbove", () => {
  it("lifts the label of a pin crowded by its left neighbour, and only that one", () => {
    const above = labelsAbove([
      { id: "t2", x: 116, y: 50 },
      { id: "t1", x: 100, y: 50 },
      { id: "basil", x: 110, y: 120 },
      { id: "kale", x: 300, y: 50 },
    ], 42);
    expect([...above]).toEqual(["t2"]);
  });
});
