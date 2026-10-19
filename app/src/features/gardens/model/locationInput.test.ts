import { locationFromDevice, locationFromText } from "./locationInput";

describe("locationFromText", () => {
  it("sends a Canadian postal code, normalised", () => {
    expect(locationFromText("m5v2t6")).toEqual({ postal_code: "M5V 2T6" });
    expect(locationFromText(" M5V 2T6 ")).toEqual({ postal_code: "M5V 2T6" });
  });

  it("sends anything else as a city", () => {
    expect(locationFromText("Guelph, ON")).toEqual({ city: "Guelph, ON" });
  });

  it("sends nothing for an empty field", () => {
    expect(locationFromText("   ")).toBeNull();
  });
});

describe("locationFromDevice", () => {
  it("keeps the chosen precision", () => {
    expect(locationFromDevice({ lat: 43.65, lon: -79.38 }, "about_1_km")).toEqual({
      lat: 43.65,
      lon: -79.38,
      precision: "about_1_km",
    });
  });
});
