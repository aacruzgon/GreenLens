// The analysis in the user's words: the 15 pipeline stages (contracts 4) as the mockup's nine steps, the phase
// line on the stage card ("Checking telltale signs · 6 s · Next: weighing the evidence", design 5) and the run
// log under it. Pure, from the reducer's state.
import type { Stage } from "@/domain/types";

import type { ScanState } from "./scanState";

export type PhaseKey = "plant" | "species" | "disease" | "measure" | "context" | "problems" | "signs" | "weigh" | "plan";

interface PhaseDef {
  key: PhaseKey;
  label: string;
  stages: readonly Stage[];
}

/** In run order. A phase is done when each of its stages that runs is done (styles.css .stages). */
export const PHASES: readonly PhaseDef[] = [
  { key: "plant", label: "Finding the plant", stages: ["plant_check", "segmentation"] },
  { key: "species", label: "Identifying the species", stages: ["species"] },
  { key: "disease", label: "Looking for disease", stages: ["disease", "image_embedding"] },
  { key: "measure", label: "Measuring spots", stages: ["measurement"] },
  { key: "context", label: "Checking weather and soil", stages: ["context"] },
  { key: "problems", label: "Gathering likely problems", stages: ["retrieve"] },
  { key: "signs", label: "Checking telltale signs", stages: ["vlm_findings", "vlm_indicators", "guided", "verify"] },
  { key: "weigh", label: "Weighing the evidence", stages: ["scoring", "fusion"] },
  { key: "plan", label: "Writing your plan", stages: ["plan"] },
];

export type PhaseState = "waiting" | "running" | "done" | "skipped";

export interface PhaseView {
  key: PhaseKey;
  label: string;
  state: PhaseState;
  /** The stages' summed time, once done. */
  durationMs?: number;
  /** The newest running stage's start, for the running clock. */
  startedAt?: string;
  /** What it found, in a few words ("9 possible problems"); never a diagnosis before the result (design 3.1). */
  note?: string;
}

function touched(state: ScanState, def: PhaseDef): boolean {
  return def.stages.some((st) => state.stages[st]);
}

/**
 * The phases this run shows. `measurement` runs only with depth (contracts 4), so its phase shows only when the
 * run lists it or it started. A phase whose stages never start while a later one does (`retrieve` on a path that
 * doesn't report it) counts as done once `candidates_selected` or a later phase shows the run moved past it.
 */
export function phasesOf(state: ScanState): PhaseView[] {
  const listed = (def: PhaseDef) =>
    def.key !== "measure" || touched(state, def) || state.runStages.includes("measurement");
  const defs = PHASES.filter(listed);
  const lastTouched = defs.reduce((acc, def, i) => (touched(state, def) ? i : acc), -1);

  return defs.map((def, i): PhaseView => {
    const progress = def.stages.map((st) => state.stages[st]).filter((p) => p !== undefined);
    const running = progress.filter((p) => p.state === "running");
    const done = progress.filter((p) => p.state === "done");
    const base = { key: def.key, label: def.label, note: noteOf(def.key, state) };
    if (running.length > 0) return { ...base, state: "running", startedAt: running[running.length - 1]?.startedAt };
    // Between two stages of one phase it still runs; it ticks when a later phase starts or the plan is in.
    const passed = i < lastTouched || state.status === "planned";
    if (done.length > 0) {
      if (!passed) return { ...base, state: "running", startedAt: done[done.length - 1]?.startedAt };
      return {
        ...base,
        state: done.every((p) => p.skipped) ? "skipped" : "done",
        durationMs: done.reduce((sum, p) => sum + (p.durationMs ?? 0), 0),
      };
    }
    if (passed || (def.key === "problems" && state.candidates)) return { ...base, state: "done" };
    return { ...base, state: "waiting" };
  });
}

