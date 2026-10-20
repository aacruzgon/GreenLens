import { ACTIONS, can, REQUIRED_ROLE, ROLES, type Action } from "./permissions";
import { canAddImage, canSubmit, IMAGE_KIND_LIMITS, isAtRest, STAGE_ORDER } from "./scan";
import {
  diagnosisCopy,
  LOW_EVIDENCE_HEADLINE,
  LOW_EVIDENCE_SUBLINE,
  organismCopy,
  planNote,
} from "./diagnosis";
import { bboxToView, fitRect, regionToView } from "./regions";
import { formatDate, formatDay, formatMeasurement, bandWord } from "./format";
import type { Role } from "./types";

// Contracts 1, the role matrix: roles that hold each action (viewer, reviewer, editor, owner).
const MATRIX: Record<Action, Role[]> = {
  "garden.read": ["viewer", "reviewer", "editor", "owner"],
  "chat.read": ["viewer", "reviewer", "editor", "owner"],
  "memory.remember": ["viewer", "reviewer", "editor", "owner"],
  "connectors.read": ["viewer", "reviewer", "editor", "owner"],
  "scan.feedback": ["reviewer", "editor", "owner"],
  "scan.create": ["editor", "owner"],
  "scan.upload": ["editor", "owner"],
  "scan.delete_pending": ["editor", "owner"],
  "scan.answer": ["editor", "owner"],
  "species.confirm": ["editor", "owner"],
  "scan.guided_check": ["editor", "owner"],
  "map.capture": ["editor", "owner"],
  "chat.write_tools": ["editor", "owner"],
  "task.write": ["editor", "owner"],
  "plant.write": ["editor", "owner"],
  "memory.share": ["editor", "owner"],
  "memory.note": ["editor", "owner"],
  "garden.manage": ["owner"],
  "plant.delete": ["owner"],
  "memory.delete_shared": ["owner"],
  "invites.manage": ["owner"],
  "members.manage": ["owner"],
  "connectors.manage": ["owner"],
};

describe("permissions", () => {
  it("covers every action in the matrix", () => {
    expect([...ACTIONS].sort()).toEqual(Object.keys(MATRIX).sort());
    expect(Object.keys(REQUIRED_ROLE).length).toBe(ACTIONS.length);
  });

  it.each(ACTIONS)("%s matches contracts 1 for every role", (action) => {
    for (const role of ROLES) {
      expect(can(role, action)).toBe(MATRIX[action].includes(role));
    }
  });

  it("denies a non-member", () => {
    for (const a of ACTIONS) expect(can(undefined, a)).toBe(false);
  });

  it("gives viewers and reviewers no write action but feedback", () => {
    expect(can("viewer", "scan.feedback")).toBe(false);
    expect(can("reviewer", "scan.feedback")).toBe(true);
    expect(can("reviewer", "scan.create")).toBe(false);
  });
});

describe("scan", () => {
  const rest = (
    status: Parameters<typeof isAtRest>[0]["status"],
    open = false,
    checks: ("analysing" | "done" | "failed")[] = [],
  ) =>
    isAtRest({
      status,
      open_question: open ? ({ id: "q" } as never) : undefined,
      guided_checks: checks.map((s) => ({ status: s })),
    });

  it("rests when planned with no question", () => expect(rest("planned")).toBe(true));
  it("does not rest when planned with a question", () => expect(rest("planned", true)).toBe(false));
  it("rests when failed", () => expect(rest("failed")).toBe(true));
  it("does not rest while a guided check is analysing, even when failed", () => {
    expect(rest("planned", false, ["analysing"])).toBe(false);
    expect(rest("failed", false, ["done", "analysing"])).toBe(false);
  });
  it("rests with finished or failed checks", () => expect(rest("planned", false, ["done", "failed"])).toBe(true));
  it.each(["pending", "queued", "analysing", "awaiting_answer", "diagnosed"] as const)(
    "does not rest at %s",
    (s) => expect(rest(s)).toBe(false),
  );

  it("keeps the kind limits of contracts 3", () => {
    expect(IMAGE_KIND_LIMITS).toEqual({
      whole_plant: 1,
      leaf_closeup: 2,
      leaf_underside: 1,
      soil_surface: 1,
      flower_fruit: 1,
      squeeze_test: 1,
      jar_test: 1,
    });
    expect(canAddImage("leaf_closeup", 1)).toBe(true);
    expect(canAddImage("leaf_closeup", 2)).toBe(false);
    expect(canSubmit({ leaf_closeup: 2 })).toBe(false);
    expect(canSubmit({ whole_plant: 1 })).toBe(true);
  });

  it("lists 15 stages in order", () => {
    expect(STAGE_ORDER).toHaveLength(15);
    expect(STAGE_ORDER[0]).toBe("plant_check");
    expect(STAGE_ORDER[14]).toBe("plan");
  });
});

