import type { Observation, ScanDetail, ScanEvent, ScanEventBody } from "@/domain/types";

import { bandShift, hotspotsOf, labelSides, readingImageId } from "./hotspots";
import { phaseLine, phasesOf, tickCount } from "./phases";
import { applyScanEvent, effectsOf, isReplay, reconcile, seed, stateAtRest, type ScanState } from "./scanState";

const TS = "2026-10-22T14:00:00Z";

function detail(patch: Partial<ScanDetail> = {}): ScanDetail {
  return {
    id: "scan_1",
    garden_id: "g",
    plant_id: "p",
    run: 1,
    status: "queued",
    created_at: TS,
    images: [
      { id: "img_whole", kind: "whole_plant", url: "https://x/w", width: 4000, height: 3000, uploaded_at: TS },
      { id: "img_leaf", kind: "leaf_closeup", url: "https://x/l", width: 4000, height: 3000, uploaded_at: TS },
    ],
    guided_checks: [],
    last_event_seq: 2,
    observations: [],
    plant: { id: "p" } as ScanDetail["plant"],
    ...patch,
  } as ScanDetail;
}

function obs(id: string, patch: Partial<Observation>): Observation {
  return {
    id,
    scan_id: "scan_1",
    run: 1,
    label: "x",
    confidence: 0.8,
    confidence_band: "high",
    created_at: TS,
    model_version: "m",
    pipeline_stage: "disease",
    source: "classifier",
    stage: "disease",
    ...patch,
  } as Observation;
}

/** A run as the server writes it: each body gets the next `seq`. */
function run(bodies: ScanEventBody[], from = 1, runNo = 1): ScanEvent[] {
  return bodies.map((b, i) => ({ ...b, v: 1, seq: from + i, run: runNo, scan_id: "scan_1", ts: TS }) as ScanEvent);
}

const stage = (name: Parameters<typeof stageOf>[0], ms = 500) => stageOf(name, ms);
function stageOf(name: "plant_check" | "segmentation" | "species" | "disease" | "image_embedding" | "context" | "vlm_findings" | "vlm_indicators" | "scoring" | "plan" | "measurement", ms: number): ScanEventBody[] {
  return [
    { type: "stage_started", stage: name },
    { type: "stage_completed", stage: name, duration_ms: ms },
  ];
}

const ALL = ["plant_check", "segmentation", "species", "disease", "image_embedding", "context", "retrieve", "vlm_findings", "vlm_indicators", "scoring", "plan"] as const;

const blight = obs("o_blight", {
  label: "Early blight",
  ref: { table: "disease", id: "tomato_early_blight" },
  region: { image_id: "img_leaf", bbox: [0.25, 0.3, 0.4, 0.35] },
});
const septoria = obs("o_sept", {
  label: "Septoria leaf spot",
  ref: { table: "disease", id: "tomato_septoria" },
  region: { image_id: "img_leaf", bbox: [0.25, 0.3, 0.4, 0.35] },
});
const larva = obs("o_larva", {
  label: "Ladybug larva",
  stage: "organism",
  pipeline_stage: "vlm_findings",
  source: "vlm",
  role: "beneficial",
  region: { image_id: "img_leaf", bbox: [0.55, 0.5, 0.12, 0.1] },
});

