import type { Question, ScanDetail, ScanEvent, ScanEventBody } from "@/domain/types";

import {
  activityContent,
  activitySegments,
  applyActivityOp,
  leaveActivityOp,
  nextActivityOp,
  runStart,
  SUMMARY_SECONDS,
  type ActivityOp,
  type ActivityTrack,
} from "./liveActivity";
import { phaseLine, phasesOf } from "./phases";
import { applyScanEvent, seed, type ScanState } from "./scanState";

const T0 = Date.parse("2026-10-22T14:00:00Z");
const at = (s: number) => new Date(T0 + s * 1000).toISOString();

function detail(): ScanDetail {
  return {
    id: "scan_1",
    garden_id: "g",
    plant_id: "p",
    run: 1,
    status: "queued",
    created_at: at(0),
    images: [],
    guided_checks: [],
    last_event_seq: 0,
    observations: [],
    plant: { id: "p", name: "Tomato 1" } as ScanDetail["plant"],
  } as ScanDetail;
}

const QUESTION = {
  id: "q_1",
  kind: "indicator",
  text: "Do the spots have rings, like a target?",
  candidates: ["early_blight", "septoria"],
  run: 1,
  status: "open",
  asked_at: at(9),
  continue_at: at(99),
} as Question;

/** The scripted early-blight run (mock runs.ts) in order, each body with its second. */
const SCRIPT: [number, ScanEventBody][] = [
  [0, { type: "status_changed", from: "queued", to: "analysing" } as ScanEventBody],
  [0, { type: "stage_started", stage: "plant_check" }],
  [1, { type: "stage_completed", stage: "plant_check", duration_ms: 1000 }],
  [1, { type: "stage_started", stage: "segmentation" }],
  [2, { type: "stage_completed", stage: "segmentation", duration_ms: 1000 }],
  [2, { type: "stage_started", stage: "species" }],
  [3, { type: "stage_completed", stage: "species", duration_ms: 1000 }],
  [3, { type: "stage_started", stage: "disease" }],
  [4, { type: "stage_completed", stage: "disease", duration_ms: 1000 }],
  [4, { type: "stage_started", stage: "context" }],
  [5, { type: "stage_completed", stage: "context", duration_ms: 1000 }],
  [5, { type: "stage_started", stage: "retrieve" }],
  [6, { type: "stage_completed", stage: "retrieve", duration_ms: 1000 }],
  [6, { type: "stage_started", stage: "vlm_findings" }],
  [8, { type: "stage_completed", stage: "vlm_findings", duration_ms: 2000 }],
  [9, { type: "status_changed", from: "analysing", to: "awaiting_answer" } as ScanEventBody],
  [9, { type: "question", question: QUESTION } as ScanEventBody],
  [12, { type: "question_resolved", question_id: "q_1", by: "user" } as ScanEventBody],
  [12, { type: "status_changed", from: "awaiting_answer", to: "analysing" } as ScanEventBody],
  [12, { type: "stage_started", stage: "scoring" }],
  [13, { type: "stage_completed", stage: "scoring", duration_ms: 1000 }],
  [13, { type: "stage_started", stage: "plan" }],
  [15, { type: "stage_completed", stage: "plan", duration_ms: 2000 }],
  [15, { type: "plan_ready", plan_id: "plan_1", provisional: false } as ScanEventBody],
  [15, { type: "status_changed", from: "analysing", to: "planned" } as ScanEventBody],
];

/** Plays the script through the reducer and the activity's rules, as the hook does: the operations it runs. */
function follow(script = SCRIPT): ActivityOp[] {
  let state: ScanState = seed(detail());
  let track: ActivityTrack = { started: false, ended: false };
  let startedAt: number | undefined;
  const ops: ActivityOp[] = [];
  script.forEach(([s, body], i) => {
    const event = { ...body, v: 1, seq: i + 1, run: 1, scan_id: "scan_1", ts: at(s) } as ScanEvent;
    state = applyScanEvent(state, event);
    startedAt ??= runStart(state) ?? T0;
    const phases = phasesOf(state);
    const content = activityContent(state, phases, phaseLine(state, phases), startedAt);
    const op = nextActivityOp(track, content, { plantName: "Tomato 1", questionText: state.question?.text });
    track = applyActivityOp(track, op);
    if (op.kind !== "none") ops.push(op);
  });
  return ops;
}

