// The stream's connection rules (flows 17) over the mock's scripted stream: a full replay, a dropped connection
// that resumes with Last-Event-ID without repeating a stage, the answer, and the close at rest.
import { newKey, openScanStream, scans, setTransport, type ScanStreamOptions } from "@/api";
import { IDS, mockStore, mockTransport, resetMock, scenario } from "@/api/mock";
import type { ScanEvent } from "@/domain/types";

import { ScanEventsController, type ScanEventsSnapshot } from "../hooks/useScanEvents";
import type { ScanEffect } from "../model/scanState";

beforeEach(() => {
  resetMock();
  scenario.set({ delayScale: 0, keepaliveMs: 50 });
  setTransport(mockTransport);
});

async function submittedScan(plantId: string): Promise<string> {
  const scan = await scans.create({ garden_id: IDS.garden, plant_id: plantId }, newKey());
  await scans.uploadImage(scan.id, { kind: "whole_plant", file: new Blob(["x"], { type: "image/jpeg" }) }, newKey());
  await scans.uploadImage(scan.id, { kind: "leaf_closeup", file: new Blob(["x"], { type: "image/jpeg" }) }, newKey());
  await scans.submit(scan.id);
  return scan.id;
}

function until(c: ScanEventsController, ok: (s: ScanEventsSnapshot) => boolean, ms = 4000): Promise<ScanEventsSnapshot> {
  return new Promise((resolve, reject) => {
    const check = () => {
      const s = c.getSnapshot();
      if (ok(s)) {
        off();
        clearTimeout(t);
        resolve(s);
      }
    };
    const off = c.subscribe(check);
    const t = setTimeout(() => {
      off();
      reject(new Error(`timed out in ${c.getSnapshot().stream} at seq ${c.getSnapshot().state?.lastSeq}`));
    }, ms);
    check();
  });
}

function controllerFor(scanId: string) {
  const opened: (number | undefined)[] = [];
  const applied: ScanEvent[] = [];
  const effects: ScanEffect[] = [];
  const c = new ScanEventsController(scanId, {
    open: (id: string, opts: ScanStreamOptions) => {
      opened.push(opts.lastEventId);
      return (async function* () {
        for await (const e of openScanStream(id, opts)) {
          applied.push(e);
          yield e;
        }
      })();
    },
    refetch: (id) => scans.get(id),
    onEffects: (e) => effects.push(...e),
    backoff: () => 0,
  });
  return { c, opened, applied, effects };
}

describe("ScanEventsController", () => {
  it("replays, resumes a dropped stream from its last seq without repeating a stage, then rests", async () => {
    const scanId = await submittedScan(IDS.tomato2);
    scenario.set({ dropNextStream: true });
    const { c, opened, applied, effects } = controllerFor(scanId);
    c.start(await scans.get(scanId));

    const asked = await until(c, (s) => s.state?.status === "awaiting_answer");
    // The first open replays everything; the reconnect after the drop sends the last applied seq.
    expect(opened[0]).toBeUndefined();
    expect(opened.length).toBeGreaterThanOrEqual(2);
    expect(opened[1]).toBeGreaterThan(0);
    const seqs = applied.map((e) => e.seq);
    expect(new Set(seqs).size).toBe(seqs.length);
    const started = applied.filter((e) => e.type === "stage_started").map((e) => e.type === "stage_started" && e.stage);
    expect(new Set(started).size).toBe(started.length);
    expect(asked.state?.question?.kind).toBe("indicator");

    await scans.answer(scanId, { question_id: asked.state!.question!.id, option_id: "yes" }, newKey());
    const done = await until(c, (s) => s.stream === "resting");
    expect(done.state?.status).toBe("planned");
    expect(done.state?.plan?.provisional).toBe(false);
    expect(done.state?.lastSeq).toBe(mockStore.scan(scanId)!.events.length);
    expect(effects).toContainEqual({ kind: "haptic", type: "success" });
    expect(effects.some((e) => e.kind === "invalidate" && e.keys.includes("tasks"))).toBe(true);
    c.dispose();
  });

  it("opens no stream for a scan at rest, and reopens it for late work", async () => {
    scenario.set({ nextScan: "healthy" });
    const scanId = await submittedScan(IDS.kale);
    const first = controllerFor(scanId);
    first.c.start(await scans.get(scanId));
    await until(first.c, (s) => s.stream === "resting");
    first.c.dispose();

    const again = controllerFor(scanId);
    again.c.start(await scans.get(scanId));
    expect(again.c.getSnapshot().stream).toBe("resting");
    expect(again.opened).toEqual([]);
    again.c.resume();
    await until(again.c, (s) => s.stream === "resting" && again.opened.length === 1);
    // Reopened at rest, it replays the history from the start and closes; nothing is applied twice.
    expect(again.c.getSnapshot().state?.status).toBe("planned");
    again.c.dispose();
  });

  it("pauses in the background and resumes from the last applied seq", async () => {
    const scanId = await submittedScan(IDS.tomato2);
    const { c, opened } = controllerFor(scanId);
    c.start(await scans.get(scanId));
    await until(c, (s) => s.state?.status === "awaiting_answer");
    c.setForeground(false);
    expect(c.getSnapshot().stream).toBe("paused");
    const seq = c.getSnapshot().state!.lastSeq;
    c.setForeground(true);
    await until(c, (s) => s.stream === "streaming");
    expect(opened[opened.length - 1]).toBe(seq);
    c.dispose();
  });

  it("reports a scan that is gone", async () => {
    const scanId = await submittedScan(IDS.tomato2);
    const { c } = controllerFor(scanId);
    const detail = await scans.get(scanId);
    mockStore.scans.delete(scanId);
    c.start(detail);
    await until(c, (s) => s.stream === "gone");
    c.dispose();
  });
});
