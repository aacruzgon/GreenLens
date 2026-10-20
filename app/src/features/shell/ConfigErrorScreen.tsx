import { View } from "react-native";

import { Screen, Text, space } from "@/ui";

// A misconfigured build says what is wrong instead of showing a blank page (README §17).
export function ConfigErrorScreen({ problems }: { problems: string[] }) {
  return (
    <Screen>
      <View style={{ paddingTop: space[12], gap: space[3] }}>
        <Text variant="title1" accessibilityRole="header">
          {"This build isn't configured"}
        </Text>
        <Text variant="body" color="textMuted">
          Check app/.env against app/.env.example, then restart the bundler.
        </Text>
        {problems.map((p) => (
          <Text key={p} variant="footnote">
            {p}
          </Text>
        ))}
      </View>
    </Screen>
  );
}
