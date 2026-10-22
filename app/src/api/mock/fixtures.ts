// The demo story as typed fixtures (docs/design/analytics.md 6, "Seeded data for mock mode").
// Every shape is a generated contract type, so a contract change that breaks one fails tsc.
//
// The story: Tomato 1 got early blight on Oct 11 and is recovering under its plan. One step of
// that plan is a weekly check of Tomato 2, 40 cm away. Today's check is the live scan.
// Dates are relative to the device's date: "today - 27 days" is the first scan, "today" is the live scan.

import { MOCK_IMAGE_CREDIT, MOCK_IMAGE_SIZE, mockImageUrl, type MockImageName } from "./images";
import { bandFor, DateKit, type S } from "./util";

// ----------------------------------------------------------------------------------------------
// Ids other code can rely on

export const IDS = {
  user: "user_alex",
  garden: "garden_back",
  bedEast: "bed_east",
  bedSouth: "bed_south",
  tomato1: "plant_tomato_1",
  tomato2: "plant_tomato_2",
  pepper: "plant_pepper",
  basil: "plant_basil",
  lettuce: "plant_lettuce",
  kale: "plant_kale",
} as const;

export type PlantKey = "tomato1" | "tomato2" | "pepper" | "basil" | "lettuce" | "kale";

/** A scan the store holds: the Scan row plus what ScanDetail adds around it. */
export interface StoredScan {
  scan: S["Scan"];
  observations: S["Observation"][];
  diagnosis?: S["Diagnosis"];
  plan?: S["ActionPlan"];
  weather?: S["WeatherSummary"];
  events: S["ScanEvent"][];
  /** A scripted run is still emitting events. */
  running: boolean;
}

export type StoredPlant = Omit<S["Plant"], "latest_scan">;

export interface Seed {
  user: S["Me"];
  garden: S["Garden"];
  map: S["GardenMap"];
  plants: StoredPlant[];
  scans: StoredScan[];
  tasks: S["Task"][];
  memories: S["Memory"][];
  weather: S["WeatherSummary"];
  species: S["SpeciesSummary"][];
}

// ----------------------------------------------------------------------------------------------
// Knowledge used by fixtures and scripted runs

export interface ProblemDef {
  id: string;
  name: string;
  kind: S["ProblemKind"];
}

export const PROBLEMS = {
  earlyBlight: { id: "tomato_early_blight", name: "Early blight", kind: "fungal" },
  septoria: { id: "tomato_septoria_leaf_spot", name: "Septoria leaf spot", kind: "fungal" },
  lateBlight: { id: "tomato_late_blight", name: "Late blight", kind: "fungal" },
  bacterialSpot: { id: "tomato_bacterial_spot", name: "Bacterial spot", kind: "bacterial" },
  bacterialSpeck: { id: "tomato_bacterial_speck", name: "Bacterial speck", kind: "bacterial" },
  targetSpot: { id: "tomato_target_spot", name: "Target spot", kind: "fungal" },
  leafMold: { id: "tomato_leaf_mold", name: "Leaf mold", kind: "fungal" },
  spiderMites: { id: "spider_mites", name: "Spider mites", kind: "pest" },
  fusarium: { id: "tomato_fusarium_wilt", name: "Fusarium wilt", kind: "fungal" },
  aphids: { id: "aphids", name: "Aphids", kind: "pest" },
  nitrogen: { id: "nitrogen_deficiency", name: "Nitrogen deficiency", kind: "abiotic" },
} as const satisfies Record<string, ProblemDef>;

export const CITATIONS = {
  blight: {
    source_id: "umn-ext-early-blight",
    publisher: "University of Minnesota Extension",
    title: "Early blight of tomato and potato",
    url: "https://extension.umn.edu/disease-management/early-blight-tomato-and-potato",
  },
  septoria: {
    source_id: "umn-ext-septoria",
    publisher: "University of Minnesota Extension",
    title: "Septoria leaf spot of tomato",
    url: "https://extension.umn.edu/disease-management/septoria-leaf-spot-tomato",
  },
  aphids: {
    source_id: "ucipm-aphids",
    publisher: "UC IPM",
    title: "Aphids pest notes",
    url: "https://ipm.ucanr.edu/home-and-landscape/aphids/",
  },
  nitrogen: {
    source_id: "osu-ext-nitrogen",
    publisher: "Oregon State Extension",
    title: "Nitrogen in the home garden",
    url: "https://extension.oregonstate.edu/gardening",
  },
} as const satisfies Record<string, S["Citation"]>;

// ----------------------------------------------------------------------------------------------
// Builders (also used by runs.ts)

export interface ObservationSpec {
  id: string;
  scanId: string;
  run?: number;
  ts: string;
  confidence: number;
  label: string;
  stage: S["ObservationStage"];
  pipelineStage: S["Stage"];
  source: S["ObservationSource"];
  model?: string;
  detail?: S["ObservationDetail"];
  region?: S["Region"];
  role?: S["OrganismRole"];
  ref?: S["KnowledgeRef"] | null;
}

export function makeObservation(o: ObservationSpec): S["Observation"] {
  const obs: S["Observation"] = {
    confidence: o.confidence,
    confidence_band: bandFor(o.confidence),
    created_at: o.ts,
    id: o.id,
    label: o.label,
    model_version: o.model ?? "mock-1",
    pipeline_stage: o.pipelineStage,
    run: o.run ?? 1,
    scan_id: o.scanId,
    source: o.source,
    stage: o.stage,
  };
  if (o.detail) obs.detail = o.detail;
  if (o.region) obs.region = o.region;
  if (o.role) obs.role = o.role;
  if (o.ref !== undefined) obs.ref = o.ref;
  return obs;
}

