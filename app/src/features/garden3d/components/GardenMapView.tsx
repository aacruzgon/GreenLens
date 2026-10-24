// The 2D garden map (roadmap F7 item 2; screens 3.8; design 3.1, "3D or Map"): beds with their names and areas, a
// pin per plant in its status shape and colour, the canopy ring where it was measured, a 1 m scale bar and north.
// It shares the 3D view's selection, date and modes: Growth puts heights on the labels, Frost rings the tender
// plants in the info colour. Drawn with react-native-svg, so it works without the native view (Expo Go, the web).
// The pins and labels are buttons; VoiceOver reads them as the plant list (design 7).
import { useState } from "react";
import { Pressable, View, type StyleProp, type ViewStyle } from "react-native";
import Svg, { Circle, G, Line, Path, Polygon, Text as SvgText } from "react-native-svg";

import { polygonArea } from "@/domain/map";
import { StatusShape, Text, hiddenSvg, radii, statusRoles, useTheme } from "@/ui";

import { fitWholeMap, labelsAbove } from "../model/garden";
import { plantSpoken, type GardenMode, type SceneBed, type ScenePlant } from "../model/gardenScene";

export interface GardenMapViewProps {
  beds: SceneBed[];
  plants: (ScenePlant & { label?: string })[];
  mode: GardenMode;
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  /** `GardenMap.north_deg`: clockwise from the map's +y axis to north. */
  northDeg?: number;
  /** Room kept clear at the top and bottom for the cards and controls over the map. */
  insets?: { top: number; bottom: number };
  style?: StyleProp<ViewStyle>;
}

const PIN = 24;
const LABEL_H = 18;


function centroid(points: number[][]): [number, number] {
  let x = 0;
  let y = 0;
  for (const [px = 0, py = 0] of points) {
    x += px;
    y += py;
  }
  return [x / points.length, y / points.length];
}

/** "8 m²", or "5.3 m²" under 10. */
function areaWords(points: number[][]): string {
  const a = polygonArea(points);
  return `${a >= 10 ? Math.round(a) : Math.round(a * 10) / 10} m²`;
}

