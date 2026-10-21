import { toE164 } from "./phone";

describe("toE164", () => {
  it("adds +1 to a ten-digit North American number", () => {
    expect(toE164("(555) 555-0100")).toBe("+15555550100");
    expect(toE164("1 555 555 0100")).toBe("+15555550100");
  });

  it("keeps an international number", () => {
    expect(toE164("+44 20 7946 0958")).toBe("+442079460958");
  });

  it("refuses what can't be a number", () => {
    expect(toE164("12345")).toBeNull();
    expect(toE164("")).toBeNull();
  });
});