// The example run of contracts 4, where the indicators confirm early blight.
const EXAMPLE: ScanEventBody[] = [
  { type: "run_started", reason: "submit", rerun_stages: [...ALL] },
  { type: "status_changed", from: "pending", to: "queued" },
  { type: "status_changed", from: "queued", to: "analysing" },
  ...stage("plant_check"),
  ...stage("segmentation"),
  ...stage("species"),
  { type: "stage_started", stage: "disease" },
  { type: "stage_completed", stage: "disease", duration_ms: 1400 },
  { type: "finding", observation: blight },
  ...stage("image_embedding"),
  { type: "stage_started", stage: "context" },
  { type: "weather_loaded", weather: { forecast: [{ mean_rh_pct: 90 }, { mean_rh_pct: 60 }] } as never },
  { type: "stage_completed", stage: "context", duration_ms: 300 },
  {
    type: "candidates_selected",
    candidates: [
      { problem_id: "tomato_early_blight", name: "Early blight", kind: "fungal", source: "classifier" },
      { problem_id: "tomato_septoria", name: "Septoria leaf spot", kind: "fungal", source: "classifier" },
    ],
  },
  ...stage("vlm_findings"),
  { type: "finding", observation: larva },
  { type: "species_suggested", candidates: [{ name: "Tomato", species_id: "tomato", score: 0.9, source: "bioclip" }], working_species_id: "tomato", needs_confirmation: true },
  ...stage("vlm_indicators"),
  ...stage("scoring"),
  { type: "diagnosis", diagnosis: { id: "d1", status: "confirmed", low_evidence: false, primary: { name: "Early blight" }, candidates: [] } as never },
  { type: "status_changed", from: "analysing", to: "diagnosed" },
  ...stage("plan"),
  { type: "status_changed", from: "diagnosed", to: "planned" },
  { type: "plan_ready", plan_id: "plan_1", provisional: false },
];

function fold(state: ScanState, events: ScanEvent[]): ScanState {
  return events.reduce(applyScanEvent, state);
}

describe("applyScanEvent", () => {
  it("folds contracts 4's example run to a planned, resting scan", () => {
    const s = fold(seed(detail()), run(EXAMPLE));
    expect(s.status).toBe("planned");
    expect(s.plan).toEqual({ id: "plan_1", provisional: false });
    expect(s.findings.map((f) => f.id)).toEqual(["o_blight", "o_larva"]);
    expect(s.candidates).toHaveLength(2);
    expect(s.species?.needsConfirmation).toBe(true);
    expect(s.diagnosis?.id).toBe("d1");
    expect(stateAtRest(s)).toBe(true);
    expect(s.lastSeq).toBe(EXAMPLE.length);
  });

  it("ignores a seq it has applied, so a replay that overlaps live events changes nothing", () => {
    const events = run(EXAMPLE);
    const once = fold(seed(detail()), events);
    const twice = fold(once, [...events.slice(10, 20), ...events]);
    expect(twice).toBe(once);
  });

  it("removes a retracted finding and keeps the others", () => {
    const s = fold(seed(detail()), run([...EXAMPLE.slice(0, 20), { type: "finding", observation: septoria }, { type: "finding_retracted", observation_id: "o_blight" }]));
    expect(s.findings.map((f) => f.id)).toEqual(["o_sept"]);
  });

  it("resets the rerun stages and their findings on a species change, keeping the other stages", () => {
    const first = fold(seed(detail()), run(EXAMPLE));
    const rerun = run(
      [
        { type: "species_confirmed", species_id: "pepper", rerun: true },
        { type: "run_started", reason: "species_changed", rerun_stages: ["species", "disease", "vlm_findings", "vlm_indicators", "scoring", "plan"] },
        { type: "status_changed", from: "planned", to: "analysing" },
      ],
      EXAMPLE.length + 1,
      2,
    );
    const s = fold(first, rerun);
    expect(s.run).toBe(2);
    expect(s.species).toBeUndefined();
    expect(s.stages.disease).toBeUndefined();
    expect(s.stages.plant_check?.state).toBe("done");
    // The blight finding came from `disease`, the larva from `vlm_findings`: both are dropped.
    expect(s.findings).toEqual([]);
    expect(stateAtRest(s)).toBe(false);
  });

  it("takes a late answer after planned: the question goes, the plan is superseded with a notice", () => {
    const question = { id: "q1", kind: "indicator", run: 1, status: "open", text: "Rings?", candidates: [], asked_at: TS, continue_at: TS } as never;
    const planned = fold(seed(detail()), run([...EXAMPLE.slice(0, -1), { type: "question", question }, { type: "plan_ready", plan_id: "plan_1", provisional: true }]));
    expect(stateAtRest(planned)).toBe(false);
    const n = planned.lastSeq;
    const s = fold(
      planned,
      run(
        [
          { type: "question_resolved", question_id: "q1", by: "user" },
          { type: "diagnosis", diagnosis: { id: "d2", status: "confirmed", low_evidence: false, candidates: [] } as never },
          { type: "plan_superseded", old_plan_id: "plan_1", new_plan_id: "plan_2", reason: "late_answer" },
        ],
        n + 1,
      ),
    );
    expect(s.question).toBeUndefined();
    expect(s.resolved).toEqual({ questionId: "q1", by: "user" });
    expect(s.plan).toEqual({ id: "plan_2", provisional: true, notice: "late_answer" });
    expect(stateAtRest(s)).toBe(true);
  });

  it("rebuilds stage progress on a full replay over a snapshot opened partway", () => {
    const events = run(EXAMPLE.slice(0, 16));
    const snap = seed(detail({ status: "analysing", last_event_seq: 16, observations: [blight] }));
    expect(snap.findings).toHaveLength(1);
    const s = fold(snap, events);
    expect(s.findings.map((f) => f.id)).toEqual(["o_blight"]);
    expect(s.stages.plant_check?.state).toBe("done");
    expect(events.every((e) => isReplay(snap, e))).toBe(true);
    expect(isReplay(snap, { seq: 17 })).toBe(false);
  });

  it("seeds every run's current observations, so a rerun keeps the first run's plant check", () => {
    const plantCheck = obs("o_plant", { run: 1, pipeline_stage: "plant_check", stage: "plant_check" });
    const s = seed(detail({ run: 2, observations: [plantCheck, { ...blight, run: 2 }] }));
    expect(s.findings.map((f) => f.id)).toEqual(["o_plant", "o_blight"]);
  });
});

