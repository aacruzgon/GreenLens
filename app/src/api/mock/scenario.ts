// What the mock does next: the debug menu and tests drive this (docs/frontend/README.md 15, fault injection).

import { create } from "zustand";
import type { S } from "./util";

/** "auto": Tomato 2 gets the live early-blight scan, every other plant a healthy result. */
export type NextScan = "auto" | "early_blight" | "healthy" | "low_evidence" | "no_plant_found";

export type UploadFault = "image_quality" | "image_too_dark" | "unsupported_media_type";
export type CallFault = "budget_exceeded" | "invalid_state" | "rate_limited";

export interface ScenarioState {
  /** Outcome of the next submitted scan; back to "auto" once a scan uses it. */
  nextScan: NextScan;
  /** The next upload fails with this, then it clears. */
  uploadFault: UploadFault | null;
  /** The next API call (any, except the event stream) fails with this, then it clears. */
  callFault: CallFault | null;
  /** The next event stream drops after a few events (a network error), then it clears. */
  dropNextStream: boolean;
  /** The garden role the mock answers as; below editor, writes get 403. */
  role: S["Role"];
  /** Multiplies every scripted delay. 0 plays a run instantly (tests). */
  delayScale: number;
  /** Milliseconds between `: keepalive` comments on an idle stream (15 000 on the server). */
  keepaliveMs: number;

  setNextScan(v: NextScan): void;
  setUploadFault(v: UploadFault | null): void;
  setCallFault(v: CallFault | null): void;
  setDropNextStream(v: boolean): void;
  setRole(v: S["Role"]): void;
  setDelayScale(v: number): void;
  reset(): void;
}

const INITIAL = {
  nextScan: "auto",
  uploadFault: null,
  callFault: null,
  dropNextStream: false,
  role: "owner",
  delayScale: 1,
  keepaliveMs: 15_000,
} as const satisfies Partial<ScenarioState>;

export const useScenario = create<ScenarioState>()((set) => ({
  ...INITIAL,
  setNextScan: (nextScan) => set({ nextScan }),
  setUploadFault: (uploadFault) => set({ uploadFault }),
  setCallFault: (callFault) => set({ callFault }),
  setDropNextStream: (dropNextStream) => set({ dropNextStream }),
  setRole: (role) => set({ role }),
  setDelayScale: (delayScale) => set({ delayScale }),
  reset: () => set({ ...INITIAL }),
}));

/** Non-hook access for the transport and tests. */
export const scenario = {
  get: () => useScenario.getState(),
  set: (patch: Partial<ScenarioState>) => useScenario.setState(patch),
  reset: () => useScenario.getState().reset(),
};
