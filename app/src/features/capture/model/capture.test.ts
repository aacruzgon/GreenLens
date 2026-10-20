import { distanceHint } from "./distanceRanges";
import {
  applySlot,
  canAnalyse,
  coverage,
  createDraft,
  modeMark,
  pendingWork,
  resumeDraft,
  shutterTarget,
  takeShot,
  uploadOutcome,
  type CaptureDraft,
  type UploadRefusal,
} from "./draft";
import { MODES } from "./shotPlan";
import { emptySlot, slotReducer, type ShotFile, type Slot } from "./slot";

const camera = (n: string): ShotFile => ({ uri: `file:///draft/${n}.jpg`, name: `${n}.jpg`, type: "image/jpeg", picked: false, width: 4032, height: 3024 });
const picked = (n: string): ShotFile => ({ ...camera(n), picked: true });

function apiError(code: string, status: number, o: Partial<{ retryable: boolean; details: Record<string, unknown>; retryAfterS: number }> = {}): UploadRefusal {
  return { code, status, message: `${code} message`, retryable: o.retryable ?? false, details: o.details, retryAfterS: o.retryAfterS };
}

const scan = { id: "scan_1", garden_id: "garden_1", plant_id: "plant_1" };

/** Takes a shot in a mode and walks it to stored, as the runner would. */
function store(draft: CaptureDraft, kind: Slot["kind"], n: string): CaptureDraft {
  const next = takeShot(draft, kind, camera(n), `key-${n}`);
  const slot = next.slots.find((s) => s.file?.name === `${n}.jpg` && s.state === "captured")!;
  return applySlot(applySlot(next, slot.id, { type: "upload" }), slot.id, { type: "stored", imageId: `img-${n}` });
}

describe("the slot machine (flows 14)", () => {
  const empty = emptySlot("whole_plant", 0);
  const captured = slotReducer(empty, { type: "capture", file: camera("a"), key: "k1" });
  const uploading = slotReducer(captured, { type: "upload" });

  it("goes empty → captured → uploading → stored", () => {
    expect(captured).toMatchObject({ state: "captured", key: "k1", file: { name: "a.jpg" } });
    expect(uploading.state).toBe("uploading");
    expect(slotReducer(uploading, { type: "stored", imageId: "img1" })).toMatchObject({ state: "stored", imageId: "img1" });
  });

  it("keeps the key on a failure, so the retry is the same action, and drops it on a rejection", () => {
    const failed = slotReducer(uploading, { type: "failed", message: "offline" });
    expect(failed).toMatchObject({ state: "failed", key: "k1", file: { name: "a.jpg" }, problem: { message: "offline" } });
    const retried = slotReducer(failed, { type: "upload" });
    expect(retried).toMatchObject({ state: "uploading", key: "k1" });
    expect(retried.problem).toBeUndefined();

    const rejected = slotReducer(uploading, { type: "rejected", message: "Too blurry", hint: "tap_to_focus" });
    expect(rejected).toMatchObject({ state: "rejected", problem: { hint: "tap_to_focus" } });
    expect(rejected.key).toBeUndefined();
    // A retake is a new action with a new key.
    const retaken = slotReducer(rejected, { type: "capture", file: camera("b"), key: "k2" });
    expect(retaken).toMatchObject({ state: "captured", key: "k2" });
    expect(retaken.problem).toBeUndefined();
  });

  it("sends a re-encoded photo as a new action and empties on any other refusal", () => {
    expect(slotReducer(uploading, { type: "reencoded", file: { ...picked("a2"), reencoded: true }, key: "k3" })).toMatchObject({
      state: "captured",
      key: "k3",
      file: { reencoded: true },
    });
    const refused = slotReducer(uploading, { type: "refused", message: "No" });
    expect(refused).toMatchObject({ state: "empty", problem: { message: "No" } });
    expect(refused.file).toBeUndefined();
  });

  it("replaces a stored shot by deleting it first, then uploads the next one", () => {
    const stored = slotReducer(uploading, { type: "stored", imageId: "img1" });
    expect(slotReducer(stored, { type: "capture", file: camera("b"), key: "k2" })).toBe(stored);
    const deleting = slotReducer(stored, { type: "replace", next: { file: camera("b"), key: "k2" } });
    expect(deleting).toMatchObject({ state: "deleting", imageId: "img1" });
    const next = slotReducer(deleting, { type: "deleted" });
    expect(next).toMatchObject({ state: "captured", key: "k2", file: { name: "b.jpg" } });
    expect(next.imageId).toBeUndefined();
    const removed = slotReducer(slotReducer(stored, { type: "replace" }), { type: "deleted" });
    expect(removed.state).toBe("empty");
    expect(removed.file).toBeUndefined();
  });

  it("ignores events a state doesn't take", () => {
    expect(slotReducer(empty, { type: "stored", imageId: "x" })).toBe(empty);
    expect(slotReducer(empty, { type: "deleted" })).toBe(empty);
    expect(slotReducer(captured, { type: "rejected", message: "x" })).toBe(captured);
  });
});

