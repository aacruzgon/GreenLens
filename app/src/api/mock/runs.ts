// Scripted scan runs (docs/frontend/README.md 15; contracts 4, the early-blight example).
// A run is a list of events with delays. `startRun` plays it into the store; the event stream
// (transport.ts) replays and follows the store's event log, so a reconnect with Last-Event-ID
// resumes exactly where it stopped.

import {
  blightSteps,
  CITATIONS,
  ev,
  frostInDays,
  healthySteps,
  IDS,
  makeDiagnosis,
  makeObservation,
  makePlan,
  makeScore,
  makeWeather,
  measureObs,
  PROBLEMS,
  provisionalSteps,
  tasksFromPlan,
  type ProblemDef,
} from "./fixtures";
import { scenario, type NextScan } from "./scenario";
import type { MockStore, RunCtx, Script, Step } from "./store";
import { nowTs, type S } from "./util";

export const ALL_STAGES: S["Stage"][] = [
  "plant_check", "segmentation", "species", "disease", "image_embedding", "measurement", "context",
  "retrieve", "vlm_findings", "vlm_indicators", "guided", "verify", "scoring", "fusion", "plan",
];

/** The indicator question of the live scan. */
export const RINGS_INDICATOR = "concentric_rings";

// ---- step helpers

const at = (delay: number, body: Step["body"]): Step => ({ delay, body });

function stage(name: S["Stage"], ms: number, mid: Step[] = [], skipped?: string): Step[] {
  return [
    at(120, () => ({ type: "stage_started", stage: name })),
    ...mid,
    at(ms, () => ({
      type: "stage_completed",
      stage: name,
      duration_ms: ms,
      ...(skipped ? { skipped: { reason: skipped } } : {}),
    })),
  ];
}

/** Steps of the `measurement` stage run only when a photo has depth (contracts 4). */
function depthStage(name: S["Stage"], ms: number, mid: Step[] = []): Step[] {
  return stage(name, ms, mid).map((s) => ({ ...s, needsDepth: true }));
}

function status(from: S["ScanStatus"], to: S["ScanStatus"], delay = 100): Step {
  return at(delay, () => ({ type: "status_changed", from, to }));
}

function finding(delay: number, make: (ctx: RunCtx) => S["Observation"]): Step {
  return at(delay, (ctx) => ({ type: "finding", observation: make(ctx) }));
}

function images(ctx: RunCtx): { whole: string; leaf: string } {
  const imgs = ctx.store.scan(ctx.scanId)?.scan.images ?? [];
  const whole = imgs.find((i) => i.kind === "whole_plant")?.id ?? "img_whole";
  const leaf = imgs.find((i) => i.kind === "leaf_closeup")?.id ?? whole;
  return { whole, leaf };
}

function plantOf(ctx: RunCtx): { id: string; name: string; speciesId: string; speciesName: string } {
  const p = ctx.store.plants.get(ctx.plantId);
  return {
    id: ctx.plantId,
    name: p?.name ?? "Your plant",
    speciesId: p?.species?.id ?? "tomato",
    speciesName: p?.species?.common_name ?? "Tomato",
  };
}

const spec = (ctx: RunCtx) => ({ id: ctx.id("obs"), scanId: ctx.scanId, run: ctx.run, ts: ctx.ts() });

function lastValue(ctx: RunCtx, q: S["GrowthQuantity"], fallback: number): number {
  const pts = ctx.store.pointsOf(ctx.plantId, q);
  return pts.length ? pts[pts.length - 1].value : fallback;
}

/** The early stages every run shares, up to and including segmentation and species. */
function opening(opts: { noPlant?: boolean } = {}): Step[] {
  return [
    status("queued", "analysing", 200),
    ...stage("plant_check", 600, opts.noPlant ? [] : [
      finding(200, (ctx) =>
        makeObservation({
          ...spec(ctx), confidence: 0.97, label: `${plantOf(ctx).speciesName} plant`, stage: "plant_check",
          pipelineStage: "plant_check", source: "segmentation",
          detail: { region_class: "plant" },
          region: { image_id: images(ctx).whole, bbox: [0.1, 0.05, 0.8, 0.9] },
        })),
    ]),
  ];
}

function afterPlantCheck(): Step[] {
  return [...segmentationStage(), ...speciesStage()];
}

function plantCheckStage(): Step[] {
  return opening().slice(1);
}

