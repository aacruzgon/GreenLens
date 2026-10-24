// The insights model against the demo's records (analytics 6) and the rules of analytics 2 and 7. Here, not in
// model/, because it reads the mock fixture, which the pure model may not import.
import { IDS, mockStore, resetMock } from "@/api/mock";
import { insightTimeline, rangePoints } from "@/api/mock/insights";
import type { InsightTimelineEntry, RangePoint } from "@/domain/proposed";
import type { Task } from "@/domain/types";

import {
  afterSteps,
  datesText,
  diagnosisEntry,
  healthHeadline,
  heightReading,
  nearbyLine,
  nearestPlant,
  notCheckedRecently,
  spotsReading,
  stepLanes,
  stripCells,
  weatherReading,
} from "../model/insights";

const TZ = "America/Toronto";
const NOW = new Date("2026-10-17T16:00:00Z");
const DAY = 86_400_000;
const ago = (d: number) => new Date(NOW.getTime() - d * DAY).toISOString().replace(/\.\d+Z$/, "Z");

function pt(d: number, value: number, extra: Partial<RangePoint> = {}): RangePoint {
  return { scan_id: `s${d}`, quantity: "affected_area_pct", value, confidence_band: "high", created_at: ago(d), ...extra };
}

function entry(d: number, extra: Partial<InsightTimelineEntry> = {}): InsightTimelineEntry {
  return { scan_id: `s${d}`, created_at: ago(d), status: "planned", plan_id: `p${d}`, diagnosis_status: "healthy", ...extra };
}

const blight = { problem_id: "early_blight", name: "Early blight" };

describe("the demo's records (analytics 6)", () => {
  beforeEach(() => resetMock(NOW));

  it("draws Tomato 1 healthy at 3 checks, then sick from Oct 11", () => {
    const tl = insightTimeline(mockStore, IDS.tomato1);
    const cells = stripCells(tl, "sick", TZ);
    expect(cells.map((c) => c.status)).toEqual(["healthy", "healthy", "healthy", "sick", "sick", "sick"]);
    expect(healthHeadline(tl, cells, TZ)).toBe("Healthy at 3 checks, then early blight on Oct 11");
    expect(cells[3]!.label).toBe("Oct 11: Sick, early blight");
  });

  it("reads spot cover 14% → 9% → 5% as lower, with its ranges", () => {
    const s = spotsReading(rangePoints(mockStore, IDS.tomato1, "affected_area_pct"), TZ)!;
    expect(s.kpi).toBe("14% → 5%");
    expect(s.direction).toBe("lower");
    expect(s.line).toBe(true);
    expect(s.headline).toBe("Lower at each check since Oct 11");
    expect(s.summary).toBe("Spot cover: 14 percent on Oct 11, 9 percent on Oct 14 and 5 percent on Oct 17, with ranges 9 to 21, 6 to 12 and 3 to 7");
    expect(s.caption).toMatch(/Bars show the range across 3 leaves/);
  });

  it("lists the plan's steps by the dates they were done, Tomato 2's check left out", () => {
    const lanes = stepLanes(mockStore.tasks, IDS.tomato1, TZ);
    expect(lanes.map((l) => [l.title, l.dates])).toEqual([
      ["Remove the infected leaves", "Oct 11"],
      ["Mulch around the base", "Oct 12"],
      ["Check Tomato 1 for new spots", "Oct 14, 17"],
    ]);
  });

  it("gives the height's rate, with the provisional band", () => {
    const h = heightReading(rangePoints(mockStore, IDS.tomato1, "plant_height_cm"), TZ)!;
    expect(h.trend).toBe(true);
    expect(h.headline).toBe("About 62 cm, growing about 8 cm a week");
    expect(h.provisional).toBe(true);
    expect(h.band[0]).toEqual({ lo: 30, hi: 34 });
  });

  it("shows the humid hours up to the diagnosis, the diagnosis' bar highlighted", () => {
    const w = weatherReading(insightTimeline(mockStore, IDS.tomato1), TZ)!;
    expect(w.bars.map((b) => b.hours)).toEqual([5, 12, 8, 46]);
    expect(w.bars.map((b) => b.highlight)).toEqual([false, false, false, true]);
    expect(w.headline).toBe("3 humid days in a row before Oct 11");
    expect(w.caption).toMatch(/favour early blight; this shows a pattern, not a cause/);
  });

  it("finds Tomato 2 40 cm away, healthy before the live scan", () => {
    expect(nearestPlant(mockStore.maps.get(IDS.garden), IDS.tomato1)).toEqual({ plantId: IDS.tomato2, cm: 40 });
    expect(nearbyLine(insightTimeline(mockStore, IDS.tomato2), blight, TZ, NOW)).toBe("Healthy at its check on Oct 13");
  });
});

