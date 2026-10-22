import type { Question } from "@/domain/types";

import { GUIDED, initialOneShot, jarTaskDue, jarTaskOf, oneShotReducer, oneShotTarget, type OneShotState } from "./oneShot";
import type { ShotFile } from "./slot";

const file: ShotFile = { uri: "file:///cache/a.jpg", name: "a.jpg", type: "image/jpeg", picked: false, width: 4032, height: 3024 };

const question = (over: Partial<Question> = {}): Question => ({
  id: "q1",
  run: 1,
  kind: "photo_request",
  text: "Are there spots on the underside?",
  photo_kind: "leaf_underside",
  photo_hint: "Turn the leaf over and photograph the underside",
  candidates: ["early_blight", "septoria_leaf_spot"],
  asked_at: "2026-10-21T10:00:00Z",
  continue_at: "2026-10-21T10:10:00Z",
  status: "open",
  ...over,
});

describe("oneShotTarget", () => {
  it("takes the question's kind and hint for an open photo request", () => {
    const t = oneShotTarget({ status: "awaiting_answer", open_question: question() }, { type: "question", questionId: "q1" });
    expect(t).toEqual({
      ok: true,
      kind: "leaf_underside",
      title: "Are there spots on the underside?",
      instruction: "Turn the leaf over and photograph the underside",
      questionId: "q1",
    });
  });

  it("refuses a question that is gone, another one, not a photo request, or already answering", () => {
    const purpose = { type: "question", questionId: "q1" } as const;
    expect(oneShotTarget({ status: "planned" }, purpose)).toMatchObject({ ok: false, message: "This question has been answered." });
    expect(oneShotTarget({ status: "planned", open_question: question({ id: "q2" }) }, purpose).ok).toBe(false);
    expect(oneShotTarget({ status: "planned", open_question: question({ kind: "indicator", photo_kind: undefined }) }, purpose).ok).toBe(false);
    expect(oneShotTarget({ status: "planned", open_question: question({ status: "answering" }) }, purpose)).toMatchObject({
      ok: false,
      message: "A photo for this question is already being checked.",
    });
  });

  it("takes a guided check in any status but failed", () => {
    for (const status of ["pending", "analysing", "planned"] as const) {
      expect(oneShotTarget({ status }, { type: "guided", kind: "jar_test" })).toMatchObject({ ok: true, kind: "jar_test", guided: GUIDED.jar_test });
    }
    expect(oneShotTarget({ status: "failed" }, { type: "guided", kind: "squeeze_test" }).ok).toBe(false);
  });
});

describe("oneShotReducer", () => {
  const run = (state: OneShotState, ...events: Parameters<typeof oneShotReducer>[1][]) => events.reduce(oneShotReducer, state);

  it("starts guided mode with its steps and question mode with the camera", () => {
    expect(initialOneShot({ type: "guided", kind: "squeeze_test" })).toEqual({ step: "intro" });
    expect(initialOneShot({ type: "question", questionId: "q1" })).toEqual({ step: "aim" });
    expect(run({ step: "intro" }, { type: "take", file, key: "k" })).toEqual({ step: "intro" });
    expect(run({ step: "intro" }, { type: "start" })).toEqual({ step: "aim" });
  });

  it("holds the photo for Send, and a retake goes back to the camera", () => {
    const taken = run({ step: "aim" }, { type: "take", file, key: "k1" });
    expect(taken).toEqual({ step: "review", file, key: "k1" });
    expect(run(taken, { type: "retake" })).toEqual({ step: "aim" });
    expect(run(taken, { type: "send" }, { type: "stored", imageId: "img1" })).toEqual({ step: "sent", imageId: "img1" });
  });

  it("keeps the photo and its key after a failed send, so Try again is the same action", () => {
    const failed = run({ step: "review", file, key: "k1" }, { type: "send" }, { type: "failed", message: "Offline" });
    expect(failed).toEqual({ step: "review", file, key: "k1", problem: { message: "Offline", retry: true } });
    expect(run(failed, { type: "send" })).toEqual({ step: "sending", file, key: "k1" });
  });

  it("drops a refused photo and asks for another", () => {
    const refused = run({ step: "review", file, key: "k1" }, { type: "send" }, { type: "rejected", message: "Too blurry", hint: "tap_to_focus" });
    expect(refused).toEqual({ step: "aim", problem: { message: "Too blurry", hint: "tap_to_focus" } });
  });

  it("sends a re-encoded photo under its new key", () => {
    const jpeg = { ...file, uri: "file:///cache/a-2.jpg", reencoded: true };
    expect(run({ step: "sending", file, key: "k1" }, { type: "reencoded", file: jpeg, key: "k2" })).toEqual({ step: "sending", file: jpeg, key: "k2" });
  });
});

describe("the jar test's task", () => {
  it("is due on the garden's date once the jar has settled for 6 hours", () => {
    // 20:00 in Toronto on Oct 21 is Oct 22 at 00:00 UTC; 6 hours later it is Oct 22 in Toronto.
    expect(jarTaskDue("America/Toronto", new Date("2026-10-22T00:00:00Z"))).toBe("2026-10-22");
    expect(jarTaskDue("America/Toronto", new Date("2026-10-21T13:00:00Z"))).toBe("2026-10-21");
  });

  it("is the scan's open task with its scan link", () => {
    const tasks = [
      { id: "t1", title: "Water", scan_id: undefined },
      { id: "t2", title: "Photograph the jar test", scan_id: "s1", done_at: "2026-10-21T10:00:00Z" },
      { id: "t3", title: "Photograph the jar test", scan_id: "s1" },
    ];
    expect(jarTaskOf(tasks, "s1")?.id).toBe("t3");
    expect(jarTaskOf(tasks, "s2")).toBeUndefined();
  });
});
