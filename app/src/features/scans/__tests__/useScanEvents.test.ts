// The stream's connection rules (flows 17) over the mock's scripted stream: a full replay, a dropped connection
// that resumes with Last-Event-ID without repeating a stage, the answer, and the close at rest; the snapshot
// shown until the replay catches up, a cached record, a fresher record, and a connection replaced mid-refetch.
import { newKey, openScanStream, scans, setTransport, type ScanStreamOptions } from "@/api";
import { IDS, mockStore, mockTransport, resetMock, scenario } from "@/api/mock";
import type { ScanDetail, ScanEvent } from "@/domain/types";

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
    c.sync(await scans.get(scanId), Date.now());

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
    first.c.sync(await scans.get(scanId), Date.now());
    await until(first.c, (s) => s.stream === "resting");
    first.c.dispose();

    const again = controllerFor(scanId);
    again.c.sync(await scans.get(scanId), Date.now());
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
    c.sync(await scans.get(scanId), Date.now());
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
    c.sync(detail, Date.now());
    await until(c, (s) => s.stream === "gone");
    c.dispose();
  });
  it("shows the snapshot, not the history folding by, until the replay reaches its seq", async () => {
    const scanId = await submittedScan(IDS.tomato2);
    const first = controllerFor(scanId);
    first.c.sync(await scans.get(scanId), Date.now());
    await until(first.c, (s) => s.state?.status === "awaiting_answer");
    first.c.dispose();

    const detail = await scans.get(scanId);
    const again = controllerFor(scanId);
    const shown: number[] = [];
    again.c.subscribe(() => shown.push(again.c.getSnapshot().state?.lastSeq ?? -1));
    again.c.sync(detail, Date.now());
    await until(again.c, (s) => (s.state?.lastSeq ?? 0) >= detail.last_event_seq);
    // Only the snapshot (no event applied) or the caught-up state ever reaches the screen.
    expect(shown.every((seq) => seq === 0 || seq >= detail.last_event_seq)).toBe(true);
    expect(again.applied.length).toBeGreaterThan(5);
    // And the replay ran no effects.
    expect(again.effects).toEqual([]);
    again.c.dispose();
  });

  it("waits for a record fetched during the visit rather than seeding from a cached one", async () => {
    const scanId = await submittedScan(IDS.tomato2);
    const cached = await scans.get(scanId);
    const { c, opened } = controllerFor(scanId);
    c.sync(cached, c.createdAt - 1);
    expect(c.getSnapshot().state).toBeUndefined();
    expect(opened).toEqual([]);
    c.sync(await scans.get(scanId), Date.now());
    expect(c.getSnapshot().state?.scanId).toBe(scanId);
    c.dispose();
  });

  it("brings in a question photo's answering state, which comes with no event", async () => {
    const scanId = await submittedScan(IDS.tomato2);
    const { c } = controllerFor(scanId);
    c.sync(await scans.get(scanId), Date.now());
    const asked = await until(c, (s) => s.state?.question?.status === "open" && s.state.status === "awaiting_answer");
    await scans.uploadImage(scanId, { kind: "leaf_closeup", file: new Blob(["x"], { type: "image/jpeg" }), questionId: asked.state!.question!.id }, newKey());
    c.sync(await scans.get(scanId), Date.now());
    expect(c.getSnapshot().state?.question?.status).toBe("answering");
    c.dispose();

    // Opened again while it is answering: the replay's open question doesn't undo it.
    const again = controllerFor(scanId);
    again.c.sync(await scans.get(scanId), Date.now());
    const caught = await until(again.c, (s) => (s.state?.lastSeq ?? 0) >= asked.state!.lastSeq);
    expect(caught.state?.question?.status).toBe("answering");
    again.c.dispose();
  });

  it("drops a replaced connection's refetch, so it can't rest or reconnect over the new one", async () => {
    let resolveRefetch: (d: ScanDetail) => void = () => undefined;
    const opened: number[] = [];
    const scanId = await submittedScan(IDS.tomato2);
    const detail = await scans.get(scanId);
    const c = new ScanEventsController(scanId, {
      // The first stream closes at once; the second stays open until aborted.
      open: (_id, opts) => {
        opened.push(opened.length);
        const n = opened.length;
        return (async function* () {
          if (n === 1) return;
          await new Promise<void>((resolve) => opts.signal?.addEventListener("abort", () => resolve()));
          yield* [];
        })();
      },
      refetch: () => new Promise<ScanDetail>((resolve) => (resolveRefetch = resolve)),
      onEffects: () => undefined,
      backoff: () => 0,
    });
    c.sync(detail, Date.now());
    await until(c, (s) => s.stream === "refetching");
    c.resume();
    expect(opened).toHaveLength(2);
    resolveRefetch({ ...detail, status: "planned" });
    await new Promise((r) => setTimeout(r, 20));
    expect(c.getSnapshot().stream).toBe("connecting");
    expect(c.active).toBe(true);
    expect(opened).toHaveLength(2);
    c.dispose();
  });

  it("never reopens a scan that is gone", async () => {
    const scanId = await submittedScan(IDS.tomato2);
    const { c, opened } = controllerFor(scanId);
    const detail = await scans.get(scanId);
    mockStore.scans.delete(scanId);
    c.sync(detail, Date.now());
    await until(c, (s) => s.stream === "gone");
    const n = opened.length;
    c.setForeground(false);
    c.setForeground(true);
    c.resume();
    expect(c.getSnapshot().stream).toBe("gone");
    expect(opened).toHaveLength(n);
    c.dispose();
  });
});