const QUANTITY_LABEL: Record<S["MeasureQuantity"], (v: number) => string> = {
  plant_height_cm: (v) => `Plant height about ${v} cm`,
  canopy_width_cm: (v) => `Canopy about ${v} cm wide`,
  leaf_length_cm: (v) => `Leaf about ${v} cm long`,
  leaf_area_cm2: (v) => `Leaf area about ${v} cm²`,
  lesion_diameter_mm: (v) => `Spots about ${v} mm across`,
  lesion_count: (v) => `${v} spots counted`,
  affected_area_pct: (v) => `About ${v}% of the leaf is affected`,
  hole_diameter_mm: (v) => `Holes about ${v} mm across`,
  hole_count: (v) => `${v} holes counted`,
};

/** A `measurement` observation. Growth points (height, width) are derived from these. */
export function measureObs(o: {
  id: string;
  scanId: string;
  run?: number;
  ts: string;
  quantity: S["MeasureQuantity"];
  value: number;
  min?: number;
  max?: number;
  n?: number;
  distance?: number;
  confidence?: number;
  imageId?: string;
}): S["Observation"] {
  return makeObservation({
    id: o.id,
    scanId: o.scanId,
    run: o.run,
    ts: o.ts,
    confidence: o.confidence ?? 0.88,
    label: QUANTITY_LABEL[o.quantity](o.value),
    stage: "measurement",
    pipelineStage: "measurement",
    source: "measurement",
    model: "mock-lidar-1",
    detail: {
      measurement: {
        quantity: o.quantity,
        value: o.value,
        min: o.min ?? o.value,
        max: o.max ?? o.value,
        n: o.n ?? 1,
        distance_m: o.distance ?? 0.8,
      },
    },
    region: o.imageId ? { image_id: o.imageId, bbox: [0.1, 0.05, 0.8, 0.9] } : undefined,
  });
}

export function ev(
  indicator_id: string,
  text: string,
  answer: S["Answer"],
  weight: number,
  source: S["EvidenceSource"] = "vlm",
  reason = "",
): S["IndicatorEvidence"] {
  return { indicator_id, text, answer, weight, source, reason: reason || text };
}

export function makeScore(
  problem: ProblemDef,
  score: number,
  parts: {
    matched?: S["IndicatorEvidence"][];
    contradicted?: S["IndicatorEvidence"][];
    unsure?: S["IndicatorEvidence"][];
    fit?: S["WeatherFit"];
  } = {},
): S["CandidateScore"] {
  return {
    problem_id: problem.id,
    name: problem.name,
    kind: problem.kind,
    score,
    matched: parts.matched ?? [],
    contradicted: parts.contradicted ?? [],
    unsure: parts.unsure ?? [],
    weather_fit: parts.fit ?? "neutral",
  };
}

export interface DiagnosisSpec {
  id: string;
  scanId: string;
  run?: number;
  ts: string;
  status: S["DiagnosisStatus"];
  primary?: S["CandidateScore"];
  candidates?: S["CandidateScore"][];
  risks?: S["Risk"][];
  organisms?: S["DiagnosisOrganism"][];
  soil?: S["Observation"][];
  lowEvidence?: boolean;
  photo?: string[];
  modelled?: string[];
  user?: string[];
  citations?: S["Citation"][];
}

export function makeDiagnosis(d: DiagnosisSpec): S["Diagnosis"] {
  const out: S["Diagnosis"] = {
    id: d.id,
    scan_id: d.scanId,
    run: d.run ?? 1,
    created_at: d.ts,
    status: d.status,
    candidates: d.candidates ?? (d.primary ? [d.primary] : []),
    citations: d.citations ?? [],
    evidence_basis: { photo: d.photo ?? [], modelled: d.modelled ?? [], user_input: d.user ?? [] },
    low_evidence: d.lowEvidence ?? false,
    model_versions: { perception: "mock-1", vlm: "mock-vlm-1", labels: "mock-labels-1" },
    organisms: d.organisms ?? [],
    risks: d.risks ?? [],
    soil: d.soil ?? [],
  };
  if (d.primary) out.primary = d.primary;
  return out;
}

export interface PlanSpec {
  id: string;
  scanId: string;
  plantId: string;
  diagnosisId: string;
  ts: string;
  summary: string;
  steps: S["PlanStep"][];
  watchFor: string[];
  provisional?: boolean;
  version?: number;
  citations?: S["Citation"][];
  supersedes?: string;
  taskIds?: string[];
}

export function makePlan(p: PlanSpec): S["ActionPlan"] {
  const plan: S["ActionPlan"] = {
    id: p.id,
    scan_id: p.scanId,
    plant_id: p.plantId,
    diagnosis_id: p.diagnosisId,
    created_at: p.ts,
    version: p.version ?? 1,
    summary: p.summary,
    provisional: p.provisional ?? false,
    steps: p.steps,
    watch_for: p.watchFor,
    citations: p.citations ?? [],
    constraints: [],
    task_ids: p.taskIds ?? [],
    llm_model: "mock-llm-1",
    prompt_version: "mock-plan-1",
  };
  if (p.supersedes) plan.supersedes = p.supersedes;
  return plan;
}

