// Drives the scan's Live Activity from the live analysis (roadmap F5 item 6): started when the scan screen shows
// a scan still running, updated as its phases tick, ended with the result or when the screen goes. The rules are
// in model/liveActivity.ts; this hook only runs them, one native call after another.
import { useEffect, useMemo, useRef } from "react";

import { liveActivity } from "@/platform";

import {
  activityContent,
  applyActivityOp,
  leaveActivityOp,
  nextActivityOp,
  runStart,
  type ActivityOp,
  type ActivityTrack,
} from "../model/liveActivity";
import type { PhaseLine, PhaseView } from "../model/phases";
import type { ScanState } from "../model/scanState";

interface Session {
  scanId: string;
  track: ActivityTrack;
  /** The activity's id once started; undefined without the module or before the start resolves. */
  id?: string;
  /** Native calls run in order, so an update never overtakes the start it needs. */
  chain: Promise<void>;
  startedAt?: number;
  mounted?: boolean;
}

function run(session: Session, op: ActivityOp, attrs?: { scanId: string; plantName: string }): void {
  if (op.kind === "none") return;
  session.track = applyActivityOp(session.track, op);
  session.chain = session.chain.then(async () => {
    if (op.kind === "start") {
      if (attrs) session.id = await liveActivity.start(attrs, op.content);
      return;
    }
    const id = session.id;
    if (!id) return;
    if (op.kind === "update") await liveActivity.update(id, op.content, op.alert);
    else {
      // The result's alert goes with its last update; ending carries no alert of its own.
      if (op.alert) await liveActivity.update(id, op.content, op.alert);
      await liveActivity.end(id, op.content, op.dismissal);
    }
  });
}

export function useScanActivity(
  scanId: string,
  plantName: string | undefined,
  state: ScanState | undefined,
  phases: readonly PhaseView[],
  line: PhaseLine,
): void {
  const session = useRef<Session>(fresh(scanId));
  const supported = useMemo(() => liveActivity.supported(), []);

  // One session per scan: a new scanId on the same screen starts a new one, and the old one is left (below).
  // Leaving the scan before it ends: nothing would update the activity any more. Checked a tick later, so a
  // remount of the same screen (React's development double mount) keeps it. Declared first, so the update
  // below already sees the new scan's session.
  useEffect(() => {
    let s = session.current;
    if (s.scanId !== scanId) {
      s = fresh(scanId);
      session.current = s;
    }
    s.mounted = true;
    return () => {
      s.mounted = false;
      setTimeout(() => {
        if (!s.mounted) run(s, leaveActivityOp(s.track));
      }, 0);
    };
  }, [scanId]);

  useEffect(() => {
    if (!supported || !state || !plantName || state.scanId !== scanId) return;
    const s = session.current;
    // Fixed once: the clock ticks on the Lock Screen from it, so it must not move with each update.
    s.startedAt ??= runStart(state) ?? Date.now();
    const content = activityContent(state, phases, line, s.startedAt);
    const op = nextActivityOp(s.track, content, { plantName, questionText: state.question?.text });
    run(s, op, { scanId, plantName });
  }, [supported, scanId, plantName, state, phases, line]);
}

function fresh(scanId: string): Session {
  return { scanId, track: { started: false, ended: false }, chain: Promise.resolve() };
}