function segmentationStage(): Step[] {
  return stage("segmentation", 700, [
    finding(250, (ctx) =>
      makeObservation({
        ...spec(ctx), confidence: 0.92, label: "Leaf", stage: "segmentation", pipelineStage: "segmentation",
        source: "segmentation", detail: { region_class: "leaf" },
        region: { image_id: images(ctx).leaf, bbox: [0.25, 0.3, 0.4, 0.35] },
      })),
  ]);
}

function speciesStage(): Step[] {
  return stage("species", 900, [
    finding(300, (ctx) => {
      const p = plantOf(ctx);
      return makeObservation({
        ...spec(ctx), confidence: 0.94, label: p.speciesName, stage: "plant_check", pipelineStage: "species",
        source: "bioclip", ref: { table: "species", id: p.speciesId },
      });
    }),
  ]);
}

// ---- plans and diagnoses

function weatherStep(ago: number, humidAhead: number, rh90: number, run: number): Step {
  return at(250, (ctx) => ({
    type: "weather_loaded",
    weather: makeWeather(ctx.dates, { ago, rh90, run, humidAhead, frostInDays: frostInDays(ctx.dates) }),
  }));
}

function planStep(
  mk: (ctx: RunCtx) => { steps: S["PlanStep"][]; summary: string; watchFor: string[]; provisional: boolean; citations: S["Citation"][] },
  diagId: () => string,
): Step {
  return at(150, (ctx) => {
    const p = plantOf(ctx);
    const made = mk(ctx);
    const plan = makePlan({
      id: ctx.id("plan"), scanId: ctx.scanId, plantId: p.id, diagnosisId: diagId(), ts: ctx.ts(),
      summary: made.summary, steps: made.steps, watchFor: made.watchFor,
      provisional: made.provisional, citations: made.citations,
    });
    const tasks = tasksFromPlan(plan, ctx.dates, () => ctx.id("task"));
    ctx.store.attachPlan(ctx.scanId, plan, tasks);
    return { type: "plan_ready", plan_id: plan.id, provisional: plan.provisional };
  });
}

function septoriaSteps(plant: { id: string; name: string }): S["PlanStep"][] {
  const target: S["PlanTarget"][] = [{ table: "disease", id: PROBLEMS.septoria.id }];
  const cite = [CITATIONS.septoria.source_id];
  return [
    { id: "remove_spotted_leaves", order: 1, category: "cultural", title: "Remove the spotted leaves", detail: "Cut off leaves with many small spots and bag them. Do not compost.", targets: target, citations: cite, plant_id: plant.id, when: { kind: "now" } },
    { id: "water_at_base", order: 2, category: "cultural", title: "Water at the base only", detail: "Wet leaves spread the spores. Water in the morning, away from the leaves.", targets: target, citations: cite, plant_id: plant.id, when: { kind: "now" } },
    { id: "check_plant", order: 3, category: "monitoring", title: `Check ${plant.name} for new spots`, detail: "Look at the lowest leaves first.", targets: target, citations: cite, plant_id: plant.id, when: { kind: "recurring", every_days: 3 } },
  ];
}

// ---- Tomato 2's live scan

const RINGS_TEXT = "Spots have target-like rings";

