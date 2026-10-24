// The garden map snippet (design 5): the beds and a pin per plant in its status shape, the plants the answer is
// about outlined with their canopy and named under their pin, and their distance to the nearest 5 cm. A tap on a
// pin shows its status and height; Open goes to the plant, and Open the garden to the garden screen on its plants. Drawn with react-native-svg, so it works without WebGL (design 3.1, the map).
import { useState } from "react";
import { Pressable, View } from "react-native";
import Svg, { Circle, Polygon } from "react-native-svg";

import type { AnswerBlock } from "@/domain/proposed";
import { Button, StatusShape, Text, hiddenSvg, minTouch, space, statusRoles, statusWords, useTheme } from "@/ui";

import { blockSummary, fitMap, focusDistance, inFrame, plantLine } from "../../model/blocks";
import { BlockFrame } from "./BlockFrame";

type Block = Extract<AnswerBlock, { kind: "garden_map" }>;

export function GardenMapBlock({ block, onOpenPlant, onOpenGarden }: { block: Block; onOpenPlant: (plantId: string) => void; onOpenGarden: (plantIds: string[]) => void }) {
  const { colors } = useTheme();
  // The card's own width, measured: an estimate from the screen left a gap on the right.
  const [width, setWidth] = useState(0);
  const frame = fitMap(block.map, Math.max(width, 1), block.focus_plant_ids);
  const [picked, setPicked] = useState<string | undefined>(block.focus_plant_ids[0]);
  const px = (x: number) => (x - frame.ox) * frame.scale;
  const py = (y: number) => (y - frame.oy) * frame.scale;
  const plant = (id: string) => block.plants.find((p) => p.plant_id === id);
  const chosen = picked ? plant(picked) : undefined;
  const distance = focusDistance(block);
  return (
    <BlockFrame title="Garden map" icon="map" summary={blockSummary(block)} testID="block-garden_map">
      <View onLayout={(e) => setWidth(e.nativeEvent.layout.width)} style={{ height: width ? frame.height : 120, overflow: "hidden", borderRadius: 12 }}>
        {width ? <Svg width={width} height={frame.height} {...hiddenSvg}>
          {block.map.beds.map((b) => (
            <Polygon key={b.id} points={b.polygon_m.map((p) => `${px(p[0]!)},${py(p[1]!)}`).join(" ")} fill={colors.wash} stroke={colors.border} strokeWidth={1} />
          ))}
          {block.map.pins.map((pin) =>
            pin.canopy_radius_m && block.focus_plant_ids.includes(pin.plant_id) ? (
              <Circle key={`c-${pin.plant_id}`} cx={px(pin.x_m)} cy={py(pin.y_m)} r={pin.canopy_radius_m * frame.scale} fill="none" stroke={colors.textMuted} strokeDasharray="3 3" strokeWidth={1} />
            ) : null,
          )}
        </Svg> : null}
        {block.map.pins.map((pin) => {
          const p = plant(pin.plant_id);
          if (!width || !p || !inFrame(frame, pin.x_m, pin.y_m)) return null;
          const focus = block.focus_plant_ids.includes(pin.plant_id);
          const on = picked === pin.plant_id;
          return [
            <Pressable
              key={pin.plant_id}
              accessibilityRole="button"
              accessibilityState={{ selected: on }}
              accessibilityLabel={plantLine(p)}
              onPress={() => setPicked(pin.plant_id)}
              hitSlop={8}
              style={{
                position: "absolute",
                left: px(pin.x_m) - 14,
                top: py(pin.y_m) - 14,
                width: 28,
                height: 28,
                borderRadius: 14,
                alignItems: "center",
                justifyContent: "center",
                backgroundColor: colors.surface,
                borderWidth: on || focus ? 2 : 1,
                borderColor: on ? colors.text : focus ? colors[statusRoles[p.status].fg] : colors.border,
              }}
            >
              <StatusShape status={p.status} size={10} />
            </Pressable>,
            focus ? (
              <Text
                key={`n-${pin.plant_id}`}
                variant="caption2"
                emphasis
                numberOfLines={1}
                accessibilityElementsHidden
                importantForAccessibility="no"
                style={{ position: "absolute", left: px(pin.x_m) - 44, top: py(pin.y_m) + 16, width: 88, textAlign: "center" }}
              >
                {p.name}
              </Text>
            ) : null,
          ];
        })}
      </View>
      {distance ? <Text variant="subheadline">{distance}</Text> : null}
      {chosen ? (
        <View style={{ flexDirection: "row", alignItems: "center", gap: space[2], minHeight: minTouch }} accessibilityLiveRegion="polite">
          <StatusShape status={chosen.status} />
          <Text variant="subheadline" style={{ flex: 1 }}>
            {chosen.name} · {statusWords[chosen.status]}
            {chosen.height_cm !== undefined ? ` · about ${Math.round(chosen.height_cm)} cm tall` : ""}
          </Text>
          <Button label="Open" variant="secondary" size="small" onPress={() => onOpenPlant(chosen.plant_id)} accessibilityLabel={`Open ${chosen.name}`} />
        </View>
      ) : null}
      {/* The garden screen on the plants the answer is about (roadmap F7 item 1). */}
      <Button label="Open the garden" variant="secondary" size="small" onPress={() => onOpenGarden(block.focus_plant_ids)} style={{ alignSelf: "flex-start" }} testID="block-open-garden" />
    </BlockFrame>
  );
}
