import type { ImageKind, Observation, Plant, Question, ScanDetail, ScanEventBody, Stage } from "@/domain/types";

import { CITATIONS } from "./fixtures";
import { db, type Scenario } from "./store";
import { band, localDate, ts, ulid } from "./util";

// Scripted scan runs (docs/frontend 15): the early-blight run of contracts 4, a run that asks a
// question, and a healthy plant. Each step mutates the records the way the server would, then emits.

type Step = { at: number; run: (d: ScanDetail) => ScanEventBody | ScanEventBody[] | void };

const ALL_STAGES: Stage[] = [
  "plant_check",
  "segmentation",
  "species",
  "disease",
  "image_embedding",
  "context",
  "retrieve",
  "vlm_findings",
  "vlm_indicators",
  "scoring",
  "fusion",
  "plan",
];

function imageFor(d: ScanDetail, prefer: ImageKind[]) {
  for (const k of prefer) {
    const img = d.images.find((i) => i.kind === k);
    if (img) return img;
  }
  return d.images[0]!;
}

function observation(d: ScanDetail, o: Omit<Observation, "id" | "scan_id" | "run" | "created_at" | "confidence_band" | "model_version"> & { model_version?: string }): Observation {
  return {
    model_version: "mock-1",
    ...o,
    id: ulid(),
    scan_id: d.id,
    run: d.run,
    created_at: ts(),
    confidence_band: band(o.confidence),
  };
}

function stage(at: number, s: Stage, ms = 900, skipped?: string): Step[] {
  return [
    { at, run: () => ({ type: "stage_started", stage: s }) },
    { at: at + ms, run: () => ({ type: "stage_completed", stage: s, duration_ms: ms, ...(skipped ? { skipped: { reason: skipped } } : {}) }) },
  ];
}

function status(d: ScanDetail, to: ScanDetail["status"]): ScanEventBody {
  const from = d.status;
  d.status = to;
  syncPlant(d);
  return { type: "status_changed", from, to };
}

function syncPlant(d: ScanDetail) {
  const p = db.plants.find((x) => x.id === d.plant_id);
  if (p?.latest_scan?.scan_id === d.id) p.latest_scan.status = d.status;
}

const EARLY_BLIGHT_SCORE = {
  problem_id: "tomato_early_blight",
  name: "Early blight",
  kind: "fungal" as const,
  weather_fit: "favourable" as const,
  unsure: [],
};

function blightIndicators(ringsAnsweredBy: "vlm" | "user") {
  return [
    {
      indicator_id: "eb_target_rings",
      text: "Concentric rings in the spots, like a target",
      weight: 3,
      answer: "yes" as const,
      reason: ringsAnsweredBy === "user" ? "You said the spots have rings." : "Rings are visible in the leaf close-up.",
      source: ringsAnsweredBy,
    },
    {
      indicator_id: "eb_lower_leaves_first",
      text: "Starts on the lowest, oldest leaves",
      weight: 2,
      answer: "yes" as const,
      reason: "The affected leaves are near the soil.",
      source: "vlm" as const,
    },
    {
      indicator_id: "eb_yellow_halo",
      text: "Yellow tissue around the spots",
      weight: 1,
      answer: "yes" as const,
      reason: "A yellow margin surrounds the larger spots.",
      source: "vlm" as const,
    },
  ];
}

function blightDiagnosis(d: ScanDetail, ladybugId: string, soil: Observation, ringsBy: "vlm" | "user") {
  const matched = blightIndicators(ringsBy);
  const primary = { ...EARLY_BLIGHT_SCORE, matched, contradicted: [], score: 6 };
  const septoria = {
    problem_id: "tomato_septoria_leaf_spot",
    name: "Septoria leaf spot",
    kind: "fungal" as const,
    weather_fit: "favourable" as const,
    matched: [],
    contradicted: [
      {
        indicator_id: "sls_tiny_spots",
        text: "Many tiny spots, 1.5–3 mm across",
        weight: 2,
        answer: "no" as const,
        reason: "The spots are about a centimetre across.",
        source: "vlm" as const,
      },
    ],
    unsure: [],
    score: -2,
  };
  return {
    id: ulid(),
    scan_id: d.id,
    run: d.run,
    status: "confirmed" as const,
    low_evidence: false,
    primary,
    candidates: [primary, septoria],
    organisms: [
      {
        observation_id: ladybugId,
        organism_id: "ladybug_larva",
        name: "Ladybug larva",
        role: "beneficial" as const,
        preys_on: [{ organism_id: "aphid", name: "Aphids" }],
      },
    ],
    soil: [soil],
    risks: [
      { risk: "fungal_spread", level: "high" as const, drivers: ["16 hours over 90% humidity forecast on " + localDate(2), "3 humid days in the past week"] },
    ],
    evidence_basis: {
      user_input: ringsBy === "user" ? ["You said the spots have rings"] : [],
      photo: ["Leaf close-up: brown spots with rings on the lower leaves"],
      modelled: ["Weather: humid, 14 °C", "Soil: dry at the surface"],
    },
    citations: [CITATIONS.omafra_early_blight!, CITATIONS.umn_tomato_diseases!],
    model_versions: { classifier: "mock-1", vlm: "mock-1" },
    created_at: ts(),
  };
}

