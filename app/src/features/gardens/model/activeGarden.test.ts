import { pickActiveGarden } from "./activeGarden";

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
