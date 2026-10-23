// A new task, or an edit of a user or chat task (roadmap 10 item 9; screens 2, tasks/edit). The date and the repeat
// are a few plain choices rather than a calendar: today, tomorrow, in a week or none, and weekly or every two weeks.
// Plan and alert tasks are the server's: they are completed or dismissed, never edited (contracts 9).
import { useLocalSearchParams, useRouter } from "expo-router";
import { useState } from "react";
import { ActivityIndicator, View } from "react-native";

import { useActionKey, useAllPlants, useAllTasks, useCreateTask, useGarden, useUpdateTask } from "@/api";
import { addLocalDays, formatLocalDate, localToday } from "@/domain/format";
import { can } from "@/domain/permissions";
import type { CreateTaskRequest, LocalDate, Repeat, Task, UpdateTaskRequest } from "@/domain/types";
import { Button, Chip, Sheet, Text, TextField, space } from "@/ui";

import { actionsFor } from "../model/groups";

const MAX_TITLE = 200;
const MAX_DETAIL = 2000;

type DueChoice = "none" | "today" | "tomorrow" | "week" | "keep";
type RepeatChoice = "none" | 1 | 7 | 14;

const REPEATS: { value: RepeatChoice; label: string }[] = [
  { value: "none", label: "Once" },
  { value: 1, label: "Daily" },
  { value: 7, label: "Weekly" },
  { value: 14, label: "Every 2 weeks" },
];

export function TaskEditorSheet({ gardenId }: { gardenId: string }) {
  const { taskId } = useLocalSearchParams<{ taskId?: string }>();
  const garden = useGarden(gardenId);
  const tasks = useAllTasks(gardenId);
  const existing = taskId ? tasks.data?.find((t) => t.id === taskId) : undefined;
  const canWrite = can(garden.data?.my_role, "task.write");

  if (!garden.data || (taskId && !tasks.data)) {
    return (
      <Sheet title={taskId ? "Edit task" : "New task"}>
        <ActivityIndicator />
      </Sheet>
    );
  }
  if (taskId && !existing) {
    return (
      <Sheet title="Task">
        <Text variant="body" color="textMuted">
          {"We can't find this task. It may have been deleted."}
        </Text>
      </Sheet>
    );
  }
  if (!canWrite || (existing && !actionsFor(existing, canWrite).edit)) {
    return (
      <Sheet title="Task">
        <Text variant="body" color="textMuted">
          {canWrite ? "Tasks from a plan or an alert are done or dismissed, not edited." : "Only editors and owners change tasks."}
        </Text>
      </Sheet>
    );
  }
  return <TaskForm gardenId={gardenId} timeZone={garden.data.time_zone} existing={existing} />;
}

function TaskForm({ gardenId, timeZone, existing }: { gardenId: string; timeZone: string; existing?: Task }) {
  const router = useRouter();
  const plants = useAllPlants(gardenId);
  const create = useCreateTask();
  const update = useUpdateTask();
  const keyFor = useActionKey();
  const [title, setTitle] = useState(existing?.title ?? "");
  const [detail, setDetail] = useState(existing?.detail ?? "");
  const [plantId, setPlantId] = useState<string | undefined>(existing?.plant_id);
  const [due, setDue] = useState<DueChoice>(existing?.due ? "keep" : "none");
  const [repeat, setRepeat] = useState<RepeatChoice>((existing?.repeat?.every_days as RepeatChoice | undefined) ?? "none");
  const [problem, setProblem] = useState<string>();

  const today = localToday(timeZone);

  const dueDate = (): LocalDate | undefined => {
    switch (due) {
      case "today": return today;
      case "tomorrow": return addLocalDays(today, 1);
      case "week": return addLocalDays(today, 7);
      case "keep": return existing?.due;
      default: return undefined;
    }
  };
  const dues: { value: DueChoice; label: string }[] = [
    ...(existing?.due ? [{ value: "keep" as const, label: formatLocalDate(existing.due) }] : []),
    { value: "today", label: "Today" },
    { value: "tomorrow", label: "Tomorrow" },
    { value: "week", label: "In a week" },
    { value: "none", label: "No date" },
  ];

  const save = () => {
    const name = title.trim();
    if (!name) return;
    const date = dueDate();
    const rep: Repeat | undefined = repeat === "none" ? undefined : { every_days: repeat };
    setProblem(undefined);
    const done = () => router.back();
    const failed = () => setProblem("The task didn't save. Try again.");
    if (existing) {
      const body: UpdateTaskRequest = { title: name, detail: detail.trim() };
      if (date) body.due = date;
      if (rep) body.repeat = rep;
      update.mutateAsync({ id: existing.id, body }).then(done, failed);
      return;
    }
    const body: CreateTaskRequest = { garden_id: gardenId, title: name };
    if (detail.trim()) body.detail = detail.trim();
    if (date) body.due = date;
    if (rep) body.repeat = rep;
    if (plantId) body.plant_id = plantId;
    create.mutateAsync(body, keyFor(body)).then(done, failed);
  };

  return (
    <Sheet
      title={existing ? "Edit task" : "New task"}
      footer={<Button label="Save" disabled={!title.trim()} busy={create.isPending || update.isPending} onPress={save} testID="task-save" />}
    >
      <TextField label="Task" placeholder="Order mulch" value={title} onChangeText={setTitle} maxLength={MAX_TITLE} autoFocus={!existing} testID="task-title" />
      <View style={{ gap: space[2] }}>
        <Text variant="subheadline" color="textMuted" emphasis>
          When
        </Text>
        <View accessibilityRole="radiogroup" style={{ flexDirection: "row", flexWrap: "wrap", gap: space[2] }}>
          {dues.map((d) => (
            <Chip key={d.value} label={d.label} selected={due === d.value} onPress={() => setDue(d.value)} />
          ))}
        </View>
      </View>
      <View style={{ gap: space[2] }}>
        <Text variant="subheadline" color="textMuted" emphasis>
          Repeat
        </Text>
        <View accessibilityRole="radiogroup" style={{ flexDirection: "row", flexWrap: "wrap", gap: space[2] }}>
          {REPEATS.map((r) => (
            <Chip key={String(r.value)} label={r.label} selected={repeat === r.value} onPress={() => setRepeat(r.value)} />
          ))}
        </View>
      </View>
      {!existing && (plants.data?.length ?? 0) > 0 ? (
        <View style={{ gap: space[2] }}>
          <Text variant="subheadline" color="textMuted" emphasis>
            Plant
          </Text>
          <View accessibilityRole="radiogroup" style={{ flexDirection: "row", flexWrap: "wrap", gap: space[2] }}>
            <Chip label="None" selected={!plantId} onPress={() => setPlantId(undefined)} />
            {plants.data?.map((p) => (
              <Chip key={p.id} label={p.name} selected={plantId === p.id} onPress={() => setPlantId(p.id)} />
            ))}
          </View>
        </View>
      ) : null}
      <TextField label="Notes" placeholder="Optional" value={detail} onChangeText={setDetail} maxLength={MAX_DETAIL} multiline testID="task-detail" />
      {problem ? (
        <Text variant="footnote" color="danger" accessibilityRole="alert">
          {problem}
        </Text>
      ) : null}
    </Sheet>
  );
}
