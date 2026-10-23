// The stage card (roadmap F3 item 3; design 3.1): the running step with its time and the next one, dropping from
// the top like the Camera app's answer card. A tap opens the run log under it, a tick per step with what it found
// and its time (index.html .stages); another tap closes it. Each new tick has its haptic (useScanEvents).
import { useEffect, useState } from "react";
import { Pressable, ScrollView, View } from "react-native";
import Animated, { Easing, useAnimatedStyle, useSharedValue, withTiming } from "react-native-reanimated";

import { DropCard, PhaseLine, Text, motion, useReducedMotion, useTheme } from "@/ui";

import { formatSeconds, type PhaseLine as PhaseLineModel, type PhaseView } from "../model/phases";
import { ScanIcon } from "./ScanIcons";

const sheetEase = Easing.bezier(...motion.sheet.bezier);

/** Seconds since `since`, ticking while it runs; undefined without a start. */
function useElapsed(since: string | undefined): number | undefined {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!since) return;
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [since]);
  if (!since) return undefined;
  return Math.max(0, Math.floor((now - Date.parse(since)) / 1000));
}

export interface StageCardProps {
  line: PhaseLineModel;
  phases: readonly PhaseView[];
  style?: object;
  onLayout?: Parameters<typeof DropCard>[0]["onLayout"];
}

export function StageCard({ line, phases, style, onLayout }: StageCardProps) {
  const [open, setOpen] = useState(false);
  const elapsed = useElapsed(line.since);
  const time = line.total ?? (elapsed !== undefined ? `${elapsed} s` : undefined);

  return (
    <DropCard testID="stage-card" dropKey={line.now} style={style} onLayout={onLayout}>
      <Pressable
        accessibilityRole="button"
        accessibilityState={{ expanded: open }}
        accessibilityHint={open ? "Hides the run log" : "Shows every step and its time"}
        onPress={() => setOpen((o) => !o)}
        style={{ minHeight: 44, justifyContent: "center" }}
      >
        <View accessibilityLiveRegion="polite">
          <PhaseLine now={line.now} time={time} next={line.next} />
        </View>
      </Pressable>
      {open ? (
        <ScrollView style={{ maxHeight: 330, marginTop: 10 }} testID="run-log">
          {phases.map((p) => (
            <PhaseRow key={p.key} phase={p} />
          ))}
        </ScrollView>
      ) : null}
    </DropCard>
  );
}

function PhaseRow({ phase }: { phase: PhaseView }) {
  const { colors } = useTheme();
  const reduced = useReducedMotion();
  const done = phase.state === "done" || phase.state === "skipped";
  const tick = useSharedValue(done ? 1 : 0);
  useEffect(() => {
    const to = done ? 1 : 0;
    tick.value = reduced || !done ? to : withTiming(to, { duration: motion.tick.duration, easing: sheetEase });
  }, [done, reduced, tick]);
  const tickStyle = useAnimatedStyle(() => ({ transform: [{ scale: motion.tick.fromScale + (1 - motion.tick.fromScale) * tick.value }] }));

  const words = phase.state === "skipped" ? "skipped" : phase.state === "done" ? "done" : phase.state === "running" ? "running" : "waiting";
  const note = phase.state === "skipped" ? "Skipped" : phase.note;
  return (
    <View
      accessible
      accessibilityLabel={[phase.label, words, note, phase.durationMs ? formatSeconds(phase.durationMs) : undefined].filter(Boolean).join(", ")}
      style={{ flexDirection: "row", alignItems: "flex-start", gap: 12, paddingVertical: 6, minHeight: 34 }}
      testID={`phase-${phase.key}-${phase.state}`}
    >
      <Animated.View
        style={[
          {
            width: 22,
            height: 22,
            borderRadius: 11,
            borderWidth: 2,
            alignItems: "center",
            justifyContent: "center",
            borderColor: done ? colors.accent : phase.state === "running" ? colors.accent : colors.border,
            backgroundColor: done ? colors.accent : "transparent",
            borderStyle: phase.state === "running" ? "dashed" : "solid",
          },
          tickStyle,
        ]}
      >
        {done ? <ScanIcon name="check" color={colors.accentText} size={14} /> : null}
      </Animated.View>
      <View style={{ flex: 1 }}>
        <Text variant="callout" color={phase.state === "waiting" ? "textMuted" : "text"} style={phase.state === "running" ? { fontWeight: "600" } : undefined}>
          {phase.label}
        </Text>
        {note && phase.state !== "waiting" ? (
          <Text variant="footnote" color="textMuted">
            {note}
          </Text>
        ) : null}
      </View>
      {done && phase.durationMs ? (
        <Text variant="caption1" color="textMuted" style={{ lineHeight: 21, fontVariant: ["tabular-nums"] }}>
          {formatSeconds(phase.durationMs)}
        </Text>
      ) : null}
    </View>
  );
}
