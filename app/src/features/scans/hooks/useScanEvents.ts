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
import { applyScanEvent, effectsOf, isReplay, seed, stateAtRest, type ScanEffect, type ScanState } from "../model/scanState";

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
  private abort: AbortController | undefined;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private attempt = 0;
  // The first `connecting` of a visit replays the whole history; later ones send the last applied seq.
  private first = true;
  private foreground = true;
  private disposed = false;

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

  /** Seeds from the first snapshot of the visit, then opens the stream unless the scan is at rest. */
  start(detail: ScanDetail): void {
    if (this.snap.state || this.disposed) return;
    this.set({ state: seed(detail), stream: needsStream(detail) ? "idle" : "resting" });
    if (needsStream(detail)) void this.connect();
  }

  /** Reopens a resting or waiting stream: late work, a species change, a retry, a push (flows 17). */
  resume(): void {
    if (!this.snap.state || this.abort || this.disposed) return;
    this.attempt = 0;
    void this.connect();
  }

  setForeground(fg: boolean): void {
    if (fg === this.foreground) return;
    this.foreground = fg;
    if (!fg) {
      if (this.abort || this.timer) {
        this.stop();
        this.set({ stream: "paused" });
      }
    } else if (this.snap.stream === "paused" || (this.snap.state && !stateAtRest(this.snap.state) && !this.abort && !this.timer)) {
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
    this.abort?.abort();
    this.abort = undefined;
    if (this.timer) clearTimeout(this.timer);
    this.timer = undefined;
  }

  private apply(event: ScanEvent): void {
    const prev = this.snap.state;
    if (!prev) return;
    const next = applyScanEvent(prev, event);
    if (next === prev) return;
    this.set({ state: next });
    if (isReplay(prev, event)) return;
    const effects = effectsOf(event);
    // One haptic per tick on the stage card, not per pipeline stage (several stages make one tick).
    if (tickCount(phasesOf(next)) > tickCount(phasesOf(prev))) effects.push({ kind: "haptic", type: "selection" });
    if (effects.length > 0) this.deps.onEffects(effects);
  }

  private async connect(): Promise<void> {
    this.stop();
    if (!this.snap.state || this.disposed || !this.foreground) return;
    const ctrl = new AbortController();
    this.abort = ctrl;
    this.set({ stream: "connecting" });

    let idle: ReturnType<typeof setTimeout> | undefined;
    let timedOut = false;
    let bytes = false;
    // Any byte, a keepalive too, means the stream is up (flows 17: 200 → streaming).
    const onActivity = () => {
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
      const lastEventId = this.first ? undefined : this.snap.state.lastSeq;
      this.first = false;
      for await (const event of this.deps.open(this.scanId, { lastEventId, signal: ctrl.signal, onActivity })) {
        if (!bytes) onActivity();
        this.apply(event);
      }
      if (timedOut) outcome = "error";
    } catch (error) {
      outcome = isApiError(error) && error.status === 404 ? "gone" : "error";
    } finally {
      if (idle) clearTimeout(idle);
    }
    // Aborted by us (the background, unmount, a new connect): whoever aborted decides what comes next.
    if ((ctrl.signal.aborted && !timedOut) || this.disposed) return;
    if (this.abort === ctrl) this.abort = undefined;
    if (outcome === "gone") {
      this.set({ stream: "gone" });
      return;
    }

    if (outcome === "closed") {
      // The server closed it: the refetched record says whether that was a resting point (README 10).
      this.set({ stream: "refetching" });
      try {
        const fresh = await this.deps.refetch(this.scanId);
        if (this.disposed) return;
        if (!needsStream(fresh)) {
          this.set({ stream: "resting" });
          return;
        }
      } catch (error) {
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

  useEffect(() => {
    if (detail) controller.start(detail);
  }, [controller, detail]);

  useEffect(() => () => controller.dispose(), [controller]);

  // Closed in the background, reopened on return from the last applied seq (README 10).
  useEffect(() => {
    const sub = AppState.addEventListener("change", (next) => controller.setForeground(next === "active"));
    return () => sub.remove();
  }, [controller]);

  // Focused again: a refetched record that isn't at rest gets its stream back (flows 17).
  const { refetch } = scan;
  useFocusEffect(
    useCallback(() => {
      if (!controller.getSnapshot().state || controller.active) return;
      void refetch().then((r) => {
        if (r.data && needsStream(r.data)) controller.resume();
      });
    }, [controller, refetch]),
  );

  const resume = useCallback(() => controller.resume(), [controller]);
  const live = snap.state !== undefined && snap.state.lastSeq >= snap.state.snapshotSeq;
  return { ...snap, detail, error: scan.error, resume, live };
}
