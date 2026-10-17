import type { Question, ScanEvent, ScanEventBody } from "@/domain/types";

import { applyScanEvent, effectsOf, foldScanEvents, initialScanView } from "./scanReducer";

let seq = 0;
function ev(body: ScanEventBody, run = 1): ScanEvent {
  seq += 1;
  return { ...body, v: 1, seq, run, scan_id: "S", ts: "2026-10-18T14:03:22Z" } as ScanEvent;
}

const obs = (id: string, pipeline_stage: "disease" | "vlm_findings", label: string) =>
  ({
    id,
    scan_id: "S",
    run: 1,
    stage: pipeline_stage === "disease" ? "disease" : "organism",
    pipeline_stage,
    label,
    source: pipeline_stage === "disease" ? "classifier" : "vlm",
    model_version: "t",
    created_at: "2026-10-18T14:03:22Z",
    confidence: 0.71,
    confidence_band: "medium",
  }) as const;

/** The typical run of contracts 4, abbreviated to the events that change state. */
function exampleRun(): ScanEvent[] {
  seq = 0;
  return [
    ev({ type: "run_started", reason: "submit", rerun_stages: ["plant_check", "disease", "vlm_findings", "scoring", "plan"] }),
    ev({ type: "status_changed", from: "pending", to: "queued" }),
    ev({ type: "status_changed", from: "queued", to: "analysing" }),
    ev({ type: "stage_started", stage: "plant_check" }),
    ev({ type: "stage_completed", stage: "plant_check", duration_ms: 10 }),
    ev({ type: "stage_started", stage: "disease" }),
    ev({ type: "stage_completed", stage: "disease", duration_ms: 10 }),
    ev({ type: "finding", observation: obs("O1", "disease", "Early blight") }),
    ev({ type: "candidates_selected", candidates: Array.from({ length: 9 }, (_, i) => ({ problem_id: `p${i}`, name: `P${i}`, kind: "fungal", source: "classifier" })) }),
    ev({ type: "stage_started", stage: "vlm_findings" }),
    ev({ type: "stage_completed", stage: "vlm_findings", duration_ms: 10 }),
    ev({ type: "finding", observation: obs("O2", "vlm_findings", "Ladybug larva") }),
    ev({ type: "species_suggested", candidates: [], working_species_id: "tomato", needs_confirmation: true }),
    ev({ type: "status_changed", from: "analysing", to: "diagnosed" }),
    ev({ type: "status_changed", from: "diagnosed", to: "planned" }),
    ev({ type: "plan_ready", plan_id: "P1", provisional: false }),
  ];
}

describe("applyScanEvent", () => {
  it("folds the example run of contracts 4", () => {
    const s = foldScanEvents(exampleRun());
    expect(s.status).toBe("planned");
    expect(s.lastSeq).toBe(16);
    expect(s.findings.map((f) => f.id)).toEqual(["O1", "O2"]);
    expect(s.candidateCount).toBe(9);
    expect(s.species?.needsConfirmation).toBe(true);
    expect(s.plan).toEqual({ planId: "P1", provisional: false });
    expect(s.stages.map((x) => [x.stage, x.state])).toEqual([
      ["plant_check", "done"],
      ["disease", "done"],
      ["vlm_findings", "done"],
      ["scoring", "waiting"],
      ["plan", "waiting"],
    ]);
  });

  it("ignores events it has already applied, so a replay that repeats seq values changes nothing", () => {
    const events = exampleRun();
    const once = foldScanEvents(events);
    const twice = foldScanEvents(events, once);
    expect(twice).toBe(once);
    const shuffledRepeat = foldScanEvents([...events.slice(0, 8), ...events.slice(4, 12), ...events.slice(8)]);
    expect(shuffledRepeat).toEqual(once);
  });

  it("removes a retracted finding", () => {
    const events = exampleRun();
    const s = foldScanEvents([...events, ev({ type: "finding_retracted", observation_id: "O1" })]);
    expect(s.findings.map((f) => f.id)).toEqual(["O2"]);
  });

  it("resets the rerun stages and clears only their findings on a species-change rerun", () => {
    const events = exampleRun();
    const s = foldScanEvents([
      ...events,
      ev({ type: "species_confirmed", species_id: "potato", rerun: true }),
      ev({ type: "run_started", reason: "species_changed", rerun_stages: ["disease", "scoring", "plan"] }, 2),
    ]);
    expect(s.run).toBe(2);
    expect(s.species).toBeUndefined();
    expect(s.findings.map((f) => f.id)).toEqual(["O2"]);
    expect(s.stages.find((x) => x.stage === "plant_check")?.state).toBe("done");
    expect(s.stages.find((x) => x.stage === "disease")?.state).toBe("waiting");
  });

  it("shows and hides a question, and keeps a late answer's plan update as a notice", () => {
    seq = 0;
    const q: Question = { id: "Q1", run: 1, kind: "indicator", text: "Rings?", candidates: [], asked_at: "x", continue_at: "y", status: "open" };
    let s = applyScanEvent(initialScanView, ev({ type: "question", question: q }));
    expect(s.question?.id).toBe("Q1");
    s = applyScanEvent(s, ev({ type: "plan_ready", plan_id: "P1", provisional: true }));
    s = applyScanEvent(s, ev({ type: "question_resolved", question_id: "Q1", by: "user" }));
    expect(s.question).toBeUndefined();
    s = applyScanEvent(s, ev({ type: "plan_superseded", old_plan_id: "P1", new_plan_id: "P2", reason: "late_answer" }));
    expect(s.plan?.planId).toBe("P2");
    expect(s.planNotice).toBe("late_answer");
  });

  it("records a failure", () => {
    seq = 0;
    const s = applyScanEvent(initialScanView, ev({ type: "failed", failure: { code: "no_plant_found", message: "No plant", retryable: false } }));
    expect(s.status).toBe("failed");
    expect(s.failure?.code).toBe("no_plant_found");
  });

  it("ignores unknown event types", () => {
    seq = 0;
    const s = applyScanEvent(initialScanView, ev({ type: "from_the_future" } as never));
    expect(s.lastSeq).toBe(1);
  });
});

describe("effectsOf", () => {
  it("refreshes the plan's records on plan_ready", () => {
    expect(effectsOf({ type: "plan_ready" }).map((e) => e.key)).toContain("tasks");
  });
  it("does nothing for progress events", () => {
    expect(effectsOf({ type: "stage_started" })).toEqual([]);
  });
});
