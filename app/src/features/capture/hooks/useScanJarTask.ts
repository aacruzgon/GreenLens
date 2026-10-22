import { useAllTasks } from "@/api";

import { jarTaskOf } from "../model/oneShot";

/**
 * A scan's open jar-test task, and whether the answer is final. `GET /tasks` has no scan filter, so this reads all of
 * the garden's open user tasks, the only ones with a scan link (contracts 9): a task beyond the first page still
 * counts. Until `settled`, "no task" isn't known yet, so Start waits for it (flows 8: Start never adds a second task
 * for a scan that has an open one).
 */
export function useScanJarTask(gardenId: string, scanId: string) {
  const tasks = useAllTasks(gardenId, { source: "user", state: "open" });
  return {
    task: tasks.data ? jarTaskOf(tasks.data, scanId) : undefined,
    settled: tasks.isSuccess,
    error: tasks.error,
    retry: () => void tasks.refetch(),
  };
}
