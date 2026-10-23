// The 3D garden's JavaScript side (docs/design/garden-3d.md 1, "Hosting it in React Native"; roadmap F5 items 3–4).
// A RealityKit view in the LiDAR module. Where the native module is absent (Expo Go, the web, jest) the view is
// null, and the screen shows the 2D map and the plant list instead.
import { requireNativeViewManager, requireOptionalNativeModule } from "expo-modules-core";
import type { ComponentType } from "react";
import type { StyleProp, ViewStyle } from "react-native";

export type GardenViewMode = "health" | "growth" | "frost";
export type GardenViewStatus = "healthy" | "watch" | "sick" | "dead" | "unknown";

/** A bed, as in `GardenMap.beds[]`; `area_m2` isn't needed. */
export interface GardenViewBed {
  id: string;
  name?: string;
  /** Ground plane, metres: `[[x, y], ...]`. */
  polygon_m: number[][];
}

/** A plant's values for the chosen date. A change here tweens; only a new id, place or bed rebuilds it. */
export interface GardenViewPlant {
  id: string;
  name: string;
  x_m: number;
  y_m: number;
  canopy_radius_m?: number;
  height_m?: number;
  status: GardenViewStatus;
  /** Gets a frost cover in Frost mode. */
  tender?: boolean;
  /** The label's text; by default the name, plus the height in Growth ("Kale · 55 cm"). */
  label?: string;
  /** The bed it stands in; without one (or with an unknown one) it stands in a pot. */
  bed_id?: string;
}

export interface GardenSelectEvent {
  nativeEvent: { plant_id: string | null };
}

export interface GardenViewProps {
  beds: GardenViewBed[];
  plants: GardenViewPlant[];
  mode: GardenViewMode;
  selectedPlantId?: string | null;
  scheme: "light" | "dark";
  /** Off the turntable and the tweens. The view also follows the system's Reduce Motion. */
  reduceMotion?: boolean;
  /** A tap on a plant or its label, or null for a tap on nothing. */
  onSelectPlant?: (e: GardenSelectEvent) => void;
  style?: StyleProp<ViewStyle>;
  accessibilityLabel?: string;
}

const native = requireOptionalNativeModule("LidarGarden");

/** The 3D garden, or null where the module is absent. */
export const GardenView: ComponentType<GardenViewProps> | null = native
  ? requireNativeViewManager<GardenViewProps>("LidarGarden", "GardenView")
  : null;