type Named = { id: string; name: string };

/** Plan steps for early blight (Tomato 1's plan v1 and Tomato 2's confirmed plan). */
export function blightSteps(
  k: DateKit,
  ago: number,
  plant: Named,
  opts: { neighbour?: Named; crowded?: boolean; ladybug?: boolean } = {},
): S["PlanStep"][] {
  const target: S["PlanTarget"][] = [{ table: "disease", id: PROBLEMS.earlyBlight.id }];
  const cite = [CITATIONS.blight.source_id];
  const steps: S["PlanStep"][] = [
    {
      id: "remove_infected_leaves",
      order: 1,
      category: "cultural",
      title: "Remove the infected leaves",
      detail:
        "Cut off the leaves with spots at the stem and bag them. Do not compost them. Wipe the pruners with alcohol between cuts.",
      targets: target,
      citations: cite,
      plant_id: plant.id,
      when: { kind: "now" },
    },
    {
      id: "mulch_base",
      order: 2,
      category: "cultural",
      title: "Mulch around the base",
      detail: "Add 5 cm of straw or bark so rain does not splash soil onto the lower leaves.",
      targets: target,
      citations: cite,
      plant_id: plant.id,
      when: { kind: "by_date", due: k.date(ago - 2) },
    },
  ];
  if (opts.crowded) {
    steps.push({
      id: "give_room",
      order: steps.length + 1,
      category: "cultural",
      title: `Give ${plant.name} more room`,
      detail:
        "Tie both tomatoes to stakes and take off the lowest leaves so air moves between them. They are 40 cm apart.",
      targets: target,
      citations: cite,
      plant_id: plant.id,
      when: { kind: "by_date", due: k.date(ago - 3) },
    });
  }
  if (opts.ladybug) {
    steps.push({
      id: "leave_ladybug",
      order: steps.length + 1,
      category: "biological",
      title: "Leave the ladybug larva alone",
      detail: "It eats aphids. Do not spray anything on these leaves while it is there.",
      targets: [{ table: "organism", id: "ladybug" }],
      citations: [],
      plant_id: plant.id,
      when: { kind: "now" },
    });
  }
  steps.push({
    id: "check_plant",
    order: steps.length + 1,
    category: "monitoring",
    title: `Check ${plant.name} for new spots`,
    detail: "Look at the upper and lower leaves. New spots on upper leaves mean the spread continues.",
    targets: target,
    citations: cite,
    plant_id: plant.id,
    when: { kind: "recurring", every_days: 3 },
  });
  if (opts.neighbour) {
    steps.push({
      id: "check_neighbour",
      order: steps.length + 1,
      category: "monitoring",
      title: `Check ${opts.neighbour.name}'s lower leaves`,
      detail: `${opts.neighbour.name} is 40 cm away. Look at the lowest leaves for the first spots.`,
      targets: target,
      citations: cite,
      plant_id: opts.neighbour.id,
      when: { kind: "recurring", every_days: 7 },
    });
  }
  return steps;
}

/** A short plan for a result that is not settled. */
export function provisionalSteps(plant: Named, what: string, k: DateKit): S["PlanStep"][] {
  return [
    {
      id: "remove_worst_leaves",
      order: 1,
      category: "cultural",
      title: "Remove the leaves with the most spots",
      detail: "Bag them, do not compost. This is safe whichever problem it turns out to be.",
      targets: [],
      citations: [],
      plant_id: plant.id,
      when: { kind: "now" },
    },
    {
      id: "check_again",
      order: 2,
      category: "monitoring",
      title: `Check ${plant.name} again in 2 days`,
      detail: `Take a close-up of the same leaves. ${what}`,
      targets: [],
      citations: [],
      plant_id: plant.id,
      when: { kind: "by_date", due: k.date(-2) },
    },
  ];
}

export function healthySteps(plant: Named): S["PlanStep"][] {
  return [
    {
      id: "weekly_look",
      order: 1,
      category: "monitoring",
      title: `Look over ${plant.name} next week`,
      detail: "Nothing to treat. Water at the base and check the undersides of leaves once a week.",
      targets: [],
      citations: [],
      plant_id: plant.id,
      when: { kind: "recurring", every_days: 7 },
    },
  ];
}

export function aphidSteps(plant: Named, ladybug: boolean): S["PlanStep"][] {
  const target: S["PlanTarget"][] = [{ table: "disease", id: PROBLEMS.aphids.id }];
  return [
    {
      id: ladybug ? "leave_ladybug" : "rinse_aphids",
      order: 1,
      category: ladybug ? "biological" : "mechanical",
      title: ladybug ? "Leave the ladybug larvae to eat the aphids" : "Rinse the aphids off with water",
      detail: ladybug
        ? "A ladybug larva eats dozens of aphids a day. Do not spray."
        : "Use a firm stream of water on the new growth, morning is best. Repeat every 2 days.",
      targets: ladybug ? [{ table: "organism", id: "ladybug" }, ...target] : target,
      citations: [CITATIONS.aphids.source_id],
      plant_id: plant.id,
      when: { kind: "now" },
    },
  ];
}

