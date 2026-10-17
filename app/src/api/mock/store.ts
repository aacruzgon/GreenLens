import type { Garden, Me, Plant, ScanDetail, ScanEvent, ScanEventBody, Task, WeatherSummary } from "@/domain/types";
import { isAtRest } from "@/domain/scan";
import { create } from "zustand";

import { seedGarden, seedMe, seedPlants, seedTasks, seedWeather } from "./fixtures";
import { ts } from "./util";

// The mock's in-memory database: the records the REST endpoints read, and each scan's event log.

export type Scenario = "early_blight" | "question" | "healthy";

/** The demo controls' choice, as React state so Settings re-renders when it changes. */
export const useMockScenario = create<{ scenario: Scenario; setScenario: (s: Scenario) => void }>()((set) => ({
  scenario: "early_blight",
  setScenario: (scenario) => set({ scenario }),
}));

interface ScanEntry {
  detail: ScanDetail;
  events: ScanEvent[];
  listeners: Set<(e: ScanEvent) => void>;
  timers: ReturnType<typeof setTimeout>[];
}

class MockDb {
  me: Me = seedMe();
  gardens: Garden[] = [];
  plants: Plant[] = [];
  tasks: Task[] = [];
  weather: WeatherSummary = seedWeather();
  scans = new Map<string, ScanEntry>();
  /** What the next scan shows; set from Settings' demo controls. */
  get scenario(): Scenario {
    return useMockScenario.getState().scenario;
  }
  /** Fault injection: the next upload fails the quality gate (docs/frontend 15). */
  failNextUpload = false;
  /** Milliseconds per scripted step; tests set it to 0. */
  pace = 1;

  constructor() {
    this.reset(true);
  }

  reset(withGarden: boolean) {
    this.me = seedMe();
    this.scans.forEach((s) => s.timers.forEach(clearTimeout));
    this.scans.clear();
    if (withGarden) {
      const g = seedGarden();
      this.gardens = [g];
      this.plants = seedPlants(g.id);
      this.tasks = seedTasks(g.id, this.plants);
    } else {
      this.gardens = [];
      this.plants = [];
      this.tasks = [];
    }
  }

  scan(id: string): ScanEntry | undefined {
    return this.scans.get(id);
  }

  addScan(detail: ScanDetail) {
    this.scans.set(detail.id, { detail, events: [], listeners: new Set(), timers: [] });
  }

  /** The one serialised writer per scan (contracts 4): assigns seq, stores, then broadcasts. */
  emit(scanId: string, body: ScanEventBody) {
    const entry = this.scans.get(scanId);
    if (!entry) return;
    const seq = entry.detail.last_event_seq + 1;
    entry.detail.last_event_seq = seq;
    const event = { ...body, v: 1, seq, run: entry.detail.run, scan_id: scanId, ts: ts() } as ScanEvent;
    entry.events.push(event);
    entry.listeners.forEach((l) => l(event));
  }

  schedule(scanId: string, delayMs: number, fn: () => void) {
    const entry = this.scans.get(scanId);
    if (!entry) return;
    entry.timers.push(setTimeout(fn, delayMs * this.pace));
  }

  atRest(scanId: string): boolean {
    const d = this.scans.get(scanId)?.detail;
    return !d || isAtRest(d);
  }
}

export const db = new MockDb();