function blightPlan(d: ScanDetail, diagnosisId: string, provisional: boolean) {
  const taskIds: string[] = [];
  const steps = [
    {
      id: "s1",
      order: 1,
      title: "Remove the lowest infected leaves",
      detail: "Cut off leaves with spots up to 30 cm above the soil. Bag them; don't compost them.",
      category: "cultural" as const,
      when: { kind: "now" as const },
      targets: [{ table: "disease" as const, id: "tomato_early_blight" }],
      citations: ["omafra_early_blight"],
    },
    {
      id: "s2",
      order: 2,
      title: "Mulch under the plant",
      detail: "A 5 cm layer of straw stops spores splashing up from the soil when it rains.",
      category: "cultural" as const,
      when: { kind: "by_date" as const, due: localDate(1) },
      targets: [{ table: "disease" as const, id: "tomato_early_blight" }],
      citations: ["umn_tomato_diseases"],
    },
    {
      id: "s3",
      order: 3,
      title: "Water at the base in the morning",
      detail: "Keep the leaves dry. Avoid overhead watering while humid weather lasts.",
      category: "cultural" as const,
      when: { kind: "recurring" as const, every_days: 2, until: localDate(14) },
      targets: [],
      citations: ["umn_tomato_diseases"],
    },
    {
      id: "s4",
      order: 4,
      title: "Check for new spots",
      detail: "Look at the lower leaves, top and underside, for new brown spots with rings.",
      category: "monitoring" as const,
      when: { kind: "recurring" as const, every_days: 3, until: localDate(21) },
      targets: [],
      citations: ["omafra_early_blight"],
    },
    {
      id: "s5",
      order: 5,
      title: "Leave the ladybug larvae",
      detail: "They eat aphids. Don't spray where you see them.",
      category: "biological" as const,
      when: { kind: "now" as const },
      targets: [],
      citations: ["umn_tomato_diseases"],
    },
  ];
  for (const s of steps) {
    if (s.category === "biological") continue;
    const id = ulid();
    taskIds.push(id);
    db.tasks.unshift({
      id,
      garden_id: d.garden_id,
      plant_id: d.plant_id,
      plan_id: "",
      plan_step_id: s.id,
      source: "plan",
      title: s.title,
      detail: s.detail,
      due: s.when.kind === "by_date" ? s.when.due : localDate(0),
      ...(s.when.kind === "recurring" ? { repeat: { every_days: s.when.every_days } } : {}),
      created_at: ts(),
    });
  }
  const plan = {
    id: ulid(),
    scan_id: d.id,
    plant_id: d.plant_id,
    version: 1,
    provisional,
    diagnosis_id: diagnosisId,
    constraints: [],
    summary: "Early blight on the lower leaves. Remove them, keep the leaves dry, and check every few days while the weather stays humid.",
    steps,
    watch_for: ["New spots higher up the plant", "Spots on the fruit near the stem"],
    citations: [CITATIONS.omafra_early_blight!, CITATIONS.umn_tomato_diseases!],
    task_ids: taskIds,
    llm_model: "mock",
    prompt_version: "mock-1",
    created_at: ts(),
  };
  db.tasks.filter((t) => taskIds.includes(t.id)).forEach((t) => (t.plan_id = plan.id));
  return plan;
}

function healthyPlan(d: ScanDetail, diagnosisId: string) {
  return {
    id: ulid(),
    scan_id: d.id,
    plant_id: d.plant_id,
    version: 1,
    provisional: false,
    diagnosis_id: diagnosisId,
    constraints: [],
    summary: "No problems found in these photos. Keep up the routine care below.",
    steps: [
      {
        id: "s1",
        order: 1,
        title: "Water when the top 2 cm of soil is dry",
        detail: "Soil moisture is moderate today; check again in two days.",
        category: "cultural" as const,
        when: { kind: "recurring" as const, every_days: 2 },
        targets: [],
        citations: ["umn_tomato_diseases"],
      },
      {
        id: "s2",
        order: 2,
        title: "Cover it before the frost",
        detail: "Frost is likely in four days. Cover the plant in the evening with a sheet or frost cloth.",
        category: "cultural" as const,
        when: { kind: "on_trigger" as const, trigger: "frost" as const },
        targets: [],
        citations: ["umn_tomato_diseases"],
      },
    ],
    watch_for: ["Spots or yellowing on the lower leaves"],
    citations: [CITATIONS.umn_tomato_diseases!],
    task_ids: [],
    llm_model: "mock",
    prompt_version: "mock-1",
    created_at: ts(),
  };
}