/** Open tasks a plan creates (one per step), like the server's plan writer. */
export function tasksFromPlan(
  plan: S["ActionPlan"],
  k: DateKit,
  idFor: (step: S["PlanStep"]) => string,
): S["Task"][] {
  return plan.steps.map((step) => {
    const task: S["Task"] = {
      id: idFor(step),
      garden_id: IDS.garden,
      plant_id: step.plant_id ?? plan.plant_id,
      // No scan link: the server sets one only on a user's guided-check task (contracts 9; pipeline replan).
      plan_id: plan.id,
      plan_step_id: step.id,
      source: "plan",
      title: step.title,
      detail: step.detail,
      created_at: plan.created_at,
    };
    if (step.when.kind === "by_date") task.due = step.when.due;
    else if (step.when.kind === "now") task.due = k.date(0);
    else if (step.when.kind === "recurring") {
      task.due = k.date(-step.when.every_days);
      task.repeat = { every_days: step.when.every_days };
    }
    return task;
  });
}

export function makeWeather(
  k: DateKit,
  o: { ago: number; rh90: number; run: number; humidAhead?: number; frostInDays?: number },
): S["WeatherSummary"] {
  const humid = [14, 16, 15, 12];
  const forecast: S["ForecastDay"][] = [];
  for (let i = 1; i <= 7; i++) {
    const isHumid = i <= (o.humidAhead ?? 0);
    const frost = o.frostInDays === i;
    forecast.push({
      date: k.date(o.ago - i),
      t_min_c: frost ? -1 : 5 + ((i * 3) % 4),
      t_max_c: frost ? 6 : 12 + ((i * 5) % 4),
      mean_rh_pct: isHumid ? 88 : 62 + ((i * 7) % 8),
      hours_rh_over_90: isHumid ? humid[i - 1] : (i * 2) % 4,
      precip_mm: isHumid ? 2.5 + (i % 3) : i % 3 === 0 ? 1 : 0,
      frost_risk: frost,
    });
  }
  return {
    provider: "open_meteo",
    fetched_at: k.ts(o.ago, 8, 30),
    location_precision: "city",
    current: { temp_c: 11, rh_pct: 84, precip_mm_last_24h: o.rh90 > 40 ? 3 : 0, wind_kmh: 9 },
    past_7_days: {
      hours_rh_over_90: o.rh90,
      longest_humid_run_days: o.run,
      mean_rh_pct: o.run >= 3 ? 82 : 66,
      precip_mm: o.run >= 3 ? 14 : 4,
    },
    soil: {
      et0_mm_last_24h: 1.2,
      moisture_0_9cm_m3m3: 0.31,
      moisture_9_27cm_m3m3: 0.34,
      temp_6cm_c: 9.5,
    },
    forecast,
  };
}

/** Days ahead of the forecast's frost after `ago` (1..6): the coming Thursday, or tomorrow on a
 * Thursday, since the home's strip shows only the next 6 days and next week's Thursday would fall off it. */
export function frostInDays(k: DateKit, ago = 0): number {
  for (let i = 1; i <= 6; i++) if (k.weekday(ago - i) === 4) return i;
  return 1;
}

// ----------------------------------------------------------------------------------------------
// The garden

interface PlantDef {
  key: PlantKey;
  id: string;
  name: string;
  species: S["SpeciesRef"];
  photo: MockImageName;
  /** Heights and widths at Sep 20, Sep 27, Oct 4, Oct 11, Oct 17 (cm). */
  heights: number[];
  widths: number[];
  at: { x: number; y: number; bed?: string; r: number };
}

const SPECIES = {
  tomato: { id: "tomato", common_name: "Tomato", scientific_name: "Solanum lycopersicum" },
  pepper: { id: "pepper", common_name: "Pepper", scientific_name: "Capsicum annuum" },
  basil: { id: "basil", common_name: "Basil", scientific_name: "Ocimum basilicum" },
  lettuce: { id: "lettuce", common_name: "Lettuce", scientific_name: "Lactuca sativa" },
  kale: { id: "kale", common_name: "Kale", scientific_name: "Brassica oleracea" },
} as const satisfies Record<string, S["SpeciesRef"]>;

const PLANTS: PlantDef[] = [
  { key: "tomato1", id: IDS.tomato1, name: "Tomato 1", species: SPECIES.tomato, photo: "tomato",
    heights: [32, 41, 50, 58, 62], widths: [30, 35, 40, 44, 48], at: { x: 4.8, y: 2.0, bed: IDS.bedEast, r: 0.25 } },
  { key: "tomato2", id: IDS.tomato2, name: "Tomato 2", species: SPECIES.tomato, photo: "tomato",
    heights: [28, 35, 43, 50, 56], widths: [26, 30, 35, 39, 44], at: { x: 5.2, y: 2.0, bed: IDS.bedEast, r: 0.25 } },
  { key: "pepper", id: IDS.pepper, name: "Pepper", species: SPECIES.pepper, photo: "pepper",
    heights: [18, 24, 30, 36, 38], widths: [15, 19, 23, 27, 29], at: { x: 2.8, y: 3.4, r: 0.2 } },
  { key: "basil", id: IDS.basil, name: "Basil", species: SPECIES.basil, photo: "basil",
    heights: [12, 16, 20, 24, 26], widths: [14, 18, 22, 26, 28], at: { x: 5.0, y: 3.6, bed: IDS.bedEast, r: 0.15 } },
  { key: "lettuce", id: IDS.lettuce, name: "Lettuce", species: SPECIES.lettuce, photo: "lettuce",
    heights: [8, 11, 14, 17, 19], widths: [12, 16, 20, 23, 25], at: { x: 1.2, y: 6.2, bed: IDS.bedSouth, r: 0.15 } },
  { key: "kale", id: IDS.kale, name: "Kale", species: SPECIES.kale, photo: "kale",
    heights: [15, 20, 26, 31, 34], widths: [18, 24, 30, 35, 38], at: { x: 2.6, y: 6.2, bed: IDS.bedSouth, r: 0.2 } },
];