describe("diagnosis wording (screens.md 6)", () => {
  const cand = (name: string) => ({ name }) as never;
  const base = { candidates: [cand("Early blight"), cand("Septoria leaf spot"), cand("Third")] };
  const NUMBER = /\d|%/;

  it("confirmed: the name, no number", () => {
    const c = diagnosisCopy({ ...base, status: "confirmed", low_evidence: false, primary: cand("Early blight") });
    expect(c.headline).toBe("Early blight");
    expect(`${c.headline} ${c.subline ?? ""}`).not.toMatch(NUMBER);
  });

  it("uncertain: top two candidates, never one confident name", () => {
    const c = diagnosisCopy({ ...base, status: "uncertain", low_evidence: false });
    expect(c.headline).toBe("Not sure yet: Early blight or Septoria leaf spot");
    expect(c.candidates).toEqual(["Early blight", "Septoria leaf spot"]);
    expect(c.headline).not.toMatch(NUMBER);
  });

  it("uncertain with low_evidence: no problem name", () => {
    const c = diagnosisCopy({ ...base, status: "uncertain", low_evidence: true });
    expect(c.headline).toBe(LOW_EVIDENCE_HEADLINE);
    expect(c.subline).toBe(LOW_EVIDENCE_SUBLINE);
    expect(`${c.headline} ${c.subline}`).not.toMatch(/blight|Septoria/);
  });

  it("healthy: 'Looks healthy', with a caveat only for one photo", () => {
    const one = diagnosisCopy({ candidates: [], status: "healthy", low_evidence: false }, 1);
    const many = diagnosisCopy({ candidates: [], status: "healthy", low_evidence: false }, 4);
    expect(one.headline).toBe("Looks healthy");
    expect(one.subline).toBeDefined();
    expect(many.subline).toBeUndefined();
    expect(one.headline).not.toMatch(/fine/);
  });

  it("provisional plan note", () => {
    expect(planNote({ provisional: true })).toBe("We're not sure yet. These steps are safe for every likely problem.");
    expect(planNote({ provisional: false })).toBeUndefined();
  });

  it("beneficial organism: leave it, with or without preys_on", () => {
    expect(organismCopy({ name: "Ladybug larva", role: "beneficial", preys_on: [{ name: "Aphids" }] }).headline).toBe(
      "Ladybug larva: leave it, it eats aphids",
    );
    expect(organismCopy({ name: "Ladybug larva", role: "beneficial" }).headline).toBe("Beneficial: leave it");
    expect(organismCopy({ name: "Ladybug larva", role: "beneficial" }).headline).not.toMatch(/remove|kill|treat/i);
  });

  it("invasive organism: may be invasive, how to report, never 'sent'", () => {
    const c = organismCopy({ name: "Spotted lanternfly", role: "invasive" });
    expect(c.headline).toBe("This may be an invasive species.");
    expect(c.action).toBe("How to report");
    expect(c.headline).not.toMatch(/sent/i);
  });

  it("a finding shows its band word; measurements are not over-precise", () => {
    expect(bandWord("high")).toBe("high");
    expect(formatMeasurement("lesion_diameter_mm", 2.04)).toBe("2 mm");
    expect(formatMeasurement("plant_height_cm", 34.4)).toBe("34 cm");
    expect(formatMeasurement("lesion_count", 7)).toBe("7");
  });
});

describe("regions", () => {
  it("maps a normalised box onto a letterboxed photo", () => {
    // A 200x100 photo in a 100x100 view is drawn 100x50, centred vertically.
    const rect = fitRect({ width: 200, height: 100 }, { width: 100, height: 100 });
    expect(rect).toEqual({ x: 0, y: 25, width: 100, height: 50 });
    expect(bboxToView([0.5, 0, 0.5, 1], rect)).toEqual({ x: 50, y: 25, width: 50, height: 50 });
  });

  it("covers", () => {
    expect(regionToView([0, 0, 1, 1], { width: 200, height: 100 }, { width: 100, height: 100 }, "cover")).toEqual({
      x: -50,
      y: 0,
      width: 200,
      height: 100,
    });
  });

  it("clamps boxes that run off the image", () => {
    const r = bboxToView([0.8, 0.8, 0.5, 0.5], { x: 0, y: 0, width: 100, height: 100 });
    expect(r.x).toBeCloseTo(80);
    expect(r.width).toBeCloseTo(20);
    expect(r.height).toBeCloseTo(20);
  });
});

describe("dates", () => {
  it("formats in the garden's time zone", () => {
    // 02:30 UTC on Oct 20 is still Oct 19 in Toronto (UTC-4).
    expect(formatDate("2026-10-20T02:30:00Z", "America/Toronto")).toBe("Oct 19, 2026");
    expect(formatDate("2026-10-20T02:30:00Z", "UTC")).toBe("Oct 20, 2026");
  });

  it("names today and yesterday by the garden's calendar", () => {
    const now = new Date("2026-10-20T02:30:00Z");
    expect(formatDay("2026-10-19T20:00:00Z", "America/Toronto", now)).toBe("Today");
    expect(formatDay("2026-10-18T20:00:00Z", "America/Toronto", now)).toBe("Yesterday");
  });
});
