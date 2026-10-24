// The frost cover planner (design 5): the frost night from the forecast, the garden's plants with the tender ones
// chosen, and a summary that follows the choice. Choosing is local. Adding writes one task per chosen plant with
// `POST /tasks` (S5: a write the user starts in the app's own UI is already confirmed, so no card), for editors
// and owners. The rows are a list with checkmarks, as iOS has no checkbox (HIG, lists and tables).
import { useRef, useState } from "react";
import { Pressable, View } from "react-native";

import { isApiError, newKey, useAllTasks, useCreateTask, type IdempotencyKey } from "@/api";
import type { AnswerBlock } from "@/domain/proposed";
import { formatLocalDate } from "@/domain/format";
import { haptics } from "@/platform";
import { Button, LineIcon, StatusShape, Text, minTouch, space, useTheme } from "@/ui";

import { alreadyCovered, blockSummary, coverTasks, defaultCover, frostNight, frostOrder, frostSummary, frostWord } from "../../model/blocks";
import { BlockFrame } from "./BlockFrame";

type Block = Extract<AnswerBlock, { kind: "frost_planner" }>;

export interface FrostPlannerBlockProps {
  block: Block;
  gardenId: string;
  /** `task.write`: editors and owners. Viewers and reviewers get the planner without Add. */
  canAddTasks: boolean;
}

export function FrostPlannerBlock({ block, gardenId, canAddTasks }: FrostPlannerBlockProps) {
  const { colors } = useTheme();
  const [chosen, setChosen] = useState<string[]>(() => defaultCover(block));
  const [adding, setAdding] = useState(false);
  const [error, setError] = useState<string>();
  const tasks = useAllTasks(canAddTasks ? gardenId : "", { state: "open" });
  const create = useCreateTask();
  // One key per plant, kept for a retry after a partial failure: the tasks already added aren't added twice.
  const keys = useRef(new Map<string, IdempotencyKey>());
  const covered = alreadyCovered(block, tasks.data ?? []);
  const night = frostNight(block);
  const toAdd = chosen.filter((id) => !covered.has(id));

  const toggle = (id: string) => {
    void haptics.selection();
    setChosen((c) => (c.includes(id) ? c.filter((x) => x !== id) : [...c, id]));
  };

  const add = async () => {
    setAdding(true);
    setError(undefined);
    try {
      for (const body of coverTasks(block, gardenId, toAdd)) {
        const id = body.plant_id!;
        if (!keys.current.has(id)) keys.current.set(id, newKey());
        await create.mutateAsync(body, keys.current.get(id));
      }
      void haptics.success();
    } catch (e) {
      setError(isApiError(e) ? e.message : "The tasks weren't all added. Check your network and try again.");
    } finally {
      setAdding(false);
    }
  };

  return (
    <BlockFrame title="Frost cover planner" icon="snow" summary={blockSummary(block)} testID="block-frost_planner">
      {night ? (
        <View style={{ flexDirection: "row", alignItems: "baseline", gap: space[2] }}>
          <Text variant="title3" emphasis>
            {night.t_min_c} °C
          </Text>
          <Text variant="subheadline" color="textMuted">
            low on the night of {formatLocalDate(night.date)}
          </Text>
        </View>
      ) : null}
      <View accessibilityRole="list">
        {frostOrder(block.plants).map((p, i) => {
          const done = covered.has(p.plant_id);
          const on = done || chosen.includes(p.plant_id);
          const word = frostWord(p);
          return (
            <Pressable
              key={p.plant_id}
              accessibilityRole="button"
              accessibilityState={{ selected: on, disabled: done }}
              accessibilityLabel={`${p.name}, ${done ? "cover task added" : word === "Not known" ? "frost tenderness not known" : word === "Tender" ? "tender to frost" : "hardy"}`}
              accessibilityHint={done ? undefined : on ? "Leaves it uncovered" : "Chooses it to cover"}
              disabled={done}
              onPress={() => toggle(p.plant_id)}
              testID={`frost-plant-${p.plant_id}`}
              style={({ pressed }) => ({
                flexDirection: "row",
                alignItems: "center",
                gap: space[3],
                minHeight: minTouch,
                opacity: pressed ? 0.6 : 1,
                borderTopWidth: i === 0 ? 0 : 1,
                borderTopColor: colors.border,
              })}
            >
              <StatusShape status={p.status} />
              <Text variant="body" style={{ flex: 1 }}>
                {p.name}
              </Text>
              <Text variant="footnote" color={done ? "textMuted" : word === "Tender" ? "info" : "textMuted"}>
                {done ? "Task added" : word}
              </Text>
              <View style={{ width: 22, alignItems: "flex-end" }}>{on ? <LineIcon name="check" color={done ? colors.textMuted : colors.accent} size={20} strokeWidth={2.2} /> : null}</View>
            </Pressable>
          );
        })}
      </View>
      <Text variant="subheadline" accessibilityLiveRegion="polite" testID="frost-summary">
        {frostSummary(block, chosen)}
      </Text>
      {canAddTasks ? (
        toAdd.length > 0 ? (
          <Button
            label={toAdd.length === 1 ? "Add a cover task" : `Add ${toAdd.length} cover tasks`}
            variant="secondary"
            size="small"
            busy={adding}
            disabled={adding}
            onPress={() => void add()}
            accessibilityHint={night ? `Due ${formatLocalDate(night.date)}, one task for each plant` : "One task for each plant"}
            style={{ alignSelf: "flex-start" }}
            testID="frost-add"
          />
        ) : covered.size > 0 ? (
          <View style={{ flexDirection: "row", alignItems: "center", gap: space[2] }} testID="frost-added">
            <LineIcon name="check" color={colors.accent} size={16} strokeWidth={2.2} />
            <Text variant="footnote" color="textMuted">
              On your tasks{night ? ` for ${formatLocalDate(night.date)}` : ""}
            </Text>
          </View>
        ) : null
      ) : null}
      {error ? (
        <Text variant="footnote" color="danger" accessibilityLiveRegion="polite">
          {error}
        </Text>
      ) : null}
    </BlockFrame>
  );
}
