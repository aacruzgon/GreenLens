// The scan screen's live state (docs/frontend/README.md 10; flows 5 and 17): the snapshot, the stream's full
// replay, live events, reconnects with Last-Event-ID and backoff, the 45-second idle rule, and closing in the
// background. Only the screen that shows a scan holds its stream.
//
// The connection is a plain object (`ScanEventsController`), so its state machine runs and is tested without
// React; the hook subscribes to it.
import { useQueryClient } from "@tanstack/react-query";
import { useFocusEffect } from "expo-router";
import { useCallback, useEffect, useState, useSyncExternalStore } from "react";
import { AppState } from "react-native";

import { isApiError, openScanStream, qk, scans, useScan, type ScanStreamOptions } from "@/api";
import type { ScanDetail, ScanEvent } from "@/domain/types";
import { needsStream } from "@/domain/scan";
import { backoffMs } from "@/lib/backoff";
import { haptics } from "@/platform";

import { phasesOf, tickCount } from "../model/phases";
import { applyScanEvent, effectsOf, isReplay, reconcile, seed, stateAtRest, type ScanEffect, type ScanState } from "../model/scanState";

/** No bytes for this long, three missed keepalives, is a broken stream (README 10). */
export const IDLE_MS = 45_000;

/** The states of flows 17. */
export type StreamStatus = "idle" | "connecting" | "streaming" | "refetching" | "waiting" | "paused" | "resting" | "gone";

export interface ScanEventsSnapshot {
  state: ScanState | undefined;
  stream: StreamStatus;
}

export interface ControllerDeps {
  open: (scanId: string, opts: ScanStreamOptions) => AsyncIterable<ScanEvent>;
  refetch: (scanId: string) => Promise<ScanDetail>;
  /** A live event's effects, and the haptic for a new tick on the stage card. */
  onEffects: (effects: ScanEffect[]) => void;
  idleMs?: number;
  /** The wait before reconnect number `attempt`; 1, 2, 4 … 30 s by default. */
  backoff?: (attempt: number) => number;
}

export class ScanEventsController {
  private snap: ScanEventsSnapshot = { state: undefined, stream: "idle" };
  private readonly listeners = new Set<() => void>();
  // Events fold into `working`. Until the full replay reaches the snapshot's seq, the screen shows `base`, the
  // snapshot itself, rather than the history folding by (README 10, step 4).
  private working: ScanState | undefined;
  private base: ScanState | undefined;
  private abort: AbortController | undefined;
  private timer: ReturnType<typeof setTimeout> | undefined;
  // Bumped by `stop`: a connection's continuations after an await check it, so one that was replaced (a resume
  // during its refetch, the background, unmount) can't touch the state or arm a timer.
  private gen = 0;
  private attempt = 0;
  // The first `connecting` of a visit replays the whole history; later ones send the last applied seq.
  private first = true;
  private foreground = true;
  private disposed = false;
  /** A record fetched before this, from the cache, can't be the visit's snapshot (`sync`). */
  readonly createdAt = Date.now();

  constructor(
    private readonly scanId: string,
    private readonly deps: ControllerDeps,
  ) {}

  subscribe = (fn: () => void): (() => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };

  getSnapshot = (): ScanEventsSnapshot => this.snap;

  private set(patch: Partial<ScanEventsSnapshot>): void {
    this.snap = { ...this.snap, ...patch };
    for (const fn of this.listeners) fn();
  }

  private publish(patch: Partial<ScanEventsSnapshot> = {}): void {
    const w = this.working;
    this.set({ state: w && w.lastSeq >= w.snapshotSeq ? w : this.base, ...patch });
  }