function plantStatusFrom(d: ScanDetail): Plant["status"] {
  if (d.diagnosis?.status === "healthy") return "healthy";
  if (d.diagnosis?.status === "confirmed") return "sick";
  return "watch";
}

function finishPlan(d: ScanDetail, plan: NonNullable<ScanDetail["plan"]>): ScanEventBody[] {
  d.plan = plan;
  d.plan_id = plan.id;
  const p = db.plants.find((x) => x.id === d.plant_id);
  if (p) p.status = plantStatusFrom(d);
  return [status(d, "planned"), { type: "plan_ready", plan_id: plan.id, provisional: plan.provisional }];
}

/** Builds the steps of a scenario's first run, from submit to its first resting point or question. */
function script(scenario: Scenario, d: ScanDetail): Step[] {
  const leaf = imageFor(d, ["leaf_closeup", "whole_plant"]);
  const whole = imageFor(d, ["whole_plant"]);
  const soilImg = d.images.find((i) => i.kind === "soil_surface") ?? whole;
  const ids = { ladybug: "", soil: null as Observation | null };
  const steps: Step[] = [
    { at: 0, run: () => ({ type: "run_started", reason: "submit", rerun_stages: ALL_STAGES }) },
    { at: 200, run: (s) => status(s, "queued") },
    { at: 900, run: (s) => status(s, "analysing") },
    ...stage(1200, "plant_check", 700),
    ...stage(2000, "segmentation", 900),
    ...stage(3000, "species", 800),
    ...stage(3900, "disease", 1100),
  ];

  if (scenario !== "healthy") {
    steps.push({
      at: 5050,
      run: (s) => {
        const o = observation(s, {
          stage: "disease",
          pipeline_stage: "disease",
          label: "Early blight",
          confidence: 0.71,
          source: "classifier",
          ref: { table: "disease", id: "tomato_early_blight" },
          region: { image_id: leaf.id, bbox: [0.32, 0.38, 0.3, 0.26] },
        });
        s.observations.push(o);
        return { type: "finding", observation: o };
      },
    });
  }

  steps.push(
    ...stage(5200, "image_embedding", 600),
    { at: 5900, run: () => ({ type: "stage_started", stage: "context" }) },
    {
      at: 6400,
      run: (s) => {
        s.weather = db.weather;
        return { type: "weather_loaded", weather: db.weather };
      },
    },
    { at: 6600, run: () => ({ type: "stage_completed", stage: "context", duration_ms: 700 }) },
    {
      at: 6800,
      run: () => ({
        type: "candidates_selected",
        candidates: [
          { problem_id: "tomato_early_blight", name: "Early blight", kind: "fungal", source: "classifier" },
          { problem_id: "tomato_septoria_leaf_spot", name: "Septoria leaf spot", kind: "fungal", source: "lookalike" },
          { problem_id: "tomato_late_blight", name: "Late blight", kind: "fungal", source: "species" },
          { problem_id: "tomato_bacterial_spot", name: "Bacterial spot", kind: "bacterial", source: "lookalike" },
          { problem_id: "aphids", name: "Aphids", kind: "pest", source: "species" },
          { problem_id: "nitrogen_deficiency", name: "Nitrogen deficiency", kind: "abiotic", source: "species" },
        ],
      }),
    },
    ...stage(7000, "vlm_findings", 1800),
  );

  if (scenario !== "healthy") {
    steps.push({
      at: 8850,
      run: (s) => {
        const o = observation(s, {
          stage: "organism",
          pipeline_stage: "vlm_findings",
          label: "Ladybug larva",
          confidence: 0.84,
          source: "vlm",
          role: "beneficial",
          ref: { table: "organism", id: "ladybug_larva" },
          detail: { count: "one" },
          region: { image_id: whole.id, bbox: [0.58, 0.22, 0.12, 0.1] },
        });
        ids.ladybug = o.id;
        s.observations.push(o);
        return { type: "finding", observation: o };
      },
    });
  }
  steps.push({
    at: 8950,
    run: (s) => {
      const o = observation(s, {
        stage: "soil",
        pipeline_stage: "vlm_findings",
        label: "Soil: dry at the surface",
        confidence: 0.66,
        source: "vlm",
        detail: { aspect: "moisture", value: "dry" },
        region: { image_id: soilImg.id, bbox: [0.05, 0.7, 0.9, 0.28] },
      });
      ids.soil = o;
      s.observations.push(o);
      return { type: "finding", observation: o };
    },
  });
  steps.push({
    at: 9050,
    run: () => ({
      type: "species_suggested",
      candidates: [
        { species_id: "tomato", name: "Tomato", score: 0.93, source: "bioclip" },
        { species_id: "potato", name: "Potato", score: 0.04, source: "bioclip" },
      ],
      working_species_id: "tomato",
      needs_confirmation: false,
    }),
  });

  if (scenario === "question") {
    steps.push(...stage(9200, "vlm_indicators", 1300), ...stage(10600, "scoring", 600), {
      at: 11300,
      run: (s) => {
        const q: Question = {
          id: ulid(),
          run: s.run,
          kind: "indicator",
          text: "Do the spots have rings inside them, like a target?",
          options: [
            { id: "yes", label: "Yes, rings" },
            { id: "no", label: "No, plain spots" },
            { id: "unsure", label: "I can't tell" },
          ],
          candidates: ["tomato_early_blight", "tomato_septoria_leaf_spot"],
          asked_at: ts(),
          continue_at: ts(new Date(Date.now() + 10 * 60_000)),
          status: "open",
        };
        s.open_question = q;
        return [{ type: "question", question: q }, status(s, "awaiting_answer")];
      },
    });
    return steps;
  }

  steps.push(...stage(9200, "vlm_indicators", 1300), ...stage(10600, "scoring", 700), {
    at: 11400,
    run: (s) => {
      const diagnosis =
        scenario === "healthy"
          ? {
              id: ulid(),
              scan_id: s.id,
              run: s.run,
              status: "healthy" as const,
              low_evidence: false,
              candidates: [],
              organisms: [],
              soil: ids.soil ? [ids.soil] : [],
              risks: [{ risk: "frost", level: "medium" as const, drivers: ["Frost likely on " + localDate(4)] }],
              evidence_basis: { user_input: [], photo: ["Whole plant: even green leaves, no spots"], modelled: ["Weather: mild, 14 °C"] },
              citations: [CITATIONS.umn_tomato_diseases!],
              model_versions: { classifier: "mock-1" },
              created_at: ts(),
            }
          : blightDiagnosis(s, ids.ladybug, ids.soil!, "vlm");
      s.diagnosis = diagnosis;
      s.diagnosis_id = diagnosis.id;
      return [{ type: "diagnosis", diagnosis }, status(s, "diagnosed")];
    },
  });
  steps.push(...stage(11700, "plan", 1500), {
    at: 13300,
    run: (s) => finishPlan(s, scenario === "healthy" ? healthyPlan(s, s.diagnosis_id!) : blightPlan(s, s.diagnosis_id!, false)),
  });
  return steps;
}

