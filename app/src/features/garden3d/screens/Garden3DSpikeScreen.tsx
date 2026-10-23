// The GardenView spike (roadmap F5 item 3; garden-3d 7): the mock garden in the native 3D view, the three modes,
// the date, tap to select, and a 10 × 4 garden for the frame-time check on the iPhone. A dev screen, not the
// garden's real screen (F7).
import { useMemo, useReducer } from "react";
import { Switch, View } from "react-native";

import { GardenView } from "@/platform";

import { Button, Chip, Screen, StatusShape, Text, space, statusWords, useTheme } from "@/ui";

import { buildScene, plantSpoken, sceneSummary, spikeReducer, type GardenMode, type SpikeState } from "../model/gardenScene";
import { largeGarden, MOCK_MAP, MOCK_PLANTS, SPIKE_DATES } from "../model/spikeGarden";

const MODES: { mode: GardenMode; label: string }[] = [
  { mode: "health", label: "HEALTH" },
  { mode: "growth", label: "GROWTH" },
  { mode: "frost", label: "FROST" },
];

const INITIAL: SpikeState = { mode: "health", dateIndex: 2, large: false, selectedId: null };

export function Garden3DSpikeScreen() {
  const { scheme, reducedMotion, colors } = useTheme();
  const [state, dispatch] = useReducer((s: SpikeState, a: Parameters<typeof spikeReducer>[2]) => spikeReducer(SPIKE_DATES, s, a), INITIAL);
  const large = useMemo(() => largeGarden(), []);
  const date = SPIKE_DATES[state.dateIndex];
  const scene = useMemo(
    () => (state.large ? buildScene(large.map, large.plants, date.date) : buildScene(MOCK_MAP, MOCK_PLANTS, date.date)),
    [state.large, large, date.date],
  );
  const selected = scene.plants.find((p) => p.id === state.selectedId);
  const summary = sceneSummary(scene);

  return (
    <Screen scroll={false}>
      <View style={{ gap: space[1] }}>
        <Text variant="title2">3D garden spike</Text>
        <Text variant="subheadline" color="textMuted">
          {summary}
        </Text>
      </View>

      <View style={{ flexDirection: "row", gap: space[2] }} accessibilityRole="radiogroup">
        {MODES.map((m) => (
          <Chip
            key={m.mode}
            label={m.label}
            selected={state.mode === m.mode}
            onPress={() => dispatch({ type: "mode", mode: m.mode })}
            accessibilityLabel={`${m.label.toLowerCase()} mode`}
          />
        ))}
      </View>

      {GardenView ? (
        <GardenView
          style={{ height: 380, marginHorizontal: -space[4], backgroundColor: colors.background }}
          beds={scene.beds}
          plants={scene.plants}
          mode={state.mode}
          selectedPlantId={state.selectedId}
          scheme={scheme}
          reduceMotion={reducedMotion}
          onSelectPlant={(e) => dispatch({ type: "select", id: e.nativeEvent.plant_id })}
          accessibilityLabel={`3D garden view, ${summary}.`}
        />
      ) : (
        <Text variant="footnote" color="textMuted" testID="garden3d-unavailable">
          The 3D garden needs the development build with the LiDAR module.
        </Text>
      )}

      <View style={{ flexDirection: "row", alignItems: "center", gap: space[3] }}>
        <Button
          label="Earlier"
          variant="secondary"
          disabled={state.dateIndex === 0}
          onPress={() => dispatch({ type: "date", index: state.dateIndex - 1 })}
        />
        <Text variant="headline" style={{ flex: 1 }} align="center" accessibilityValue={{ text: date.label }}>
          {date.label}
        </Text>
        <Button
          label="Later"
          variant="secondary"
          disabled={state.dateIndex === SPIKE_DATES.length - 1}
          onPress={() => dispatch({ type: "date", index: state.dateIndex + 1 })}
        />
      </View>

      <View style={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between" }}>
        <Text>40 plants</Text>
        <Switch value={state.large} onValueChange={(on) => dispatch({ type: "large", on })} accessibilityLabel="40 plants" />
      </View>

      <View style={{ flexDirection: "row", alignItems: "center", gap: space[2] }} accessibilityLabel={selected ? plantSpoken(selected) : "No plant selected"}>
        {selected && selected.status !== "unknown" ? <StatusShape status={selected.status} size={11} /> : null}
        <Text emphasis>{selected ? selected.name : "No plant selected"}</Text>
        {selected && selected.status !== "unknown" ? <Text color="textMuted">{statusWords[selected.status]}</Text> : null}
      </View>
    </Screen>
  );
}
