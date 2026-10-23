import type { TimelineEntry } from "@/domain/types";

import { timelineRow } from "./timeline";

const at = "2026-10-11T14:40:00Z";
const entry = (over: Partial<TimelineEntry>): TimelineEntry => ({ scan_id: "s1", created_at: at, status: "planned", ...over });

describe("a plant's timeline", () => {
  it("names the result in the words of screens 6, never a number", () => {
    expect(timelineRow(entry({ diagnosis_status: "confirmed", primary: { name: "Early blight", problem_id: "x" }, plan_id: "p" }))).toEqual({ scanId: "s1", title: "Early blight", opens: "result" });
    expect(timelineRow(entry({ diagnosis_status: "healthy", plan_id: "p" })).title).toBe("Looks healthy");
    expect(timelineRow(entry({ diagnosis_status: "uncertain", plan_id: "p" })).title).toBe("Not sure yet");
  });

  it("opens a running scan's analysis, a pending one's capture, and words a failure", () => {
    expect(timelineRow(entry({ status: "analysing" }))).toMatchObject({ title: "Analysing", opens: "scan" });
    expect(timelineRow(entry({ status: "pending" }))).toMatchObject({ title: "Not submitted", opens: "capture" });
    expect(timelineRow(entry({ status: "failed", failure_code: "no_plant_found" })).title).toBe("Failed: no plant found");
  });
});
