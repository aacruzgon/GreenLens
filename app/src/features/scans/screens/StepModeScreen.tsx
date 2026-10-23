// The plan, step by step (roadmap 10 item 7; design 5, after Crouton's step mode): one step per screen in large
// type, with when, why and how, its source, and the time phrase as named tasks ("Adds 5 tasks: Oct 20, 23 …").
// Done checks the step's task, so it reads done on the plan and the Tasks tab too; Later moves on without it.
import { useLocalSearchParams, useRouter } from "expo-router";
import { useState } from "react";
import { View } from "react-native";
import Animated, { FadeIn, SlideInRight } from "react-native-reanimated";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { useScan } from "@/api";
import { diagnosisCopy } from "@/domain/diagnosis";
import { Banner, Button, IconButton, LineIcon, Text, Wash, displayStyles, gutter, motion, space, useTheme } from "@/ui";

import type { StepView } from "../model/plan";
import { usePlanSteps } from "../hooks/usePlanSteps";

const STEP_TITLE = { fontSize: 30, lineHeight: 36, letterSpacing: displayStyles.display.letterSpacing * 0.75 };

function Dots({ steps, at }: { steps: StepView[]; at: number }) {
  const { colors } = useTheme();
  return (
    <View style={{ flexDirection: "row", gap: 6, justifyContent: "center" }} accessible accessibilityLabel={`${steps.filter((s) => s.done).length} of ${steps.length} done`}>
      {steps.map((s, i) => (
        <View key={s.id} style={{ width: 24, height: 4, borderRadius: 2, backgroundColor: i === at ? colors.accent : s.done ? colors.healthy : colors.border }} />
      ))}
    </View>
  );
}

function StepBody({ step, source }: { step: StepView; source?: { n: number; label: string } }) {
  const { colors } = useTheme();
  return (
    <View style={{ gap: 14 }}>
      <View style={{ flexDirection: "row", alignItems: "center", gap: 6 }}>
        <LineIcon name={step.dates ? "repeat" : step.triggerNote ? "rain" : step.isChemical ? "flask" : "hand"} color={colors.accent} size={16} />
        <Text variant="subheadline" color="accent" emphasis>
          {step.when} · {step.category}
        </Text>
      </View>
      <Text variant="display" style={STEP_TITLE} testID="step-title">
        {step.title}
      </Text>
      {step.otherPlant ? (
        <Text variant="subheadline" color="textMuted">
          For {step.otherPlant}
        </Text>
      ) : null}
      <Text variant="body">{step.detail}</Text>
      {step.dates ? (
        <View style={{ gap: 6 }} testID="step-dates">
          <Text variant="footnote" color="textMuted">
            {step.dates.label}
          </Text>
          <View style={{ flexDirection: "row", flexWrap: "wrap", gap: 6 }}>
            {step.dates.dates.map((d) => (
              <View key={d} style={{ paddingHorizontal: 10, paddingVertical: 5, borderRadius: 999, backgroundColor: colors.accentSoft }}>
                <Text variant="footnote" style={{ color: colors.accent, fontWeight: "600", fontVariant: ["tabular-nums"] }}>
                  {d}
                </Text>
              </View>
            ))}
          </View>
        </View>
      ) : null}
      {step.triggerNote ? (
        <Text variant="footnote" color="textMuted">
          {step.triggerNote}
        </Text>
      ) : null}
      {step.safetyNote ? <Banner tone="info" title="Before you use it" message={step.safetyNote} /> : null}
      {source ? (
        <Text variant="footnote" color="textMuted">
          [{source.n}] {source.label}
        </Text>
      ) : null}
    </View>
  );
}

export function StepModeScreen() {
  const { scanId, start } = useLocalSearchParams<{ scanId: string; start?: string }>();
  const router = useRouter();
  const insets = useSafeAreaInsets();
  const { colors, reducedMotion } = useTheme();
  const scan = useScan(scanId);
  const plan = usePlanSteps(scan.data);
  const [at, setAt] = useState(() => Math.max(0, Number(start ?? 0) || 0));
  const [moved, setMoved] = useState(false);

  const steps = plan.steps;
  const step = steps[at];
  const diagnosis = scan.data?.diagnosis;
  const subtitle = [scan.data?.plant.name, diagnosis && !diagnosis.low_evidence ? diagnosisCopy(diagnosis).headline : undefined].filter(Boolean).join(" · ");
  const firstCite = step?.citations[0];
  const citation = firstCite ? plan.plan?.citations[firstCite - 1] : undefined;

  const advance = () => {
    setMoved(true);
    setAt((i) => i + 1);
  };
  const done = () => {
    if (step) plan.check(step);
    advance();
  };
  const canDo = Boolean(step?.task && plan.canCheck && plan.tasksLoaded && !step.done);
  const finished = steps.length > 0 && at >= steps.length;

  return (
    <View style={{ flex: 1, backgroundColor: colors.background }} testID="step-mode">
      <Wash />
      <View style={{ paddingTop: insets.top + space[2], paddingHorizontal: space[2], flexDirection: "row", alignItems: "center", gap: space[2] }}>
        <IconButton accessibilityLabel="Close" onPress={() => router.back()} icon={({ color }) => <LineIcon name="x" color={color} size={22} />} />
        <View style={{ flex: 1 }}>
          <Text variant="headline">Your plan</Text>
          {subtitle ? (
            <Text variant="footnote" color="textMuted">
              {subtitle}
            </Text>
          ) : null}
        </View>
        <Text variant="footnote" color="textMuted" style={{ marginRight: space[2], fontVariant: ["tabular-nums"] }}>
          {finished ? "Done" : steps.length > 0 ? `${at + 1} of ${steps.length}` : ""}
        </Text>
      </View>
      <View style={{ marginTop: space[1], marginBottom: 18 }}>{steps.length > 0 ? <Dots steps={steps} at={at} /> : null}</View>

      <View style={{ flex: 1, paddingHorizontal: 22 }}>
        {finished ? (
          <Animated.View entering={FadeIn.duration(motion.fade.duration)} style={{ gap: 14 }} testID="steps-finished">
            <Text variant="subheadline" color="accent" emphasis>
              All set
            </Text>
            <Text variant="display" style={STEP_TITLE}>
              {steps.filter((s) => s.done).length} of {steps.length} steps done
            </Text>
            <Text variant="body" color="textMuted">
              The rest are on your Tasks tab, with their dates.
            </Text>
          </Animated.View>
        ) : step ? (
          <Animated.View
            key={step.id}
            entering={!moved || reducedMotion ? FadeIn.duration(motion.fade.duration) : SlideInRight.duration(motion.sheet.duration)}
          >
            <StepBody step={step} source={firstCite && citation ? { n: firstCite, label: `${citation.publisher}, ${citation.title}` } : undefined} />
          </Animated.View>
        ) : null}
      </View>

      <View style={{ paddingHorizontal: gutter, paddingTop: space[4], paddingBottom: insets.bottom + space[4], gap: space[1] }}>
        {plan.failed ? <Banner tone="danger" title="That step didn't save. Try again." style={{ marginBottom: space[2] }} /> : null}
        {finished ? (
          <Button label="Close" size="large" onPress={() => router.back()} testID="steps-close" />
        ) : step ? (
          <>
            <Button label={canDo ? "Done" : "Next"} size="large" onPress={canDo ? done : advance} testID="step-done" />
            {canDo ? <Button label="Later" variant="plain" onPress={advance} testID="step-later" /> : null}
          </>
        ) : null}
      </View>
    </View>
  );
}
