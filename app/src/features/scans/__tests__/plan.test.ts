import { CITATIONS, blightSteps, makePlan, tasksFromPlan } from "@/api/mock/fixtures";
import { DateKit } from "@/api/mock/util";
import type { PlanStep, Task } from "@/domain/types";

import { occurrences, stepViews, supersededNote, upNext, whenWord } from "../model/plan";

const k = new DateKit(new Date(2026, 9, 17, 12));
const TODAY = k.date(0);
const t2 = { id: "plant_tomato_2", name: "Tomato 2" };
const t1 = { id: "plant_tomato_1", name: "Tomato 1" };

function plan(steps: PlanStep[]) {
  return makePlan({ id: "plan1", scanId: "s1", plantId: t2.id, diagnosisId: "d1", ts: "2026-10-17T14:42:00Z", summary: "", steps, watchFor: [], citations: [CITATIONS.blight] });
}

let n = 0;
const names = (id: string) => ({ [t1.id]: t1.name, [t2.id]: t2.name })[id];

describe("plan steps", () => {
  it("words each kind of timing", () => {
    expect(whenWord({ kind: "now" })).toBe("Today");
    expect(whenWord({ kind: "by_date", due: "2026-10-22" })).toBe("By Oct 22");
    expect(whenWord({ kind: "recurring", every_days: 3, until: "2026-11-01" })).toBe("Every 3 days until Nov 1");
    expect(whenWord({ kind: "recurring", every_days: 7 })).toBe("Every week");
    expect(whenWord({ kind: "on_trigger", trigger: "after_rain" })).toBe("After the next rain");
  });

  it("names a repeating step's dates up to its end, or the next four", () => {
    expect(occurrences("2026-10-20", 3, "2026-11-01")).toEqual(["2026-10-20", "2026-10-23", "2026-10-26", "2026-10-29", "2026-11-01"]);
    expect(occurrences("2026-10-20", 7)).toHaveLength(4);
  });

  it("joins each step to its task, its source number and the other plant's name", () => {
    const p = plan(blightSteps(k, 0, t2, { crowded: true, ladybug: true, neighbour: t1 }));
    const tasks = tasksFromPlan(p, k, () => `task_${++n}`);
    const steps = stepViews(p, tasks, names, TODAY);
    expect(steps.map((s) => s.title)).toEqual([
      "Remove the infected leaves",
      "Mulch around the base",
      "Give Tomato 2 more room",
      "Leave the ladybug larva alone",
      "Check Tomato 2 for new spots",
      "Check Tomato 1's lower leaves",
    ]);
    expect(steps[0]).toMatchObject({ when: "Today", category: "Cultural", citations: [1], done: false });
    expect(steps[3]?.citations).toEqual([]);
    expect(steps[5]?.otherPlant).toBe("Tomato 1");
    expect(steps[4]?.dates).toEqual({ label: "Every 3 days, starting Oct 20", dates: ["Oct 20", "Oct 23", "Oct 26", "Oct 29"] });
    expect(stepViews(p, tasks, () => undefined, TODAY)[5]?.otherPlant).toBe("plant removed");
  });

  it("plays the first step still to do, and counts a checked one as done before the refetch", () => {
    const p = plan(blightSteps(k, 0, t2));
    const tasks = tasksFromPlan(p, k, () => `task_${++n}`);
    const first = tasks[0] as Task;
    expect(upNext(stepViews(p, tasks, names, TODAY))?.step.title).toBe("Remove the infected leaves");
    const after = stepViews(p, tasks, names, TODAY, new Set([first.id]));
    expect(after[0]?.done).toBe(true);
    expect(upNext(after)).toMatchObject({ index: 1, step: { title: "Mulch around the base" } });
  });

  it("shows a repeating step's next open task after its last one was done", () => {
    const p = plan(blightSteps(k, 0, t2).filter((s) => s.id === "check_plant"));
    const [first] = tasksFromPlan(p, k, () => "t_first");
    const done = { ...first!, done_at: "2026-10-20T09:00:00Z" };
    const next = { ...first!, id: "t_next", due: "2026-10-23" };
    const [step] = stepViews(p, [done, next], names, TODAY);
    expect(step).toMatchObject({ done: false, task: { id: "t_next" } });
  });

  it("leaves dismissed tasks out, and says why a plan was updated", () => {
    const p = plan(blightSteps(k, 0, t2).slice(0, 1));
    const [task] = tasksFromPlan(p, k, () => "t1");
    expect(stepViews(p, [{ ...task!, dismissed_at: "2026-10-18T00:00:00Z" }], names, TODAY)[0]?.task).toBeUndefined();
    expect(supersededNote({ supersedes: "plan0" }, "late_answer")).toBe("Updated after your answer");
    expect(supersededNote({}, "late_answer")).toBeUndefined();
  });
});
