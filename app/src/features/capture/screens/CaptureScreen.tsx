import { useLocalSearchParams, useRouter } from "expo-router";
import { View } from "react-native";

import { useDeleteScan, useScan } from "@/api";
import { Button, Text, ThemeProvider, space, useTheme } from "@/ui";

// F1 only starts a scan; the camera, its mode bar and the uploads are F2 (roadmap §8).
// Until then the pending scan can be discarded, so nothing is left half-made.
// Capture is always dark, like the camera (design 3.1).
export function CaptureScreen() {
  return (
    <ThemeProvider scheme="dark">
      <CapturePlaceholder />
    </ThemeProvider>
  );
}

function CapturePlaceholder() {
  const { colors } = useTheme();
  const { scanId } = useLocalSearchParams<{ scanId: string }>();
  const router = useRouter();
  const scan = useScan(scanId);
  const deleteScan = useDeleteScan();

  return (
    <View style={{ flex: 1, backgroundColor: colors.background, justifyContent: "center", padding: space[6], gap: space[4] }}>
      <Text variant="title2" accessibilityRole="header">
        The camera comes next
      </Text>
      <Text variant="body" color="textMuted">
        This scan was created. Capture is the next slice of the build.
      </Text>
      <Button
        label="Discard this scan"
        variant="secondary"
        busy={deleteScan.isPending}
        disabled={!scan.data}
        onPress={() => scan.data && deleteScan.mutate(scan.data, { onSettled: () => router.back() })}
      />
    </View>
  );
}
