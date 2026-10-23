// The plan as the app shows it (roadmap 10 items 5–7; screens 3.4): each step with its timing, category,
// citations and the task it created, the step the mini player plays next, and step-by-step's time phrase as
// named dates. Pure: steps and tasks in, plain rows out.
import { addLocalDays, formatLocalDate } from "@/domain/format";
import type { ActionPlan, FarmCitation, LocalDate, PlanStep, PlanWhen, StepCategory, SupersedeReason, Task, TriggerKind } from "@/domain/types";
import { assertNever } from "@/lib/assertNever";

const CATEGORY_WORD: Record<StepCategory, string> = {
  cultural: "Cultural",
  biological: "Biological",
  mechanical: "Mechanical",
  chemical: "Chemical",
  monitoring: "Monitoring",
  wildlife_deterrent: "Wildlife",
};

const TRIGGER_WORD: Record<TriggerKind, string> = {
  frost: "Before the next frost",
  heat: "Before the next heat wave",
  humid_stretch: "When a humid stretch starts",
  heavy_rain: "Before heavy rain",
  after_rain: "After the next rain",
  dry_soil: "When the soil dries out",
};

/** What makes an `on_trigger` step's task appear (design 5, step by step). */
const TRIGGER_NOTE: Record<TriggerKind, string> = {
  frost: "A task appears when the forecast shows frost.",
  heat: "A task appears when the forecast shows a heat wave.",
  humid_stretch: "A task appears when the forecast turns humid.",
  heavy_rain: "A task appears when the forecast shows heavy rain.",
  after_rain: "A task appears when the forecast shows rain.",
  dry_soil: "A task appears when the soil is dry.",
};

export function categoryWord(c: StepCategory): string {
  return CATEGORY_WORD[c];
}

function repeatPhrase(everyDays: number): string {
  if (everyDays === 1) return "Every day";
  if (everyDays === 7) return "Every week";
  return `Every ${everyDays} days`;
}

/** "Today", "By Oct 22", "Every 3 days until Nov 1", "After the next rain" (screens 3.4). */
export function whenWord(when: PlanWhen): string {
  switch (when.kind) {
    case "now":
      return "Today";
    case "by_date":
      return `By ${formatLocalDate(when.due)}`;
    case "recurring":
      return when.until ? `${repeatPhrase(when.every_days)} until ${formatLocalDate(when.until)}` : repeatPhrase(when.every_days);
    case "on_trigger":
      return TRIGGER_WORD[when.trigger];
    default:
      return assertNever(when);
  }
}

/** The dates a repeating step's tasks fall on, from its first due date: all of them up to `until`, else the next 4. */
export function occurrences(first: LocalDate, everyDays: number, until?: LocalDate, cap = 12): LocalDate[] {
  const out: LocalDate[] = [];
  const limit = until ? cap : 4;
  for (let d = first; out.length < limit && (!until || d <= until); d = addLocalDays(d, everyDays)) out.push(d);
  return out;
}

export interface StepView {
  id: string;
  order: number;
  title: string;
  detail: string;
  when: string;
  category: string;
  isChemical: boolean;
  /** The server's safety note, shown as sent (screens 6). */
  safetyNote?: string;
  /** 1-based numbers into the plan's sources. */
  citations: number[];
  farmRecords: FarmCitation[];
  /** A step for another plant ("Check Tomato 1's lower leaves"): that plant's name, or "plant removed". */
  otherPlant?: string;
  /** The step's current task: the open one, else the latest done one. None for an `on_trigger` step until it fires. */
  task?: Task;
  done: boolean;
  /** Step by step's time phrase: the named dates a repeating step adds, or what makes a trigger's task appear. */
  dates?: { label: string; dates: string[] };
  triggerNote?: string;
}

/** The step's task: an open one first (a repeating step's next date), else the most recently done. */
function taskFor(step: PlanStep, plan: ActionPlan, tasks: readonly Task[]): Task | undefined {
  const mine = tasks.filter((t) => t.plan_id === plan.id && t.plan_step_id === step.id && !t.dismissed_at);
  const open = mine.filter((t) => !t.done_at).sort((a, b) => (a.due ?? "").localeCompare(b.due ?? ""));
  if (open[0]) return open[0];
  return mine.sort((a, b) => (b.done_at ?? "").localeCompare(a.done_at ?? ""))[0];
}

function timePhrase(step: PlanStep, task: Task | undefined, today: LocalDate): StepView["dates"] {
  if (step.when.kind !== "recurring") return undefined;
  const first = task?.due ?? addLocalDays(today, step.when.every_days);
  const dates = occurrences(first, step.when.every_days, step.when.until);
  const label = step.when.until
    ? `Adds ${dates.length} ${dates.length === 1 ? "task" : "tasks"}`
    : `${repeatPhrase(step.when.every_days)}, starting ${formatLocalDate(first)}`;
  return { label, dates: dates.map(formatLocalDate) };
}

export function stepViews(
  plan: ActionPlan,
  tasks: readonly Task[],
  plantName: (id: string) => string | undefined,
  today: LocalDate,
  /** Tasks checked on this screen and not refetched yet. */
  checked: ReadonlySet<string> = new Set(),
): StepView[] {
  const sourceIndex = new Map(plan.citations.map((c, i) => [c.source_id, i + 1]));
  const farm = new Map((plan.farm_records ?? []).map((r) => [r.record_id, r]));
  return [...plan.steps]
    .sort((a, b) => a.order - b.order)
    .map((step) => {
      const task = taskFor(step, plan, tasks);
      const other = step.plant_id && step.plant_id !== plan.plant_id ? (plantName(step.plant_id) ?? "plant removed") : undefined;
      return {
        id: step.id,
        order: step.order,
        title: step.title,
        detail: step.detail,
        when: whenWord(step.when),
        category: categoryWord(step.category),
        isChemical: step.category === "chemical",
        safetyNote: step.safety_note,
        citations: step.citations.flatMap((id) => sourceIndex.get(id) ?? []),
        farmRecords: (step.farm_citations ?? []).flatMap((id) => farm.get(id) ?? []),
        otherPlant: other,
        task,
        // A repeating step's done task is followed by an open one for its next date, so it reads as to do again.
        done: Boolean(task && (task.done_at || checked.has(task.id))),
        dates: timePhrase(step, task, today),
        triggerNote: step.when.kind === "on_trigger" ? TRIGGER_NOTE[step.when.trigger] : undefined,
      };
    });
}

/** The mini player's step: the first one with a task still to do. Undefined when every step is done. */
export function upNext(steps: readonly StepView[]): { index: number; step: StepView } | undefined {
  const index = steps.findIndex((s) => s.task && !s.done);
  const step = steps[index];
  return step ? { index, step } : undefined;
}

const SUPERSEDED: Record<SupersedeReason, string> = {
  late_answer: "Updated after your answer",
  species_changed: "Updated for the new species",
  guided_check: "Updated after the soil check",
  chat_revision: "Updated from chat",
};

/** "Updated after your answer" (screens 3.4), from the `plan_superseded` event's reason. */
export function supersededNote(plan: Pick<ActionPlan, "supersedes">, reason: SupersedeReason | undefined): string | undefined {
  if (!plan.supersedes) return undefined;
  return reason ? SUPERSEDED[reason] : "Updated since the scan";
}
