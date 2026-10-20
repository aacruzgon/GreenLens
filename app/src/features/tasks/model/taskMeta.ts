// The small line under a task's title (styles.css .task .t-fn): its plant, how often it repeats, and
// when it was due if that has passed. Pure, so the Tasks tab and the garden home word it the same way.
import { formatLocalDate } from "@/domain/format";
import type { LocalDate, Task } from "@/domain/types";

export function repeatWord(everyDays: number): string {
  if (everyDays === 1) return "daily";
  if (everyDays === 7) return "weekly";
  if (everyDays === 14) return "every 2 weeks";
  return `every ${everyDays} days`;
}

/** "Tomato 2 · weekly", "Tomato 1 · every 3 days · due Oct 16". */
export function taskMeta(
  task: Pick<Task, "plant_id" | "repeat" | "due">,
  plantName: (id: string) => string | undefined,
  today: LocalDate,
): string | undefined {
  const parts: string[] = [];
  const name = task.plant_id ? plantName(task.plant_id) : undefined;
  if (name) parts.push(name);
  if (task.repeat) parts.push(repeatWord(task.repeat.every_days));
  if (task.due && task.due < today) parts.push(`due ${formatLocalDate(task.due)}`);
  return parts.length > 0 ? parts.join(" · ") : undefined;
}
