// "Your plan ›" (roadmap 10 item 5; screens 3.4): the steps with their timing, category and sources, a farm record
// among the sources opening in Slack or Drive, the server's safety note on a chemical step as sent, the provisional
// banner and "Updated after your answer". Checking a step completes its task; viewers see the steps without boxes.
import { Pressable, View } from "react-native";

import type { FarmCitation } from "@/domain/types";
import { TaskRow } from "@/features/tasks";
import { Banner, CiteNumber, LineIcon, Text, space, useTheme, type LineIconName } from "@/ui";

import type { StepView } from "../model/plan";
import { TintCard } from "./TintCard";

export interface PlanChecklistProps {
  steps: StepView[];
  summary: string;
  /** The provisional note (screens 6), when the plan is provisional. */
  provisional?: string;
  /** "Updated after your answer". */
  updated?: string;
  canCheck: boolean;
  checkProblem?: string;
  fill: string;
  separator: string;
  onCheck: (step: StepView) => void;
  onOpenSteps: (start?: number) => void;
  onOpenFarmRecord: (url: string) => void;
}

const SYSTEM_WORD: Record<FarmCitation["system"], string> = { slack: "Slack", google_drive: "Google Drive" };

function stepIcon(s: StepView): LineIconName {
  if (s.isChemical) return "flask";
  if (s.triggerNote) return "rain";
  if (s.dates) return "repeat";
  if (s.otherPlant) return "pin";
  return "hand";
}

function StepMeta({ step }: { step: StepView }) {
  const { colors } = useTheme();
  const parts = [step.when, step.category, ...(step.otherPlant ? [step.otherPlant] : [])];
  return (
    <View style={{ flexDirection: "row", alignItems: "center", flexWrap: "wrap", gap: 6 }}>
      <LineIcon name={stepIcon(step)} color={colors.textMuted} size={15} />
      <Text variant="footnote" color="textMuted">
        {parts.join(" · ")}
      </Text>
      {step.citations.map((n) => (
        <CiteNumber key={n} n={n} />
      ))}
    </View>
  );
}

export function PlanChecklist({
  steps,
  summary,
  provisional,
  updated,
  canCheck,
  checkProblem,
  fill,
  separator,
  onCheck,
  onOpenSteps,
  onOpenFarmRecord,
}: PlanChecklistProps) {
  const { colors } = useTheme();
  return (
    <View style={{ gap: 10 }} testID="plan-checklist">
      <Pressable
        accessibilityRole="button"
        accessibilityLabel="Your plan"
        accessibilityHint="Opens the plan step by step"
        onPress={() => onOpenSteps()}
        style={({ pressed }) => ({ flexDirection: "row", alignItems: "baseline", gap: 6, marginHorizontal: space[1], opacity: pressed ? 0.6 : 1 })}
      >
        <Text variant="section">Your plan</Text>
        <Text variant="title3" color="textMuted">
          ›
        </Text>
      </Pressable>
      {updated ? (
        <Text variant="footnote" color="info" emphasis style={{ marginHorizontal: space[1] }} testID="plan-updated">
          {updated}
        </Text>
      ) : null}
      {provisional ? <Banner tone="watch" title="Provisional plan" message={provisional} /> : null}
      <Text variant="subheadline" color="textMuted" style={{ marginHorizontal: space[1] }}>
        {summary}
      </Text>
      {checkProblem ? <Banner tone="danger" title={checkProblem} /> : null}
      <TintCard fill={fill}>
        {steps.map((s, i) => (
          <View key={s.id} style={{ borderTopWidth: i === 0 ? 0 : 1, borderTopColor: separator }} testID={`plan-step-${s.id}`}>
            <TaskRow
              title={s.title}
              done={s.done}
              onCheck={canCheck && s.task ? () => onCheck(s) : undefined}
              onPress={() => onOpenSteps(i)}
              pressHint="Opens this step"
            />
            <View style={{ marginTop: -space[2], paddingBottom: space[3], paddingRight: space[4], paddingLeft: canCheck ? space[4] + 24 + space[3] : space[4], gap: space[2] }}>
              <StepMeta step={s} />
              {s.safetyNote ? (
                <View style={{ flexDirection: "row", gap: space[2], padding: space[3], borderRadius: 14, backgroundColor: colors.infoSoft }} testID="safety-note">
                  <LineIcon name="info" color={colors.info} size={18} />
                  <Text variant="footnote" style={{ flex: 1 }}>
                    {s.safetyNote}
                  </Text>
                </View>
              ) : null}
              {s.farmRecords.map((r) => (
                <Pressable
                  key={r.record_id}
                  accessibilityRole="link"
                  accessibilityHint={`Opens in ${SYSTEM_WORD[r.system]}`}
                  onPress={() => onOpenFarmRecord(r.link)}
                  hitSlop={8}
                  style={({ pressed }) => ({ flexDirection: "row", alignItems: "center", gap: 6, opacity: pressed ? 0.6 : 1 })}
                >
                  <LineIcon name="book" color={colors.accent} size={15} />
                  <Text variant="footnote" color="accent" style={{ flexShrink: 1 }}>
                    {r.title} · {SYSTEM_WORD[r.system]}
                    {r.author ? ` · ${r.author}` : ""}
                  </Text>
                </Pressable>
              ))}
            </View>
          </View>
        ))}
      </TintCard>
    </View>
  );
}
