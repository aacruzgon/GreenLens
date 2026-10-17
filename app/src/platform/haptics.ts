import { Platform } from "react-native";
import * as Haptics from "expo-haptics";

// Shot accepted, retake needed, card confirmed (docs/frontend 4). Silent on the web.
export const haptics = {
  success: () => Platform.OS !== "web" && void Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success),
  warning: () => Platform.OS !== "web" && void Haptics.notificationAsync(Haptics.NotificationFeedbackType.Warning),
  tap: () => Platform.OS !== "web" && void Haptics.selectionAsync(),
};
