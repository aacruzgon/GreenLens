// The live analysis's state: the snapshot (`ScanDetail`) folded with the scan's events (docs/frontend/README.md 10,
// the reducer table; contracts 4, the event catalogue). Pure: `applyScanEvent` and `effectsOf` do no I/O, so the
// hook replays, resumes and tests through the same code.
import type {
  Diagnosis,
  GuidedCheck,
  Observation,
  ProblemKind,
  Question,
  ScanDetail,
  ScanEvent,
  ScanFailure,
  ScanStatus,
  SpeciesCandidate,
  Stage,
  Timestamp,
  WeatherSummary,
} from "@/domain/types";
import { isAtRest } from "@/domain/scan";

export interface StageProgress {
  state: "running" | "done";
  /** The `stage_started` event's time, for the phase line's running clock. */
  startedAt?: Timestamp;
  durationMs?: number;
  /** `stage_completed.skipped.reason`, such as `species_not_supported`. */
  skipped?: string;
}

export interface SelectedCandidate {
  problem_id: string;
  name: string;
  kind: ProblemKind;
}

export interface SpeciesSuggestion {
  candidates: SpeciesCandidate[];
  workingSpeciesId: string | null;
  needsConfirmation: boolean;
}

export type PlanNotice = "late_answer" | "species_changed" | "chat_revision" | "guided_check";

export interface ScanState {
  scanId: string;
  run: number;
  status: ScanStatus;
  /** The last applied `seq`; 0 before any event. A reconnect sends it as `Last-Event-ID`. */
  lastSeq: number;
  /** The snapshot's `last_event_seq`: events up to it are a replay, folded without animation or effects. */
  snapshotSeq: number;
  /** The current run's stages, from `run_started`; empty when the run's start wasn't seen. */
  runStages: Stage[];
  stages: Partial<Record<Stage, StageProgress>>;
  /** Findings by observation ID, in the order they arrived. */
  findings: Observation[];
  /** Findings that arrived live, after the snapshot: only these animate in (README 10, step 4). */
  liveFindingIds: string[];
  weather?: WeatherSummary;
  candidates?: SelectedCandidate[];
  /** Set by `species_suggested`, cleared by `species_confirmed`. */
  species?: SpeciesSuggestion;
  question?: Question;
  /** The last question to resolve, and how, for "You answered …" and the dropped-photo note. */
  resolved?: { questionId: string; by: "user" | "photo" | "superseded" | "dropped" };
  guidedChecks: GuidedCheck[];
  diagnosis?: Diagnosis;
  plan?: { id: string; provisional: boolean; notice?: PlanNotice };
  failure?: ScanFailure;
}

/** The screen's starting state from `GET /scans/{id}`. Stage progress, candidates and the species check come only
 * from the replay (README 10, step 3), so they start empty. */
export function seed(detail: ScanDetail): ScanState {
  return {
    scanId: detail.id,
    run: detail.run,
    status: detail.status,
    lastSeq: 0,
    snapshotSeq: detail.last_event_seq,
    runStages: [],
    stages: {},
    // Every run's current observations: a species change keeps run 1's plant check, segmentation and
    // measurements, and the server returns only the ones still current (contracts 3, observations across runs).
    findings: detail.observations,
    liveFindingIds: [],
    weather: detail.weather,
    question: detail.open_question,
    guidedChecks: detail.guided_checks,
    diagnosis: detail.diagnosis,
    plan: detail.plan ? { id: detail.plan.id, provisional: detail.plan.provisional } : undefined,
    failure: detail.failure,
  };
}

/**
 * Brings the state up to a refetched `ScanDetail` that is at least as fresh (`last_event_seq >= lastSeq`). Some
 * changes come with no event: a question going to `answering` after its photo, a guided check `analysing`, and a
 * superseding plan's `provisional`. Stage progress and candidates stay the stream's.
 */
export function reconcile(state: ScanState, detail: ScanDetail): ScanState {
  if (detail.id !== state.scanId || detail.last_event_seq < state.lastSeq) return state;
  const plan = detail.plan
    ? { id: detail.plan.id, provisional: detail.plan.provisional, notice: state.plan?.id === detail.plan.id ? state.plan.notice : undefined }
    : undefined;
  return {
    ...state,
    run: Math.max(state.run, detail.run),
    status: detail.status,
    question: detail.open_question,
    guidedChecks: detail.guided_checks,
    diagnosis: detail.diagnosis,
    plan,
    failure: detail.failure,
  };
}

/** Whether `event` was already in the snapshot, so it is folded without animation and runs no effects. */
export function isReplay(state: Pick<ScanState, "snapshotSeq">, event: Pick<ScanEvent, "seq">): boolean {
  return event.seq <= state.snapshotSeq;
}