function liveBlight(): Script {
  const shared: { diagId?: string; ladyObsId?: string } = {};

  const start = (): Step[] => [
    ...opening(),
    ...afterPlantCheck(),
    ...stage("disease", 1400, [
      finding(350, (ctx) =>
        makeObservation({
          ...spec(ctx), confidence: 0.46, label: PROBLEMS.earlyBlight.name, stage: "disease", pipelineStage: "disease",
          source: "classifier", ref: { table: "disease", id: PROBLEMS.earlyBlight.id },
          region: { image_id: images(ctx).leaf, bbox: [0.25, 0.3, 0.4, 0.35] },
        })),
      finding(150, (ctx) =>
        makeObservation({
          ...spec(ctx), confidence: 0.41, label: PROBLEMS.septoria.name, stage: "disease", pipelineStage: "disease",
          source: "classifier", ref: { table: "disease", id: PROBLEMS.septoria.id },
          region: { image_id: images(ctx).leaf, bbox: [0.25, 0.3, 0.4, 0.35] },
        })),
    ]),
    ...stage("image_embedding", 400),
    ...depthStage("measurement", 900, [
      finding(250, (ctx) => measureObs({ ...spec(ctx), quantity: "plant_height_cm", value: 56, min: 54, max: 58, n: 3, confidence: 0.86, imageId: images(ctx).whole })),
      finding(100, (ctx) => measureObs({ ...spec(ctx), quantity: "canopy_width_cm", value: 44, min: 42, max: 46, n: 3, confidence: 0.82, imageId: images(ctx).whole })),
      finding(150, (ctx) => measureObs({ ...spec(ctx), quantity: "lesion_diameter_mm", value: 9, min: 8, max: 12, n: 3, distance: 0.4, imageId: images(ctx).leaf })),
      finding(100, (ctx) => measureObs({ ...spec(ctx), quantity: "affected_area_pct", value: 6, min: 3, max: 9, n: 3, distance: 0.4, imageId: images(ctx).leaf })),
    ]),
    ...stage("context", 800, [weatherStep(0, 4, 61, 3)]),
    at(250, () => ({
      type: "candidates_selected",
      candidates: [
        { problem_id: PROBLEMS.earlyBlight.id, name: PROBLEMS.earlyBlight.name, kind: "fungal", source: "classifier" },
        { problem_id: PROBLEMS.septoria.id, name: PROBLEMS.septoria.name, kind: "fungal", source: "classifier" },
        { problem_id: PROBLEMS.lateBlight.id, name: PROBLEMS.lateBlight.name, kind: "fungal", source: "lookalike" },
        { problem_id: PROBLEMS.targetSpot.id, name: PROBLEMS.targetSpot.name, kind: "fungal", source: "lookalike" },
        { problem_id: PROBLEMS.bacterialSpot.id, name: PROBLEMS.bacterialSpot.name, kind: "bacterial", source: "lookalike" },
        { problem_id: PROBLEMS.bacterialSpeck.id, name: PROBLEMS.bacterialSpeck.name, kind: "bacterial", source: "lookalike" },
        { problem_id: PROBLEMS.leafMold.id, name: PROBLEMS.leafMold.name, kind: "fungal", source: "species" },
        { problem_id: PROBLEMS.spiderMites.id, name: PROBLEMS.spiderMites.name, kind: "pest", source: "species" },
        { problem_id: PROBLEMS.fusarium.id, name: PROBLEMS.fusarium.name, kind: "fungal", source: "species" },
      ],
    })),
    ...stage("vlm_findings", 1800, [
      finding(600, (ctx) => {
        const o = makeObservation({
          ...spec(ctx), confidence: 0.78, label: "Ladybug larva", stage: "organism", pipelineStage: "vlm_findings",
          source: "vlm", role: "beneficial", detail: { count: "one", length_mm: 6 },
          ref: { table: "organism", id: "ladybug" },
          region: { image_id: images(ctx).leaf, bbox: [0.55, 0.5, 0.12, 0.1] },
        });
        shared.ladyObsId = o.id;
        return o;
      }),
      finding(150, (ctx) =>
        makeObservation({
          ...spec(ctx), confidence: 0.7, label: "Soil is moist", stage: "soil", pipelineStage: "vlm_findings",
          source: "vlm", detail: { aspect: "moisture", value: "moist" },
        })),
    ]),
    at(200, (ctx) => ({
      type: "species_suggested",
      candidates: [{ name: plantOf(ctx).speciesName, species_id: plantOf(ctx).speciesId, score: 0.94, source: "bioclip" }],
      working_species_id: plantOf(ctx).speciesId,
      needs_confirmation: false,
    })),
    at(120, () => ({ type: "stage_started", stage: "vlm_indicators" })),
    at(1500, (ctx) => ({
      type: "question",
      question: {
        id: ctx.id("q"),
        kind: "indicator",
        run: ctx.run,
        status: "open",
        asked_at: ctx.ts(),
        continue_at: nowTs(10 * 60_000),
        candidates: [PROBLEMS.earlyBlight.id, PROBLEMS.septoria.id],
        indicator_id: RINGS_INDICATOR,
        text: "Do the spots on the lower leaves have target-like rings, dark circles inside circles?",
        options: [
          { id: "yes", label: "Yes" },
          { id: "no", label: "No" },
          { id: "unsure", label: "Not sure" },
        ],
      },
    })),
    at(100, () => ({ type: "stage_completed", stage: "vlm_indicators", duration_ms: 1500 })),
    status("analysing", "awaiting_answer"),
  ];

  const afterAnswer = (_ctx: RunCtx, answer: S["Answer"]): Step[] => {
    const lowerLeaves = ev("lower_leaves_first", "Spots started on the lowest leaves", "yes", 2, "vlm");
    const halo = ev("yellow_halo", "Yellow halo around the spots", "yes", 1, "vlm");
    const scoreOf = (): { primary?: S["CandidateScore"]; candidates: S["CandidateScore"][]; status: S["DiagnosisStatus"] } => {
      if (answer === "yes") {
        const blight = makeScore(PROBLEMS.earlyBlight, 6, { fit: "favourable", matched: [ev(RINGS_INDICATOR, RINGS_TEXT, "yes", 3, "user"), lowerLeaves, halo] });
        const sept = makeScore(PROBLEMS.septoria, 2, { fit: "favourable", matched: [lowerLeaves], unsure: [ev("small_many_spots", "Many small round spots with grey centres", "unsure", 3, "vlm")] });
        return { primary: blight, candidates: [blight, sept], status: "confirmed" };
      }
      if (answer === "no") {
        const sept = makeScore(PROBLEMS.septoria, 5, { fit: "favourable", matched: [lowerLeaves, ev("small_many_spots", "Many small round spots with grey centres", "yes", 3, "vlm")] });
        const blight = makeScore(PROBLEMS.earlyBlight, 0, { fit: "favourable", contradicted: [ev(RINGS_INDICATOR, RINGS_TEXT, "no", 3, "user")], matched: [lowerLeaves] });
        return { primary: sept, candidates: [sept, blight], status: "confirmed" };
      }
      const blight = makeScore(PROBLEMS.earlyBlight, 3, { fit: "favourable", matched: [lowerLeaves, halo], unsure: [ev(RINGS_INDICATOR, RINGS_TEXT, "unsure", 3, "user")] });
      const sept = makeScore(PROBLEMS.septoria, 2, { fit: "favourable", matched: [lowerLeaves], unsure: [ev("small_many_spots", "Many small round spots with grey centres", "unsure", 3, "vlm")] });
      return { candidates: [blight, sept], status: "uncertain" };
    };
    const problem: ProblemDef = answer === "no" ? PROBLEMS.septoria : PROBLEMS.earlyBlight;
    const provisional = answer === "unsure";

    return [
      at(200, (ctx) => ({ type: "question_resolved", question_id: ctx.store.scan(ctx.scanId)?.scan.open_question?.id ?? "", by: "user" })),
      ...stage("scoring", 700),
      at(250, (ctx) => {
        const p = plantOf(ctx);
        const s = scoreOf();
        const isT2 = p.id === IDS.tomato2;
        const diagnosis = makeDiagnosis({
          id: ctx.id("diag"), scanId: ctx.scanId, run: ctx.run, ts: ctx.ts(), status: s.status,
          primary: s.primary, candidates: s.candidates,
          risks: [{
            risk: "fungal_spread", level: "high",
            drivers: ["4 humid days in the forecast", ...(isT2 ? ["Crowded: 40 cm from Tomato 1"] : [])],
          }],
          organisms: shared.ladyObsId
            ? [{ name: "Ladybug larva", organism_id: "ladybug", role: "beneficial", observation_id: shared.ladyObsId, preys_on: [{ name: "Aphids", organism_id: "aphid" }] }]
            : [],
          soil: ctx.store.scan(ctx.scanId)?.observations.filter((o) => o.stage === "soil") ?? [],
          citations: provisional ? [CITATIONS.blight, CITATIONS.septoria] : [problem === PROBLEMS.septoria ? CITATIONS.septoria : CITATIONS.blight],
          photo: ["Spots on the lower leaves", "A ladybug larva on the same leaves"],
          modelled: ["3 humid days in a row, 4 more in the forecast"],
          user: answer === "unsure" ? [] : [answer === "yes" ? "The spots have target-like rings" : "The spots do not have rings"],
        });
        shared.diagId = diagnosis.id;
        return { type: "diagnosis", diagnosis };
      }),
      status("awaiting_answer", "diagnosed"),
      ...stage("plan", 900),
      status("diagnosed", "planned"),
      planStep((ctx) => {
        const p = plantOf(ctx);
        const isT2 = p.id === IDS.tomato2;
        if (provisional) {
          return {
            steps: provisionalSteps(p, "Look for rings on the spots, dark circles inside circles.", ctx.dates),
            summary: "Not sure yet whether this is early blight or Septoria leaf spot. These steps help with both.",
            watchFor: ["Rings forming on the spots", "New spots on upper leaves within 5 days"],
            provisional: true, citations: [CITATIONS.blight, CITATIONS.septoria],
          };
        }
        if (answer === "no") {
          return {
            steps: septoriaSteps(p), summary: `Septoria leaf spot on ${p.name}. Remove the spotted leaves and keep the leaves dry.`,
            watchFor: ["New spots on upper leaves within 5 days"], provisional: false, citations: [CITATIONS.septoria],
          };
        }
        return {
          steps: blightSteps(ctx.dates, 0, p, { crowded: isT2, ladybug: Boolean(shared.ladyObsId) }),
          summary: `Early blight on ${p.name}, caught early. Remove the infected leaves, give the plants room, and check every 3 days.`,
          watchFor: ["New spots on upper leaves within 5 days"], provisional: false, citations: [CITATIONS.blight],
        };
      }, () => shared.diagId ?? ""),
    ];
  };

  return { start, afterAnswer };
}