function play(scanId: string, steps: Step[]) {
  for (const step of steps) {
    db.schedule(scanId, step.at, () => {
      const entry = db.scan(scanId);
      if (!entry) return;
      const out = step.run(entry.detail);
      if (!out) return;
      (Array.isArray(out) ? out : [out]).forEach((e) => db.emit(scanId, e));
    });
  }
}

export function startRun(d: ScanDetail) {
  d.run += 1;
  d.submitted_at = ts();
  play(d.id, script(db.scenario, d));
}

/** An answer to the question scenario: the run continues and confirms early blight (contracts 3, answers). */
export function answerQuestion(d: ScanDetail, questionId: string, option: "yes" | "no" | "unsure") {
  const soil = d.observations.find((o) => o.stage === "soil")!;
  const ladybug = d.observations.find((o) => o.stage === "organism");
  d.open_question = undefined;
  play(d.id, [
    { at: 0, run: () => ({ type: "question_resolved", question_id: questionId, by: "user" }) },
    { at: 100, run: (s) => status(s, "analysing") },
    ...stage(300, "scoring", 700),
    {
      at: 1100,
      run: (s) => {
        const diagnosis = blightDiagnosis(s, ladybug?.id ?? "", soil, "user");
        if (option !== "yes") {
          diagnosis.status = "uncertain" as never;
          diagnosis.primary.matched = diagnosis.primary.matched.filter((m) => m.indicator_id !== "eb_target_rings");
          diagnosis.primary.unsure = [
            { ...blightIndicators("user")[0]!, answer: "unsure", reason: option === "no" ? "You said the spots have no rings." : "You couldn't tell." },
          ] as never;
        }
        s.diagnosis = diagnosis;
        s.diagnosis_id = diagnosis.id;
        return [{ type: "diagnosis", diagnosis }, status(s, "diagnosed")];
      },
    },
    ...stage(1400, "plan", 1400),
    { at: 2900, run: (s) => finishPlan(s, blightPlan(s, s.diagnosis_id!, option !== "yes")) },
  ]);
}
