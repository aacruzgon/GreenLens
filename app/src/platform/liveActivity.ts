// The scan's Live Activity on the Lock Screen and in the Dynamic Island (roadmap F5 item 6; design 5). The
// native module is modules/live-activity; it is in development builds only, so in Expo Go, on the web and in
// Jest every call does nothing. A Live Activity is a courtesy: a failure is logged and never reaches the scan.
import * as LiveActivity from "@modules/live-activity";
import type { ScanActivityAlert, ScanActivityAttributes, ScanActivityDismissal, ScanActivityState } from "@modules/live-activity";

export type { ScanActivityAlert, ScanActivityAttributes, ScanActivityDismissal, ScanActivityState };

// Started and not yet ended in this JavaScript session.
const running = new Set<string>();

function warn(what: string, error: unknown): void {
  if (__DEV__) console.warn(`Live Activity: ${what} failed`, error);
}

export const liveActivity = {
  /** The module is in this build and the person allows Live Activities. */
  supported: (): boolean => LiveActivity.isSupported(),

  /** Whether one of this session's activities is still running: the scan stream stays open a little in the
   * background while one is (features/scans useScanEvents). */
  isRunning: (): boolean => running.size > 0,

  start: async (attrs: ScanActivityAttributes, state: ScanActivityState): Promise<string | undefined> => {
    if (!LiveActivity.isSupported()) return undefined;
    try {
      const id = await LiveActivity.start(attrs, state);
      if (id) running.add(id);
      return id;
    } catch (error) {
      warn("start", error);
      return undefined;
    }
  },

  update: async (id: string, state: ScanActivityState, alert?: ScanActivityAlert): Promise<void> => {
    try {
      await LiveActivity.update(id, state, alert);
    } catch (error) {
      warn("update", error);
    }
  },

  end: async (id: string, state: ScanActivityState, dismissal: ScanActivityDismissal): Promise<void> => {
    running.delete(id);
    try {
      await LiveActivity.end(id, state, dismissal);
    } catch (error) {
      warn("end", error);
    }
  },
};
