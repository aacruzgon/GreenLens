import { useState } from "react";

import { useUpdateTask } from "@/api";
import type { Task } from "@/domain/types";
import { haptics } from "@/platform";

// Completes a task from a check box or an alert's Done. The row shows done at once with a success haptic
// (design 6: success on a checked task); a failed PATCH turns it back and warns. The tasks checked here are
// kept while the screen lives, so a checked row keeps its check after the open list refetches without it.
export function useCompleteTask() {
  const update = useUpdateTask();
  const [done, setDone] = useState<ReadonlyMap<string, Task>>(() => new Map());

  function complete(task: Task) {
    if (done.has(task.id)) return;
    setDone((prev) => new Map(prev).set(task.id, task));
    void haptics.success();
    update.mutate(
      { id: task.id, body: { done: true } },
      {
        onError: () => {
          setDone((prev) => {
            const next = new Map(prev);
            next.delete(task.id);
            return next;
          });
          void haptics.warning();
        },
      },
    );
  }

  return { complete, done, error: update.error };
}
