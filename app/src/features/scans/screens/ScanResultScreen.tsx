import { useLocalSearchParams, useRouter } from "expo-router";
import { View } from "react-native";

import { useScan } from "@/api";
import { diagnosisCopy } from "@/domain/diagnosis";
import { Button, Text, ThemeProvider, space, useTheme } from "@/ui";

// "See the diagnosis and plan" lands here. The tinted diagnosis, the plan and its mini player are F4 (roadmap 10);
// until then this names the result in the words of screens 6 and goes back to the garden.
export function ScanResultScreen() {
  return (
    <ThemeProvider scheme="dark">
      <ResultPlaceholder />
    </ThemeProvider>
  );
}

function ResultPlaceholder() {
  const { colors } = useTheme();
  const { scanId } = useLocalSearchParams<{ scanId: string }>();
  const router = useRouter();
  const scan = useScan(scanId);
  const diagnosis = scan.data?.diagnosis;
  const copy = diagnosis ? diagnosisCopy(diagnosis, scan.data?.images.length) : undefined;

  return (
    <View style={{ flex: 1, backgroundColor: colors.background, justifyContent: "center", padding: space[6], gap: space[4] }} testID="scan-result">
      <Text variant="display" accessibilityRole="header">
        {copy?.headline ?? "Your result"}
      </Text>
      {copy?.subline ? (
        <Text variant="body" color="textMuted">
          {copy.subline}
        </Text>
      ) : null}
      <Text variant="body" color="textMuted">
        The diagnosis and the plan, step by step, are the next slice of the build.
      </Text>
      <Button label="Back to the garden" variant="secondary" onPress={() => router.dismissAll()} />
    </View>
  );
}
