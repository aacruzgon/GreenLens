// The garden screen's pieces over the scene (design 3.1, the 3D garden; styles.css .g3d-card, .g3d-sheet,
// .g3d-ctrl): the card from the top saying what the mode shows, with the status legend; the selected plant resting
// as a pill that grows into its card, with its timeline and the date scrubber; and the shutter row, Insights, a
// white Scan shutter and Ask. Cards are solid, never glass; the controls are neutral (design 6).
import { Host, Slider } from "@expo/ui/swift-ui";
import { accessibilityLabel, accessibilityValue, tint } from "@expo/ui/swift-ui/modifiers";
import { useEffect } from "react";
import { Platform, Pressable, View } from "react-native";
import Animated, { Easing, useAnimatedStyle, useSharedValue, withTiming } from "react-native-reanimated";

import type { PlantStatus } from "@/domain/types";
import { Card, IconButton, LineIcon, StatusShape, Text, minTouch, motion, radii, space, statusRoles, statusWords, useCardStyle, useReducedMotion, useTheme } from "@/ui";

import type { TopCard } from "../model/garden";
import type { SceneDate, ScenePlant } from "../model/gardenScene";

const LEGEND: PlantStatus[] = ["healthy", "watch", "sick"];
const sheetEase = Easing.bezier(...motion.sheet.bezier);

export function Legend() {
  return (
    <View style={{ flexDirection: "row", flexWrap: "wrap", gap: space[3] }} accessibilityElementsHidden importantForAccessibility="no-hide-descendants">
      {LEGEND.map((s) => (
        <View key={s} style={{ flexDirection: "row", alignItems: "center", gap: 5 }}>
          <StatusShape status={s} size={10} />
          <Text variant="caption1" color="textMuted">
            {statusWords[s]}
          </Text>
        </View>
      ))}
    </View>
  );
}

/** The card from the top (.g3d-card). A polite live region, so a new mode's words are read. */
export function GardenTopCard({ card }: { card: TopCard }) {
  return (
    <Card accessibilityLiveRegion="polite" testID="garden-top-card" style={{ gap: space[1] }}>
      <Text variant="headline">{card.title}</Text>
      <Text variant="footnote" color="textMuted">
        {card.text}
      </Text>
      <View style={{ marginTop: space[2] }}>
        <Legend />
      </View>
    </Card>
  );
}

export interface PlantCardProps {
  plant: ScenePlant;
  expanded: boolean;
  onExpand: (on: boolean) => void;
  note?: string;
  near?: { name: string; cm: number };
  /** Each scrubber date with the plant's status then. */
  steps: { date: SceneDate; status?: PlantStatus }[];
  dateIndex: number;
  onDate: (index: number) => void;
  onOpen: () => void;
  distance: (cm: number) => string;
}

/**
 * The selected plant: a pill ("Tomato 1 · ◆ Sick") above the shutter row that grows into its card on a tap (height,
 * width, note, the nearest plant, and the timeline to the forecast night with the date scrubber) and shrinks back
 * on a tap on its header or grabber (styles.css .g3d-sheet). The sheet curve over 420 ms; a fade when reduced.
 */
