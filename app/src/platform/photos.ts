import { Platform } from "react-native";
import * as ImagePicker from "expo-image-picker";
import { ImageManipulator, SaveFormat } from "expo-image-manipulator";

// Photo capture and picking (docs/frontend 4 and 13). Phones use the system camera, which takes a
// full-resolution photo; desktops get a file picker. HEIC is converted to JPEG before the first upload.

export interface PickedPhoto {
  uri: string;
  name: string;
  type: string;
  width: number;
  height: number;
  /** The web build's File, uploaded as is. */
  blob?: Blob;
}

export type PhotoSource = "camera" | "library";

export type PickResult = { kind: "photo"; photo: PickedPhoto } | { kind: "cancelled" } | { kind: "denied"; source: PhotoSource };

async function ensurePermission(source: PhotoSource): Promise<boolean> {
  if (Platform.OS === "web") return true;
  const res = source === "camera" ? await ImagePicker.requestCameraPermissionsAsync() : await ImagePicker.requestMediaLibraryPermissionsAsync();
  return res.granted;
}

export async function pickPhoto(source: PhotoSource): Promise<PickResult> {
  if (!(await ensurePermission(source))) return { kind: "denied", source };
  const options: ImagePickerOptionsLike = { mediaTypes: ["images"], quality: 0.9, exif: false };
  const result = source === "camera" ? await ImagePicker.launchCameraAsync(options) : await ImagePicker.launchImageLibraryAsync(options);
  const asset = result.canceled ? undefined : result.assets[0];
  if (!asset) return { kind: "cancelled" };
  return { kind: "photo", photo: await normalise(asset) };
}

type ImagePickerOptionsLike = Parameters<typeof ImagePicker.launchCameraAsync>[0];

async function normalise(asset: ImagePicker.ImagePickerAsset): Promise<PickedPhoto> {
  const type = asset.mimeType ?? "image/jpeg";
  const isHeic = /heic|heif/i.test(type) || /\.hei[cf]$/i.test(asset.fileName ?? "");
  if (isHeic && Platform.OS !== "web") {
    const rendered = await ImageManipulator.manipulate(asset.uri).renderAsync();
    const saved = await rendered.saveAsync({ format: SaveFormat.JPEG, compress: 0.9 });
    return { uri: saved.uri, name: "photo.jpg", type: "image/jpeg", width: saved.width, height: saved.height };
  }
  return {
    uri: asset.uri,
    name: asset.fileName ?? `photo.${type.split("/")[1] ?? "jpg"}`,
    type,
    width: asset.width,
    height: asset.height,
    blob: asset.file ?? undefined,
  };
}

/** Whether this device can open a camera at all; desktops pick files instead. */
export function hasCamera(): boolean {
  if (Platform.OS !== "web") return true;
  if (typeof navigator === "undefined") return false;
  return /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);
}