// ---- a healthy plant

function healthy(): Script {
  const shared: { diagId?: string } = {};
  return {
    start: () => [
      ...opening(),
      ...afterPlantCheck(),
      ...stage("disease", 1100, [
        finding(300, (ctx) =>
          makeObservation({
            ...spec(ctx), confidence: 0.93, label: "Healthy", stage: "disease", pipelineStage: "disease",
            source: "classifier", region: { image_id: images(ctx).whole, bbox: [0.1, 0.05, 0.8, 0.9] },
          })),
      ]),
      ...stage("image_embedding", 400),
      ...depthStage("measurement", 800, [
        finding(250, (ctx) => measureObs({ ...spec(ctx), quantity: "plant_height_cm", value: lastValue(ctx, "plant_height_cm", 30) + 2, n: 3, confidence: 0.86, imageId: images(ctx).whole })),
        finding(100, (ctx) => measureObs({ ...spec(ctx), quantity: "canopy_width_cm", value: lastValue(ctx, "canopy_width_cm", 30) + 1, n: 3, confidence: 0.82, imageId: images(ctx).whole })),
      ]),
      ...stage("context", 700, [weatherStep(0, 0, 8, 0)]),
      ...stage("vlm_findings", 1400, [
        finding(400, (ctx) =>
          makeObservation({
            ...spec(ctx), confidence: 0.72, label: "Soil is moist", stage: "soil", pipelineStage: "vlm_findings",
            source: "vlm", detail: { aspect: "moisture", value: "moist" },
          })),
      ]),
      ...stage("vlm_indicators", 300, [], "nothing_to_check"),
      ...stage("scoring", 500),
      at(250, (ctx) => {
        const diagnosis = makeDiagnosis({
          id: ctx.id("diag"), scanId: ctx.scanId, run: ctx.run, ts: ctx.ts(), status: "healthy",
          soil: ctx.store.scan(ctx.scanId)?.observations.filter((o) => o.stage === "soil") ?? [],
          photo: ["No spots, holes or pests on the leaves"],
        });
        shared.diagId = diagnosis.id;
        return { type: "diagnosis", diagnosis };
      }),
      status("analysing", "diagnosed"),
      ...stage("plan", 700),
      status("diagnosed", "planned"),
      planStep((ctx) => ({
        steps: healthySteps(plantOf(ctx)), summary: `${plantOf(ctx).name} looks healthy.`,
        watchFor: ["Spots or yellowing on the lowest leaves"], provisional: false, citations: [],
      }), () => shared.diagId ?? ""),
    ],
  };
}