describe("activitySegments", () => {
  it("has nine, in the stage list's order, with measuring skipped when the run has no depth", () => {
    let state = seed(detail());
    state = applyScanEvent(state, { type: "stage_started", stage: "plant_check", v: 1, seq: 1, run: 1, scan_id: "scan_1", ts: at(0) } as ScanEvent);
    expect(activitySegments(phasesOf(state))).toEqual([
      "running", "waiting", "waiting", "skipped", "waiting", "waiting", "waiting", "waiting", "waiting",
    ]);
  });
});

describe("runStart", () => {
  it("is the first stage's start", () => {
    expect(runStart({ stages: {} })).toBeUndefined();
    expect(runStart({ stages: { species: { state: "done", startedAt: at(2) }, plant_check: { state: "done", startedAt: at(0) } } })).toBe(T0);
  });
});

describe("the Live Activity following a scripted scan", () => {
  const ops = follow();

  it("starts on the first stage and ticks with the stage card", () => {
    expect(ops[0]).toMatchObject({ kind: "start", content: { mode: "running", stage: "Finding the plant", startedAt: T0 } });
    const stages = ops.filter((o) => o.kind === "update" && o.content.mode === "running").map((o) => (o.kind === "update" ? o.content.stage : ""));
    expect(stages).toContain("Identifying the species");
    expect(stages).toContain("Checking telltale signs");
    expect(stages).toContain("Writing your plan");
  });

  it("updates only when what it shows changes", () => {
    const shown = ops.map((o) => JSON.stringify("content" in o ? o.content : null));
    shown.slice(1).forEach((s, i) => expect(s).not.toBe(shown[i]));
    // 25 events, 12 operations: one per tick, the question, the answer and the end.
    expect(ops).toHaveLength(12);
  });

  it("alerts only for the question and the result", () => {
    const alerts = ops.filter((o) => (o.kind === "update" || o.kind === "end") && o.alert);
    expect(alerts.map((o) => (o.kind === "update" || o.kind === "end" ? o.alert : undefined))).toEqual([
      { title: "Tomato 1 needs an answer", body: "Do the spots have rings, like a target?" },
      { title: "Tomato 1: plan ready", body: "See the diagnosis and what to do." },
    ]);
  });

  it("ends with the plan, its total and the summary kept for 15 minutes", () => {
    const last = ops[ops.length - 1];
    expect(last).toMatchObject({
      kind: "end",
      content: { mode: "planned", stage: "Plan ready", total: "in 11 s" },
      dismissal: { policy: "after", afterSeconds: SUMMARY_SECONDS },
    });
    expect(last?.kind === "end" && last.content.segments).toEqual([
      "done", "done", "done", "skipped", "done", "done", "done", "done", "done",
    ]);
    expect(ops.filter((o) => o.kind === "end")).toHaveLength(1);
  });

  it("ends with an alert when the scan stops", () => {
    const failed = follow([
      ...SCRIPT.slice(0, 4),
      [2, { type: "failed", failure: { code: "no_plant", message: "No plant found" } } as unknown as ScanEventBody],
    ]);
    expect(failed[failed.length - 1]).toMatchObject({ kind: "end", content: { mode: "failed", stage: "Stopped" }, alert: { title: "Tomato 1: the scan stopped" } });
  });
});

describe("start and leave", () => {
  const content = { mode: "planned" as const, stage: "Plan ready", segments: [], startedAt: T0 };

  it("doesn't start for a scan already at rest", () => {
    expect(nextActivityOp({ started: false, ended: false }, content, { plantName: "Tomato 1" })).toEqual({ kind: "none" });
  });

  it("ends at once when the screen goes before the scan ends, and not after", () => {
    const running = { ...content, mode: "running" as const, stage: "Finding the plant" };
    expect(leaveActivityOp({ started: true, ended: false, last: running })).toEqual({ kind: "end", content: running, dismissal: { policy: "immediate" } });
    expect(leaveActivityOp({ started: true, ended: true, last: content })).toEqual({ kind: "none" });
    expect(leaveActivityOp({ started: false, ended: false })).toEqual({ kind: "none" });
  });
});
