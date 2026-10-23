// Capture sources (docs/frontend/README.md 13, adapters): what a feature photographs with. CameraPreview is the
// LiDAR camera from modules/lidar on LiDAR iPhones, with depth and the distance (roadmap F5 item 1), and
// expo-camera everywhere else. pickFromLibrary is the PickerCaptureSource.
import { CameraView, useCameraPermissions } from "expo-camera";
import * as ImagePicker from "expo-image-picker";
import { forwardRef, useImperativeHandle, useRef, useState } from "react";
import { Linking, type StyleProp, type ViewStyle } from "react-native";

import type { DepthMeta } from "@/domain/depth";
import { LidarCameraView, type CameraInterruption, type CaptureReport, type LidarCameraHandle } from "@modules/lidar/src/LidarCamera";

import { capabilities } from "../capabilities";

/** One shot as a source hands it over (README 13). `depth` comes only from the LiDAR module. */
export interface CapturedShot {
  file: {
    uri: string;
    mimeType: "image/jpeg" | "image/png" | "image/webp" | "image/heic";
    width: number;
    height: number;
  };
  depth?: { depthUri: string; confidenceUri?: string; meta: DepthMeta };
  /** LiDAR only: what the module measured about the shot, for the day-1 check. */
  report?: CaptureReport;
}

export type { CameraInterruption, CaptureReport, DepthMeta };

export interface CameraPreviewHandle {
  capture(): Promise<CapturedShot>;
}

export interface CameraPreviewProps {
  /** False pauses the session, so a sheet or another screen over capture doesn't keep the camera running. */
  active: boolean;
  style?: StyleProp<ViewStyle>;
  onReady?: () => void;
  /** LiDAR phones only: metres to what is at the centre of the frame, or null when it has no depth. */
  onDistance?: (metres: number | null) => void;
  /** LiDAR phones only: why the camera was taken away, or null when it is back. The shutter can't work meanwhile. */
  onInterruption?: (reason: CameraInterruption | null) => void;
}

/**
 * The back camera, full frame. Its photos are JPEG with EXIF orientation applied by the camera (skipProcessing off).
 * A LiDAR camera that can't start (no depth format, a session error) falls back to expo-camera for the rest of the
 * screen's life, so the user gets photos without depth rather than a black screen (README 13, adapters).
 */
export const CameraPreview = forwardRef<CameraPreviewHandle, CameraPreviewProps>(function CameraPreview(props, ref) {
  const [lidarFailed, setLidarFailed] = useState(false);
  return LidarCameraView && capabilities().depth && !lidarFailed ? (
    <LidarPreview {...props} ref={ref} onMountError={() => setLidarFailed(true)} />
  ) : (
    <PlainPreview {...props} ref={ref} />
  );
});

/**
 * The LiDAR camera: each photo comes with its depth map and `DepthMeta`, in the sensor frame with its EXIF
 * orientation, and is uploaded unchanged (architecture 6.7). A shot whose depth was dropped is a plain photo.
 */
const LidarPreview = forwardRef<CameraPreviewHandle, CameraPreviewProps & { onMountError: () => void }>(function LidarPreview(
  { active, style, onReady, onDistance, onInterruption, onMountError },
  ref,
) {
  const camera = useRef<LidarCameraHandle>(null);
  useImperativeHandle(ref, () => ({
    async capture() {
      const view = camera.current;
      if (!view) throw new Error("The camera isn't ready.");
      const shot = await view.capture();
      return {
        file: shot.file,
        report: shot.report,
        ...(shot.depth ? { depth: { depthUri: shot.depth.depthUri, meta: { ...shot.depth.meta } } } : {}),
      };
    },
  }));
  if (!LidarCameraView) return null;
  return (
    <LidarCameraView
      ref={camera}
      style={style}
      active={active}
      onReady={onReady}
      onDistance={onDistance ? (e) => onDistance(e.nativeEvent.distance_m) : undefined}
      onInterruption={onInterruption ? (e) => onInterruption(e.nativeEvent.interrupted ? (e.nativeEvent.reason ?? "other") : null) : undefined}
      onMountError={onMountError}
    />
  );
});

const PlainPreview = forwardRef<CameraPreviewHandle, CameraPreviewProps>(function PlainPreview({ active, style, onReady }, ref) {
  const camera = useRef<CameraView>(null);
  useImperativeHandle(ref, () => ({
    async capture() {
      const view = camera.current;
      if (!view) throw new Error("The camera isn't ready.");
      const pic = await view.takePictureAsync({ quality: 0.92, exif: false });
      return { file: { uri: pic.uri, mimeType: "image/jpeg", width: pic.width, height: pic.height } };
    },
  }));
  return <CameraView ref={camera} style={style} facing="back" active={active} onCameraReady={onReady} />;
});

const MIME: Record<string, CapturedShot["file"]["mimeType"]> = {
  "image/jpeg": "image/jpeg",
  "image/png": "image/png",
  "image/webp": "image/webp",
  "image/heic": "image/heic",
};

/**
 * One photo from the library, or null when the user cancels. iOS's "compatible" representation hands over HEIC
 * photos as JPEG, so a picked photo is converted before its first upload (README 11, picked photos).
 */
export async function pickFromLibrary(): Promise<CapturedShot | null> {
  const result = await ImagePicker.launchImageLibraryAsync({
    mediaTypes: ["images"],
    allowsEditing: false,
    quality: 1,
    preferredAssetRepresentationMode: ImagePicker.UIImagePickerPreferredAssetRepresentationMode.Compatible,
  });
  const asset = result.canceled ? undefined : result.assets[0];
  if (!asset) return null;
  return {
    file: { uri: asset.uri, mimeType: MIME[asset.mimeType ?? ""] ?? "image/jpeg", width: asset.width, height: asset.height },
  };
}

export type CameraAccess =
  | { status: "unknown" }
  | { status: "granted" }
  /** Not asked yet, or asked and refused but iOS will ask again. */
  | { status: "ask"; request: () => Promise<void> }
  /** Refused for good: only Settings can change it (HIG Privacy, requesting permission). */
  | { status: "denied"; openSettings: () => Promise<void> };

/** The camera permission, asked only when the user is on the camera and can see why (HIG Privacy). */
export function useCameraAccess(): CameraAccess {
  const [permission, request] = useCameraPermissions();
  if (!permission) return { status: "unknown" };
  if (permission.granted) return { status: "granted" };
  if (permission.canAskAgain) return { status: "ask", request: async () => void (await request()) };
  return { status: "denied", openSettings: () => Linking.openSettings() };
}