// ---- low_evidence: the photos can't settle it

function lowEvidence(): Script {
  const shared: { diagId?: string } = {};
  return {
    start: () => [
      ...opening(),
      ...afterPlantCheck(),
      ...stage("disease", 1100, [
        finding(300, (ctx) =>
          makeObservation({
            ...spec(ctx), confidence: 0.22, label: PROBLEMS.earlyBlight.name, stage: "disease", pipelineStage: "disease",
            source: "classifier", ref: { table: "disease", id: PROBLEMS.earlyBlight.id },
          })),
      ]),
      ...stage("image_embedding", 400),
      ...stage("context", 700, [weatherStep(0, 0, 8, 0)]),
      ...stage("vlm_findings", 1400),
      ...stage("vlm_indicators", 900),
      ...stage("scoring", 500),
      at(250, (ctx) => {
        const diagnosis = makeDiagnosis({
          id: ctx.id("diag"), scanId: ctx.scanId, run: ctx.run, ts: ctx.ts(), status: "uncertain", lowEvidence: true,
          candidates: [
            makeScore(PROBLEMS.earlyBlight, 1, { unsure: [ev(RINGS_INDICATOR, RINGS_TEXT, "unsure", 3, "vlm")] }),
            makeScore(PROBLEMS.nitrogen, 1, { unsure: [ev("yellow_lower_leaves", "Lower leaves are turning yellow", "unsure", 3, "vlm")] }),
          ],
          photo: ["The leaves are too small and dark in these photos to read"],
          citations: [],
        });
        shared.diagId = diagnosis.id;
        return { type: "diagnosis", diagnosis };
      }),
      status("analysing", "diagnosed"),
      ...stage("plan", 700),
      status("diagnosed", "planned"),
      planStep((ctx) => ({
        steps: provisionalSteps(plantOf(ctx), "Take the photo in daylight, with the leaf close to the camera and in focus.", ctx.dates),
        summary: "There is not enough to go on from these photos. Take new ones in better light.",
        watchFor: ["Spots, yellowing or insects on any leaf"], provisional: true, citations: [],
      }), () => shared.diagId ?? ""),
    ],
  };
}

