// Capture's mode bar as Apple's segmented control on Liquid Glass (ios/ModePicker.swift). Null where the module is
// absent (Expo Go, the web, Jest), where capture draws its own bar.
import { requireNativeViewManager, requireOptionalNativeModule } from "expo-modules-core";
import type { ComponentType } from "react";
import type { StyleProp, ViewStyle } from "react-native";

export interface ModePickerItem {
  kind: string;
  label: string;
  mark: "none" | "stored" | "busy" | "problem";
  selected: boolean;
}

export interface ModePickerViewProps {
  items: ModePickerItem[];
  onSelect: (e: { nativeEvent: { kind: string } }) => void;
  style?: StyleProp<ViewStyle>;
  testID?: string;
}

const native = requireOptionalNativeModule("ModePicker");

export const ModePickerView: ComponentType<ModePickerViewProps> | null = native
  ? requireNativeViewManager<ModePickerViewProps>("ModePicker", "ModePickerView")
  : null;