describe("reconcile", () => {
  const question = { id: "q1", kind: "photo_request", run: 1, status: "open", text: "A closer photo?", candidates: [], asked_at: TS, continue_at: TS } as never;
  const asked = fold(seed(detail()), run([...EXAMPLE.slice(0, 20), { type: "question", question }, { type: "status_changed", from: "analysing", to: "awaiting_answer" }]));

  it("takes what changed with no event from a record as fresh as the state", () => {
    const fresh = detail({
      status: "awaiting_answer",
      last_event_seq: asked.lastSeq,
      open_question: { ...(question as object), status: "answering" } as never,
      guided_checks: [{ image_id: "img_jar", kind: "jar_test", status: "analysing" }],
    });
    const s = reconcile(asked, fresh);
    expect(s.question?.status).toBe("answering");
    expect(s.guidedChecks).toEqual([{ image_id: "img_jar", kind: "jar_test", status: "analysing" }]);
    // The stream's own progress stays.
    expect(s.stages).toBe(asked.stages);
    expect(s.candidates).toBe(asked.candidates);
  });

  it("brings back answering after a replay whose earlier question was resolved (review of #130)", () => {
    const q = (id: string, run: number) => ({ id, kind: "photo_request", run, status: "open", text: "Closer?", candidates: [], asked_at: TS, continue_at: TS }) as never;
    const events = run([
      ...EXAMPLE.slice(0, 20),
      { type: "question", question: q("q0", 1) },
      { type: "run_started", reason: "species_changed", rerun_stages: ["species", "disease", "plan"] },
      { type: "question_resolved", question_id: "q0", by: "superseded" },
      { type: "question", question: q("q1", 2) },
    ]);
    const record = detail({ run: 2, status: "awaiting_answer", last_event_seq: events.length, open_question: { ...(q("q1", 2) as object), status: "answering" } as never });
    const replayed = fold(seed(record), events);
    expect(replayed.question?.status).toBe("open");
    expect(reconcile(replayed, record).question).toMatchObject({ id: "q1", status: "answering" });
  });

  it("never takes back answering or an analysing check for a record no newer (refetches out of order)", () => {
    const answering = reconcile(asked, detail({ status: "awaiting_answer", last_event_seq: asked.lastSeq, open_question: { ...(question as object), status: "answering" } as never, guided_checks: [{ image_id: "img_jar", kind: "jar_test", status: "analysing" }] }));
    const older = reconcile(answering, detail({ status: "awaiting_answer", last_event_seq: asked.lastSeq, open_question: question, guided_checks: [] }));
    expect(older.question?.status).toBe("answering");
    expect(older.guidedChecks.map((c) => c.image_id)).toEqual(["img_jar"]);
  });

  it("takes a record for the snapshot only when it is at least as new as the snapshot", () => {
    const base = seed(detail({ last_event_seq: 30 }));
    expect(reconcile(base, detail({ last_event_seq: 29, status: "planned" }), base.snapshotSeq)).toBe(base);
    expect(reconcile(base, detail({ last_event_seq: 30, status: "planned" }), base.snapshotSeq).status).toBe("planned");
  });

  it("ignores an older record", () => {
    expect(reconcile(asked, detail({ last_event_seq: asked.lastSeq - 1 }))).toBe(asked);
  });

  it("gives a superseding plan its provisional flag and keeps the notice", () => {
    const superseded = applyScanEvent(
      { ...asked, plan: { id: "plan_1", provisional: true } },
      run([{ type: "plan_superseded", old_plan_id: "plan_1", new_plan_id: "plan_2", reason: "late_answer" }], asked.lastSeq + 1)[0]!,
    );
    expect(superseded.plan).toEqual({ id: "plan_2", provisional: true, notice: "late_answer" });
    const s = reconcile(superseded, detail({ status: "planned", last_event_seq: superseded.lastSeq, plan: { id: "plan_2", provisional: false } as never }));
    expect(s.plan).toEqual({ id: "plan_2", provisional: false, notice: "late_answer" });
  });
});

