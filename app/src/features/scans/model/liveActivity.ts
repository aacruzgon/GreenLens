// The scan's Live Activity in the scan's terms (roadmap F5 item 6; design 5, "Live Activity"; inspiration 3.2):
// the stage in words, the elapsed time, a nine-segment bar and what comes next, from the same phases as the stage
// card. Start on the live analysis, update only when the content changes, alert only for the question and the
// result, end when the scan ends and keep the summary for 15 minutes (HIG: "15 to 30 minutes is adequate").
// Pure: the hook (hooks/useScanActivity.ts) runs the operations.
import type { PhaseLine, PhaseView } from "./phases";
import { PHASES } from "./phases";
import type { ScanState } from "./scanState";

export type SegmentState = "waiting" | "running" | "done" | "skipped";
export type ActivityMode = "running" | "question" | "planned" | "failed";

/** The activity's content; the same shape as the platform's ScanActivityState. */
export interface ActivityContent {
  mode: ActivityMode;
  stage: string;
  next?: string;
  /** One per phase of PHASES, nine. */
  segments: SegmentState[];
  /** Milliseconds since 1970. */
  startedAt: number;
  total?: string;
}

export interface ActivityAlert {
  title: string;
  body: string;
}

export type ActivityDismissal = { policy: "immediate" } | { policy: "after"; afterSeconds: number };

/** How long the summary stays on the Lock Screen after the scan ends. */
export const SUMMARY_SECONDS = 15 * 60;

/**
 * The nine segments, one per phase in PHASES order. A phase the run doesn't show (measuring, without depth) is
 * drawn as skipped, so the bar always has nine and a segment always means the same step.
 */
export function activitySegments(phases: readonly PhaseView[]): SegmentState[] {
  const byKey = new Map(phases.map((p) => [p.key, p.state]));
  return PHASES.map((def) => byKey.get(def.key) ?? "skipped");
}

export function activityMode(state: Pick<ScanState, "status" | "plan" | "question" | "failure">): ActivityMode {
  if (state.status === "failed") return "failed";
  if (state.plan && state.status === "planned") return "planned";
  if (state.question && state.status === "awaiting_answer") return "question";
  return "running";
}

/** When the run started: its first stage's start, in milliseconds since 1970. */
export function runStart(state: Pick<ScanState, "stages">): number | undefined {
  const starts = Object.values(state.stages)
    .map((p) => (p?.startedAt ? Date.parse(p.startedAt) : NaN))
    .filter((t) => !Number.isNaN(t));
  return starts.length > 0 ? Math.min(...starts) : undefined;
}

export function activityContent(
  state: ScanState,
  phases: readonly PhaseView[],
  line: PhaseLine,
  startedAt: number,
): ActivityContent {
  const mode = activityMode(state);
  const content: ActivityContent = { mode, stage: line.now, segments: activitySegments(phases), startedAt };
  if (line.next && (mode === "running" || mode === "question")) content.next = line.next;
  if (mode === "planned" && line.total) content.total = line.total;
  return content;
}

/** What the hook has done so far for one scan screen. */
export interface ActivityTrack {
  started: boolean;
  ended: boolean;
  last?: ActivityContent;
}

export type ActivityOp =
  | { kind: "none" }
  | { kind: "start"; content: ActivityContent }
  | { kind: "update"; content: ActivityContent; alert?: ActivityAlert }
  | { kind: "end"; content: ActivityContent; alert?: ActivityAlert; dismissal: ActivityDismissal };

function same(a: ActivityContent | undefined, b: ActivityContent): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/**
 * The next operation for new content. An activity starts only for a scan still running (an old scan opened at
 * rest gets none), updates only when something it shows changed, alerts when a question opens and when the
 * result arrives (a plan, or the scan stopping), and ends with the result.
 */
export function nextActivityOp(
  track: ActivityTrack,
  content: ActivityContent,
  ctx: { plantName: string; questionText?: string },
): ActivityOp {
  if (track.ended) return { kind: "none" };
  const over = content.mode === "planned" || content.mode === "failed";
  if (!track.started) return over ? { kind: "none" } : { kind: "start", content };
  if (over) {
    const alert =
      content.mode === "planned"
        ? { title: `${ctx.plantName}: plan ready`, body: "See the diagnosis and what to do." }
        : { title: `${ctx.plantName}: the scan stopped`, body: "Open the scan to try again." };
    return { kind: "end", content, alert, dismissal: { policy: "after", afterSeconds: SUMMARY_SECONDS } };
  }
  if (same(track.last, content)) return { kind: "none" };
  if (content.mode === "question" && track.last?.mode !== "question") {
    return {
      kind: "update",
      content,
      alert: { title: `${ctx.plantName} needs an answer`, body: ctx.questionText ?? "Open the scan to answer one question." },
    };
  }
  return { kind: "update", content };
}

/** Leaving the scan screen before the scan ends: the activity goes at once, since nothing will update it. */
export function leaveActivityOp(track: ActivityTrack): ActivityOp {
  if (!track.started || track.ended || !track.last) return { kind: "none" };
  return { kind: "end", content: track.last, dismissal: { policy: "immediate" } };
}

/** The track after `op` ran. */
export function applyActivityOp(track: ActivityTrack, op: ActivityOp): ActivityTrack {
  switch (op.kind) {
    case "none":
      return track;
    case "start":
      return { started: true, ended: false, last: op.content };
    case "update":
      return { ...track, last: op.content };
    case "end":
      return { ...track, ended: true, last: op.content };
  }
}
