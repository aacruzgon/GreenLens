// The frost cover planner (design 5): the frost night from the forecast, every plant on the map with the tender
// ones chosen, and a summary that follows the choice. Choosing is local; "Make it a task" asks the agent, which
// answers with a `create_task` card, so nothing is written without a confirm.
import { useState } from "react";
import { Pressable, View } from "react-native";

import type { AnswerBlock } from "@/domain/proposed";
import { formatLocalDate } from "@/domain/format";
import { haptics } from "@/platform";
import { Button, LineIcon, StatusShape, Text, minTouch, radii, space, useTheme } from "@/ui";

import { blockSummary, coverRequest, defaultCover, frostNight, frostSummary } from "../../model/blocks";
import { BlockFrame } from "./BlockFrame";

type Block = Extract<AnswerBlock, { kind: "frost_planner" }>;

export interface FrostPlannerBlockProps {
  block: Block;
  canWrite: boolean;
  /** Sends the planner's request as the next message. */
  onAsk: (text: string) => void;
  busy: boolean;
}

export function FrostPlannerBlock({ block, canWrite, onAsk, busy }: FrostPlannerBlockProps) {
  const { colors } = useTheme();
  const [chosen, setChosen] = useState<string[]>(() => defaultCover(block));
  const night = frostNight(block);
  const toggle = (id: string) => {
    void haptics.selection();
    setChosen((c) => (c.includes(id) ? c.filter((x) => x !== id) : [...c, id]));
  };
  const plants = [...block.plants].sort((a, b) => Number(b.frost_tender) - Number(a.frost_tender) || a.name.localeCompare(b.name));
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
        {plants.map((p) => {
          const on = chosen.includes(p.plant_id);
          return (
            <Pressable
              key={p.plant_id}
              accessibilityRole="checkbox"
              accessibilityState={{ checked: on }}
              accessibilityLabel={`${p.name}${p.frost_tender ? ", tender to frost" : ", hardy"}`}
              onPress={() => toggle(p.plant_id)}
              testID={`frost-plant-${p.plant_id}`}
              style={({ pressed }) => ({ flexDirection: "row", alignItems: "center", gap: space[3], minHeight: minTouch, opacity: pressed ? 0.6 : 1 })}
            >
              <View style={{ width: 24, height: 24, borderRadius: radii.small, borderWidth: 1.5, borderColor: on ? colors.text : colors.border, backgroundColor: on ? colors.text : "transparent", alignItems: "center", justifyContent: "center" }}>
                {on ? <LineIcon name="check" color={colors.background} size={16} strokeWidth={2.2} /> : null}
              </View>
              <StatusShape status={p.status} />
              <Text variant="body" style={{ flex: 1 }}>
                {p.name}
              </Text>
              <Text variant="footnote" color={p.frost_tender ? "info" : "textMuted"}>
                {p.frost_tender ? "Tender" : "Hardy"}
              </Text>
            </Pressable>
          );
        })}
      </View>
      <Text variant="subheadline" accessibilityLiveRegion="polite" testID="frost-summary">
        {frostSummary(block, chosen)}
      </Text>
      {canWrite ? (
        <Button
          label="Make it a task"
          variant="secondary"
          size="small"
          disabled={chosen.length === 0 || busy}
          onPress={() => onAsk(coverRequest(block, chosen))}
          style={{ alignSelf: "flex-start" }}
          testID="frost-ask"
        />
      ) : null}
    </BlockFrame>
  );
}
