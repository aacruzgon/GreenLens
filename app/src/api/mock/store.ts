// The in-memory fixture store behind the mock transport (docs/frontend/README.md 15).
// Resettable: `mockStore.reset()` re-seeds the demo story with dates relative to now.

import { buildSeed, type StoredPlant, type StoredScan, type Seed } from "./fixtures";
import { DateKit, type S } from "./util";

export type { StoredScan, StoredPlant };

export type RunCtx = {
  store: MockStore;
  scanId: string;
  plantId: string;
  run: number;
  dates: DateKit;
  /** A new id with the prefix, unique until the next reset. */
  id(prefix: string): string;
  /** The current time, as a Timestamp. */
  ts(): string;
};

/** One scripted event: wait `delay` ms (scaled by the scenario), then send what `body` returns. */
export interface Step {
  delay: number;
  body: (ctx: RunCtx) => S["ScanEventBody"];
}

/** A scripted scan run. `afterAnswer` exists when the run stops at an indicator question. */
export interface Script {
  start: (ctx: RunCtx) => Step[];
  afterAnswer?: (ctx: RunCtx, answer: S["Answer"]) => Step[];
}

const GROWTH: S["GrowthQuantity"][] = ["plant_height_cm", "canopy_width_cm"];

type WaitResult = "event" | "timeout" | "reset";

export class MockStore {
  user!: S["Me"];
  gardens = new Map<string, S["Garden"]>();
  maps = new Map<string, S["GardenMap"] | null>();
  weather!: S["WeatherSummary"];
  species: S["SpeciesSummary"][] = [];
  plants = new Map<string, StoredPlant>();
  scans = new Map<string, StoredScan>();
  tasks: S["Task"][] = [];
  memories: S["Memory"][] = [];
  pushTokens = new Set<string>();
  scripts = new Map<string, Script>();
  /** Bumped by every reset; running scripts and open streams stop when it changes. */
  generation = 0;
  dates = new DateKit();

  private counter = 1000;
  private timers = new Set<ReturnType<typeof setTimeout>>();
  private listeners = new Map<string, Set<() => void>>();

  constructor() {
    this.reset();
  }

  reset(now: Date = new Date()): void {
    for (const t of this.timers) clearTimeout(t);
    this.timers.clear();
    this.generation += 1;
    const old = [...this.listeners.values()].flatMap((set) => [...set]);
    this.listeners.clear();
    for (const fn of old) fn();

    this.dates = new DateKit(now);
    const seed: Seed = buildSeed(this.dates);
    this.user = seed.user;
    this.gardens = new Map([[seed.garden.id, seed.garden]]);
    this.maps = new Map([[seed.garden.id, seed.map]]);
    this.weather = seed.weather;
    this.species = seed.species;
    this.plants = new Map(seed.plants.map((p) => [p.id, p]));
    this.scans = new Map(seed.scans.map((s) => [s.scan.id, s]));
    this.tasks = seed.tasks;
    this.memories = seed.memories;
    this.pushTokens = new Set();
    this.scripts = new Map();
    this.counter = 1000;
  }

  nextId(prefix: string): string {
    this.counter += 1;
    return `${prefix}_${this.counter}`;
  }

  /** Resolves after `ms`; never resolves after a reset (the caller checks `generation`). */
  sleep(ms: number): Promise<void> {
    return new Promise((resolve) => {
      const t = setTimeout(() => {
        this.timers.delete(t);
        resolve();
      }, ms);
      this.timers.add(t);
    });
  }

  // ---- change notification, for open event streams

  subscribe(scanId: string, fn: () => void): () => void {
    let set = this.listeners.get(scanId);
    if (!set) this.listeners.set(scanId, (set = new Set()));
    set.add(fn);
    return () => set.delete(fn);
  }

  notify(scanId: string): void {
    for (const fn of [...(this.listeners.get(scanId) ?? [])]) fn();
  }

  /** Waits for the next change to the scan, a reset, or `ms` (the keepalive interval). */
  waitFor(scanId: string, ms: number): Promise<WaitResult> {
    return new Promise((resolve) => {
      const gen = this.generation;
      let timer: ReturnType<typeof setTimeout> | undefined;
      const done = (r: WaitResult) => {
        if (timer) {
          clearTimeout(timer);
          this.timers.delete(timer);
        }
        unsub();
        resolve(r);
      };
      const unsub = this.subscribe(scanId, () => done(this.generation !== gen ? "reset" : "event"));
      timer = setTimeout(() => done("timeout"), ms);
      this.timers.add(timer);
    });
  }

  // ---- reads

  scan(id: string): StoredScan | undefined {
    return this.scans.get(id);
  }

  plantView(p: StoredPlant): S["Plant"] {
    const latest = this.scansOf(p.id).filter((s) => s.scan.status !== "pending").at(-1);
    if (!latest) return { ...p };
    const thumb = latest.scan.images.find((i) => i.kind === "whole_plant");
    const latest_scan: S["LatestScan"] = {
      scan_id: latest.scan.id,
      status: latest.scan.status,
      created_at: latest.scan.created_at,
      ...(thumb ? { thumbnail_image_id: thumb.id, thumbnail_url: thumb.url } : {}),
    };
    return { ...p, latest_scan };
  }

  /** A plant's scans, oldest first. */
  scansOf(plantId: string): StoredScan[] {
    return [...this.scans.values()]
      .filter((s) => s.scan.plant_id === plantId)
      .sort((a, b) => a.scan.created_at.localeCompare(b.scan.created_at));
  }