/** Days ago for the five regular scans: Sep 20, Sep 27, Oct 4, Oct 11, Oct 17. */
const SCAN_AGO = [27, 20, 13, 6, 0];

type Outcome = "healthy" | "blight" | "aphids" | "aphids_ladybug" | "yellow";

/** Scan table (analytics 6). `affected` is [pct, min, max, spot count]. */
interface ScanSeed {
  key: PlantKey;
  ago: number;
  h: number;
  w: number;
  outcome: Outcome;
  affected?: [number, number, number, number];
}

function scanSeeds(): ScanSeed[] {
  const out: ScanSeed[] = [];
  const outcomeOf = (key: PlantKey, i: number): Outcome => {
    if (key === "tomato1") return i >= 3 ? "blight" : "healthy";
    if (key === "pepper") return (["healthy", "aphids", "aphids_ladybug", "healthy", "yellow"] as const)[i];
    return "healthy";
  };
  for (const p of PLANTS) {
    SCAN_AGO.forEach((ago, i) => {
      // Tomato 2's Oct 17 scan is the one the demo takes live.
      if (p.key === "tomato2" && ago === 0) return;
      out.push({ key: p.key, ago, h: p.heights[i], w: p.widths[i], outcome: outcomeOf(p.key, i) });
    });
  }
  const t1 = out.filter((s) => s.key === "tomato1");
  t1.find((s) => s.ago === 6)!.affected = [14, 9, 21, 24];
  t1.find((s) => s.ago === 0)!.affected = [5, 3, 7, 8];
  out.push({ key: "tomato1", ago: 3, h: 60, w: 46, outcome: "blight", affected: [9, 6, 12, 15] });
  out.push({ key: "tomato2", ago: 4, h: 52, w: 42, outcome: "healthy" });
  return out.sort((a, b) => b.ago - a.ago);
}

const WEATHER_BY_AGO: Record<number, [number, number]> = {
  27: [5, 0], 20: [12, 0], 13: [8, 0], 6: [46, 3], 4: [52, 3], 3: [55, 3], 0: [61, 3],
};

function historyEvents(st: StoredScan, k: DateKit, ago: number): S["ScanEvent"][] {
  const sc = st.scan;
  const events: S["ScanEvent"][] = [];
  const push = (body: S["ScanEventBody"], minute: number) => {
    events.push({ v: 1, seq: events.length + 1, run: 1, scan_id: sc.id, ts: k.ts(ago, 9, minute), ...body });
  };
  push({ type: "run_started", reason: "submit", rerun_stages: [] }, 1);
  push({ type: "status_changed", from: "pending", to: "queued" }, 1);
  push({ type: "status_changed", from: "queued", to: "analysing" }, 2);
  if (st.weather) push({ type: "weather_loaded", weather: st.weather }, 2);
  for (const o of st.observations) push({ type: "finding", observation: o }, 3);
  if (st.diagnosis) {
    push({ type: "diagnosis", diagnosis: st.diagnosis }, 4);
    push({ type: "status_changed", from: "analysing", to: "diagnosed" }, 4);
  }
  if (st.plan) {
    push({ type: "status_changed", from: "diagnosed", to: "planned" }, 5);
    push({ type: "plan_ready", plan_id: st.plan.id, provisional: st.plan.provisional }, 5);
  }
  return events;
}

