import { emptyMapping, mappingReducer, openBedArea, sessionSummary, toMapInput, undoRemovesMarker, type MappingEvent, type MappingState } from "./session";

const run = (events: MappingEvent[], from: MappingState = emptyMapping) => events.reduce(mappingReducer, from);

const square: MappingEvent[] = [
  { type: "corner", at: [0, 0] },
  { type: "corner", at: [2, 0] },
  { type: "corner", at: [2, 1] },
  { type: "corner", at: [0, 1] },
];

describe("a mapping session (architecture 6.8)", () => {
  it("draws a bed corner by corner and pins plants in it", () => {
    const s = run([
      ...square,
      { type: "closeBed", name: "East bed" },
      { type: "choosePlant", plant: { plantId: "p1", name: "Tomato 1" } },
      { type: "pin", at: [0.5, 0.5], canopy: 0.333 },
      { type: "choosePlant", plant: { newName: " Chard " } },
      { type: "pin", at: [5, 5] },
      { type: "heading", deg: 271.26, accuracy: 15 },
    ]);
    const input = toMapInput(s, 3);
    expect(input.base_version).toBe(3);
    expect(input.beds).toEqual([{ id: s.beds[0].id, name: "East bed", polygon_m: [[0, 0], [2, 0], [2, 1], [0, 1]] }]);
    expect(input.pins[0]).toEqual({ plant_id: "p1", x_m: 0.5, y_m: 0.5, canopy_radius_m: 0.33, bed_id: s.beds[0].id });
    // A plant outside every bed is a pot: no bed_id.
    expect(input.pins[1]).toEqual({ new_plant: { name: "Chard" }, x_m: 5, y_m: 5 });
    expect(input.north_deg).toBe(271.3);
    expect(sessionSummary(s)).toBe("1 bed · 2 plants · north");
  });

  it("needs three corners to close a bed, and shows its area meanwhile", () => {
    const two = run(square.slice(0, 2));
    expect(run([{ type: "closeBed", name: "x" }], two).beds).toHaveLength(0);
    expect(openBedArea(run(square))).toBe("2.0 m²");
  });

  it("leaves north out when the compass is worse than 20°", () => {
    expect(toMapInput(run([{ type: "heading", deg: 10, accuracy: 25 }]), 0).north_deg).toBeUndefined();
  });

  it("moves a plant's pin when it is pinned again", () => {
    const tomato = { type: "choosePlant", plant: { plantId: "p1", name: "Tomato 1" } } as const;
    const s = run([tomato, { type: "pin", at: [1, 1] }, tomato, { type: "pin", at: [2, 2] }]);
    expect(s.pins).toEqual([{ plant: { plantId: "p1", name: "Tomato 1" }, at: [2, 2] }]);
  });

  it("undoes one step at a time, saying when the AR view's marker goes too", () => {
    const closed = run([...square, { type: "closeBed", name: "Bed" }]);
    expect(undoRemovesMarker(closed)).toBe(false);
    const reopened = run([{ type: "undo" }], closed);
    expect(reopened.beds).toHaveLength(0);
    expect(reopened.corners).toHaveLength(4);
    expect(undoRemovesMarker(reopened)).toBe(true);
    expect(run([{ type: "undo" }], reopened).corners).toHaveLength(3);
  });

  it("starts again from nothing when tracking is lost, keeping the compass", () => {
    const s = run([...square, { type: "heading", deg: 90, accuracy: 10 }, { type: "restart" }]);
    expect(s.corners).toEqual([]);
    expect(s.north).toEqual({ deg: 90, accuracy: 10 });
  });

  it("loads the current map to edit it after relocalising", () => {
    const s = run([
      {
        type: "load",
        names: { p1: "Tomato 1" },
        map: { garden_id: "g", version: 3, created_at: "2026-10-23T00:00:00Z", beds: [{ id: "b", polygon_m: [[0, 0], [1, 0], [1, 1]], area_m2: 0.5 }], pins: [{ plant_id: "p1", x_m: 0.5, y_m: 0.2 }] },
      },
    ]);
    expect(toMapInput(s, 3).pins).toEqual([{ plant_id: "p1", x_m: 0.5, y_m: 0.2, bed_id: "b" }]);
  });
});