export function PlantCard({ plant, expanded, onExpand, note, near, steps, dateIndex, onDate, onOpen, distance }: PlantCardProps) {
  const card = useCardStyle();
  const reduced = useReducedMotion();
  const open = useSharedValue(expanded ? 1 : 0);
  useEffect(() => {
    open.value = withTiming(expanded ? 1 : 0, { duration: reduced ? motion.fade.duration : motion.sheet.duration, easing: sheetEase });
  }, [expanded, open, reduced]);
  const body = useAnimatedStyle(() => ({ opacity: open.value }));
  const status = plant.status === "unknown" ? undefined : plant.status;
  const height = plant.height_m !== undefined ? `${Math.round(plant.height_m * 100)} cm` : undefined;
  const width = plant.canopy_radius_m !== undefined ? `${Math.round(plant.canopy_radius_m * 200)} cm` : undefined;
  const head = `${plant.name} · ${status ? statusWords[status] : "Not scanned yet"}`;

  return (
    <View testID="plant-card" style={[card, { marginHorizontal: expanded ? 0 : 48, borderRadius: radii.sheet, paddingHorizontal: expanded ? 18 : 16, paddingVertical: expanded ? 8 : 6 }]}>
      {expanded ? (
        <Pressable accessibilityRole="button" accessibilityLabel="Shrink the plant's card" onPress={() => onExpand(false)} hitSlop={8} style={{ alignSelf: "center", paddingVertical: 4 }}>
          <View style={{ width: 38, height: 5, borderRadius: 3, backgroundColor: card.borderColor as string }} />
        </Pressable>
      ) : null}
      <Pressable
        accessibilityRole="button"
        accessibilityState={{ expanded }}
        accessibilityLabel={head}
        accessibilityHint={expanded ? "Shrinks the card" : "Shows its height, width and timeline"}
        onPress={() => onExpand(!expanded)}
        testID="plant-pill"
        style={{ minHeight: minTouch - 4, flexDirection: "row", alignItems: "center", justifyContent: expanded ? "space-between" : "center", gap: space[2] }}
      >
        <Text variant={expanded ? "title3" : "headline"} emphasis numberOfLines={1} style={{ flexShrink: 1 }}>
          {plant.name}
        </Text>
        <View style={{ flexDirection: "row", alignItems: "center", gap: 5 }}>
          {status ? <StatusShape status={status} size={10} /> : null}
          <Text variant="subheadline" emphasis color={status ? statusRoles[status].fg : "textMuted"}>
            {status ? statusWords[status] : "Not scanned"}
          </Text>
        </View>
      </Pressable>
      {expanded ? (
        <Animated.View style={[{ gap: space[2], paddingBottom: space[2] }, body]}>
          <View style={{ flexDirection: "row", flexWrap: "wrap", columnGap: space[4], rowGap: space[1] }}>
            <Fact value={height ?? "Not measured"} unit={height ? "tall" : undefined} />
            {width ? <Fact value={width} unit="across" /> : null}
            {note ? (
              <Text variant="subheadline" color="textMuted">
                {note}
              </Text>
            ) : null}
          </View>
          {near ? (
            <Text variant="footnote" color="textMuted">
              {distance(near.cm)} to {near.name}
            </Text>
          ) : null}
          <DateScrubber steps={steps} index={dateIndex} onChange={onDate} />
          <Pressable accessibilityRole="button" onPress={onOpen} hitSlop={6} style={{ alignSelf: "flex-start", minHeight: minTouch, justifyContent: "center" }}>
            <Text variant="subheadline" emphasis>
              Open {plant.name}
            </Text>
          </Pressable>
          <Text variant="caption1" color="textMuted">
            Drawn from your garden map and plant measurements. The camera scan of your yard stays on your phone.
          </Text>
        </Animated.View>
      ) : null}
    </View>
  );
}

function Fact({ value, unit }: { value: string; unit?: string }) {
  return (
    <Text variant="subheadline">
      <Text variant="subheadline" emphasis>
        {value}
      </Text>
      {unit ? <Text variant="subheadline" color="textMuted">{` ${unit}`}</Text> : null}
    </Text>
  );
}

/**
 * The date scrubber: from the first scan to one step past today, a tick per scan date, each in the plant's status
 * shape then, and the forecast night last (design 3.1). Apple's slider on iOS, stepping by date; the ticks under it
 * are buttons too. A tick of haptics per date comes from the screen.
 */