export function buildSeed(k: DateKit = new DateKit()): Seed {
  const user: S["Me"] = {
    id: IDS.user,
    display_name: "Alex Doe",
    created_at: k.ts(40),
    location_consent: true,
  };
  const creator: S["UserRef"] = { user_id: IDS.user, display_name: user.display_name };

  const garden: S["Garden"] = {
    id: IDS.garden,
    name: "Back garden",
    location_label: "Toronto, ON",
    location_precision: "city",
    time_zone: "America/Toronto",
    member_count: 1,
    my_role: "owner",
    soil_profile_status: "ready",
    created_at: k.ts(40),
  };

  const map: S["GardenMap"] = {
    garden_id: IDS.garden,
    version: 3,
    created_at: k.ts(30),
    created_by: creator,
    north_deg: 15,
    beds: [
      { id: IDS.bedEast, name: "East bed", polygon_m: [[4, 1], [6, 1], [6, 5], [4, 5]], area_m2: 8 },
      { id: IDS.bedSouth, name: "South bed", polygon_m: [[0.5, 5.5], [4, 5.5], [4, 7], [0.5, 7]], area_m2: 5.25 },
    ],
    // The pot (Pepper) has no bed.
    pins: PLANTS.map((p) => ({
      plant_id: p.id,
      x_m: p.at.x,
      y_m: p.at.y,
      canopy_radius_m: p.at.r,
      ...(p.at.bed ? { bed_id: p.at.bed } : {}),
    })),
  };

  const plants: StoredPlant[] = PLANTS.map((p) => ({
    id: p.id,
    garden_id: IDS.garden,
    name: p.name,
    species: p.species,
    species_status: "confirmed",
    species_candidates: [],
    status: "healthy",
    created_at: k.ts(30),
    position: { map_version: 3, x_m: p.at.x, y_m: p.at.y },
  }));
  const plantOf = (key: PlantKey) => PLANTS.find((p) => p.key === key)!;
  const named = (key: PlantKey): Named => ({ id: plantOf(key).id, name: plantOf(key).name });

  // ---- scans
  const scans: StoredScan[] = [];
  const tasks: S["Task"][] = [];
  for (const s of scanSeeds()) {
    const p = plantOf(s.key);
    const id = `scan_${s.key}_${s.ago}`;
    const ts = k.ts(s.ago, 10, 40);
    const [rh90, run] = WEATHER_BY_AGO[s.ago] ?? [8, 0];
    const wpImg = `${id}_img_whole`;
    const cuImg = `${id}_img_leaf`;
    const sick = s.outcome !== "healthy";
    const photoName: MockImageName = p.photo;
    const closeName: MockImageName = s.outcome === "blight" ? "tomato-blight" : p.photo;
    const images: S["ScanImage"][] = [
      { id: wpImg, kind: "whole_plant", uploaded_at: ts, url: mockImageUrl(photoName), ...MOCK_IMAGE_SIZE[photoName], depth: { has_gravity: true, width: 256, height: 192 } },
    ];
    if (sick) {
      images.push({ id: cuImg, kind: "leaf_closeup", uploaded_at: ts, url: mockImageUrl(closeName), ...MOCK_IMAGE_SIZE[closeName], depth: { has_gravity: true, width: 256, height: 192 } });
    }

    const obs: S["Observation"][] = [
      measureObs({ id: `${id}_o_h`, scanId: id, ts, quantity: "plant_height_cm", value: s.h, min: s.h - 2, max: s.h + 2, n: 3, confidence: 0.86, imageId: wpImg }),
      measureObs({ id: `${id}_o_w`, scanId: id, ts, quantity: "canopy_width_cm", value: s.w, min: s.w - 2, max: s.w + 2, n: 3, confidence: 0.82, imageId: wpImg }),
    ];
    const leafRegion: S["Region"] = { image_id: cuImg, bbox: [0.25, 0.3, 0.4, 0.35] };
    let diagnosis: S["Diagnosis"] | undefined;
    let plan: S["ActionPlan"] | undefined;
    const diagId = `diag_${id}`;
    const planId = `plan_${id}`;

    if (s.outcome === "healthy") {
      diagnosis = makeDiagnosis({
        id: diagId, scanId: id, ts, status: "healthy",
        photo: ["No spots, holes or pests on the leaves"], modelled: [],
      });
      plan = makePlan({
        id: planId, scanId: id, plantId: p.id, diagnosisId: diagId, ts,
        summary: `${p.name} looks healthy.`, steps: healthySteps(named(s.key)),
        watchFor: ["Spots or yellowing on the lowest leaves"],
      });
    } else if (s.outcome === "blight") {
      const [pct, lo, hi, spots] = s.affected!;
      obs.push(
        makeObservation({ id: `${id}_o_d`, scanId: id, ts, confidence: 0.82, label: PROBLEMS.earlyBlight.name, stage: "disease", pipelineStage: "disease", source: "classifier", region: leafRegion, ref: { table: "disease", id: PROBLEMS.earlyBlight.id } }),
        measureObs({ id: `${id}_o_a`, scanId: id, ts, quantity: "affected_area_pct", value: pct, min: lo, max: hi, n: 3, distance: 0.4, imageId: cuImg }),
        measureObs({ id: `${id}_o_c`, scanId: id, ts, quantity: "lesion_count", value: spots, n: 3, distance: 0.4, imageId: cuImg }),
      );
      const primary = makeScore(PROBLEMS.earlyBlight, 6, {
        fit: "favourable",
        matched: [
          ev("concentric_rings", "Spots have target-like rings", "yes", 3, "vlm"),
          ev("lower_leaves_first", "Spots started on the lowest leaves", "yes", 2, "vlm"),
          ev("yellow_halo", "Yellow halo around the spots", "yes", 1, "vlm"),
        ],
      });
      const risks: S["Risk"][] = s.ago === 6
        ? [{ risk: "fungal_spread", level: "medium", drivers: ["3 humid days in a row"] }]
        : [{ risk: "fungal_spread", level: "low", drivers: ["Humid days are behind us"] }];
      diagnosis = makeDiagnosis({
        id: diagId, scanId: id, ts, status: "confirmed", primary,
        candidates: [primary, makeScore(PROBLEMS.septoria, 2, { matched: [ev("lower_leaves_first", "Spots started on the lowest leaves", "yes", 2)] })],
        risks, citations: [CITATIONS.blight],
        photo: ["Spots with rings on the lower leaves"], modelled: ["Humid weather in the past week"],
      });
      if (s.ago === 6) {
        plan = makePlan({
          id: planId, scanId: id, plantId: p.id, diagnosisId: diagId, ts,
          summary: "Remove the infected leaves, mulch, and check Tomato 1 every 3 days. Check Tomato 2 once a week.",
          steps: blightSteps(k, 6, named("tomato1"), { neighbour: named("tomato2") }),
          watchFor: ["New spots on upper leaves within 5 days", "Spots on Tomato 2's lower leaves"],
          citations: [CITATIONS.blight],
        });
      } else {
        // The follow-up checks of plan v1 (analytics 6): a short plan, no new tasks.
        plan = makePlan({
          id: planId, scanId: id, plantId: p.id, diagnosisId: diagId, ts,
          summary: s.ago === 3
            ? "The spots are fewer. Keep checking Tomato 1 every 3 days."
            : "The spots are down to a few. Keep checking Tomato 1 every 3 days.",
          steps: blightSteps(k, s.ago, named("tomato1")).filter((st) => st.id === "check_plant").map((st) => ({ ...st, order: 1 })),
          watchFor: ["New spots on upper leaves within 5 days"],
          citations: [CITATIONS.blight],
        });
      }
    } else if (s.outcome === "aphids" || s.outcome === "aphids_ladybug") {
      const lady = s.outcome === "aphids_ladybug";
      const aphidObs = makeObservation({ id: `${id}_o_aph`, scanId: id, ts, confidence: 0.84, label: "Aphids", stage: "organism", pipelineStage: "vlm_findings", source: "vlm", role: "pest", region: leafRegion, detail: { count: "many" }, ref: { table: "organism", id: "aphid" } });
      obs.push(aphidObs);
      const orgs: S["DiagnosisOrganism"][] = [{ name: "Aphids", organism_id: "aphid", role: "pest", observation_id: aphidObs.id }];
      if (lady) {
        const l = makeObservation({ id: `${id}_o_lady`, scanId: id, ts, confidence: 0.78, label: "Ladybug larva", stage: "organism", pipelineStage: "vlm_findings", source: "vlm", role: "beneficial", region: leafRegion, detail: { count: "one", length_mm: 6 }, ref: { table: "organism", id: "ladybug" } });
        obs.push(l);
        orgs.push({ name: "Ladybug larva", organism_id: "ladybug", role: "beneficial", observation_id: l.id, preys_on: [{ name: "Aphids", organism_id: "aphid" }] });
      }
      const primary = makeScore(PROBLEMS.aphids, 5, { matched: [ev("clustered_insects", "Small insects clustered on new growth", "yes", 3), ev("curled_new_growth", "New leaves are curled", "yes", 2)] });
      diagnosis = makeDiagnosis({ id: diagId, scanId: id, ts, status: "confirmed", primary, organisms: orgs, citations: [CITATIONS.aphids], photo: ["Insects on the new growth"] });
      plan = makePlan({ id: planId, scanId: id, plantId: p.id, diagnosisId: diagId, ts, summary: lady ? "A ladybug larva is already eating the aphids." : "Rinse the aphids off and check again.", steps: aphidSteps(named(s.key), lady), watchFor: ["More aphids on the new growth"], citations: [CITATIONS.aphids] });
    } else {
      const primary = makeScore(PROBLEMS.nitrogen, 3, { matched: [ev("yellow_lower_leaves", "Lower leaves are turning yellow", "yes", 3)], unsure: [ev("even_yellowing", "Yellowing is even across the leaf", "unsure", 2)] });
      obs.push(makeObservation({ id: `${id}_o_y`, scanId: id, ts, confidence: 0.55, label: "Yellow lower leaves", stage: "disease", pipelineStage: "vlm_findings", source: "vlm", region: leafRegion }));
      diagnosis = makeDiagnosis({ id: diagId, scanId: id, ts, status: "uncertain", candidates: [primary, makeScore(PROBLEMS.aphids, 1)], citations: [CITATIONS.nitrogen], photo: ["Yellow lower leaves"] });
      plan = makePlan({ id: planId, scanId: id, plantId: p.id, diagnosisId: diagId, ts, provisional: true, summary: "Not sure yet. Watch the yellow lower leaves.", steps: provisionalSteps(named(s.key), "Look for pests on the undersides.", k), watchFor: ["Yellowing that spreads upward"], citations: [CITATIONS.nitrogen] });
    }

    const stored: StoredScan = {
      scan: {
        id, plant_id: p.id, garden_id: IDS.garden, created_by: creator,
        status: plan ? "planned" : "failed",
        run: 1, last_event_seq: 0, created_at: ts, submitted_at: ts,
        images, guided_checks: [],
        diagnosis_id: diagId,
        ...(plan ? { plan_id: planId } : {}),
      },
      observations: obs,
      diagnosis,
      plan,
      weather: makeWeather(k, { ago: s.ago, rh90, run }),
      events: [],
      running: false,
    };
    stored.events = historyEvents(stored, k, s.ago);
    stored.scan.last_event_seq = stored.events.length;
    scans.push(stored);
  }

  // Latest status per plant: a confirmed problem is sick, an uncertain result is watch.
  const status: Partial<Record<PlantKey, S["PlantStatus"]>> = { tomato1: "sick", pepper: "watch" };
  for (const p of plants) {
    const key = PLANTS.find((d) => d.id === p.id)!.key;
    p.status = status[key] ?? "healthy";
  }

  // ---- Tomato 1's plan v1 tasks, and the weekly check of Tomato 2
  const planId1 = "plan_scan_tomato1_6";
  const scan1 = "scan_tomato1_6";
  const base = { garden_id: IDS.garden, source: "plan" as const };
  tasks.push(
    { ...base, id: "task_t1_remove", plant_id: IDS.tomato1, plan_id: planId1, plan_step_id: "remove_infected_leaves", title: "Remove the infected leaves", due: k.date(6), done_at: k.ts(6, 11, 30), created_at: k.ts(6, 10, 45) },
    { ...base, id: "task_t1_mulch", plant_id: IDS.tomato1, plan_id: planId1, plan_step_id: "mulch_base", title: "Mulch around the base", due: k.date(4), done_at: k.ts(5, 16, 0), created_at: k.ts(6, 10, 45) },
    { ...base, id: "task_t1_check_a", plant_id: IDS.tomato1, plan_id: planId1, plan_step_id: "check_plant", title: "Check Tomato 1 for new spots", due: k.date(3), repeat: { every_days: 3 }, done_at: k.ts(3, 9, 40), created_at: k.ts(6, 10, 45) },
    { ...base, id: "task_t1_check_b", plant_id: IDS.tomato1, plan_id: planId1, plan_step_id: "check_plant", title: "Check Tomato 1 for new spots", due: k.date(0), repeat: { every_days: 3 }, done_at: k.ts(0, 8, 15), created_at: k.ts(3, 9, 40) },
    { ...base, id: "task_t1_check_c", plant_id: IDS.tomato1, plan_id: planId1, plan_step_id: "check_plant", title: "Check Tomato 1 for new spots", due: k.date(-3), repeat: { every_days: 3 }, created_at: k.ts(0, 8, 15) },
    { ...base, id: "task_t2_check_a", plant_id: IDS.tomato2, plan_id: planId1, plan_step_id: "check_neighbour", title: "Check Tomato 2's lower leaves", due: k.date(4), repeat: { every_days: 7 }, done_at: k.ts(4, 10, 0), created_at: k.ts(6, 10, 45) },
    { ...base, id: "task_t2_check_b", plant_id: IDS.tomato2, plan_id: planId1, plan_step_id: "check_neighbour", title: "Check Tomato 2's lower leaves", detail: "Tomato 2 is 40 cm from Tomato 1. Look at the lowest leaves for the first spots.", due: k.date(0), repeat: { every_days: 7 }, created_at: k.ts(4, 10, 0) },
  );
  // The plan's task ids.
  const p1 = scans.find((s) => s.scan.id === scan1)!;
  p1.plan!.task_ids = ["task_t1_remove", "task_t1_mulch", "task_t1_check_c", "task_t2_check_b"];

  // Frost alerts: Oct 16's is done, the coming one's (Thursday, or Friday on a Thursday) is open.
  const frostIn = frostInDays(k);
  tasks.push(
    { id: "task_frost_done", garden_id: IDS.garden, source: "forecast_alert", title: "Frost tonight: cover tender plants", detail: "The low is -1 °C. Move pots under cover or throw a sheet over them.", trigger: { kind: "frost", date: k.date(1) }, due: k.date(2), done_at: k.ts(2, 18, 0), created_at: k.ts(3, 7, 0) },
    { id: "task_frost_open", garden_id: IDS.garden, source: "forecast_alert", title: `Frost on ${k.weekdayName(-frostIn)}: cover tender plants`, detail: "The low is -1 °C. Move pots under cover or throw a sheet over them.", trigger: { kind: "frost", date: k.date(-frostIn) }, due: k.date(-(frostIn - 1)), created_at: k.ts(0, 7, 0) },
  );

  const memories: S["Memory"][] = [
    { id: "mem_containers", garden_id: IDS.garden, kind: "fact", source: "user", shared: true, topic: "container", text: "Everything is in containers", created_at: k.ts(30) },
    { id: "mem_t1_leaves", garden_id: IDS.garden, plant_id: IDS.tomato1, kind: "event", source: "chat", shared: true, topic: "treatment", text: "Removed the infected leaves from Tomato 1.", created_at: k.ts(6, 11, 35) },
    { id: "mem_t1_mulch", garden_id: IDS.garden, plant_id: IDS.tomato1, kind: "event", source: "chat", shared: true, topic: "treatment", text: "Mulched around Tomato 1 with straw.", created_at: k.ts(5, 16, 5) },
  ];

  const weather = makeWeather(k, { ago: 0, rh90: 61, run: 3, humidAhead: 4, frostInDays: frostIn });

  const refs = (names: MockImageName[]): S["ReferenceImage_Signed"][] =>
    names.map((n) => ({ url: mockImageUrl(n), credit: MOCK_IMAGE_CREDIT[n] }));
  const sp = (r: S["SpeciesRef"], supported: boolean, photos: MockImageName[]): S["SpeciesSummary"] => ({
    ...r, classifier_supported: supported, profile: "full", reference_images: refs(photos),
  });
  const species: S["SpeciesSummary"][] = [
    sp(SPECIES.basil, false, ["basil"]),
    { id: "cucumber", common_name: "Cucumber", scientific_name: "Cucumis sativus", classifier_supported: false, profile: "label_only", reference_images: [] },
    sp(SPECIES.kale, false, ["kale"]),
    sp(SPECIES.lettuce, false, ["lettuce"]),
    sp(SPECIES.pepper, true, ["pepper"]),
    { id: "potato", common_name: "Potato", scientific_name: "Solanum tuberosum", classifier_supported: true, profile: "full", reference_images: [] },
    { id: "spinach", common_name: "Spinach", scientific_name: "Spinacia oleracea", classifier_supported: false, profile: "label_only", reference_images: [] },
    sp(SPECIES.tomato, true, ["tomato", "tomato-blight"]),
  ];

  return { user, garden, map, plants, scans, tasks, memories, weather, species };
}
