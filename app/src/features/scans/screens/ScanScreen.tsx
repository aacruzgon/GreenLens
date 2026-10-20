import { useLocalSearchParams, useRouter } from "expo-router";
import { View } from "react-native";

import { useScan } from "@/api";
import { Button, Text, ThemeProvider, space, useTheme } from "@/ui";

// F2 ends at Analyse; the live analysis over the photo is F3 (roadmap §9). Until then this says the scan was
// sent and what state it is in. Always dark, like the capture it continues (design 3.1).
export function ScanScreen() {
  return (
    <ThemeProvider scheme="dark">
      <ScanPlaceholder />
    </ThemeProvider>
  );
}

function ScanPlaceholder() {
  const { colors } = useTheme();
  const { scanId } = useLocalSearchParams<{ scanId: string }>();
  const router = useRouter();
  const scan = useScan(scanId);

  return (
    <View style={{ flex: 1, backgroundColor: colors.background, justifyContent: "center", padding: space[6], gap: space[4] }}>
      <Text variant="title2" accessibilityRole="header">
        The analysis comes next
      </Text>
      <Text variant="body" color="textMuted">
        {scan.data ? `This scan was sent and is ${scan.data.status}.` : "This scan was sent."} The live analysis is the next
        slice of the build.
      </Text>
      <Button label="Back to the garden" variant="secondary" onPress={() => router.dismissAll()} />
    </View>
  );
}
