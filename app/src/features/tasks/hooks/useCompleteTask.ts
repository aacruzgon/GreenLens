import { useState } from "react";

import { isApiError, useUpdateTask } from "@/api";
import type { Task } from "@/domain/types";
import { haptics } from "@/platform";

// Completes a task from a check box or an alert's Done. The row shows done at once with a success haptic
// (design 6: success on a checked task); a failed PATCH turns it back, warns, and leaves `failed` for the screen
// to say so until the next try. The tasks checked here are kept while the screen lives, so a checked row keeps
// its check after the open list refetches without it.
export function useCompleteTask() {
  const update = useUpdateTask();
  const [done, setDone] = useState<ReadonlyMap<string, Task>>(() => new Map());
  const [failed, setFailed] = useState<{ task: Task; message?: string }>();

  function complete(task: Task) {
    if (done.has(task.id)) return;
    setFailed(undefined);
    setDone((prev) => new Map(prev).set(task.id, task));
    void haptics.success();
    // mutateAsync, not mutate's onError: TanStack runs a mutate callback only for the latest call, so a quick
    // second check would leave the first row checked when its PATCH fails.
    update.mutateAsync({ id: task.id, body: { done: true } }).catch((error: unknown) => {
      setFailed({ task, message: isApiError(error) ? error.message : undefined });
      setDone((prev) => {
        const next = new Map(prev);
        next.delete(task.id);
        return next;
      });
      void haptics.warning();
    });
  }

  return { complete, done, failed };
}
