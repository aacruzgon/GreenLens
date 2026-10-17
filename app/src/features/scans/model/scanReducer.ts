import type { Diagnosis, GuidedCheck, Observation, Question, ScanEvent, ScanFailure, ScanStatus, SpeciesCandidate, Stage, SupersedeReason, WeatherSummary } from "@/domain/types";
import { STAGE_ORDER } from "@/domain/scan";

// docs/frontend 10, scan events: the pure reducer that folds a scan's events into its live view.
// Events change what the screen shows; records are refreshed by refetching, never written from events.

export type StageProgressState = "waiting" | "running" | "done" | "skipped";

export interface ScanViewState {
  lastSeq: number;
  run: number;
  status?: ScanStatus;
  /** The stages of the current run, in pipeline order, with their progress. */
  stages: { stage: Stage; state: StageProgressState; skippedReason?: string }[];
  findings: Observation[];
  weather?: WeatherSummary;
  candidateCount?: number;
  species?: { candidates: SpeciesCandidate[]; workingSpeciesId: string | null; needsConfirmation: boolean };
  question?: Question;
  checks: GuidedCheck[];
  diagnosis?: Diagnosis;
  plan?: { planId: string; provisional: boolean };
  planNotice?: SupersedeReason;
  failure?: ScanFailure;
}

export const initialScanView: ScanViewState = { lastSeq: 0, run: 0, stages: [], findings: [], checks: [] };

function setStage(state: ScanViewState, stage: Stage, next: StageProgressState, skippedReason?: string): ScanViewState["stages"] {
  const exists = state.stages.some((s) => s.stage === stage);
  const stages = exists ? state.stages : [...state.stages, { stage, state: "waiting" as const }];
  return stages
    .map((s) => (s.stage === stage ? { stage, state: next, ...(skippedReason ? { skippedReason } : {}) } : s))
    .sort((a, b) => STAGE_ORDER.indexOf(a.stage) - STAGE_ORDER.indexOf(b.stage));
}

export function applyScanEvent(state: ScanViewState, event: ScanEvent): ScanViewState {
  if (event.seq <= state.lastSeq) return state;
  const s: ScanViewState = { ...state, lastSeq: event.seq, run: Math.max(state.run, event.run) };

  switch (event.type) {
    case "run_started": {
      const rerun = new Set(event.rerun_stages);
      const kept = state.stages.filter((st) => !rerun.has(st.stage));
      const stages = [...kept, ...event.rerun_stages.map((stage) => ({ stage, state: "waiting" as const }))].sort(
        (a, b) => STAGE_ORDER.indexOf(a.stage) - STAGE_ORDER.indexOf(b.stage),
      );
      return {
        ...s,
        run: event.run,
        stages,
        findings: state.findings.filter((f) => !rerun.has(f.pipeline_stage)),
        failure: undefined,
        species: event.reason === "species_changed" ? undefined : state.species,
      };
    }
    case "status_changed":
      return { ...s, status: event.to };
    case "stage_started":
      return { ...s, stages: setStage(state, event.stage, "running") };
    case "stage_completed":
      return { ...s, stages: setStage(state, event.stage, event.skipped ? "skipped" : "done", event.skipped?.reason) };
    case "finding":
      return { ...s, findings: [...state.findings.filter((f) => f.id !== event.observation.id), event.observation] };
    case "finding_retracted":
      return { ...s, findings: state.findings.filter((f) => f.id !== event.observation_id) };
    case "weather_loaded":
      return { ...s, weather: event.weather };
    case "candidates_selected":
      return { ...s, candidateCount: event.candidates.length };
    case "species_suggested":
      return {
        ...s,
        species: { candidates: event.candidates, workingSpeciesId: event.working_species_id, needsConfirmation: event.needs_confirmation },
      };
    case "species_confirmed":
      return { ...s, species: undefined };
    case "question":
      return { ...s, question: event.question };
    case "question_resolved":
      return state.question?.id === event.question_id ? { ...s, question: undefined } : s;
    case "guided_check_done":
      return { ...s, checks: [...state.checks.filter((c) => c.image_id !== event.check.image_id), event.check] };
    case "diagnosis":
      return { ...s, diagnosis: event.diagnosis };
    case "plan_ready":
      return { ...s, plan: { planId: event.plan_id, provisional: event.provisional } };
    case "plan_superseded":
      return { ...s, plan: { planId: event.new_plan_id, provisional: state.plan?.provisional ?? false }, planNotice: event.reason };
    case "failed":
      return { ...s, failure: event.failure, status: "failed" };
    default:
      // Unknown event types are ignored (contracts 1, versioning).
      return s;
  }
}

export function foldScanEvents(events: ScanEvent[], from: ScanViewState = initialScanView): ScanViewState {
  return events.reduce(applyScanEvent, from);
}

export type Invalidation =
  | { key: "scan" }
  | { key: "plant" }
  | { key: "plants" }
  | { key: "timeline" }
  | { key: "measurements" }
  | { key: "tasks" }
  | { key: "memories" };

/** The side effects of a live event: which queries to refetch (docs/frontend 10, its effects). */
export function effectsOf(event: Pick<ScanEvent, "type">): Invalidation[] {
  switch (event.type) {
    case "plan_ready":
    case "plan_superseded":
      return [{ key: "scan" }, { key: "plant" }, { key: "plants" }, { key: "timeline" }, { key: "measurements" }, { key: "tasks" }, { key: "memories" }];
    case "species_confirmed":
      return [{ key: "plant" }, { key: "plants" }];
    case "guided_check_done":
      return [{ key: "scan" }];
    case "failed":
      return [{ key: "timeline" }, { key: "plant" }, { key: "plants" }];
    default:
      return [];
  }
}
