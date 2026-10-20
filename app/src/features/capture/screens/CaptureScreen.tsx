import { useLocalSearchParams, useRouter } from "expo-router";
import { View } from "react-native";

import { isApiError, useDeleteScan, useScan } from "@/api";
import { Banner, Button, Text, ThemeProvider, space, useTheme } from "@/ui";

// F1 only starts a scan; the camera, its mode bar and the uploads are F2 (roadmap §8).
// Until then the pending scan can be discarded, so nothing is left half-made. The screen closes only once the
// delete succeeds; a failed one says so and stays, so the scan is never left pending without a word.
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
      {deleteScan.error ? (
        <Banner
          tone="danger"
          title="Couldn't discard this scan"
          message={isApiError(deleteScan.error) ? deleteScan.error.message : undefined}
        />
      ) : null}
      <Button
        label={deleteScan.error ? "Try again" : "Discard this scan"}
        variant="secondary"
        busy={deleteScan.isPending}
        disabled={!scan.data}
        onPress={() => scan.data && deleteScan.mutate(scan.data, { onSuccess: () => router.back() })}
      />
      {/* The placeholder has no other way out; F2's capture screen brings its own close. */}
      {deleteScan.error ? <Button label="Go back, keep the scan" variant="plain" onPress={() => router.back()} /> : null}
    </View>
  );
}
