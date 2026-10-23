// The Tasks tab's lists (roadmap 10 item 9; screens 3.7): tasks grouped by due date in the garden's calendar, and
// which swipe actions each task's source allows (contracts 9: Done for every task, Dismiss for plan and alert tasks,
// Delete and edit only for user and chat tasks). Pure: tasks and today in, sections out.
import { addLocalDays, formatLocalDate } from "@/domain/format";
import type { LocalDate, Task, TaskSource } from "@/domain/types";

export type TaskView = "open" | "done" | "alerts";

export interface TaskSection {
  key: string;
  title: string;
  tasks: Task[];
}

const WEEKDAY = new Intl.DateTimeFormat("en-US", { weekday: "short" });

function weekdayOf(local: LocalDate): string {
  const [y = 1970, m = 1, d = 1] = local.split("-").map(Number);
  return WEEKDAY.format(new Date(y, m - 1, d));
}

function byDue(a: Task, b: Task): number {
  if (a.due && b.due && a.due !== b.due) return a.due < b.due ? -1 : 1;
  return a.created_at.localeCompare(b.created_at);
}

/** Which tasks a view shows: open ones, done ones (newest first), or the alert inbox (open and dismissed alerts). */
export function tasksIn(view: TaskView, tasks: readonly Task[]): Task[] {
  switch (view) {
    case "open":
      return tasks.filter((t) => !t.done_at && !t.dismissed_at);
    case "done":
      return tasks.filter((t) => t.done_at).sort((a, b) => (b.done_at ?? "").localeCompare(a.done_at ?? ""));
    case "alerts":
      return tasks.filter((t) => t.source === "forecast_alert" && !t.done_at);
  }
}

/** Overdue, Today, Tomorrow, then a section per later date, then No date (screens 3.7). */
export function groupByDue(tasks: readonly Task[], today: LocalDate): TaskSection[] {
  const tomorrow = addLocalDays(today, 1);
  const sections = new Map<string, TaskSection>();
  const add = (key: string, title: string, t: Task) => {
    const s = sections.get(key) ?? { key, title, tasks: [] };
    s.tasks.push(t);
    sections.set(key, s);
  };
  for (const t of [...tasks].sort(byDue)) {
    if (!t.due) add("none", "No date", t);
    else if (t.due < today) add("overdue", "Overdue", t);
    else if (t.due === today) add("today", "Today", t);
    else if (t.due === tomorrow) add("tomorrow", "Tomorrow", t);
    else add(t.due, `${weekdayOf(t.due)}, ${formatLocalDate(t.due)}`, t);
  }
  const order = (s: TaskSection) => (s.key === "overdue" ? "0" : s.key === "today" ? "1" : s.key === "tomorrow" ? "2" : s.key === "none" ? "9" : `3${s.key}`);
  return [...sections.values()].sort((a, b) => order(a).localeCompare(order(b)));
}

export interface TaskActions {
  done: boolean;
  dismiss: boolean;
  delete: boolean;
  edit: boolean;
}

const DISMISSABLE: readonly TaskSource[] = ["plan", "forecast_alert"];
const OWN: readonly TaskSource[] = ["user", "chat"];

/** The swipe actions a task allows, for an editor or owner; a viewer gets none (screens 3.7). */
export function actionsFor(task: Pick<Task, "source" | "done_at" | "dismissed_at">, canWrite: boolean): TaskActions {
  const open = !task.done_at && !task.dismissed_at;
  return {
    done: canWrite && open,
    dismiss: canWrite && open && DISMISSABLE.includes(task.source),
    delete: canWrite && OWN.includes(task.source),
    edit: canWrite && open && OWN.includes(task.source),
  };
}

const SOURCE_WORD: Record<TaskSource, string> = { plan: "Plan", forecast_alert: "Alert", chat: "Chat", user: "" };

/** The source chip's word; none for the user's own tasks. */
export function sourceWord(source: TaskSource): string | undefined {
  return SOURCE_WORD[source] || undefined;
}
