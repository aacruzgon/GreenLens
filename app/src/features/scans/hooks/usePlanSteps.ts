// A plan's steps joined with their tasks, for the diagnosis's checklist, its mini player and step by step
// (roadmap 10 items 5–7). Checking a step completes its task (screens 3.4): the row shows done at once, and the
// task lists refetch, so the step reads done on every screen that shows it.
import { useMemo } from "react";

import { useAllPlants, useAllTasks, useGarden } from "@/api";
import { localToday } from "@/domain/format";
import { can } from "@/domain/permissions";
import type { ScanDetail } from "@/domain/types";
import { useCompleteTask } from "@/features/tasks";

import { stepViews, upNext, type StepView } from "../model/plan";

export function usePlanSteps(detail: ScanDetail | undefined) {
  const gardenId = detail?.garden_id ?? "";
  const garden = useGarden(gardenId);
  const tasks = useAllTasks(gardenId);
  const plants = useAllPlants(gardenId);
  const { complete, done, failed } = useCompleteTask();

  const plan = detail?.plan;
  const canCheck = can(garden.data?.my_role, "task.write");
  const tz = garden.data?.time_zone;
  const steps = useMemo(() => {
    if (!plan) return [];
    const names = new Map((plants.data ?? []).map((p) => [p.id, p.name]));
    const today = localToday(tz ?? Intl.DateTimeFormat().resolvedOptions().timeZone);
    return stepViews(plan, tasks.data ?? [], (id) => names.get(id), today, new Set(done.keys()));
  }, [plan, plants.data, tasks.data, tz, done]);

  return {
    plan,
    steps,
    next: upNext(steps),
    /** Checks need the tasks: until they load, a step has nothing to complete. */
    tasksLoaded: tasks.isSuccess,
    canCheck,
    failed,
    check: (step: StepView) => {
      if (step.task && canCheck && !step.done) complete(step.task);
    },
  };
}
