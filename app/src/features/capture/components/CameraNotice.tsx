// A line on the camera, above the shutter: why the camera is paused, or why the last photo failed. A solid dark
// pill like the distance chip (design 2), announced as it appears.
import { View } from "react-native";

import { Text, camera } from "@/ui";

export function CameraNotice({ text }: { text: string }) {
  return (
    <View
      testID="camera-notice"
      accessible
      accessibilityRole="alert"
      accessibilityLiveRegion="polite"
      style={{ alignSelf: "center", maxWidth: "92%", paddingHorizontal: 12, paddingVertical: 6, borderRadius: 999, backgroundColor: camera.pill }}
    >
      <Text variant="footnote" style={{ color: camera.ink, textAlign: "center" }}>
        {text}
      </Text>
    </View>
  );
}
