// The Tasks tab's view model (roadmap 10 item 9; screens 3.7): the garden's tasks grouped by due date in its time
// zone, the Open, Done and Alerts views, and the swipe actions each task's source allows. Viewers read only.
import { useFocusEffect, useRouter } from "expo-router";
import { useCallback, useMemo, useState } from "react";

import { isApiError, useAllPlants, useAllTasks, useDeleteTask, useGarden, useUpdateTask } from "@/api";
import { localToday } from "@/domain/format";
import { can } from "@/domain/permissions";
import { jarScanOf, scanTargetOf } from "@/domain/tasks";
import type { Task } from "@/domain/types";
import { haptics } from "@/platform";

import { actionsFor, groupByDue, sourceWord, tasksIn, type TaskView } from "../model/groups";
import { taskMeta } from "../model/taskMeta";
import { useCompleteTask } from "./useCompleteTask";

export interface TaskRowModel {
  task: Task;
  title: string;
  meta?: string;
  source?: string;
  done: boolean;
  dismissed: boolean;
  actions: ReturnType<typeof actionsFor>;
  /** What a tap opens, for VoiceOver; undefined when a tap does nothing. */
  pressHint?: string;
}

export function useTasksTab(gardenId: string) {
  const router = useRouter();
  const garden = useGarden(gardenId);
  const tasks = useAllTasks(gardenId);
  const plants = useAllPlants(gardenId);
  const { complete, done, failed, clear } = useCompleteTask();
  const update = useUpdateTask();
  const remove = useDeleteTask();
  const [view, setView] = useState<TaskView>("open");
  const [problem, setProblem] = useState<string>();
  // The tab stays mounted, so the tasks checked in Open leave it when the tab loses focus or the view changes.
  useFocusEffect(useCallback(() => clear, [clear]));

  const role = garden.data?.my_role;
  const canWrite = can(role, "task.write");
  const canScan = can(role, "scan.create");
  const tz = garden.data?.time_zone;
  const today = tz ? localToday(tz) : undefined;

  const sections = useMemo(() => {
    if (!tasks.data || !today) return undefined;
    const names = new Map((plants.data ?? []).map((p) => [p.id, p.name]));
    // A task checked here stays in Open, struck through, until the tab loses focus or the view changes.
    const shown = view === "open" ? tasks.data.filter((t) => done.has(t.id) || (!t.done_at && !t.dismissed_at)) : tasksIn(view, tasks.data);
    const groups = view === "done" ? [{ key: "done", title: "Done", tasks: shown }] : groupByDue(shown, today);
    return groups.map((g) => ({
      ...g,
      rows: g.tasks.map((t): TaskRowModel => {
        const checked = Boolean(t.done_at) || done.has(t.id);
        const opens = canScan && (jarScanOf(t) || scanTargetOf(t)) ? "Starts a scan" : t.plant_id ? `Opens ${names.get(t.plant_id) ?? "the plant"}` : undefined;
        return {
          task: t,
          title: t.title,
          meta: taskMeta(t, (id) => names.get(id), today),
          source: sourceWord(t.source),
          done: checked,
          dismissed: Boolean(t.dismissed_at),
          actions: actionsFor({ ...t, done_at: checked ? (t.done_at ?? "now") : undefined }, canWrite),
          pressHint: actionsFor(t, canWrite).edit ? "Edits the task" : opens,
        };
      }),
    }));
  }, [tasks.data, plants.data, today, view, done, canWrite, canScan]);

  const fail = (error: unknown, what: string) => {
    setProblem(isApiError(error) ? error.message : `${what} didn't save. Try again.`);
    void haptics.warning();
  };

  // The garden gives the dates their time zone, so the list can't show without it.
  const loadError = tasks.error ?? garden.error;
  return {
    loading: !sections && !loadError,
    error: loadError ? (isApiError(loadError) ? loadError.message : "Your tasks didn't load.") : undefined,
    refreshing: tasks.isRefetching,
    view,
    sections: sections ?? [],
    canWrite,
    problem: problem ?? (failed ? (failed.message ?? "That task didn't save. Try again.") : undefined),
    actions: {
      setView: (next: TaskView) => {
        clear();
        setView(next);
      },
      refresh: () => void Promise.all([tasks.refetch(), plants.refetch()]),
      retry: () => void Promise.all([tasks.refetch(), garden.refetch()]),
      done: (t: Task) => {
        setProblem(undefined);
        if (canWrite) complete(t);
      },
      dismiss: (t: Task) => {
        setProblem(undefined);
        update.mutateAsync({ id: t.id, body: { dismissed: true } }).catch((e: unknown) => fail(e, "Dismissing"));
      },
      delete: (t: Task) => {
        setProblem(undefined);
        remove.mutateAsync(t).catch((e: unknown) => fail(e, "Deleting"));
      },
      add: () => router.push("/tasks/edit"),
      open: (t: Task) => {
        if (actionsFor(t, canWrite).edit) {
          router.push({ pathname: "/tasks/edit", params: { taskId: t.id } });
          return;
        }
        const jar = canScan ? jarScanOf(t) : undefined;
        if (jar) {
          router.push({ pathname: "/capture/[scanId]/guided/[kind]", params: { scanId: jar, kind: "jar_test" } });
          return;
        }
        const scanPlant = canScan ? scanTargetOf(t) : undefined;
        if (scanPlant) {
          router.push({ pathname: "/scan/new", params: { plantId: scanPlant } });
          return;
        }
        if (t.plant_id) router.push({ pathname: "/plants/[plantId]", params: { plantId: t.plant_id } });
      },
    },
  };
}

export type TasksTabView = ReturnType<typeof useTasksTab>;
