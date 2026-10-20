import { repeatWord, taskMeta } from "./taskMeta";

const names: Record<string, string> = { plant_tomato_2: "Tomato 2" };
const nameOf = (id: string) => names[id];

describe("taskMeta", () => {
  it("names the plant and how often it repeats", () => {
    expect(taskMeta({ plant_id: "plant_tomato_2", repeat: { every_days: 7 }, due: "2026-10-19" }, nameOf, "2026-10-19")).toBe("Tomato 2 · weekly");
  });

  it("marks a plan's repeating check on a plant as monitoring", () => {
    expect(
      taskMeta({ source: "plan", plant_id: "plant_tomato_2", repeat: { every_days: 7 }, due: "2026-10-19" }, nameOf, "2026-10-19"),
    ).toBe("Tomato 2 · Monitoring · weekly");
  });

  it("adds when an overdue task was due", () => {
    expect(taskMeta({ repeat: { every_days: 3 }, due: "2026-10-16" }, nameOf, "2026-10-19")).toBe("every 3 days · due Oct 16");
  });

  it("has nothing to say about a plain task", () => {
    expect(taskMeta({ due: "2026-10-19" }, nameOf, "2026-10-19")).toBeUndefined();
  });

  it("words the repeats", () => {
    expect([1, 7, 14, 3].map(repeatWord)).toEqual(["daily", "weekly", "every 2 weeks", "every 3 days"]);
  });
});