// ---- no plant in the photos

function noPlant(): Script {
  return {
    start: () => [
      status("queued", "analysing", 200),
      ...stage("plant_check", 700),
      status("analysing", "failed", 100),
      at(100, () => ({
        type: "failed",
        failure: {
          code: "no_plant_found", retryable: false,
          message: "We could not find a plant in your photos. Take a new photo with the plant in the middle.",
        },
      })),
    ],
  };
}

export function scriptFor(outcome: Exclude<NextScan, "auto">): Script {
  switch (outcome) {
    case "early_blight": return liveBlight();
    case "healthy": return healthy();
    case "low_evidence": return lowEvidence();
    case "no_plant_found": return noPlant();
  }
}

/** The outcome the next submit plays, consuming the scenario's `nextScan`. */
export function takeOutcome(plantId: string): Exclude<NextScan, "auto"> {
  const next = scenario.get().nextScan;
  if (next !== "auto") {
    scenario.set({ nextScan: "auto" });
    return next;
  }
  return plantId === IDS.tomato2 ? "early_blight" : "healthy";
}

// ---- the runner

function hasDepth(store: MockStore, scanId: string): boolean {
  return (store.scan(scanId)?.scan.images ?? []).some((i) => i.depth);
}

/** Plays `steps` into the store. A later `play` on the same scan (a species change mid-run) stops this one. */
async function play(store: MockStore, scanId: string, steps: Step[], onDone: () => void): Promise<void> {
  const gen = store.generation;
  const st = store.scan(scanId);
  if (!st) return;
  const token = (st.playing = (st.playing ?? 0) + 1);
  const current = () => store.generation === gen && st.playing === token && store.scan(scanId) === st;
  for (const step of steps) {
    if (step.needsDepth && !hasDepth(store, scanId)) continue;
    await store.sleep(step.delay * scenario.get().delayScale);
    if (!current()) return;
    store.emit(scanId, step.body(store.runCtx(scanId)));
  }
  if (!current()) return;
  onDone();
}

/** Starts the run: `run_started` and `pending -> queued` are sent before this returns. */
export function startRun(store: MockStore, scanId: string, script: Script): void {
  const st = store.scan(scanId);
  if (!st) return;
  st.scan.run += 1;
  st.scan.submitted_at = nowTs();
  const from = st.scan.status === "failed" ? "failed" : "pending";
  delete st.scan.failure;
  st.running = true;
  st.workingSpeciesId = store.plants.get(st.scan.plant_id)?.species?.id ?? null;
  store.scripts.set(scanId, script);
  const stages = hasDepth(store, scanId) ? ALL_STAGES : ALL_STAGES.filter((x) => x !== "measurement");
  store.emit(scanId, { type: "run_started", reason: "submit", rerun_stages: stages });
  store.emit(scanId, { type: "status_changed", from, to: "queued" });
  void play(store, scanId, script.start(store.runCtx(scanId)), () => {
    st.running = false;
    store.notify(scanId);
  });
}

