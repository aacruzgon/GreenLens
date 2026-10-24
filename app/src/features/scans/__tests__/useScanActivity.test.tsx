import { act, renderHook } from "@testing-library/react-native";

import type { ScanDetail, ScanEvent, ScanEventBody } from "@/domain/types";

import { useScanActivity } from "../hooks/useScanActivity";
import { phaseLine, phasesOf } from "../model/phases";
import { applyScanEvent, seed, type ScanState } from "../model/scanState";

// The native calls, in the order the hook makes them; start resolves only when the test lets it.
const calls: string[] = [];
let releaseStart: (() => void) | undefined;
jest.mock("@/platform", () => ({
  liveActivity: {
    supported: () => true,
    start: jest.fn(
      (attrs: { scanId: string }) =>
        new Promise<string>((resolve) => {
          calls.push(`start ${attrs.scanId}`);
          releaseStart = () => resolve(`act_${attrs.scanId}`);
        }),
    ),
    update: jest.fn(async (id: string, state: { stage: string }) => void calls.push(`update ${id} ${state.stage}`)),
    end: jest.fn(async (id: string) => void calls.push(`end ${id}`)),
  },
}));

const T0 = Date.parse("2026-10-22T14:00:00Z");
const at = (s: number) => new Date(T0 + s * 1000).toISOString();

function running(scanId: string, stages: string[]): ScanState {
  let state = seed({ id: scanId, garden_id: "g", plant_id: "p", run: 1, status: "queued", created_at: at(0), images: [], guided_checks: [], last_event_seq: 0, observations: [] } as unknown as ScanDetail);
  stages.forEach((stage, i) => {
    const body = { type: "stage_started", stage } as ScanEventBody;
    state = applyScanEvent(state, { ...body, v: 1, seq: i + 1, run: 1, scan_id: scanId, ts: at(i) } as ScanEvent);
  });
  return state;
}

function props(scanId: string, stages: string[]) {
  const state = running(scanId, stages);
  const phases = phasesOf(state);
  return { scanId, state, phases, line: phaseLine(state, phases) };
}

const flush = () => act(async () => {
  await new Promise((r) => setTimeout(r, 0));
});

beforeEach(async () => {
  // The last test's hook leaves its activity a tick after its unmount.
  await new Promise((r) => setTimeout(r, 0));
  calls.length = 0;
  releaseStart = undefined;
});

describe("useScanActivity (roadmap F5 item 6)", () => {
  it("never lets an update overtake the start it needs", async () => {
    const hook = await renderHook((p: ReturnType<typeof props>) => useScanActivity(p.scanId, "Tomato 1", p.state, p.phases, p.line), {
      initialProps: props("scan_1", ["plant_check"]),
    });
    await hook.rerender(props("scan_1", ["plant_check", "segmentation", "species"]));
    await flush();
    expect(calls).toEqual(["start scan_1"]);
    await act(async () => releaseStart!());
    await flush();
    expect(calls).toHaveLength(2);
    expect(calls[1]).toMatch(/^update act_scan_1 /);
  });

  it("ends the activity when the screen goes", async () => {
    const hook = await renderHook((p: ReturnType<typeof props>) => useScanActivity(p.scanId, "Tomato 1", p.state, p.phases, p.line), {
      initialProps: props("scan_1", ["plant_check"]),
    });
    await act(async () => releaseStart!());
    await flush();
    await hook.unmount();
    await flush();
    expect(calls).toEqual(["start scan_1", "end act_scan_1"]);
  });

  it("starts a new session for a new scan on the same screen, and leaves the old one", async () => {
    const hook = await renderHook((p: ReturnType<typeof props>) => useScanActivity(p.scanId, "Tomato 1", p.state, p.phases, p.line), {
      initialProps: props("scan_1", ["plant_check"]),
    });
    await act(async () => releaseStart!());
    await flush();
    await hook.rerender(props("scan_2", ["plant_check"]));
    await flush();
    // The old one is left a tick later; the module's start ends any other scan activity first anyway.
    expect(calls).toEqual(["start scan_1", "start scan_2", "end act_scan_1"]);
  });
});
