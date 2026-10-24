// The garden screen (roadmap F7 items 1–2; design 3.1, the 3D garden; screens 3.8), full screen like a viewfinder
// and following the light or dark scheme: the GardenView scene edge to edge on the iPhone, the 2D map in Expo Go,
// when the scene can't draw, and on **Map**; the card from the top; the selected plant's pill and card; the
// shutter row; and the mode bar HEALTH · GROWTH · FROST. 3D / Map sits at the top right.
import { useState } from "react";
import { View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { GardenView } from "@/platform";
import { Banner, Button, EmptyState, ErrorState, IconButton, LineIcon, Skeleton, Text, space, useTheme } from "@/ui";

import { GardenMapView } from "../components/GardenMapView";
import { GardenTopCard, PlantCard, ShutterRow } from "../components/GardenControls";
import { Segmented } from "../components/Segmented";
import { useGardenScreen, type GardenScreenParams } from "../hooks/useGardenScreen";
import { formatDistance } from "../model/garden";
import { plantSpoken, type GardenMode } from "../model/gardenScene";

const MODES: { value: GardenMode; label: string }[] = [
  { value: "health", label: "Health" },
  { value: "growth", label: "Growth" },
  { value: "frost", label: "Frost" },
];

const VIEWS = [
  { value: "3d" as const, label: "3D" },
  { value: "map" as const, label: "Map" },
];

export function GardenScreen(params: GardenScreenParams) {
  const g = useGardenScreen(params);
  const { colors, scheme, reducedMotion } = useTheme();
  const insets = useSafeAreaInsets();
  // The map keeps its own band between the cards and the controls (design 3.1).
  const [top, setTop] = useState(0);
  const [bottom, setBottom] = useState(0);

  const back = (
    <IconButton
      accessibilityLabel="Back"
      filled
      onPress={g.actions.back}
      testID="garden-back"
      icon={({ color }) => <LineIcon name="back" color={color} size={22} />}
    />
  );

  if (g.loading) {
    return (
      <View style={{ flex: 1, backgroundColor: colors.background, paddingTop: insets.top + space[2], paddingHorizontal: space[3], gap: space[3] }} testID="garden-loading">
        {back}
        <Skeleton height={96} />
        <View style={{ flex: 1 }} />
        <Skeleton height={52} />
      </View>
    );
  }
  if (g.error) {
    return (
      <View style={{ flex: 1, backgroundColor: colors.background, paddingTop: insets.top + space[2], paddingHorizontal: space[3] }}>
        {back}
        <ErrorState message="The garden couldn't load. Check your network." onRetry={g.retry} />
      </View>
    );
  }
  if (g.noMap || !g.map) {
    return (
      <View style={{ flex: 1, backgroundColor: colors.background, paddingTop: insets.top + space[2], paddingHorizontal: space[3] }} testID="garden-no-map">
        {back}
        <EmptyState
          title="No map yet"
          message="Map your garden from an iPhone or iPad with LiDAR."
          icon={<LineIcon name="map" color={colors.textMuted} size={36} />}
          {...(g.can.edit ? { action: { label: "Start mapping", onPress: g.actions.editMap } } : {})}
        />
      </View>
    );
  }

  const showMap = g.state.view === "map" || !g.canDraw3d || !GardenView;
  const selected = g.selected;

  return (
    <View style={{ flex: 1, backgroundColor: colors.background }} testID="garden-screen">
      {showMap ? (
        <GardenMapView
          beds={g.beds}
          plants={g.plants}
          mode={g.state.mode}
          selectedId={g.state.selectedId}
          onSelect={g.actions.select}
          {...(g.map.north_deg !== undefined ? { northDeg: g.map.north_deg } : {})}
          insets={{ top, bottom }}
          style={{ position: "absolute", inset: 0 }}
        />
      ) : GardenView ? (
        <GardenView
          style={{ position: "absolute", inset: 0, backgroundColor: colors.background }}
          beds={g.beds}
          plants={g.plants}
          mode={g.state.mode}
          selectedPlantId={g.state.selectedId}
          scheme={scheme}
          reduceMotion={reducedMotion}
          onSelectPlant={(e) => g.actions.select(e.nativeEvent.plant_id)}
          accessibilityLabel={`3D garden, ${g.summary}. The plant list follows.`}
        />
      ) : null}

      {/* VoiceOver's view of the 3D garden: the plants as a list (design 7). The map's pins are already buttons. */}
      {!showMap ? (
        <View accessible={false} style={{ position: "absolute", width: 1, height: 1, overflow: "hidden" }}>
          {g.plants.map((p) => (
            <Text key={p.id} accessibilityRole="button" onPress={() => g.actions.select(p.id)}>
              {plantSpoken(p)}
            </Text>
          ))}
        </View>
      ) : null}

      {/* Top: Back, 3D / Map, then the card from the top. */}
      <View pointerEvents="box-none" onLayout={(e) => setTop(e.nativeEvent.layout.height + e.nativeEvent.layout.y)} style={{ position: "absolute", left: 0, right: 0, top: 0, paddingTop: insets.top + space[1], paddingHorizontal: space[3], gap: space[3] }}>
        <View pointerEvents="box-none" style={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between" }}>
          {back}
          <View style={{ flex: 1, paddingHorizontal: space[2] }}>
            <Text variant="headline" numberOfLines={1} align="center">
              {g.gardenName}
            </Text>
            {g.subtitle ? (
              <Text variant="caption1" color="textMuted" numberOfLines={1} align="center">
                {g.subtitle}
              </Text>
            ) : null}
          </View>
          {g.canDraw3d ? (
            <Segmented items={VIEWS} value={showMap ? "map" : "3d"} onChange={g.actions.setView} label="View" testID="garden-view" />
          ) : (
            <View style={{ width: 44 }} />
          )}
        </View>
        {g.card ? <GardenTopCard card={g.card} /> : null}
        {showMap && g.can.edit ? (
          <Button label="Edit" variant="secondary" size="small" onPress={g.actions.editMap} style={{ alignSelf: "flex-end" }} accessibilityLabel="Edit the map" />
        ) : null}
      </View>

      {/* Bottom: the plant's pill, the shutter row and the mode bar. */}
      <View
        pointerEvents="box-none"
        onLayout={(e) => setBottom(e.nativeEvent.layout.height)}
        style={{ position: "absolute", left: 0, right: 0, bottom: 0, paddingBottom: insets.bottom + space[2], paddingHorizontal: space[3], gap: space[4] }}
      >
        {g.problem ? <Banner tone="danger" title={g.problem} /> : null}
        {selected ? (
          <PlantCard
            plant={selected.plant}
            expanded={g.state.expanded}
            onExpand={g.actions.expand}
            {...(selected.note ? { note: selected.note } : {})}
            {...(selected.near ? { near: selected.near } : {})}
            steps={selected.steps}
            dateIndex={g.state.dateIndex}
            onDate={g.actions.setDate}
            onOpen={() => g.actions.openPlant(selected.plant.id)}
            distance={formatDistance}
          />
        ) : null}
        <ShutterRow
          onInsights={g.actions.insights}
          onScan={g.actions.scan}
          onAsk={g.actions.ask}
          canScan={g.can.scan}
          {...(selected ? { plantName: selected.plant.name } : {})}
          busy={g.scanning || g.asking}
        />
        <View style={{ alignItems: "center" }}>
          <Segmented items={MODES} value={g.state.mode} onChange={g.actions.setMode} label="Garden mode" glass testID="garden-mode" />
        </View>
      </View>
    </View>
  );
}
