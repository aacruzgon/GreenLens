// Capture's mode bar as Apple's own control, from modules/lidar (design 3.1 and 6). Null where the module is absent
// (Expo Go, the web), where features/capture draws its bar in React Native.
export { ModePickerView, type ModePickerItem, type ModePickerViewProps } from "@modules/lidar/src/ModePicker";