/** Folds one event in (README 10, the reducer). A `seq` already applied changes nothing. */
export function applyScanEvent(state: ScanState, event: ScanEvent): ScanState {
  if (event.seq <= state.lastSeq) return state;
  const s: ScanState = { ...state, lastSeq: event.seq, run: Math.max(state.run, event.run) };
  switch (event.type) {
    case "run_started": {
      const reset = new Set<Stage>(event.rerun_stages);
      const stages = { ...s.stages };
      for (const stage of reset) delete stages[stage];
      return {
        ...s,
        run: event.run,
        runStages: event.rerun_stages,
        stages,
        findings: s.findings.filter((f) => !reset.has(f.pipeline_stage)),
        // A new run answers afresh: its candidates and its failure are its own.
        candidates: reset.has("retrieve") || reset.has("vlm_findings") ? undefined : s.candidates,
        failure: undefined,
      };
    }
    case "status_changed":
      return { ...s, status: event.to };
    case "stage_started":
      return { ...s, stages: { ...s.stages, [event.stage]: { state: "running", startedAt: event.ts } } };
    case "stage_completed":
      return {
        ...s,
        stages: {
          ...s.stages,
          [event.stage]: {
            state: "done",
            startedAt: s.stages[event.stage]?.startedAt,
            durationMs: event.duration_ms,
            skipped: event.skipped?.reason,
          },
        },
      };
    case "finding": {
      const rest = s.findings.filter((f) => f.id !== event.observation.id);
      const live = event.seq > s.snapshotSeq ? [...s.liveFindingIds, event.observation.id] : s.liveFindingIds;
      return { ...s, findings: [...rest, event.observation], liveFindingIds: live };
    }
    case "finding_retracted":
      return { ...s, findings: s.findings.filter((f) => f.id !== event.observation_id) };
    case "weather_loaded":
      return { ...s, weather: event.weather };
    case "candidates_selected": {
      // A `verify` job sends it again with what it added; keep one entry per problem.
      const byId = new Map((s.candidates ?? []).map((c) => [c.problem_id, c]));
      for (const c of event.candidates) byId.set(c.problem_id, { problem_id: c.problem_id, name: c.name, kind: c.kind });
      return { ...s, candidates: [...byId.values()] };
    }
    case "species_suggested":
      return {
        ...s,
        species: {
          candidates: event.candidates,
          workingSpeciesId: event.working_species_id,
          needsConfirmation: event.needs_confirmation,
        },
      };
    case "species_confirmed":
      return { ...s, species: undefined };
    case "question": {
      // The move to `answering` sends no event (a question photo only updates the row), and the stored event
      // keeps `open`: a replay mustn't undo what a fresher record said.
      const answering = s.question?.id === event.question.id && s.question.status === "answering";
      return { ...s, question: answering ? { ...event.question, status: "answering" } : event.question };
    }
    case "question_resolved":
      return {
        ...s,
        question: s.question?.id === event.question_id ? undefined : s.question,
        resolved: { questionId: event.question_id, by: event.by },
      };
    case "guided_check_done": {
      const rest = s.guidedChecks.filter((c) => c.image_id !== event.check.image_id);
      return { ...s, guidedChecks: [...rest, event.check] };
    }
    case "diagnosis":
      return { ...s, diagnosis: event.diagnosis };
    case "plan_ready":
      return { ...s, plan: { id: event.plan_id, provisional: event.provisional } };
    case "plan_superseded":
      // The event doesn't say whether the new plan is provisional: until the refetched record says (its "scan"
      // invalidation, then `reconcile`), it keeps the old plan's.
      return { ...s, plan: { id: event.new_plan_id, provisional: s.plan?.provisional ?? false, notice: event.reason } };
    case "failed":
      return { ...s, status: "failed", failure: event.failure };
    default:
      return s;
  }
}

/** The resting rule (domain/scan.ts) on the reducer's state. */
export function stateAtRest(state: ScanState): boolean {
  return isAtRest({ status: state.status, open_question: state.question, guided_checks: state.guidedChecks });
}

export type ScanEffect =
  | { kind: "invalidate"; keys: ("scan" | "plant" | "plants" | "timeline" | "measurements" | "tasks" | "memories")[] }
  | { kind: "haptic"; type: "selection" | "success" | "warning" };

/**
 * The side effects of a live event (README 10, its effects), plus the haptic it earns (design 6: haptics mark
 * real events only). Replayed events get none: the caller checks `isReplay` first. A stage's tick has its haptic
 * where the phases are counted (phases.ts), since several stages make one tick.
 */
export function effectsOf(event: ScanEvent): ScanEffect[] {
  switch (event.type) {
    case "plan_ready":
    case "plan_superseded":
      return [
        { kind: "invalidate", keys: ["scan", "plant", "plants", "timeline", "measurements", "tasks", "memories"] },
        { kind: "haptic", type: "success" },
      ];
    case "species_confirmed":
      return [{ kind: "invalidate", keys: ["plant", "plants"] }];
    case "guided_check_done":
      return [{ kind: "invalidate", keys: ["scan"] }];
    case "failed":
      return [
        { kind: "invalidate", keys: ["timeline", "plant", "plants"] },
        { kind: "haptic", type: "warning" },
      ];
    case "question":
      return [{ kind: "haptic", type: "selection" }];
    default:
      return [];
  }
}