export function DateScrubber({ steps, index, onChange }: { steps: { date: SceneDate; status?: PlantStatus }[]; index: number; onChange: (index: number) => void }) {
  const { colors } = useTheme();
  const current = steps[index]?.date;
  if (steps.length === 0) return null;
  const last = steps.length - 1;
  return (
    <View style={{ gap: space[1] }}>
      <View style={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between" }}>
        <Text variant="footnote" color="textMuted">
          Date
        </Text>
        <Text variant="footnote" emphasis testID="scrubber-date">
          {current ? (current.forecast ? `${current.label}, forecast` : current.label) : ""}
        </Text>
      </View>
      {Platform.OS === "ios" && process.env.NODE_ENV !== "test" && last > 0 ? (
        <Host matchContents={{ vertical: true }} style={{ alignSelf: "stretch" }}>
          <Slider
            value={index}
            min={0}
            max={last}
            step={1}
            onValueChange={(v) => onChange(Math.round(v))}
            modifiers={[tint(colors.text), accessibilityLabel("Date"), accessibilityValue(current?.label ?? "")]}
          />
        </Host>
      ) : null}
      <View style={{ flexDirection: "row", justifyContent: "space-between" }} accessibilityRole="tablist" accessibilityLabel="Scan dates">
        {steps.map((s, i) => {
          const on = i === index;
          return (
            <Pressable
              key={s.date.date}
              accessibilityRole="tab"
              accessibilityState={{ selected: on }}
              accessibilityLabel={`${s.date.label}${s.date.forecast ? ", forecast" : ""}${s.status ? `, ${statusWords[s.status]}` : ""}`}
              onPress={() => onChange(i)}
              hitSlop={4}
              testID={`scrubber-tick-${i}`}
              style={{ alignItems: "center", gap: 3, minWidth: 30, minHeight: minTouch, justifyContent: "center" }}
            >
              <View style={{ height: 12, justifyContent: "center" }}>
                {s.date.forecast ? (
                  <LineIcon name="snow" color={on ? colors.info : colors.textMuted} size={12} />
                ) : s.status ? (
                  <StatusShape status={s.status} size={9} />
                ) : (
                  <View style={{ width: 5, height: 5, borderRadius: 3, backgroundColor: colors.border }} />
                )}
              </View>
              <Text variant="caption2" emphasis={on} color={on ? "text" : "textMuted"}>
                {shortLabel(s.date, i === 0 ? undefined : steps[i - 1]?.date)}
              </Text>
            </Pressable>
          );
        })}
      </View>
    </View>
  );
}

/** "Sep 20", then the day alone within the same month ("27"), as the mockup's ticks; the forecast's weekday. */
function shortLabel(d: SceneDate, prev?: SceneDate): string {
  if (d.forecast || !prev || prev.forecast) return d.label;
  return d.date.slice(0, 7) === prev.date.slice(0, 7) ? String(Number(d.date.slice(8))) : d.label;
}

/** The shutter row (.g3d-ctrl): Insights, a white Scan shutter and Ask, all for the selected plant. */
export function ShutterRow({
  onInsights,
  onScan,
  onAsk,
  canScan,
  plantName,
  busy,
}: {
  onInsights: () => void;
  onScan: () => void;
  onAsk: () => void;
  canScan: boolean;
  plantName?: string;
  busy: boolean;
}) {
  const { colors } = useTheme();
  const disc = { width: 52, height: 52, borderRadius: 26, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.border };
  return (
    <View style={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between", paddingHorizontal: 30 }}>
      <IconButton
        accessibilityLabel={plantName ? `Insights for ${plantName}` : "Insights"}
        disabled={!plantName}
        onPress={onInsights}
        testID="garden-insights"
        icon={({ color }) => <LineIcon name="chart" color={color} size={22} />}
        style={disc}
      />
      {canScan ? (
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={plantName ? `Scan ${plantName}` : "New scan"}
          accessibilityState={{ busy, disabled: busy }}
          disabled={busy}
          onPress={onScan}
          testID="garden-scan"
          style={({ pressed }) => ({
            width: 72,
            height: 72,
            borderRadius: 36,
            alignItems: "center",
            justifyContent: "center",
            backgroundColor: "#FFFFFF",
            // The camera's white shutter, ringed so it reads on the light ground too (styles.css .g3d-shutter).
            boxShadow: "0 0 0 4px rgba(0,0,0,0.28), 0 0 0 7px #FFFFFF, 0 12px 28px -10px rgba(0,0,0,0.45)",
            transform: [{ scale: pressed ? 0.94 : 1 }],
            opacity: busy ? 0.7 : 1,
          })}
        >
          <LineIcon name="camera" color="#0B0E0D" size={28} />
        </Pressable>
      ) : (
        <View style={{ width: 72 }} />
      )}
      <IconButton
        accessibilityLabel={plantName ? `Ask about ${plantName}` : "Ask about the garden"}
        onPress={onAsk}
        testID="garden-ask"
        icon={({ color }) => <LineIcon name="chat" color={color} size={22} />}
        style={disc}
      />
    </View>
  );
}
