// The LiDAR camera's JavaScript side (docs/frontend/README.md 13, the module's JavaScript surface). Where the
// native module is absent (Expo Go, the web, a phone without the module) everything reports no depth and the
// camera view is null, so capture keeps expo-camera.
import { requireNativeViewManager, requireOptionalNativeModule } from "expo-modules-core";
import type { ComponentType, Ref } from "react";
import type { StyleProp, ViewStyle } from "react-native";

/** `DepthMeta` (contracts 3), as the module writes it. A shot without intrinsics carries no depth. */
export interface DepthMeta {
  source: "lidar";
  intrinsics: { fx: number; fy: number; cx: number; cy: number; ref_width: number; ref_height: number };
  distortion?: { center: [number, number]; inverse_lookup: number[] };
  gravity?: [number, number, number];
  device_model: string;
  filtered: boolean;
}

/** What the module measured about a shot: what the LiDAR day-1 check records (architecture 10). */
export interface CaptureReport {
  /** Stored pixels, in the sensor frame, and the EXIF orientation. */
  photo: { width: number; height: number; orientation: number };
  depth?: {
    width: number;
    height: number;
    valid_fraction: number;
    accuracy: "absolute" | "relative";
    filtered: boolean;
    has_distortion_table: boolean;
    centre_distance_m: number | null;
  };
  /** The streamed distance at the moment of the shot. */
  centre_distance_m: number | null;
  gravity: [number, number, number] | null;
  /** Why the shot has no depth part. */
  dropped?: "no_depth" | "no_valid_depth" | "relative_accuracy" | "no_intrinsics" | "encoding";
}

export interface LidarShot {
  file: { uri: string; mimeType: "image/jpeg"; width: number; height: number };
  depth?: { depthUri: string; meta: DepthMeta };
  report: CaptureReport;
}

export interface LidarCameraHandle {
  capture(): Promise<LidarShot>;
}

export interface LidarCameraViewProps {
  active: boolean;
  style?: StyleProp<ViewStyle>;
  ref?: Ref<LidarCameraHandle>;
  /** The median depth at the centre of the frame, five times a second; null when none of it has depth. */
  onDistance?: (e: { nativeEvent: { distance_m: number | null } }) => void;
  onReady?: () => void;
  onMountError?: (e: { nativeEvent: { message: string } }) => void;
  /** The camera was taken away (a call, another app, iPhone Mirroring) or given back. The shutter can't work meanwhile. */
  onInterruption?: (e: { nativeEvent: { interrupted: boolean; reason: CameraInterruption | null } }) => void;
}

export type CameraInterruption = "in_use" | "multitasking" | "system_pressure" | "background" | "other";

interface LidarNative {
  getCapabilities(): { depth: boolean; mapping: boolean };
  sampleDepth(uri: string, points: [number, number][], halfSide: number): Promise<(number | null)[]>;
}

export const native = requireOptionalNativeModule<LidarNative>("Lidar");

let capabilities: { depth: boolean; mapping: boolean } | undefined;

/** Asked once: a back LiDAR depth camera, and ARKit's scene reconstruction. */
export function getCapabilities(): { depth: boolean; mapping: boolean } {
  capabilities ??= native?.getCapabilities() ?? { depth: false, mapping: false };
  return capabilities;
}

/** The camera with depth, or null where the module is absent. */
export const LidarCameraView: ComponentType<LidarCameraViewProps> | null = native
  ? requireNativeViewManager<LidarCameraViewProps>("Lidar", "LidarCameraView")
  : null;

/** The median depth in metres around normalised sensor-frame points of a depth PNG the module wrote. */
export async function sampleDepth(uri: string, points: [number, number][], halfSide = 2): Promise<(number | null)[]> {
  if (!native) throw new Error("No LiDAR module in this build.");
  return native.sampleDepth(uri, points, halfSide);
}
