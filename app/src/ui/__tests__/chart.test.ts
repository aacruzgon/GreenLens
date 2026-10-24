import { DAY_MS, dateAnchor, fittingLabels, isTrend, niceTicks, timeScale } from "../composites/chart";

describe("the chart helpers (analytics 2; HIG Charts, Axes)", () => {
  it("needs 3 points over 14 days for a trend", () => {
    expect(isTrend([0, 7 * DAY_MS, 14 * DAY_MS])).toBe(true);
    expect(isTrend([0, 7 * DAY_MS, 13 * DAY_MS])).toBe(false);
    expect(isTrend([0, 20 * DAY_MS])).toBe(false);
  });

  it("ticks on familiar steps covering the data", () => {
    expect(niceTicks(0, 21)).toEqual([0, 10, 20, 30]);
    expect(niceTicks(30, 64)).toEqual([20, 40, 60, 80]);
    expect(niceTicks(0, 20)).toEqual([0, 10, 20]);
    expect(niceTicks(5, 5)).toEqual([5, 5.5, 6]);
  });

  it("keeps the first and last date labels and drops the ones that collide", () => {
    expect(fittingLabels([0, 20, 100, 110, 200])).toEqual([0, 2, 4]);
    expect(fittingLabels([0, 10])).toEqual([0, 1]);
  });

  it("maps time inside the plot, one instant in the middle", () => {
    const x = timeScale([0, 10], 100, 0);
    expect(x(0)).toBe(10);
    expect(x(10)).toBe(86);
    expect(timeScale([5, 5], 100, 0)(5)).toBe(48);
  });
});

describe("dateAnchor", () => {
  it("centres a date, and ends one at the right edge where centring would cut it", () => {
    expect(dateAnchor(150, 311)).toEqual({ x: 150, textAnchor: "middle" });
    expect(dateAnchor(300, 311)).toEqual({ x: 310, textAnchor: "end" });
  });
});