  /**
   * Takes a `ScanDetail` fetched at `fetchedAt`. The first one fetched during this visit seeds the state and opens
   * the stream unless the scan is at rest: a cached one has an older `last_event_seq`, so events already seen
   * would count as live, with their haptics and animations. Later ones bring the state up to the record
   * (`reconcile`).
   */
  sync(detail: ScanDetail, fetchedAt: number): void {
    if (this.disposed || detail.id !== this.scanId) return;
    if (!this.working || !this.base) {
      if (fetchedAt < this.createdAt) return;
      this.working = this.base = seed(detail);
      const stream = needsStream(detail);
      this.publish({ stream: stream ? "idle" : "resting" });
      if (stream) void this.connect();
      return;
    }
    this.working = reconcile(this.working, detail);
    this.base = reconcile(this.base, detail);
    this.publish();
  }

  /** Reopens a resting or waiting stream: late work, a species change, a retry, a push (flows 17). A scan that
   * is gone stays gone. */
  resume(): void {
    if (!this.working || this.abort || this.disposed || this.snap.stream === "gone") return;
    this.attempt = 0;
    void this.connect();
  }

  setForeground(fg: boolean): void {
    if (fg === this.foreground) return;
    this.foreground = fg;
    if (!fg) {
      if (this.active || this.snap.stream === "refetching") {
        this.stop();
        this.set({ stream: "paused" });
      }
    } else if (this.snap.stream === "paused" || (this.working && !stateAtRest(this.working) && !this.active)) {
      this.resume();
    }
  }

  /** Whether a stream is open or about to be. */
  get active(): boolean {
    return this.abort !== undefined || this.timer !== undefined;
  }

  dispose(): void {
    this.disposed = true;
    this.stop();
    this.listeners.clear();
  }

  private stop(): void {
    this.gen += 1;
    this.abort?.abort();
    this.abort = undefined;
    if (this.timer) clearTimeout(this.timer);
    this.timer = undefined;
  }

  private apply(event: ScanEvent): void {
    const prev = this.working;
    if (!prev) return;
    const next = applyScanEvent(prev, event);
    if (next === prev) return;
    this.working = next;
    this.publish();
    if (isReplay(prev, event)) return;
    const effects = effectsOf(event);
    // One haptic per tick on the stage card, not per pipeline stage (several stages make one tick).
    if (tickCount(phasesOf(next)) > tickCount(phasesOf(prev))) effects.push({ kind: "haptic", type: "selection" });
    if (effects.length > 0) this.deps.onEffects(effects);
  }

  private async connect(): Promise<void> {
    this.stop();
    const gen = this.gen;
    const working = this.working;
    if (!working || this.disposed || !this.foreground) return;
    const ctrl = new AbortController();
    this.abort = ctrl;
    this.set({ stream: "connecting" });

    let idle: ReturnType<typeof setTimeout> | undefined;
    let timedOut = false;
    let bytes = false;
    // Any byte, a keepalive too, means the stream is up (flows 17: 200 → streaming).
    const onActivity = () => {
      if (gen !== this.gen) return;
      if (!bytes) {
        bytes = true;
        this.set({ stream: "streaming" });
        this.attempt = 0;
      }
      touch();
    };
    const touch = () => {
      if (idle) clearTimeout(idle);
      idle = setTimeout(() => {
        timedOut = true;
        ctrl.abort();
      }, this.deps.idleMs ?? IDLE_MS);
    };

    let outcome: "closed" | "error" | "gone" = "closed";
    try {
      touch();
      const lastEventId = this.first ? undefined : working.lastSeq;
      this.first = false;
      for await (const event of this.deps.open(this.scanId, { lastEventId, signal: ctrl.signal, onActivity })) {
        if (gen !== this.gen) break;
        if (!bytes) onActivity();
        this.apply(event);
      }
      if (timedOut) outcome = "error";
    } catch (error) {
      outcome = isApiError(error) && error.status === 404 ? "gone" : "error";
    } finally {
      if (idle) clearTimeout(idle);
    }
    // Replaced or stopped meanwhile (the background, unmount, a newer connect): whoever did that decides what
    // comes next. The idle timeout aborts without `stop`, so it carries on as an error.
    if (gen !== this.gen || this.disposed) return;
    this.abort = undefined;
    if (outcome === "gone") {
      this.set({ stream: "gone" });
      return;
    }

    if (outcome === "closed") {
      // The server closed it: the refetched record says whether that was a resting point (README 10).
      this.set({ stream: "refetching" });
      try {
        const fresh = await this.deps.refetch(this.scanId);
        if (gen !== this.gen || this.disposed) return;
        this.sync(fresh, Date.now());
        if (!needsStream(fresh)) {
          this.set({ stream: "resting" });
          return;
        }
      } catch (error) {
        if (gen !== this.gen || this.disposed) return;
        if (isApiError(error) && error.status === 404) {
          this.set({ stream: "gone" });
          return;
        }
      }
    }

    const wait = (this.deps.backoff ?? backoffMs)(this.attempt);
    this.attempt += 1;
    this.set({ stream: "waiting" });
    this.timer = setTimeout(() => {
      this.timer = undefined;
      void this.connect();
    }, wait);
  }
}

