import * as Haptics from "expo-haptics";
import { Platform } from "react-native";

// Haptics mark real events only (design 6). None on the web.
const enabled = Platform.OS !== "web";

export const haptics = {
  success: () => (enabled ? Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success) : Promise.resolve()),
  warning: () => (enabled ? Haptics.notificationAsync(Haptics.NotificationFeedbackType.Warning) : Promise.resolve()),
  selection: () => (enabled ? Haptics.selectionAsync() : Promise.resolve()),
};