/** Continues a run that stopped at its question. The caller has checked the question is open. */
export function answerRun(store: MockStore, scanId: string, answer: S["Answer"]): boolean {
  const st = store.scan(scanId);
  const script = store.scripts.get(scanId);
  if (!st || !script?.afterAnswer || !st.scan.open_question) return false;
  st.scan.open_question.status = "answering";
  st.running = true;
  void play(store, scanId, script.afterAnswer(store.runCtx(scanId), answer), () => {
    st.running = false;
    store.notify(scanId);
  });
  return true;
}

// ---- a species change (flows 7)

/**
 * The stages a species change runs (domain rules/stages.rs, `run_stages`): when the scan's current observations
 * have a plant check and segmentation (pipeline runs.rs, `earlier_run`), it keeps them, the image embeddings and
 * its measurements, and runs species identification on; otherwise every perception stage again. A typed name
 * ("something else") skips species identification.
 */
export function speciesRerunStages(store: MockStore, scanId: string, custom: boolean): S["Stage"][] {
  const st = store.scan(scanId);
  const has = (stage: S["ObservationStage"]) => (st?.observations ?? []).some((o) => o.stage === stage);
  const depth = hasDepth(store, scanId);
  const keepBase = has("plant_check") && has("segmentation");
  const measure = depth && (!keepBase || !has("measurement"));
  return ALL_STAGES.filter((stage) => {
    if (stage === "plant_check" || stage === "segmentation" || stage === "image_embedding") return !keepBase;
    if (stage === "species") return !custom;
    if (stage === "measurement") return measure;
    return stage !== "guided" && stage !== "verify";
  });
}

/**
 * Reruns a scan past `queued` after its plant's species changed, mid-run too, as the server does (pipeline
 * runs.rs, `species_confirmed`): the running script stops; `species_confirmed` and `run_started` with
 * `species_changed` go out under the new run; the open question is superseded, the stages `speciesRerunStages`
 * lists run, and the plan is superseded. The mock's rerun reads the new species as healthy, whatever it is.
 */
export function speciesRerun(store: MockStore, scanId: string, speciesId: string | null, customName?: string): void {
  const st = store.scan(scanId);
  if (!st) return;
  const stages = speciesRerunStages(store, scanId, speciesId === null);
  const has = (x: S["Stage"]) => stages.includes(x);
  const oldPlanId = st.plan?.id;
  const from = st.scan.status;
  st.scan.run += 1;
  st.running = true;
  st.workingSpeciesId = speciesId;
  // Under the new run, as the server sends them.
  store.emit(scanId, { type: "species_confirmed", species_id: speciesId, ...(customName ? { custom_name: customName } : {}), rerun: true });
  store.emit(scanId, { type: "run_started", reason: "species_changed", rerun_stages: stages });
  st.observations = st.observations.filter((o) => !stages.includes(o.pipeline_stage));
  const open = st.scan.open_question;
  if (open) store.emit(scanId, { type: "question_resolved", question_id: open.id, by: "superseded" });
  if (from !== "analysing") store.emit(scanId, { type: "status_changed", from, to: "analysing" });

  const shared: { diagId?: string } = {};
  const steps: Step[] = [
    ...(has("plant_check") ? plantCheckStage() : []),
    ...(has("segmentation") ? segmentationStage() : []),
    ...(has("species") ? speciesStage() : []),
    ...stage("disease", 900, [
      finding(250, (ctx) =>
        makeObservation({
          ...spec(ctx), confidence: 0.9, label: "Healthy", stage: "disease", pipelineStage: "disease",
          source: "classifier", region: { image_id: images(ctx).whole, bbox: [0.1, 0.05, 0.8, 0.9] },
        })),
    ]),
    ...(has("image_embedding") ? stage("image_embedding", 300) : []),
    ...(has("measurement")
      ? depthStage("measurement", 800, [
          finding(250, (ctx) => measureObs({ ...spec(ctx), quantity: "plant_height_cm", value: lastValue(ctx, "plant_height_cm", 30), n: 3, confidence: 0.86, imageId: images(ctx).whole })),
        ])
      : []),
    ...stage("context", 500, [weatherStep(0, 0, 8, 0)]),
    ...stage("vlm_findings", 900),
    ...stage("vlm_indicators", 200, [], "nothing_to_check"),
    ...stage("scoring", 400),
    at(200, (ctx) => {
      const diagnosis = makeDiagnosis({
        id: ctx.id("diag"), scanId: ctx.scanId, run: ctx.run, ts: ctx.ts(), status: "healthy",
        photo: ["No spots, holes or pests on the leaves"],
      });
      shared.diagId = diagnosis.id;
      return { type: "diagnosis", diagnosis };
    }),
    status("analysing", "diagnosed"),
    ...stage("plan", 600),
    status("diagnosed", "planned"),
    at(150, (ctx) => {
      const p = plantOf(ctx);
      const plan = makePlan({
        id: ctx.id("plan"), scanId: ctx.scanId, plantId: p.id, diagnosisId: shared.diagId ?? "", ts: ctx.ts(),
        summary: `${p.name} looks healthy.`, steps: healthySteps(p), watchFor: ["Spots or yellowing on the lowest leaves"],
        provisional: false, citations: [],
      });
      ctx.store.attachPlan(ctx.scanId, plan, tasksFromPlan(plan, ctx.dates, () => ctx.id("task")));
      ctx.store.scan(ctx.scanId)!.scan.plan_id = plan.id;
      return oldPlanId
        ? { type: "plan_superseded", old_plan_id: oldPlanId, new_plan_id: plan.id, reason: "species_changed" }
        : { type: "plan_ready", plan_id: plan.id, provisional: false };
    }),
  ];
  void play(store, scanId, steps, () => {
    st.running = false;
    store.notify(scanId);
  });
}