  scanDetail(st: StoredScan): S["ScanDetail"] {
    const plant = this.plants.get(st.scan.plant_id);
    if (!plant) throw new Error(`mock: scan ${st.scan.id} has no plant`);
    const detail: S["ScanDetail"] = {
      ...st.scan,
      observations: st.observations,
      plant: this.plantView(plant),
    };
    if (st.diagnosis) detail.diagnosis = st.diagnosis;
    if (st.plan) detail.plan = st.plan;
    if (st.weather) detail.weather = st.weather;
    return detail;
  }

  /** Newest first. */
  timelineOf(plantId: string): S["TimelineEntry"][] {
    return this.scansOf(plantId)
      .filter((s) => s.scan.status !== "pending")
      .map((s): S["TimelineEntry"] => {
        const entry: S["TimelineEntry"] = {
          scan_id: s.scan.id,
          created_at: s.scan.created_at,
          status: s.scan.status,
        };
        if (s.diagnosis) entry.diagnosis_status = s.diagnosis.status;
        if (s.diagnosis?.primary) {
          entry.primary = { problem_id: s.diagnosis.primary.problem_id, name: s.diagnosis.primary.name };
        }
        if (s.scan.plan_id) entry.plan_id = s.scan.plan_id;
        if (s.scan.failure) entry.failure_code = s.scan.failure.code;
        const thumb = s.scan.images.find((i) => i.kind === "whole_plant");
        if (thumb) {
          entry.thumbnail_image_id = thumb.id;
          entry.thumbnail_url = thumb.url;
        }
        const m: S["TimelineMeasurements"] = {};
        for (const q of GROWTH) {
          const best = this.bestGrowth(s, q);
          if (best) m[q] = best.value;
        }
        if (Object.keys(m).length) entry.measurements = m;
        return entry;
      })
      .reverse();
  }

  private bestGrowth(s: StoredScan, q: S["GrowthQuantity"]): S["MeasurementPoint"] | undefined {
    let best: S["Observation"] | undefined;
    for (const o of s.observations) {
      if (o.detail?.measurement?.quantity !== q || o.confidence_band === "low") continue;
      if (!best || o.confidence > best.confidence) best = o;
    }
    if (!best?.detail?.measurement) return undefined;
    return {
      scan_id: s.scan.id,
      quantity: q,
      value: best.detail.measurement.value,
      confidence_band: best.confidence_band,
      created_at: s.scan.created_at,
    };
  }

  /** The growth series, oldest first: one point per scan and quantity (contracts 7). */
  pointsOf(plantId: string, quantity?: S["GrowthQuantity"]): S["MeasurementPoint"][] {
    const out: S["MeasurementPoint"][] = [];
    for (const s of this.scansOf(plantId)) {
      if (s.scan.status === "pending" || s.scan.status === "failed") continue;
      for (const q of quantity ? [quantity] : GROWTH) {
        const p = this.bestGrowth(s, q);
        if (p) out.push(p);
      }
    }
    return out;
  }

  // ---- scan events

  /** Assigns the next `seq`, stores the event, applies it to the scan and wakes open streams. */
  emit(scanId: string, body: S["ScanEventBody"]): S["ScanEvent"] {
    const st = this.scans.get(scanId);
    if (!st) throw new Error(`mock: no scan ${scanId}`);
    const event = {
      ...body,
      v: 1,
      seq: st.scan.last_event_seq + 1,
      run: st.scan.run,
      scan_id: scanId,
      ts: new Date().toISOString(),
    } as S["ScanEvent"];
    st.scan.last_event_seq = event.seq;
    st.events.push(event);
    this.reduce(st, event);
    this.notify(scanId);
    return event;
  }

  private reduce(st: StoredScan, ev: S["ScanEvent"]): void {
    switch (ev.type) {
      case "status_changed":
        st.scan.status = ev.to;
        break;
      case "finding":
        st.observations.push(ev.observation);
        break;
      case "finding_retracted":
        st.observations = st.observations.filter((o) => o.id !== ev.observation_id);
        break;
      case "weather_loaded":
        st.weather = ev.weather;
        break;
      case "question":
        st.scan.open_question = ev.question;
        break;
      case "question_resolved":
        delete st.scan.open_question;
        break;
      case "diagnosis": {
        st.diagnosis = ev.diagnosis;
        st.scan.diagnosis_id = ev.diagnosis.id;
        const plant = this.plants.get(st.scan.plant_id);
        if (plant && plant.status !== "dead") {
          plant.status =
            ev.diagnosis.status === "confirmed" ? "sick" : ev.diagnosis.status === "uncertain" ? "watch" : "healthy";
        }
        break;
      }
      case "plan_ready":
        st.scan.plan_id = ev.plan_id;
        break;
      case "failed":
        st.scan.failure = ev.failure;
        break;
      default:
        break;
    }
  }

  /** Stores a plan on its scan and opens the tasks its steps create. */
  attachPlan(scanId: string, plan: S["ActionPlan"], tasks: S["Task"][]): void {
    const st = this.scans.get(scanId);
    if (!st) return;
    plan.task_ids = tasks.map((t) => t.id);
    st.plan = plan;
    this.tasks.push(...tasks);
  }

  /** True when the server would close the stream: nothing running, planned or failed, no open question. */
  atRest(scanId: string): boolean {
    const st = this.scans.get(scanId);
    if (!st) return true;
    if (st.running) return false;
    if (st.scan.open_question) return false;
    return st.scan.status === "planned" || st.scan.status === "failed" || st.scan.status === "pending";
  }

  runCtx(scanId: string): RunCtx {
    const st = this.scans.get(scanId);
    if (!st) throw new Error(`mock: no scan ${scanId}`);
    return {
      store: this,
      scanId,
      plantId: st.scan.plant_id,
      run: st.scan.run,
      dates: new DateKit(),
      id: (prefix) => this.nextId(prefix),
      ts: () => new Date().toISOString(),
    };
  }
}

export const mockStore = new MockStore();
