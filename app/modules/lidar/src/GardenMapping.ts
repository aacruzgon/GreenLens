// The garden mapping session's JavaScript side (docs/frontend/README.md 13; architecture 6.8). The world map is kept
// per garden, so the functions take the garden's ID; the README's surface named only the version.
import { requireNativeViewManager } from "expo-modules-core";
import type { ComponentType, Ref } from "react";
import type { StyleProp, ViewStyle } from "react-native";

import { native as lidar } from "./LidarCamera";

export type TrackingState = "normal" | "limited" | "lost";

export interface GardenMappingHandle {
  undoLast(): Promise<void>;
  clearMarkers(): Promise<void>;
  /** Starts the session over with no map. */
  restart(): Promise<void>;
}

export interface GardenMappingViewProps {
  /** "corner" puts a bed corner on the ground, "pin" a plant's pin. */
  tapMode: "corner" | "pin";
  style?: StyleProp<ViewStyle>;
  ref?: Ref<GardenMappingHandle>;
  onTrackingState?: (e: { nativeEvent: { state: TrackingState } }) => void;
  /** A bed corner, in metres on the session's ground plane. */
  onGroundTap?: (e: { nativeEvent: { x_m: number; y_m: number } }) => void;
  onPinPlaced?: (e: { nativeEvent: { x_m: number; y_m: number; canopy_radius_m?: number } }) => void;
  /** Once a second while tracking is normal: north clockwise from the map's +y axis. */
  onHeading?: (e: { nativeEvent: { north_deg: number; accuracy_deg: number } }) => void;
  onMountError?: (e: { nativeEvent: { message: string } }) => void;
}

interface MappingNative {
  saveWorldMap(gardenId: string, version: number): Promise<void>;
  savedWorldMapVersion(gardenId: string): number | null;
  relocalise(gardenId: string): Promise<"ok" | "failed">;
  removeWorldMaps(): void;
}

const native = lidar as unknown as MappingNative | null;

/** The AR mapping view, or null where the module is absent. */
export const GardenMappingView: ComponentType<GardenMappingViewProps> | null = native
  ? requireNativeViewManager<GardenMappingViewProps>("Lidar", "GardenMappingView")
  : null;

/** Keeps the open session's world map on the phone, tagged with the map version the upload produced. */
export function saveWorldMap(gardenId: string, version: number): Promise<void> {
  if (!native) return Promise.reject(new Error("No LiDAR module in this build."));
  return native.saveWorldMap(gardenId, version);
}

export function savedWorldMapVersion(gardenId: string): number | null {
  return native?.savedWorldMapVersion(gardenId) ?? null;
}

/** Restarts the open session from the saved world map: "ok" once tracking is back, "failed" after 20 s. */
export function relocalise(gardenId: string): Promise<"ok" | "failed"> {
  return native ? native.relocalise(gardenId) : Promise.resolve("failed");
}

/** On sign-out. */
export function removeWorldMaps(): void {
  native?.removeWorldMaps();
}