describe("the rules (analytics 2)", () => {
  it("says lower only when the last range is below the first", () => {
    expect(spotsReading([pt(6, 14, { min: 9, max: 21 }), pt(3, 9, { min: 6, max: 12 }), pt(0, 10, { min: 7, max: 14 })], TZ)!.direction).toBe("about the same");
    expect(spotsReading([pt(6, 14), pt(3, 9)], TZ)!.direction).toBeUndefined();
    expect(spotsReading([pt(6, 14)], TZ)!.headline).toBe("Measured once: spots cover about 14% of the measured leaves");
    expect(spotsReading([], TZ)).toBeUndefined();
  });

  it("leaves low-confidence points out of a series", () => {
    expect(spotsReading([pt(6, 14), pt(3, 30, { confidence_band: "low" })], TZ)!.checks).toBe(1);
  });

  it("gives no rate without 3 points over 14 days, or inside the noise", () => {
    const h = (vals: [number, number][]) => heightReading(vals.map(([d, v]) => pt(d, v, { quantity: "plant_height_cm" })), TZ)!;
    expect(h([[10, 30], [5, 40], [0, 50]]).headline).toBe("About 50 cm. 3 checks so far: too few for a rate");
    expect(h([[20, 50], [10, 51], [0, 52]]).headline).toBe("About 52 cm. No clear change yet");
    expect(h([[20, 50], [10, 51], [0, 52]]).trend).toBe(true);
  });

  it("uses the API's noise when it sends one", () => {
    const r = heightReading([pt(0, 40, { quantity: "plant_height_cm", noise_cm: 1 })], TZ)!;
    expect(r.provisional).toBe(false);
    expect(r.band[0]).toEqual({ lo: 39, hi: 41 });
  });

  it("draws a failed scan and a low-evidence result grey, never amber", () => {
    const cells = stripCells([entry(6, { status: "failed", diagnosis_status: undefined }), entry(3, { diagnosis_status: "uncertain", low_evidence: true }), entry(0)], "healthy", TZ);
    expect(cells.map((c) => c.status)).toEqual(["unknown", "unknown", "healthy"]);
    expect(cells[0]!.label).toBe("Oct 11: Failed");
  });

  it("words the strip for a first check and an empty history", () => {
    expect(healthHeadline([], [], TZ)).toBe("Scan this plant to start its history.");
    const one = [entry(0)];
    expect(healthHeadline(one, stripCells(one, "healthy", TZ), TZ)).toBe("One check so far");
  });

  it("marks a plant not checked in 14 days", () => {
    expect(notCheckedRecently([entry(20)], NOW)).toBe(true);
    expect(notCheckedRecently([entry(3)], NOW)).toBe(false);
  });

  it("dates the steps across a month", () => {
    expect(datesText(["2026-10-30T15:00:00Z", "2026-11-02T15:00:00Z", "2026-11-05T15:00:00Z"], TZ)).toBe("Oct 30, Nov 2, 5");
  });

  it("leaves dismissed tasks and other plants out of the lanes", () => {
    const base = { garden_id: "g", source: "plan" as const, plan_id: "p", created_at: ago(9) };
    const tasks: Task[] = [
      { ...base, id: "a", plant_id: "me", title: "Mulch", plan_step_id: "mulch", done_at: ago(5) },
      { ...base, id: "b", plant_id: "me", title: "Spray", plan_step_id: "spray", dismissed_at: ago(4), dismissed_reason: "plan_superseded" },
      { ...base, id: "c", plant_id: "other", title: "Check", plan_step_id: "check", done_at: ago(4) },
    ];
    expect(stepLanes(tasks, "me", TZ).map((l) => l.key)).toEqual(["mulch"]);
  });

  it("says after, never worked", () => {
    const lanes = [{ key: "a", title: "Remove", times: [0], dates: "Oct 11" }];
    const s = spotsReading([pt(6, 14, { min: 9, max: 21 }), pt(3, 9, { min: 6, max: 12 }), pt(0, 5, { min: 3, max: 7 })], TZ);
    expect(afterSteps(lanes, s)).toMatch(/^After the steps were done/);
    expect(afterSteps(lanes, undefined)).toBe("Scan again to see how it's doing.");
    expect(afterSteps([], s)).toBeUndefined();
  });

  it("names the neighbour's same problem as also found on it, never spread from", () => {
    const n = [entry(4), entry(0, { diagnosis_status: "confirmed", primary: blight })];
    const line = nearbyLine(n, blight, TZ, NOW);
    expect(line).toBe("Also found on it today: early blight. Healthy at its check on Oct 13");
    expect(line).not.toMatch(/spread/);
  });

  it("finds the diagnosis at the start of the latest run of one problem", () => {
    const tl = [entry(6, { diagnosis_status: "confirmed", primary: blight }), entry(3, { diagnosis_status: "confirmed", primary: blight }), entry(9)];
    expect(diagnosisEntry(tl)?.scan_id).toBe("s6");
    expect(diagnosisEntry([entry(0)])).toBeUndefined();
  });
});