describe("applyScanEvent, failures", () => {
  it("shows a failure, and clears it on the retry's run", () => {
    const failed = fold(seed(detail()), run([...EXAMPLE.slice(0, 5), { type: "failed", failure: { code: "no_plant_found", retryable: false, message: "No plant" } }]));
    expect(failed.status).toBe("failed");
    expect(failed.failure?.code).toBe("no_plant_found");
    expect(stateAtRest(failed)).toBe(true);
    const retried = applyScanEvent(failed, { ...run([{ type: "run_started", reason: "resubmit", rerun_stages: [...ALL] }], 7, 2)[0]! });
    expect(retried.failure).toBeUndefined();
  });
});

describe("effectsOf", () => {
  it("invalidates the scan's records on a plan and the plant's on a species change", () => {
    const [plan] = run([{ type: "plan_ready", plan_id: "p", provisional: false }]);
    expect(effectsOf(plan!)).toContainEqual({ kind: "invalidate", keys: ["scan", "plant", "plants", "timeline", "measurements", "tasks", "memories"] });
    const [species] = run([{ type: "species_confirmed", species_id: "tomato", rerun: false }]);
    expect(effectsOf(species!)).toEqual([{ kind: "invalidate", keys: ["plant", "plants"] }]);
    const [finding] = run([{ type: "finding", observation: larva }]);
    expect(effectsOf(finding!)).toEqual([]);
  });
});

describe("phases", () => {
  it("ticks each phase as a later one starts and says what is next", () => {
    const upTo = (n: number) => fold(seed(detail()), run(EXAMPLE.slice(0, n)));
    // Running the disease stage: plant and species have ticked.
    const mid = upTo(10);
    const phases = phasesOf(mid);
    expect(phases.map((p) => p.state).slice(0, 3)).toEqual(["done", "done", "running"]);
    expect(phases.some((p) => p.key === "measure")).toBe(false);
    expect(phaseLine(mid)).toMatchObject({ now: "Looking for disease", next: "Next: checking weather and soil" });
    expect(tickCount(phases)).toBe(2);

    const done = fold(seed(detail()), run(EXAMPLE));
    expect(phasesOf(done).every((p) => p.state === "done")).toBe(true);
    expect(phaseLine(done).now).toBe("Plan ready");
    expect(phasesOf(done).find((p) => p.key === "problems")?.note).toBe("2 possible problems");
    expect(phasesOf(done).find((p) => p.key === "context")?.note).toBe("1 humid day ahead");
    // The result names the problem only once the diagnosis is in.
    expect(phasesOf(done).find((p) => p.key === "weigh")?.note).toBe("Early blight fits best");
    expect(phasesOf(mid).find((p) => p.key === "disease")?.note).toBeUndefined();
  });

  it("waits for the answer on the telltale signs", () => {
    const question = { id: "q1", kind: "indicator", run: 1, status: "open", text: "Rings?", candidates: [], asked_at: TS, continue_at: TS } as never;
    const s = fold(seed(detail()), run([...EXAMPLE.slice(0, 24), { type: "question", question }, { type: "status_changed", from: "analysing", to: "awaiting_answer" }]));
    expect(phaseLine(s)).toEqual({ now: "Waiting for your answer", next: "Next: weighing the evidence" });
    expect(phasesOf(s).find((p) => p.key === "signs")).toMatchObject({ state: "running", note: "Waiting for your answer" });
  });

  it("shows the measuring phase only for a run with depth", () => {
    const withDepth = fold(seed(detail()), run([{ type: "run_started", reason: "submit", rerun_stages: [...ALL, "measurement"] }]));
    expect(phasesOf(withDepth).some((p) => p.key === "measure")).toBe(true);
  });
});

