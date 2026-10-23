// Where a task leads (roadmap F1 item 5; flows 8), shared by the garden home and the Tasks tab.
import type { Task } from "./types";

/** A plan's repeating task on a plant is a monitoring check: it leads to a scan of that plant (roadmap F1 item 5). */
export function scanTargetOf(task: Pick<Task, "source" | "plant_id" | "repeat">): string | undefined {
  return task.source === "plan" && task.repeat && task.plant_id ? task.plant_id : undefined;
}

/**
 * A jar-test task's scan, which opens that scan's jar capture (flows 8). Only a user's task carries a scan link on the
 * server, and only the jar test makes one (contracts 9); a plan's task opens a new scan instead (scanTargetOf).
 */
export function jarScanOf(task: Pick<Task, "source" | "scan_id">): string | undefined {
  return task.source === "user" ? task.scan_id : undefined;
}