export function GardenMapView({ beds, plants, mode, selectedId, onSelect, northDeg, insets = { top: 0, bottom: 0 }, style }: GardenMapViewProps) {
  const { colors } = useTheme();
  const [size, setSize] = useState({ width: 0, height: 0 });
  const band = { width: size.width, height: Math.max(size.height - insets.top - insets.bottom, 1) };
  const frame = fitWholeMap({ beds: beds.map((b) => ({ ...b, area_m2: 0 })), pins: plants.map((p) => ({ plant_id: p.id, x_m: p.x_m, y_m: p.y_m, ...(p.canopy_radius_m !== undefined ? { canopy_radius_m: p.canopy_radius_m } : {}) })) }, band.width || 1, band.height, 36);
  const px = (x: number) => (x - frame.ox) * frame.scale;
  const py = (y: number) => (y - frame.oy) * frame.scale + insets.top;
  const drawn = size.width > 0;
  const scaleBar = frame.scale;
  const above = labelsAbove(plants.map((p) => ({ id: p.id, x: px(p.x_m), y: py(p.y_m) })), PIN + LABEL_H);

  return (
    <View
      style={[{ flex: 1, backgroundColor: colors.background }, style]}
      onLayout={(e) => setSize({ width: e.nativeEvent.layout.width, height: e.nativeEvent.layout.height })}
      testID="garden-map"
    >
      {/* A tap on the ground clears the selection, as on the 3D view. */}
      <Pressable style={{ position: "absolute", inset: 0 }} onPress={() => onSelect(null)} accessible={false} />
      {drawn ? (
        <Svg width={size.width} height={size.height} pointerEvents="none" style={{ position: "absolute", left: 0, top: 0 }} {...hiddenSvg}>
          {beds.map((b) => {
            const [cx] = centroid(b.polygon_m);
            return (
              <G key={b.id}>
                <Polygon
                  points={b.polygon_m.map((p) => `${px(p[0] ?? 0)},${py(p[1] ?? 0)}`).join(" ")}
                  fill={colors.wash}
                  stroke={colors.border}
                  strokeWidth={1.5}
                  strokeLinejoin="round"
                />
                {/* The name above the bed, clear of the plant labels inside it. */}
                <SvgText x={px(cx)} y={py(b.polygon_m.reduce((m, p) => Math.min(m, p[1] ?? 0), Infinity)) - 7} fill={colors.textMuted} fontSize={12} fontWeight="600" textAnchor="middle">
                  {`${b.name ?? "Bed"} · ${areaWords(b.polygon_m)}`}
                </SvgText>
              </G>
            );
          })}
          {plants.map((p) => {
            if (p.canopy_radius_m === undefined) return null;
            const frost = mode === "frost" && p.tender;
            return (
              <Circle
                key={`canopy-${p.id}`}
                cx={px(p.x_m)}
                cy={py(p.y_m)}
                r={Math.max(p.canopy_radius_m * frame.scale, PIN / 2 + 3)}
                fill={frost ? colors.infoSoft : "none"}
                stroke={frost ? colors.info : colors.textMuted}
                strokeWidth={frost ? 2 : 1}
                strokeDasharray={frost ? undefined : "3 3"}
              />
            );
          })}
          {/* The scale bar: 1 m, bottom left of the map's band. */}
          <G>
            <Line x1={20} x2={20 + scaleBar} y1={size.height - insets.bottom - 14} y2={size.height - insets.bottom - 14} stroke={colors.text} strokeWidth={2} strokeLinecap="round" />
            <Line x1={20} x2={20} y1={size.height - insets.bottom - 19} y2={size.height - insets.bottom - 9} stroke={colors.text} strokeWidth={2} strokeLinecap="round" />
            <Line x1={20 + scaleBar} x2={20 + scaleBar} y1={size.height - insets.bottom - 19} y2={size.height - insets.bottom - 9} stroke={colors.text} strokeWidth={2} strokeLinecap="round" />
            <SvgText x={20 + scaleBar + 8} y={size.height - insets.bottom - 10} fill={colors.text} fontSize={12} fontWeight="600">
              1 m
            </SvgText>
          </G>
          {/* North, when the compass was good enough during mapping (contracts 9). The map's +y axis points down. */}
          {northDeg !== undefined ? (
            <G transform={`translate(${size.width - 30}, ${size.height - insets.bottom - 30}) rotate(${180 + northDeg})`}>
              <Circle r={16} fill={colors.surface} stroke={colors.border} />
              <Path d="M0 -10 L5 4 L0 1 L-5 4 Z" fill={colors.text} />
            </G>
          ) : null}
        </Svg>
      ) : null}
      {northDeg !== undefined && drawn ? (
        <Text
          variant="caption2"
          emphasis
          accessibilityLabel="The arrow points north"
          style={{ position: "absolute", right: 26, bottom: insets.bottom + 48 }}
        >
          N
        </Text>
      ) : null}
      {drawn
        ? // The selected pin last, so it draws over a neighbour it touches.
          [...plants].sort((a, b) => Number(a.id === selectedId) - Number(b.id === selectedId)).map((p) => {
            const on = p.id === selectedId;
            const up = above.has(p.id);
            const status = p.status === "unknown" ? undefined : p.status;
            const label = p.label ?? p.name;
            const left = px(p.x_m);
            const top = py(p.y_m);
            return (
              <Pressable
                key={p.id}
                accessibilityRole="button"
                accessibilityState={{ selected: on }}
                accessibilityLabel={plantSpoken(p) + (mode === "frost" && p.tender ? ", needs a frost cover" : "")}
                onPress={() => onSelect(p.id)}
                hitSlop={6}
                testID={`map-pin-${p.id}`}
                style={{
                  position: "absolute",
                  left: left - 50,
                  top: up ? top - PIN / 2 - LABEL_H - 4 : top - PIN / 2,
                  width: 100,
                  alignItems: "center",
                  flexDirection: up ? "column-reverse" : "column",
                  gap: 4,
                }}
              >
                <View
                  style={{
                    width: PIN,
                    height: PIN,
                    borderRadius: PIN / 2,
                    alignItems: "center",
                    justifyContent: "center",
                    backgroundColor: colors.surface,
                    borderWidth: on ? 2.5 : 1,
                    borderColor: on ? colors.text : status ? colors[statusRoles[status].fg] : colors.border,
                  }}
                >
                  {status ? <StatusShape status={status} size={11} /> : <Text variant="caption2" color="textMuted">?</Text>}
                </View>
                {/* The label is a chip, the selected one outlined (design 3.1). */}
                <View
                  style={{
                    height: LABEL_H,
                    justifyContent: "center",
                    paddingHorizontal: 8,
                    paddingVertical: 2,
                    borderRadius: radii.pill,
                    backgroundColor: colors.surface,
                    borderWidth: on ? 1.5 : 0,
                    borderColor: colors.text,
                  }}
                >
                  <Text variant="caption2" emphasis numberOfLines={1}>
                    {label}
                  </Text>
                </View>
              </Pressable>
            );
          })
        : null}
    </View>
  );
}
