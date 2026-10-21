// Capture sources (docs/frontend/README.md 13, adapters): what a feature photographs with. CameraPreview is the
// CameraCaptureSource on expo-camera; LidarCaptureSource replaces it on LiDAR iPhones once modules/lidar lands
// (roadmap F5 item 1). pickFromLibrary is the PickerCaptureSource.
import { CameraView, useCameraPermissions } from "expo-camera";
import * as ImagePicker from "expo-image-picker";
import { forwardRef, useImperativeHandle, useRef } from "react";
import { Linking, type StyleProp, type ViewStyle } from "react-native";

/** One shot as a source hands it over (README 13). `depth` comes only from the LiDAR module. */
export interface CapturedShot {
  file: {
    uri: string;
    mimeType: "image/jpeg" | "image/png" | "image/webp" | "image/heic";
    width: number;
    height: number;
  };
  depth?: { depthUri: string; confidenceUri?: string; meta: Record<string, unknown> };
}

export interface CameraPreviewHandle {
  capture(): Promise<CapturedShot>;
}

export interface CameraPreviewProps {
  /** False pauses the session, so a sheet or another screen over capture doesn't keep the camera running. */
  active: boolean;
  style?: StyleProp<ViewStyle>;
  onReady?: () => void;
}

/** The back camera, full frame. Its photos are JPEG with EXIF orientation applied by the camera (skipProcessing off). */
export const CameraPreview = forwardRef<CameraPreviewHandle, CameraPreviewProps>(function CameraPreview(
  { active, style, onReady },
  ref,
) {
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