describe("hotspots", () => {
  it("draws one hotspot per region, worded as the symptom, never the problem's name", () => {
    const sizeObs = obs("o_size", {
      stage: "measurement",
      pipeline_stage: "measurement",
      source: "measurement",
      region: { image_id: "img_leaf", bbox: [0.2, 0.2, 0.5, 0.5] },
      detail: { measurement: { quantity: "lesion_diameter_mm", value: 9, min: 8, max: 12, n: 3, distance_m: 0.4 } } as never,
    });
    const spots = hotspotsOf({ findings: [blight, septoria, larva, sizeObs] }, detail().images);
    expect(spots).toHaveLength(2);
    expect(spots[0]).toMatchObject({ label: "Leaf spots", size: "8–12 mm", kind: "symptom", observationIds: ["o_blight", "o_sept"] });
    expect(spots[1]).toMatchObject({ label: "Ladybug larva", kind: "organism" });
    expect(spots.map((s) => s.label).join()).not.toMatch(/blight|septoria/i);
    expect(readingImageId(spots, detail().images)).toBe("img_leaf");
    expect(readingImageId([], detail().images)).toBe("img_leaf");
  });

  it("puts a label above its oval when the one below would cover a label already placed", () => {
    // Tomato 2's whole-plant photo on a 390 × 844 screen: the spots' oval, and the larva's low on its right.
    const leaf = { key: "leaf", oval: { x: -19, y: 297, width: 315, height: 207 }, text: "Affected area · 8–12 mm" };
    const larva = { key: "larva", oval: { x: 272, y: 435, width: 94, height: 59 }, text: "Ladybug larva" };
    const sides = labelSides([larva, leaf]);
    expect(sides.get("leaf")).toBe("below");
    expect(sides.get("larva")).toBe("above");
    // Far apart, both stay below.
    expect(labelSides([leaf, { ...larva, oval: { ...larva.oval, y: 700 } }]).get("larva")).toBe("below");
  });

  it("slides the photo so its hotspots and their labels sit between the cards, within a limit", () => {
    const ovals = [{ oval: { x: 100, y: 250, width: 160, height: 200 }, text: "Leaf spots" }];
    // Spots and label 250–494; clear band 320–560: down by 68.
    expect(bandShift(ovals, { top: 320, bottom: 560 }, 250)).toBe(68);
    expect(bandShift(ovals, { top: 200, bottom: 600 }, 250)).toBe(0);
    expect(bandShift(ovals, { top: 700, bottom: 800 }, 100)).toBe(100);
    expect(bandShift([], { top: 320, bottom: 560 }, 250)).toBe(0);
    expect(bandShift(ovals, undefined, 250)).toBe(0);
    // A label above its oval counts too: 206–450, so down by 112, not 68.
    expect(bandShift([{ ...ovals[0]!, side: "above" }], { top: 320, bottom: 560 }, 250)).toBe(112);
  });

  it("leaves out the classifier's Healthy finding and boxes without a problem", () => {
    const healthy = obs("o_h", { label: "Healthy", region: { image_id: "img_whole", bbox: [0.1, 0.1, 0.8, 0.8] } });
    expect(hotspotsOf({ findings: [healthy] }, detail().images)).toEqual([]);
  });
});