describe("the draft", () => {
  it("has a slot per allowed shot: two close-ups, one of every other kind", () => {
    const d = createDraft(scan);
    expect(d.slots.map((s) => s.id)).toEqual([
      "whole_plant:0",
      "leaf_closeup:0",
      "leaf_closeup:1",
      "leaf_underside:0",
      "soil_surface:0",
      "flower_fruit:0",
    ]);
    expect(MODES.map((m) => m.label)).toEqual(["Whole plant", "Close-up", "Underside", "Soil", "Fruit"]);
  });

  it("starts from the shots already on the server, leaving out photos that answered a question", () => {
    const d = createDraft({
      ...scan,
      images: [
        { id: "img_w", kind: "whole_plant" },
        { id: "img_c", kind: "leaf_closeup" },
        { id: "img_q", kind: "leaf_closeup", answers_question_id: "q_1" },
      ],
    });
    expect(d.slots.filter((s) => s.state === "stored").map((s) => [s.id, s.imageId])).toEqual([
      ["whole_plant:0", "img_w"],
      ["leaf_closeup:0", "img_c"],
    ]);
    expect(canAnalyse(d)).toBe(true);
  });

  it("enables Analyse once the whole plant is stored and nothing is on its way", () => {
    let d = createDraft(scan);
    expect(canAnalyse(d)).toBe(false);
    d = store(d, "whole_plant", "w");
    expect(canAnalyse(d)).toBe(true);
    d = takeShot(d, "leaf_closeup", camera("c"), "key-c");
    expect(canAnalyse(d)).toBe(false);
  });

  it("fills the dial with the whole plant and a close-up, and marks each mode", () => {
    let d = store(createDraft(scan), "whole_plant", "w");
    expect(coverage(d)).toEqual([
      { kind: "whole_plant", stored: true },
      { kind: "leaf_closeup", stored: false },
    ]);
    d = takeShot(d, "leaf_closeup", camera("c"), "key-c");
    expect([modeMark(d, "whole_plant"), modeMark(d, "leaf_closeup"), modeMark(d, "soil_surface")]).toEqual(["stored", "busy", "none"]);
  });

  it("aims the shutter at a free slot, then retakes the last stored shot of a full mode", () => {
    let d = store(createDraft(scan), "leaf_closeup", "c1");
    expect(shutterTarget(d, "leaf_closeup")).toEqual({ slotId: "leaf_closeup:1", replace: false });
    d = store(d, "leaf_closeup", "c2");
    expect(shutterTarget(d, "leaf_closeup")).toEqual({ slotId: "leaf_closeup:1", replace: true });
    d = takeShot(d, "leaf_closeup", camera("c3"), "key-c3");
    expect(d.slots.find((s) => s.id === "leaf_closeup:1")).toMatchObject({ state: "deleting", next: { key: "key-c3" } });
    // While it is busy, the shutter has nothing to aim at.
    expect(shutterTarget(d, "leaf_closeup")).toBeUndefined();
    expect(takeShot(d, "leaf_closeup", camera("c4"), "key-c4")).toBe(d);
  });

  it("lists the work to send and resumes an interrupted upload under its key", () => {
    let d = takeShot(createDraft(scan), "whole_plant", camera("w"), "key-w");
    d = applySlot(d, "whole_plant:0", { type: "upload" });
    expect(pendingWork(d).uploads).toEqual([]);
    const resumed = resumeDraft(d);
    expect(pendingWork(resumed).uploads.map((s) => [s.id, s.key])).toEqual([["whole_plant:0", "key-w"]]);
  });
});

describe("uploadOutcome", () => {
  it("turns image_quality into a retake with its hint, though the server marks it retryable", () => {
    const e = apiError("image_quality", 422, { retryable: true, details: { problem: "too_blurry", hint: "tap_to_focus" } });
    expect(uploadOutcome(e, camera("a"))).toEqual({ type: "rejected", message: "image_quality message", hint: "tap_to_focus" });
  });

  it("re-encodes a picked photo once after 413 or 415, never a camera shot", () => {
    expect(uploadOutcome(apiError("upload_too_large", 413), picked("a"))).toEqual({ type: "reencode" });
    expect(uploadOutcome(apiError("unsupported_media_type", 415), { ...picked("a"), reencoded: true }).type).toBe("refused");
    expect(uploadOutcome(apiError("upload_too_large", 413), camera("a")).type).toBe("refused");
  });

  it("retries the network, 5xx and request_in_progress with the server's wait", () => {
    expect(uploadOutcome(undefined, camera("a")).type).toBe("retry");
    expect(uploadOutcome(apiError("processing_unavailable", 503, { retryable: true, retryAfterS: 2 }), camera("a"))).toMatchObject({ type: "retry", afterMs: 2000 });
    expect(uploadOutcome(apiError("request_in_progress", 409, { retryAfterS: 2 }), camera("a")).type).toBe("retry");
    expect(uploadOutcome(apiError("too_many_images", 422), camera("a")).type).toBe("refused");
  });
});

describe("distanceHint", () => {
  it("words the distance against the kind's range", () => {
    expect(distanceHint("leaf_closeup", 0.32)).toEqual({ fit: "good", text: "32 cm · good distance" });
    expect(distanceHint("leaf_closeup", 0.9).text).toBe("90 cm · move closer");
    expect(distanceHint("whole_plant", 0.3).text).toBe("30 cm · move back");
    expect(distanceHint("whole_plant", 1.24).text).toBe("1.2 m · good distance");
  });
});