export interface ScanEventsResult extends ScanEventsSnapshot {
  detail: ScanDetail | undefined;
  /** The snapshot failed to load (not a stream error). */
  error: unknown;
  resume: () => void;
  /** Whether the replay has caught up with the snapshot, so new events animate. */
  live: boolean;
}

export function useScanEvents(scanId: string): ScanEventsResult {
  const qc = useQueryClient();
  const scan = useScan(scanId);
  const detail = scan.data;

  const [controller] = useState(
    () =>
      new ScanEventsController(scanId, {
        open: openScanStream,
        refetch: async (id) => {
          const fresh = await scans.get(id);
          qc.setQueryData(qk.scan(id), fresh);
          return fresh;
        },
        onEffects: (effects) => {
          const cached = qc.getQueryData<ScanDetail>(qk.scan(scanId));
          const plantId = cached?.plant_id;
          const gardenId = cached?.garden_id;
          for (const e of effects) {
            if (e.kind === "haptic") {
              void haptics[e.type]();
              continue;
            }
            for (const key of e.keys) {
              const queryKey =
                key === "scan" ? qk.scan(scanId)
                : key === "plant" ? plantId && qk.plant(plantId)
                : key === "plants" ? gardenId && qk.plants(gardenId)
                : key === "timeline" ? plantId && qk.timeline(plantId)
                : key === "measurements" ? plantId && qk.measurements(plantId)
                : key === "tasks" ? gardenId && qk.tasks(gardenId)
                : gardenId && qk.memories(gardenId);
              if (queryKey) void qc.invalidateQueries({ queryKey });
            }
          }
        },
      }),
  );
  const snap = useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot);

  // Every record the query holds, the first one fetched during this visit seeding the state.
  const fetchedAt = scan.dataUpdatedAt;
  useEffect(() => {
    if (detail) controller.sync(detail, fetchedAt);
  }, [controller, detail, fetchedAt]);

  useEffect(() => () => controller.dispose(), [controller]);

  // Closed in the background, reopened on return from the last applied seq (README 10). `inactive` (Control
  // Center, Notification Center, a call's banner) leaves the stream open.
  useEffect(() => {
    const sub = AppState.addEventListener("change", (next) => {
      if (next === "background") controller.setForeground(false);
      else if (next === "active") controller.setForeground(true);
    });
    return () => sub.remove();
  }, [controller]);

  // Focused, on open and on return (a question photo, a soil check): the record is fetched again, which seeds a
  // visit whose cached record is older than it, brings an `answering` question or an `analysing` check in, and
  // gives a scan that isn't at rest its stream back (flows 17).
  const { refetch } = scan;
  useFocusEffect(
    useCallback(() => {
      void refetch({ cancelRefetch: false }).then((r) => {
        if (r.data && needsStream(r.data) && !controller.active) controller.resume();
      });
    }, [controller, refetch]),
  );

  const resume = useCallback(() => controller.resume(), [controller]);
  const live = snap.state !== undefined && snap.state.lastSeq >= snap.state.snapshotSeq;
  return { ...snap, detail, error: scan.error, resume, live };
}
