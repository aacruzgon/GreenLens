import { gateShowsError, pickActiveGarden } from "./activeGarden";

describe("pickActiveGarden", () => {
  const gardens = [{ id: "g1" }, { id: "g2" }];

  it("keeps the remembered garden while the user belongs to it", () => {
    expect(pickActiveGarden(gardens, "g2")).toBe("g2");
  });

  it("falls back to the first garden", () => {
    expect(pickActiveGarden(gardens, "gone")).toBe("g1");
    expect(pickActiveGarden(gardens, null)).toBe("g1");
  });

  it("has none without gardens", () => {
    expect(pickActiveGarden([], "g1")).toBeNull();
  });
});

describe("gateShowsError", () => {
  it("shows the error when a first load failed", () => {
    expect(gateShowsError({ error: new Error("x"), data: undefined }, { error: null, data: [] })).toBe(true);
    expect(gateShowsError({ error: null, data: {} }, { error: new Error("x"), data: undefined })).toBe(true);
  });

  it("keeps the app when a background refetch failed with cached data", () => {
    expect(gateShowsError({ error: new Error("x"), data: {} }, { error: new Error("y"), data: [] })).toBe(false);
  });

  it("shows nothing without an error", () => {
    expect(gateShowsError({ error: null, data: undefined }, { error: null, data: undefined })).toBe(false);
  });
});