// ---- a guided soil check (flows 8; contracts 7, guided checks and the diagnosis)

/**
 * What the mock's guided job reads in each check's photo, in the server's words (pipeline results.rs,
 * `guided_observation`): the squeeze test's result, the settled jar's texture.
 */
const GUIDED_TEXTURE: Record<S["GuidedKind"], { label: string; value: string }> = {
  squeeze_test: { label: "Soil texture: holds_ball", value: "holds_ball" },
  jar_test: { label: "Soil texture: loamy", value: "loamy" },
};

/**
 * Runs the `guided` job of a check whose photo just arrived: its stage, then `guided_check_done` with a texture.
 * It runs beside the scan's own script, never stopping it. On a scan that is `diagnosed` or `planned`, the check
 * writes a new diagnosis with the soil updated, and a plan written from a diagnosis without that texture is
 * replaced (`plan_superseded`, reason `guided_check`). During a run the run's diagnosis takes it; on a failed scan
 * it only stays on the scan.
 */
export async function guidedCheck(store: MockStore, scanId: string, imageId: string, kind: S["GuidedKind"]): Promise<void> {
  const gen = store.generation;
  const live = () => store.generation === gen && store.scan(scanId) !== undefined;
  store.emit(scanId, { type: "stage_started", stage: "guided" });
  await store.sleep(1500 * scenario.get().delayScale);
  if (!live()) return;
  const ctx = store.runCtx(scanId);
  const texture = GUIDED_TEXTURE[kind];
  const result = makeObservation({
    ...spec(ctx), confidence: 0.85, label: texture.label, stage: "soil", pipelineStage: "guided", source: "vlm",
    detail: { aspect: "texture", value: texture.value, guided_kind: kind },
  });
  store.emit(scanId, { type: "guided_check_done", check: { image_id: imageId, kind, status: "done", result } });
  store.emit(scanId, { type: "stage_completed", stage: "guided", duration_ms: 1500 });

  const st = store.scan(scanId)!;
  if ((st.scan.status !== "diagnosed" && st.scan.status !== "planned") || !st.diagnosis) return;
  const before = st.diagnosis;
  const hadTexture = before.soil.some((o) => o.detail?.aspect === "texture" && o.detail.value === texture.value);
  const diagnosis: S["Diagnosis"] = {
    ...before,
    id: ctx.id("diag"),
    created_at: ctx.ts(),
    soil: [...before.soil.filter((o) => !(o.detail?.aspect === "texture" && o.detail.guided_kind === kind)), result],
  };
  store.emit(scanId, { type: "diagnosis", diagnosis });
  const plan = st.plan;
  if (!plan || hadTexture) return;
  // The mock's re-plan keeps the steps and their tasks; the server writes it afresh.
  const next: S["ActionPlan"] = { ...plan, id: ctx.id("plan"), diagnosis_id: diagnosis.id, created_at: ctx.ts() };
  st.plan = next;
  st.scan.plan_id = next.id;
  store.emit(scanId, { type: "plan_superseded", old_plan_id: plan.id, new_plan_id: next.id, reason: "guided_check" });
}
