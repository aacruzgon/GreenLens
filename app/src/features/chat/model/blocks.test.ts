import type { AnswerBlock } from "@/domain/proposed";
import type { ForecastDay, MeasurementPoint } from "@/domain/types";

import { blockSummary, coverRequest, defaultCover, fitMap, focusDistance, frostSummary, hasTrend, humidHoursText, inFrame, pinDistanceCm, plot, trendWord } from "./blocks";

const night: ForecastDay = { date: "2026-10-29", t_min_c: -1, t_max_c: 6, mean_rh_pct: 70, hours_rh_over_90: 3, precip_mm: 0, frost_risk: true };

const planner: Extract<AnswerBlock, { kind: "frost_planner" }> = {
  kind: "frost_planner",
  call_id: "c",
  nights: [night],
  plants: [
    { plant_id: "t1", name: "Tomato 1", status: "sick", frost_tender: true },
    { plant_id: "b", name: "Basil", status: "healthy", frost_tender: true },
    { plant_id: "k", name: "Kale", status: "healthy", frost_tender: false },
  ],
};

const point = (daysAgo: number, value: number, band: MeasurementPoint["confidence_band"] = "high"): MeasurementPoint => ({
  scan_id: `s${daysAgo}`,
  quantity: "plant_height_cm",
  value,
  confidence_band: band,
  created_at: new Date(Date.UTC(2026, 9, 23) - daysAgo * 86_400_000).toISOString().replace(/\.\d{3}Z$/, "Z"),
});

describe("the frost cover planner (design 5)", () => {
  it("starts with the tender plants chosen", () => {
    expect(defaultCover(planner)).toEqual(["t1", "b"]);
  });

  it("updates its summary with the choice", () => {
    expect(frostSummary(planner, ["t1", "b"])).toBe("Cover 2 plants on Thursday evening: Tomato 1 and Basil.");
    expect(frostSummary(planner, ["t1"])).toBe("Cover 1 plant on Thursday evening: Tomato 1. Basil stays uncovered.");
    expect(frostSummary(planner, [])).toBe("No plants chosen to cover.");
  });

  it("asks for the task in words, so the agent answers with a card", () => {
    expect(coverRequest(planner, ["t1", "k"])).toBe("Add a task to cover Tomato 1 and Kale on Thursday evening.");
  });
});

describe("the measurement trend (analytics 2)", () => {
  it("needs 3 points over 14 days, of medium band or more", () => {
    expect(hasTrend([point(27, 32), point(20, 41), point(13, 50)])).toBe(true);
    expect(hasTrend([point(10, 32), point(5, 41), point(0, 50)])).toBe(false);
    expect(hasTrend([point(27, 32), point(20, 41, "low"), point(13, 50)])).toBe(false);
  });

  it("says higher only when the noise bands don't overlap", () => {
    expect(trendWord([point(27, 32), point(20, 41), point(0, 62)])).toBe("higher");
    expect(trendWord([point(27, 50), point(20, 51), point(0, 52)])).toBe("about the same");
    expect(trendWord([point(2, 32), point(1, 41), point(0, 62)])).toBeUndefined();
  });

  it("plots oldest left and highest at the top, with no point past the last", () => {
    const pts = plot([point(27, 32), point(0, 62)], 100, 50);
    expect(pts[0]!.x).toBe(0);
    expect(pts[1]!.x).toBe(100);
    expect(pts[1]!.y).toBeLessThan(pts[0]!.y);
  });
});

describe("the garden map snippet", () => {
  const block: Extract<AnswerBlock, { kind: "garden_map" }> = {
    kind: "garden_map",
    call_id: "c",
    map: {
      garden_id: "g",
      version: 1,
      created_at: "2026-10-01T10:00:00Z",
      beds: [],
      pins: [
        { plant_id: "t1", x_m: 4.8, y_m: 2 },
        { plant_id: "t2", x_m: 5.21, y_m: 2 },
      ],
    },
    plants: [
      { plant_id: "t1", name: "Tomato 1", status: "sick", height_cm: 62 },
      { plant_id: "t2", name: "Tomato 2", status: "watch" },
    ],
    focus_plant_ids: ["t1", "t2"],
    neighbours: {},
  };

  it("gives distances to the nearest 5 cm, since pins are tapped in AR", () => {
    expect(pinDistanceCm(block.map.pins[0]!, block.map.pins[1]!)).toBe(40);
    expect(focusDistance(block)).toBe("Tomato 1 and Tomato 2 are 40 cm apart.");
  });

  it("frames the snippet around the focus plants, so 40 cm apart isn't one pin (found on the simulator)", () => {
    const wide = { ...block.map, pins: [...block.map.pins, { plant_id: "far", x_m: 0.5, y_m: 7 }] };
    const whole = fitMap(wide, 300);
    const snippet = fitMap(wide, 300, ["t1", "t2"]);
    const apart = (f: ReturnType<typeof fitMap>) => 0.41 * f.scale;
    expect(apart(whole)).toBeLessThan(28);
    expect(apart(snippet)).toBeGreaterThan(56);
    expect(inFrame(snippet, 4.8, 2)).toBe(true);
    expect(inFrame(snippet, 0.5, 7)).toBe(false);
  });

  it("summarises every block for VoiceOver", () => {
    expect(blockSummary(block)).toBe("Garden map with 0 beds and 2 plants. Tomato 1 and Tomato 2 are 40 cm apart.");
    expect(blockSummary(planner)).toBe("Frost cover planner. Frost on Thursday night, low of -1 °C. Tender plants: Tomato 1 and Basil.");
    expect(blockSummary({ kind: "weather_hours", call_id: "c", days: [night] })).toBe("Humid hours, 1 days: Thursday 3.");
    expect(humidHoursText({ ...night, hours_rh_over_90: 1 })).toBe("1 hour over 90% humidity");
  });
});