function noteOf(key: PhaseKey, state: ScanState): string | undefined {
  const findings = state.findings;
  switch (key) {
    case "plant": {
      const plant = findings.find((f) => f.pipeline_stage === "plant_check");
      return plant ? `A ${plant.label.toLowerCase()} in ${photoCount(state)}` : undefined;
    }
    case "species":
      return findings.find((f) => f.pipeline_stage === "species")?.label;
    case "disease": {
      if (state.stages.disease?.skipped) return "Not checked for this plant";
      const regions = findings.filter((f) => f.pipeline_stage === "disease" && f.ref?.table === "disease" && f.region);
      if (regions.length > 0) return "Spots found";
      return state.stages.disease?.state === "done" ? "No spots found" : undefined;
    }
    case "measure": {
      const spot = lesionSize(state);
      return spot ? `${spot} spots, from the depth map` : undefined;
    }
    case "context": {
      if (!state.weather) return undefined;
      const humid = state.weather.forecast.filter((d) => d.mean_rh_pct >= 85).length;
      return humid > 0 ? `${humid} humid ${humid === 1 ? "day" : "days"} ahead` : "Weather loaded";
    }
    case "problems": {
      const n = state.candidates?.length;
      return n ? `${n} possible ${n === 1 ? "problem" : "problems"}` : undefined;
    }
    case "signs":
      return state.question?.kind === "indicator" || state.question?.kind === "photo_request"
        ? "Waiting for your answer"
        : undefined;
    case "weigh": {
      const d = state.diagnosis;
      if (!d) return undefined;
      if (d.status === "healthy") return "Looks healthy";
      if (d.status === "uncertain") return "Not sure yet";
      const name = d.primary?.name ?? d.candidates[0]?.name;
      return name ? `${name} fits best` : undefined;
    }
    case "plan":
      return undefined;
  }
}

function photoCount(state: ScanState): string {
  const ids = new Set(state.findings.map((f) => f.region?.image_id).filter(Boolean));
  const n = Math.max(1, ids.size);
  return n === 1 ? "the photo" : `${n} photos`;
}

/** "8–12 mm" from the run's lesion measurement, or undefined. */
export function lesionSize(state: Pick<ScanState, "findings">): string | undefined {
  const m = state.findings.find((f) => f.detail?.measurement?.quantity === "lesion_diameter_mm")?.detail?.measurement;
  if (!m) return undefined;
  const lo = m.min;
  const hi = m.max;
  return Math.round(lo) === Math.round(hi) ? `${Math.round(m.value)} mm` : `${Math.round(lo)}–${Math.round(hi)} mm`;
}

export interface PhaseLine {
  /** "Checking telltale signs", "Waiting for your answer", "Plan ready". */
  now: string;
  /** When the clock runs: the running phase's start. */
  since?: string;
  /** "in 18 s" once the plan is ready. */
  total?: string;
  /** "Next: weighing the evidence", "Next: your plan". */
  next?: string;
}

/** The phase line (app.js liveController.phase). */
export function phaseLine(state: ScanState, phases: readonly PhaseView[] = phasesOf(state)): PhaseLine {
  const totalMs = phases.reduce((sum, p) => sum + (p.durationMs ?? 0), 0);
  if (state.status === "failed") return { now: "Stopped" };
  if (state.plan && state.status === "planned") return { now: "Plan ready", total: `in ${formatSeconds(totalMs)}` };
  const idx = phases.findIndex((p) => p.state === "running" || p.state === "waiting");
  const current = phases[idx];
  if (state.question && state.status === "awaiting_answer") {
    const nextPhase = phases.find((p) => p.key === "weigh");
    return { now: "Waiting for your answer", next: nextPhase ? `Next: ${nextPhase.label.toLowerCase()}` : undefined };
  }
  if (!current) return { now: state.status === "queued" ? "Waiting for analysis" : "Finishing" };
  const after = phases.slice(idx + 1).find((p) => p.state !== "done" && p.state !== "skipped");
  const now = current.state === "waiting" && state.status === "queued" ? "Waiting for analysis" : current.label;
  return {
    now,
    since: current.state === "running" ? current.startedAt : undefined,
    next: after ? `Next: ${after.label.toLowerCase()}` : "Next: your plan",
  };
}

/** "0.6 s" under 10 s, "12 s" above: the run log's times (index.html .stage .sd). */
export function formatSeconds(ms: number): string {
  const s = ms / 1000;
  return s < 10 ? `${s.toFixed(1)} s` : `${Math.round(s)} s`;
}

/** How many phases have ticked, for the haptic per tick. */
export function tickCount(phases: readonly PhaseView[]): number {
  return phases.filter((p) => p.state === "done" || p.state === "skipped").length;
}
