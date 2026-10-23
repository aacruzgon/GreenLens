// When the scan's stream closes and reopens as the app leaves and returns (README 10; roadmap F5 item 6). Pure
// TypeScript with timers: the hook feeds it AppState's changes.
//
// While the scan's Live Activity runs, the stream stays open for `graceMs` after the app leaves, so the Lock Screen
// follows the scan. iOS may suspend JavaScript before that timer fires; then the stream was never closed, and
// marking the app foreground again would do nothing, leaving a dead socket until an error or the watchdog. So the
// time the app left is kept, and a return after the grace closes the stream first, which resumes it from
// Last-Event-ID.

export interface ForegroundTarget {
  setForeground(foreground: boolean): void;
}

export type AppPhase = "active" | "background" | "inactive" | "unknown" | "extension";

export function foregroundGrace(target: ForegroundTarget, opts: { graceMs: number; liveActivityRunning: () => boolean; now?: () => number }) {
  const now = opts.now ?? Date.now;
  let grace: ReturnType<typeof setTimeout> | undefined;
  let backgroundAt: number | undefined;
  const clearGrace = () => {
    if (grace) clearTimeout(grace);
    grace = undefined;
  };
  return {
    change(next: AppPhase) {
      if (next === "background") {
        clearGrace();
        backgroundAt = now();
        if (opts.liveActivityRunning()) {
          grace = setTimeout(() => {
            grace = undefined;
            target.setForeground(false);
          }, opts.graceMs);
        } else {
          target.setForeground(false);
        }
      } else if (next === "active") {
        // A grace timer still pending past its time didn't run: JavaScript was suspended with the stream open.
        const missed = grace !== undefined && backgroundAt !== undefined && now() - backgroundAt >= opts.graceMs;
        clearGrace();
        backgroundAt = undefined;
        if (missed) target.setForeground(false);
        target.setForeground(true);
      }
    },
    dispose: clearGrace,
  };
}
