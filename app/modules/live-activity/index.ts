// The scan's Live Activity module (roadmap F5 item 6). The native side is ios/LiveActivityModule.swift; the
// widget that draws it is targets/scan-activity. Absent in Expo Go, on the web and in Jest, where every call is a
// no-op: `isSupported()` is false and `start` resolves to undefined. The app reaches it through
// src/platform/liveActivity.ts only.
import { requireOptionalNativeModule } from "expo";

export type SegmentState = "waiting" | "running" | "done" | "skipped";

/** Fixed for the activity's life. */
export interface ScanActivityAttributes {
  scanId: string;
  plantName: string;
}

/** ScanActivityAttributes.ContentState in Swift. */
export interface ScanActivityState {
  mode: "running" | "question" | "planned" | "failed";
  /** The stage in words. */
  stage: string;
  /** "Next: weighing the evidence". */
  next?: string;
  /** Nine, one per step of the stage list. */
  segments: SegmentState[];
  /** Milliseconds since 1970: the run's start, from which the elapsed time ticks. */
  startedAt: number;
  /** "took 18 s" once the run ends. */
  total?: string;
}

export interface ScanActivityAlert {
  title: string;
  body: string;
}

export type ScanActivityDismissal = { policy: "default" } | { policy: "immediate" } | { policy: "after"; afterSeconds: number };

interface NativeLiveActivity {
  isSupported(): boolean;
  start(attrs: ScanActivityAttributes, state: ScanActivityState): Promise<string>;
  update(id: string, state: ScanActivityState, alert: ScanActivityAlert | null): Promise<void>;
  end(id: string, state: ScanActivityState, dismissal: ScanActivityDismissal): Promise<void>;
}

const native = requireOptionalNativeModule<NativeLiveActivity>("LiveActivity");

/** Whether the module is in this build and Live Activities are allowed now. */
export function isSupported(): boolean {
  try {
    return native?.isSupported() ?? false;
  } catch {
    return false;
  }
}

/** Starts the activity; its id, or undefined without the module. */
export async function start(attrs: ScanActivityAttributes, state: ScanActivityState): Promise<string | undefined> {
  return native ? native.start(attrs, state) : undefined;
}

export async function update(id: string, state: ScanActivityState, alert?: ScanActivityAlert): Promise<void> {
  await native?.update(id, state, alert ?? null);
}

export async function end(id: string, state: ScanActivityState, dismissal: ScanActivityDismissal): Promise<void> {
  await native?.end(id, state, dismissal);
}
