import { buildScene, measurementAt, plantSpoken, sceneSummary, spikeReducer, type SpikeState } from "./gardenScene";
import { largeGarden, MOCK_MAP, MOCK_PLANTS, SPIKE_DATES } from "./spikeGarden";

const at = (i: number) => SPIKE_DATES[i].date;

describe("buildScene", () => {
  it("draws the mock garden: 2 beds and 6 plants, the pepper in a pot", () => {
    const scene = buildScene(MOCK_MAP, MOCK_PLANTS, at(2));
    expect(scene.beds).toHaveLength(2);
    expect(scene.plants).toHaveLength(6);
    const pepper = scene.plants.find((p) => p.id === "plant_pepper")!;
    expect(pepper.bed_id).toBeUndefined();
    expect(scene.plants.find((p) => p.id === "plant_tomato_1")?.bed_id).toBe("bed_east");
  });

  it("changes heights and statuses with the date, and keeps places and ids", () => {
    const sep = buildScene(MOCK_MAP, MOCK_PLANTS, at(0));
    const oct = buildScene(MOCK_MAP, MOCK_PLANTS, at(2));
    const t1 = (s: typeof sep) => s.plants.find((p) => p.id === "plant_tomato_1")!;
    expect(t1(sep)).toMatchObject({ height_m: 0.32, canopy_radius_m: 0.15, status: "healthy" });
    expect(t1(oct)).toMatchObject({ height_m: 0.62, canopy_radius_m: 0.24, status: "sick" });
    expect(oct.plants.map((p) => [p.id, p.x_m, p.y_m, p.bed_id])).toEqual(sep.plants.map((p) => [p.id, p.x_m, p.y_m, p.bed_id]));
  });

  it("carries the latest scan to the forecast night", () => {
    const forecast = buildScene(MOCK_MAP, MOCK_PLANTS, at(3));
    expect(forecast).toEqual(buildScene(MOCK_MAP, MOCK_PLANTS, at(2)));
  });

  it("marks a plant with no measurement yet as unknown, with the pin's canopy", () => {
    const scene = buildScene(MOCK_MAP, MOCK_PLANTS, "2026-09-01");
    const kale = scene.plants.find((p) => p.id === "plant_kale")!;
    expect(kale).toMatchObject({ status: "unknown", canopy_radius_m: 0.2 });
    expect(kale.height_m).toBeUndefined();
  });

  it("leaves out a pin with no plant record", () => {
    const scene = buildScene({ ...MOCK_MAP, pins: [...MOCK_MAP.pins, { plant_id: "gone", x_m: 0, y_m: 0 }] }, MOCK_PLANTS, at(1));
    expect(scene.plants).toHaveLength(6);
  });
});

describe("measurementAt", () => {
  it("takes the latest on or before the date, whatever the order", () => {
    const record = { ...MOCK_PLANTS[0], history: [...MOCK_PLANTS[0].history].reverse() };
    expect(measurementAt(record, "2026-10-10")?.date).toBe("2026-10-04");
  });
});

describe("largeGarden", () => {
  it("makes 10 beds of 4 plants, every plant in its bed's polygon", () => {
    const { map, plants } = largeGarden();
    expect(map.beds).toHaveLength(10);
    expect(map.pins).toHaveLength(40);
    expect(plants).toHaveLength(40);
    for (const pin of map.pins) {
      const bed = map.beds.find((b) => b.id === pin.bed_id)!;
      const xs = bed.polygon_m.map((p) => p[0]);
      const ys = bed.polygon_m.map((p) => p[1]);
      expect(pin.x_m).toBeGreaterThan(Math.min(...xs));
      expect(pin.x_m).toBeLessThan(Math.max(...xs));
      expect(pin.y_m).toBeGreaterThan(Math.min(...ys));
      expect(pin.y_m).toBeLessThan(Math.max(...ys));
    }
    expect(new Set(plants.map((p) => p.name)).size).toBe(40);
    const statuses = new Set(buildScene(map, plants, at(2)).plants.map((p) => p.status));
    expect([...statuses].sort()).toEqual(["dead", "healthy", "sick", "watch"]);
  });
});

describe("words", () => {
  it("summarises the garden", () => {
    expect(sceneSummary(buildScene(MOCK_MAP, MOCK_PLANTS, at(2)))).toBe("2 beds, 6 plants, 2 need attention");
    expect(sceneSummary(buildScene(MOCK_MAP, MOCK_PLANTS, at(0)))).toBe("2 beds, 6 plants");
  });

  it("says a plant in words", () => {
    const scene = buildScene(MOCK_MAP, MOCK_PLANTS, at(2));
    expect(plantSpoken(scene.plants[0])).toBe("Tomato 1, sick, 62 cm tall");
  });
});

describe("spikeReducer", () => {
  const start: SpikeState = { mode: "health", dateIndex: 2, large: false, selectedId: "plant_kale" };
  const reduce = (s: SpikeState, a: Parameters<typeof spikeReducer>[2]) => spikeReducer(SPIKE_DATES, s, a);

  it("jumps to the forecast for Frost, and back to the latest scan after", () => {
    const frost = reduce(start, { type: "mode", mode: "frost" });
    expect(frost).toMatchObject({ mode: "frost", dateIndex: 3 });
    expect(reduce(frost, { type: "mode", mode: "growth" })).toMatchObject({ mode: "growth", dateIndex: 2 });
  });

  it("chooses Frost when the date reaches the forecast, and Health when it leaves", () => {
    const frost = reduce(start, { type: "date", index: 3 });
    expect(frost.mode).toBe("frost");
    expect(reduce(frost, { type: "date", index: 1 })).toMatchObject({ mode: "health", dateIndex: 1 });
    expect(reduce({ ...start, mode: "growth" }, { type: "date", index: 0 }).mode).toBe("growth");
  });

  it("clamps the date and clears the selection when the garden changes", () => {
    expect(reduce(start, { type: "date", index: -4 }).dateIndex).toBe(0);
    expect(reduce(start, { type: "large", on: true })).toMatchObject({ large: true, selectedId: null });
  });
});
