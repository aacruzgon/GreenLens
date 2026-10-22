import type { Task } from "@/domain/types";

import { actionsFor, groupByDue, tasksIn } from "./groups";

let n = 0;
function task(over: Partial<Task> = {}): Task {
  n += 1;
  return { id: `t${n}`, garden_id: "g", source: "user", title: `Task ${n}`, created_at: `2026-10-0${n % 9}T00:00:00Z`, ...over };
}

const TODAY = "2026-10-22";

describe("the Tasks tab's groups", () => {
  it("groups by due date in order: overdue, today, tomorrow, later dates, no date", () => {
    const tasks = [
      task({ title: "none" }),
      task({ title: "later", due: "2026-10-26" }),
      task({ title: "today", due: TODAY }),
      task({ title: "late", due: "2026-10-18" }),
      task({ title: "tomorrow", due: "2026-10-23" }),
    ];
    const groups = groupByDue(tasks, TODAY);
    expect(groups.map((g) => [g.title, g.tasks.map((t) => t.title)])).toEqual([
      ["Overdue", ["late"]],
      ["Today", ["today"]],
      ["Tomorrow", ["tomorrow"]],
      ["Mon, Oct 26", ["later"]],
      ["No date", ["none"]],
    ]);
  });

  it("shows open, done or the alert inbox", () => {
    const open = task();
    const done = task({ done_at: "2026-10-21T10:00:00Z" });
    const alert = task({ source: "forecast_alert" });
    const dismissedAlert = task({ source: "forecast_alert", dismissed_at: "2026-10-21T10:00:00Z" });
    expect(tasksIn("open", [open, done, alert, dismissedAlert])).toEqual([open, alert]);
    expect(tasksIn("done", [open, done])).toEqual([done]);
    expect(tasksIn("alerts", [open, alert, dismissedAlert])).toEqual([alert, dismissedAlert]);
  });

  it("allows Done for every task, Dismiss for plan and alert tasks, Delete and edit for user and chat tasks", () => {
    expect(actionsFor({ source: "plan" }, true)).toEqual({ done: true, dismiss: true, delete: false, edit: false });
    expect(actionsFor({ source: "forecast_alert" }, true)).toEqual({ done: true, dismiss: true, delete: false, edit: false });
    expect(actionsFor({ source: "user" }, true)).toEqual({ done: true, dismiss: false, delete: true, edit: true });
    expect(actionsFor({ source: "chat" }, true)).toEqual({ done: true, dismiss: false, delete: true, edit: true });
    expect(actionsFor({ source: "user" }, false)).toEqual({ done: false, dismiss: false, delete: false, edit: false });
  });
});
